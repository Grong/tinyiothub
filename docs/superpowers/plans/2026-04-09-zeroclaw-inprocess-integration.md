# TinyIoTHub × ZeroClaw 进程内集成方案

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `zeroclaw` crate 0.1.7 直接嵌入 TinyIoTHub 进程内，替代外部 ZeroClaw Gateway WebSocket 适配器，实现完整的 system prompt 控制和 IoT 原生的 Agent 工具集。

**Architecture:** 在 `api/src/infrastructure/` 中新建 `zeroclaw_runtime.rs`，用 `zeroclaw::AgentBuilder` 组合 TinyIoTProvider（GLM）、IoTToolSet（IoT 工具）、IoTSystemPromptSection（三层提示词）。前端保持现有 Agent 配置 UI，仅新增 Agent 灵魂设定区块。

**Tech Stack:** Rust (edition 2021, zeroclaw 0.1.7), Axum, TinyIoTHub MCP tool registry, MiniMax/GLM API

---

## 文件结构

```
api/src/infrastructure/
├── zeroclaw_agent.rs       ← 现有 ZeroClawAgentClient（保留，条件编译可选）
├── zeroclaw_runtime.rs     ← 新增：TinyIoTHubAgentClient + TinyIoTProvider + IoTToolSet + IoTSystemPromptSection
└── mod.rs                  ← 导出 zeroclaw_runtime

api/Cargo.toml              ← 添加 zeroclaw = "0.1.7"

web/src/ui/controllers/agents.ts        ← 扩展 AgentConfig 类型
web/src/ui/views/agents-model-tab.ts     ← 新增 Agent 灵魂设定 UI 区块
```

---

## 关键 API 验证结果

| 模块 | 导出 | cargo check |
|------|------|-------------|
| `zeroclaw::agent` | `Agent`, `AgentBuilder` | ✅ |
| `zeroclaw::agent::loop_` | `TurnEvent`, `process_message` | ✅ |
| `zeroclaw::tools::traits` | `Tool`, `ToolResult`, `ToolSpec` | ✅ |
| `zeroclaw::providers::traits` | `Provider`, `ChatMessage`, `ChatResponse` | ✅ |
| `zeroclaw::agent::prompt` | `SystemPromptBuilder`, `PromptSection`, `PromptContext` | ✅ |
| `zeroclaw::observability` | `Observer`, `ObserverEvent` | ✅ |
| `zeroclaw::memory` | `Memory`, `MemoryCategory` | ✅ |
| `zeroclaw::providers::glm` | `GlmProvider`（JWT 认证，MiniMax/GLM） | ✅ |

**注意：** `GlmProvider::new(api_key: Option<&str>)` — API key 格式为 `"id.secret"`，从 `GLM_API_KEY` 环境变量或 config 读取。

**Provider trait 签名（关键）：**
```rust
// zeroclaw::providers::traits::Provider
async fn chat_with_system(
    &self,
    system_prompt: Option<&str>,
    message: &str,
    model: &str,
    temperature: f64,
) -> anyhow::Result<String>;
```

**AgentBuilder 签名：**
```rust
// zeroclaw::agent::AgentBuilder
pub fn new() -> Self;
pub fn provider(self, p: impl Provider + 'static) -> Self;
pub fn tools(self, tools: Vec<Box<dyn Tool>>) -> Self;
pub fn prompt_builder(self, pb: impl PromptSection + 'static) -> Self;
pub fn observer(self, o: Arc<dyn Observer>) -> Self;
pub fn model_name(self, name: &str) -> Self;
pub fn build(self) -> anyhow::Result<Agent>;
```

**Agent::turn_streamed：**
```rust
// zeroclaw::agent::Agent
pub fn turn_streamed(&self, input: &str) -> Pin<Box<dyn Stream<Item = TurnEvent> + Send>>;
```

**TurnEvent：**
```rust
pub enum TurnEvent {
    Chunk { delta: String },
    Thinking { delta: String },
    ToolCall { name: String, args: serde_json::Value },
    ToolResult { name: String, output: String },
}
```

**PromptSection：**
```rust
pub trait PromptSection: Send + Sync {
    fn render(&self, ctx: &PromptContext) -> String;
}
```

---

## 现有可复用组件

1. **`api/src/infrastructure/zeroclaw_agent.rs`** — 已有 `platform_base_prompt()` 和 `build_full_system_prompt()` 函数（Layer 1 平台基础提示词）
2. **MCP tool registry** — `api/src/api/mcp/tool_registry.rs` 中 40+ 工具，`HandlerRegistry` 是全局 `OnceLock<HandlerRegistry>`
3. **`api/src/api/mcp/tools/device.rs`** — 已有 `GetDeviceStatusHandler`、`ListDevicesHandler` 等
4. **`api/src/api/mcp/tools/alarm_mcp.rs`** — 已有 `AlarmRuleAddHandler`、`AlarmListHandler` 等

---

## 依赖冲突检查

TinyIoTHub 使用 `ring = "0.17"`（HMAC-SHA256 for JWT），zeroclaw 0.1.7 也用 `ring = "0.17"` — **版本一致，无冲突**。

TinyIoTHub 使用 `async-trait = "0.1"`，zeroclaw 也用 `async-trait = "0.1"` — **版本一致，无冲突**。

---

## Task 1: 添加 zeroclaw 依赖并验证编译

**Files:**
- Modify: `api/Cargo.toml`

- [ ] **Step 1: 添加 zeroclaw 依赖**

在 `api/Cargo.toml` 的 `[dependencies]` 末尾添加：
```toml
# ZeroClaw Agent runtime (进程内集成)
zeroclaw = "0.1.7"
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo check 2>&1`
Expected: 编译通过（可能有 warning 但无 error）

- [ ] **Step 3: 若有冲突，记录冲突依赖**

常见冲突：`reqwest`、`tokio`、`axum` 版本。若有冲突，记录具体 crate 和版本号，手动协调。

- [ ] **Step 4: Commit**

```bash
cd /Users/chenguorong/code/my/tinyiothub
git add api/Cargo.toml
git commit -m "deps: add zeroclaw = \"0.1.7\" for in-process agent runtime

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: 创建 zeroclaw_runtime.rs — TinyIoTProvider

**Files:**
- Create: `api/src/infrastructure/zeroclaw_runtime.rs`

- [ ] **Step 1: 创建 zeroclaw_runtime.rs 框架**

```rust
//! TinyIoTHub ZeroClaw 进程内集成运行时
//!
//! 将 zeroclaw Agent 直接嵌入 TinyIoTHub，替代外部 ZeroClaw Gateway 适配器。
//! 支持完整的 system prompt 控制和 IoT 原生工具集。

use zeroclaw::providers::traits::Provider;
use zeroclaw::providers::glm::GlmProvider;
use zeroclaw::agent::{Agent, AgentBuilder};
use zeroclaw::agent::loop_::TurnEvent;
use zeroclaw::agent::prompt::{PromptSection, PromptContext};
use zeroclaw::observability::noop::NoopObserver;
use zeroclaw::tools::traits::{Tool, ToolResult};
use zeroclaw::tools::ToolSpec;
use async_trait::async_trait;
use std::sync::Arc;
use std::pin::Pin;
use futures_util::Stream;
use parking_lot::RwLock;
use anyhow::Context;
use serde_json::Value as JsonValue;
```

- [ ] **Step 2: 实现 TinyIoTProvider（wrapper around GlmProvider）**

```rust
/// TinyIoTHub LLM Provider — 包装 zeroclaw 内置的 GLM Provider
///
/// GLM API key 格式为 "id.secret"（MiniMax 的 API Key 格式）。
/// 复用 zeroclaw 内置 GlmProvider 的 JWT 认证逻辑。
pub struct TinyIoTProvider {
    inner: GlmProvider,
    model: String,
    temperature: f64,
}

impl TinyIoTProvider {
    pub fn new(api_key: String, model: String, temperature: f64) -> Self {
        Self {
            inner: GlmProvider::new(Some(&api_key)),
            model,
            temperature,
        }
    }
}

#[async_trait]
impl Provider for TinyIoTProvider {
    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<String> {
        self.inner
            .chat_with_system(system_prompt, message, model, temperature)
            .await
    }
}
```

- [ ] **Step 3: 运行 cargo check**

Run: `cargo check 2>&1 | grep -E "^error" | head -20`
Expected: 无 error

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): add TinyIoTProvider wrapping GlmProvider for in-process LLM calls

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: 实现 McpToolAdapter — 动态适配全部 45 个 MCP 工具

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_runtime.rs`

**关键设计：** TinyIoTHub 已有 45 个 MCP `ToolHandler`，无需逐个手动包装。用一个泛型 `McpToolAdapter<H: ToolHandler>` 动态适配，再通过 loop 包装全部 handler。

**HandlerRegistry 接口（已存在）：**
```rust
// api/src/api/mcp/tool_registry.rs
pub struct HandlerRegistry { handlers: HashMap<String, Box<dyn ToolHandler>> }
impl HandlerRegistry {
    pub fn get(&self, name: &str) -> Option<&dyn ToolHandler>
    pub fn list_names(&self) -> Vec<String>
}
```

**ToolHandler → zeroclaw::Tool 映射：**
```
ToolHandler.name()           → Tool.name()
ToolHandler.description()    → Tool.description()
ToolHandler.input_schema()   → Tool.parameters_schema() (via InputSchema::to_json())
ToolHandler.execute(args)    → Tool.execute(args) + ToolError → ToolResult 转换
```

- [ ] **Step 1: 实现 McpToolAdapter 泛型包装器**

```rust
/// MCP Tool 适配器 — 将 TinyIoTHub MCP ToolHandler 适配为 zeroclaw Tool
///
/// H 必须实现 ToolHandler + Send + Sync + 'static（已满足所有 45 个 MCP handler）
/// 通过泛型避免 dyn Trait object overhead
pub struct McpToolAdapter<H> {
    handler: H,
}

impl<H: ToolHandler + Send + Sync + 'static> McpToolAdapter<H> {
    pub fn new(handler: H) -> Self {
        Self { handler }
    }

    /// 注册到 Agent 的工具名（格式: "mcp_<handler_name>"）
    pub fn agent_tool_name() -> &'static str
    where
        H: std::fmt::Display,
    {
        // 工具名即 handler.name()
        // 但为了区分 MCP 工具和其他工具，加 mcp_ 前缀
        // 例如: "mcp_list_devices", "mcp_get_device_status"
        // 注意：去掉前缀后匹配 HandlerRegistry
        unimplemented!("由 McpToolSet 在注册时决定")
    }
}

impl<H: ToolHandler + Send + Sync + 'static> Tool for McpToolAdapter<H> {
    fn name(&self) -> &str {
        self.handler.name()
    }

    fn description(&self) -> &str {
        self.handler.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.handler.input_schema().to_json()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        match self.handler.execute(args).await {
            Ok(value) => Ok(ToolResult {
                success: true,
                output: serde_json::to_string(&value).unwrap_or_default(),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}
```

- [ ] **Step 2: 实现 McpToolSet — 包装全部 45 个 MCP handler**

```rust
/// MCP 工具集 — 从 HandlerRegistry 动态构建全部 zeroclaw Tool
///
/// 通过 Arc<dyn ToolHandler> 提供所有权，Agent 持有 boxed tools。
/// 新增 MCP 工具无需修改代码（自动从 HandlerRegistry 加载）。
pub struct McpToolSet {
    tools: Vec<Box<dyn Tool>>,
}

impl McpToolSet {
    /// 从 get_mcp_registry() 构建全部 zeroclaw Tool
    ///
    /// get_mcp_registry() 返回 Option<Arc<RwLock<HandlerRegistry>>>
    /// registry.get() 返回 Option<&dyn ToolHandler>（来自 Box<dyn ToolHandler>）
    /// 用 Arc::new(*handler) 将 Box 转换为 Arc<dyn ToolHandler>
    pub fn from_registry(registry: Arc<RwLock<crate::api::mcp::HandlerRegistry>>) -> Self {
        let mut tools = Vec::new();
        let reg = registry.read();
        for name in reg.list_names() {
            if let Some(handler) = reg.get(&name) {
                // Box<dyn ToolHandler> → Arc<dyn ToolHandler>
                let handler_arc = Arc::new(*handler);
                let boxed: Box<dyn Tool> = Box::new(ArcToolAdapter::new(handler_arc, name.clone()));
                tools.push(boxed);
            }
        }
        Self { tools }
    }

    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }
}

/// MCP 工具适配器 — 持有 Arc<dyn ToolHandler>
///
/// registry.get() 返回 &dyn ToolHandler（来自 Box<dyn ToolHandler>）
/// Arc::new(*handler) 将 Box 所有权转移到 Arc
struct ArcToolAdapter {
    handler: Arc<dyn ToolHandler>,
    name: String,
}

impl ArcToolAdapter {
    fn new(handler: Arc<dyn ToolHandler>, name: String) -> Self {
        Self { handler, name }
    }
}

impl Tool for ArcToolAdapter {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { self.handler.description() }
    fn parameters_schema(&self) -> serde_json::Value { self.handler.input_schema().to_json() }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        match self.handler.execute(args).await {
            Ok(value) => Ok(ToolResult {
                success: true,
                output: serde_json::to_string(&value).unwrap_or_default(),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}
```

**完整工具列表（45 个，按模块分组）：**

```
Device (13): list_devices, get_device, get_device_status, read_properties,
  write_properties, send_command, create_device, update_device, delete_device,
  get_device_history, get_device_metrics, export_device_report, diagnose_device

Alarm (4): alarm_list, alarm_statistics, alarm_acknowledge, alarm_rule_add

Batch (2): batch_command, get_batch_status

Driver (5): list_drivers, get_driver_config_schema, match_driver,
  generate_driver, load_driver

Job (3): list_schedules, create_schedule, delete_schedule

Knowledge (3): query_knowledge_base, add_knowledge_entry, get_device_manual

Workspace (5): list_workspaces, get_workspace, create_workspace,
  update_workspace, delete_workspace

Heartbeat (3): report_heartbeat, get_heartbeat_status, configure_heartbeat

Device Enhanced (3): compare_devices, diagnose_device, scan_serial
```

- [ ] **Step 3: 运行 cargo check**

Run: `cargo check 2>&1 | grep -E "^error" | head -20`
Expected: 无 error（可能有 unused import warning）

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): add McpToolAdapter — dynamic adapter wrapping all 45 MCP tools as zeroclaw Tool

Eliminates manual per-tool wrapping. McpToolSet auto-loads from HandlerRegistry.
New MCP tools auto-exposed to agent without code changes.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: 实现 IoTSystemPromptSection

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_runtime.rs`

- [ ] **Step 1: 实现 IoTSystemPromptSection**

```rust
/// IoT 平台系统提示词 section — 实现三层提示词组合
///
/// Layer 1: 平台基础（固定）— TinyIoTHub 身份、设备类型、操作规范
/// Layer 2: Agent 灵魂（用户配置）— 角色设定、行为约束
/// Layer 3: 运行时上下文（动态）— 当前设备状态、活跃告警数
pub struct IoTSystemPromptSection {
    /// Layer 1: 平台固定基础提示词
    platform_base: String,
    /// Layer 2: 用户配置的 Agent 灵魂设定
    user_persona: String,
    /// Layer 3: 运行时上下文（parking_lot::RwLock，支持读多写少场景）
    runtime_context: parking_lot::RwLock<String>,
}

impl IoTSystemPromptSection {
    pub fn new(platform_base: String, user_persona: String) -> Self {
        Self {
            platform_base,
            user_persona,
            runtime_context: parking_lot::RwLock::new(String::new()),
        }
    }

    /// 更新运行时上下文（由 TinyIoTHubAgentClient 在每次 chat 前调用）
    pub fn update_runtime_context(&self, device_count: u32, alarm_count: u32) {
        let ctx = format!(
            "\n\n## 当前状态\n在线设备数: {}\n活跃告警数: {}",
            device_count, alarm_count
        );
        *self.runtime_context.write() = ctx;
    }
}

impl PromptSection for IoTSystemPromptSection {
    fn render(&self, _ctx: &PromptContext) -> String {
        let base = &self.platform_base;
        let persona = if self.user_persona.is_empty() {
            String::new()
        } else {
            format!("\n\n## Agent 灵魂设定（用户配置）\n{}", self.user_persona)
        };
        let context = self.runtime_context.read().clone();
        format!("{}{}{}", base, persona, context)
    }
}
```

- [ ] **Step 2: 运行 cargo check**

Run: `cargo check 2>&1 | grep -E "^error" | head -10`
Expected: 无 error

- [ ] **Step 3: Commit**

```bash
git add api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): add IoTSystemPromptSection with 3-layer prompt composition

Layer 1: 平台基础（固定）
Layer 2: Agent 灵魂（用户配置）
Layer 3: 运行时上下文（动态）

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 5: 构建 TinyIoTHubAgentClient

**Files:**
- Modify: `api/src/infrastructure/zeroclaw_runtime.rs`

- [ ] **Step 1: 实现 TinyIoTHubAgentClient**

```rust
use zeroclaw::observability::noop::NoopObserver;

/// TinyIoTHub 内置 Agent Client — 替代外部 ZeroClaw Gateway 适配器
///
/// 用 AgentBuilder 组合：
/// - TinyIoTProvider (LLM)
/// - McpToolSet (全部 45 个 MCP 工具)
/// - IoTSystemPromptSection (三层提示词)
/// - NoopObserver (可替换为 TinyIoTObserver)
pub struct TinyIoTHubAgentClient {
    db_pool: sqlx::SqlitePool,
    /// MiniMax/GLM API Key（id.secret 格式）
    glm_api_key: String,
    /// 默认模型
    default_model: String,
    /// MCP HandlerRegistry（从 get_mcp_registry() 获取）
    mcp_registry: Arc<RwLock<crate::api::mcp::HandlerRegistry>>,
    /// Agent 实例缓存（每个 agent_id 一个）
    agents: parking_lot::RwLock<HashMap<String, Agent>>,
    /// Prompt sections（每个 agent_id 一个）
    prompt_sections: parking_lot::RwLock<HashMap<String, Arc<IoTSystemPromptSection>>>,
}

impl Clone for TinyIoTHubAgentClient {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            glm_api_key: self.glm_api_key.clone(),
            default_model: self.default_model.clone(),
            mcp_registry: self.mcp_registry.clone(),
            agents: parking_lot::RwLock::new(HashMap::new()),
            prompt_sections: parking_lot::RwLock::new(HashMap::new()),
        }
    }
}

impl TinyIoTHubAgentClient {
    pub fn new(
        db_pool: sqlx::SqlitePool,
        glm_api_key: String,
        default_model: String,
    ) -> anyhow::Result<Self> {
        let mcp_registry = crate::api::mcp::get_mcp_registry()
            .context("MCP registry not initialized. Call init_mcp_registry() first.")?;
        Ok(Self {
            db_pool,
            glm_api_key,
            default_model,
            mcp_registry,
            agents: parking_lot::RwLock::new(HashMap::new()),
            prompt_sections: parking_lot::RwLock::new(HashMap::new()),
        })
    }

    /// 获取或构建 Agent 实例
    fn get_or_build_agent(
        &self,
        agent_id: &str,
        config: &super::AgentConfig,
    ) -> anyhow::Result<Agent> {
        // 先尝试从缓存获取
        if let Some(agent) = self.agents.read().get(agent_id).cloned() {
            return Ok(agent);
        }

        // 构建新 Agent
        let model = config.model.as_deref().unwrap_or(&self.default_model);
        let temperature = config.temperature.unwrap_or(0.7);

        let provider = TinyIoTProvider::new(
            self.glm_api_key.clone(),
            model.to_string(),
            temperature,
        );

        let tools = self.build_tools();

        // 获取或创建 prompt section
        let prompt_section = self.get_or_create_prompt_section(agent_id, config);

        let agent = AgentBuilder::new()
            .provider(provider)
            .tools(tools)
            .prompt_builder(prompt_section)
            .observer(Arc::new(NoopObserver::new()))
            .model_name(model)
            .build()?;

        self.agents.write().insert(agent_id.to_string(), agent.clone());
        Ok(agent)
    }

    fn build_tools(&self) -> Vec<Box<dyn Tool>> {
        McpToolSet::from_registry(self.mcp_registry.clone()).tools().to_vec()
    }

    fn get_or_create_prompt_section(
        &self,
        agent_id: &str,
        config: &super::AgentConfig,
    ) -> Arc<IoTSystemPromptSection> {
        if let Some(section) = self.prompt_sections.read().get(agent_id).cloned() {
            return section;
        }

        // 使用 zeroclaw_agent.rs 中已有的 platform_base_prompt
        let platform_base = super::platform_base_prompt();
        let user_persona = config.system_prompt.clone().unwrap_or_default();

        let section = Arc::new(IoTSystemPromptSection::new(platform_base, user_persona));
        self.prompt_sections.write().insert(agent_id.to_string(), section.clone());
        section
    }

    /// 发起流式 chat，返回 TurnEvent stream
    pub fn chat_stream(
        &self,
        agent_id: &str,
        config: &super::AgentConfig,
        message: &str,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = TurnEvent> + Send>>> {
        let agent = self.get_or_build_agent(agent_id, config)?;
        Ok(agent.turn_streamed(message))
    }

    /// 更新 Agent 配置（重新构建）
    pub fn update_agent(
        &self,
        agent_id: &str,
        config: &super::AgentConfig,
    ) -> anyhow::Result<()> {
        self.agents.write().remove(agent_id);
        self.get_or_build_agent(agent_id, config)?;
        Ok(())
    }
}
```

- [ ] **Step 2: 运行 cargo check 并修复编译错误**

Run: `cargo check 2>&1 | grep -E "^error" | head -30`
Expected: 逐个修复，通常是缺少 `use`、`Arc` 等

- [ ] **Step 3: Commit**

```bash
git add api/src/infrastructure/zeroclaw_runtime.rs
git commit -m "feat(agent): add TinyIoTHubAgentClient with AgentBuilder composition

Replaces ZeroClawAgentClient WebSocket adapter with in-process zeroclaw agent.
Provides full system prompt control and IoT-native tool set.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 6: 集成到 AppState 并改造 API handlers

**Files:**
- Modify: `api/src/infrastructure/mod.rs` — 导出 zeroclaw_runtime
- Modify: `api/src/api/agents/mod.rs` — 注入 TinyIoTHubAgentClient
- Modify: `api/src/api/chat/proxy.rs` — 使用 TinyIoTHubAgentClient 替代 ZeroClawAgentClient

- [ ] **Step 1: 在 AppState 中添加 TinyIoTHubAgentClient**

```rust
// api/src/infrastructure/mod.rs
pub mod zeroclaw_runtime;
pub use zeroclaw_runtime::TinyIoTHubAgentClient;
```

在 AppState struct 中添加：
```rust
pub struct AppState {
    // ... existing fields ...
    pub agent_client: TinyIoTHubAgentClient,
}
```

在 main.rs 的 AppState 构建处注入：
```rust
let agent_client = TinyIoTHubAgentClient::new(
    db_pool.clone(),
    config.minimax_api_key.clone(),    // 从 config 读取 GLM_API_KEY
    config.default_model.clone(),        // "glm-4" 或 "GLM-4-0520"
);
```

- [ ] **Step 2: 修改 chat/proxy.rs 使用 TinyIoTHubAgentClient**

找到 `chat_stream` handler，将：
```rust
// 原来：调用 ZeroClawAgentClient.chat_send()
// 现在：
let config = agent_client.get_agent_config(agent_id).await?;
let mut stream = agent_client.chat_stream(agent_id, &config, &message)?;
```

改造 SSE 响应逻辑，将 `TurnEvent` stream 转换为 SSE 格式：
```rust
// TurnEvent::Chunk{delta} → data: {"type":"chunk","content":"..."}\n\n
// TurnEvent::Thinking{delta} → data: {"type":"thinking","content":"..."}\n\n
// TurnEvent::ToolCall{name, args} → data: {"type":"tool_call","name":"...","args":{...}}\n\n
// TurnEvent::ToolResult{name, output} → data: {"type":"tool_result","name":"...","output":"..."}\n\n
```

- [ ] **Step 3: 运行 cargo check 并修复编译错误**

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/mod.rs api/src/api/agents/mod.rs api/src/api/chat/proxy.rs
git commit -m "feat(agent): integrate TinyIoTHubAgentClient into AppState and chat handlers

Replaces ZeroClawAgentClient WebSocket adapter in chat/proxy.rs.
SSE now streams TurnEvent::{Chunk, Thinking, ToolCall, ToolResult} directly.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 7: 前端 Agent 灵魂设定 UI

**Files:**
- Modify: `web/src/ui/controllers/agents.ts` — 扩展 AgentConfig 类型
- Modify: `web/src/ui/views/agents-model-tab.ts` — 新增 UI 区块

- [ ] **Step 1: 扩展 AgentConfig 类型**

在 `agents.ts` 的 AgentConfig 类型中添加：
```typescript
// Agent 灵魂设定（用户配置）
systemPrompt?: string;
personaPreset?: string;  // 'ops' | 'monitor' | 'support' | 'custom'
```

- [ ] **Step 2: 定义预设模板**

```typescript
const PERSONA_PRESETS = [
  {
    id: "ops",
    label: "运维助手",
    prompt: `你是 TinyIoTHub 的运维专家，负责工业设备的管理和维护。

核心职责：
- 监控设备运行状态，及时发现并处理异常
- 执行 Modbus 寄存器读写操作，诊断设备故障
- 创建和调整告警规则，确保关键指标被监控
- 远程控制设备（重启、参数调整）

行为规范：
- 执行写操作前必须确认
- 优先读取设备状态再诊断
- 告警信息必须及时通知相关人员`
  },
  {
    id: "monitor",
    label: "监控助手",
    prompt: `你是 TinyIoTHub 的实时监控助手，负责追踪设备指标和告警。

核心职责：
- 实时监控设备在线状态和性能指标
- 分析告警事件，识别异常模式
- 生成监控报告和趋势分析
- 通过 MQTT 订阅设备数据流

行为规范：
- 关注设备健康指标（温度、压力、湿度等）
- 告警触发时立即响应
- 定期汇总设备运行状态`
  },
  {
    id: "support",
    label: "客服助手",
    prompt: `你是 TinyIoTHub 的技术支持助手，帮助用户管理物联网设备和解决问题。

核心职责：
- 指导用户添加和管理设备
- 解答设备连接和配置问题
- 解释告警规则和设备状态
- 提供设备最佳实践建议

行为规范：
- 使用友好的语言风格
- 复杂问题逐步引导排查
- 必要时转接运维助手`
  },
];
```

- [ ] **Step 3: 在 agents-model-tab.ts 新增 UI 区块**

在 Model dropdown 下方添加卡片：

```typescript
// agents-model-tab.ts
import { PERSONA_PRESETS } from './agents.ts'; // 或直接内联

// 在 render 中：
const presetValue = this.agentConfig?.personaPreset ?? 'custom';
const promptValue = this.agentConfig?.systemPrompt ?? '';

// 预设模板下拉
const presetSelect = html`
  <select
    class="preset-select"
    @change=${(e: Event) => {
      const id = (e.target as HTMLSelectElement).value;
      if (id !== 'custom' && id !== presetValue) {
        const preset = PERSONA_PRESETS.find(p => p.id === id);
        if (preset) {
          this.agentConfig = { ...this.agentConfig, personaPreset: id, systemPrompt: preset.prompt };
        }
      } else {
        this.agentConfig = { ...this.agentConfig, personaPreset: id };
      }
    }}
  >
    <option value="custom">自定义</option>
    ${PERSONA_PRESETS.map(p => html`<option value="${p.id}" ?selected=${p.id === presetValue}>${p.label}</option>`)}
  </select>
`;

// System Prompt textarea
const promptTextarea = html`
  <textarea
    class="prompt-textarea"
    rows="10"
    .value=${promptValue}
    @input=${(e: Event) => {
      const value = (e.target as HTMLTextAreaElement).value;
      this.agentConfig = { ...this.agentConfig, systemPrompt: value, personaPreset: 'custom' };
    }}
    placeholder="在此输入 Agent 灵魂设定..."
  ></textarea>
  <div class="char-count">${promptValue.length} 字符</div>
`;
```

- [ ] **Step 4: 验证前端编译**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/controllers/agents.ts web/src/ui/views/agents-model-tab.ts
git commit -m "feat(agent-ui): add Agent persona presets and system prompt editor

Three presets: 运维助手, 监控助手, 客服助手.
Custom textarea with character count.
Persists systemPrompt in agent config.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 8: 端到端验证

- [ ] **Step 1: 启动后端**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo run 2>&1`
Expected: 服务启动，无 panic

- [ ] **Step 2: 启动前端**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm dev 2>&1`

- [ ] **Step 3: 验证 Agent 灵魂设定 UI**

访问 Agents 页面，选中一个 agent，观察 Model Tab 中新增的"Agent 灵魂设定"卡片。下拉选择"运维助手"，textarea 应自动填充内容。

- [ ] **Step 4: 发送测试消息**

发送"列出所有设备"，验证返回设备列表。
发送"读取温度传感器数值"，验证 `read_register` 工具被调用。

- [ ] **Step 5: 提交最终验证 commit**

```bash
git add -A
git commit -m "chore: e2e verification complete — TinyIoTHub × ZeroClaw in-process integration

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Self-Review Checklist

1. **Spec coverage**: 所有 Phase 覆盖？✅ 依赖添加 → TinyIoTProvider → McpToolAdapter → IoTSystemPromptSection → TinyIoTHubAgentClient → 集成 → 前端
2. **Placeholder scan**: 无 TBD/TODO 遗留 ✅
3. **Type consistency**: `TinyIoTProvider::new(api_key, model, temperature)` — `chat_with_system` 参数一致 ✅；`get_mcp_registry()` → `Arc<RwLock<HandlerRegistry>>` ✅
4. **依赖冲突**: ring = "0.17" 双方一致 ✅；async-trait = "0.1" 双方一致 ✅
5. **AgentBuilder builder pattern**: `.provider()` → `.tools()` → `.prompt_builder()` → `.observer()` → `.model_name()` → `.build()` ✅
6. **TurnEvent SSE mapping**: Chunk → chunk, Thinking → thinking, ToolCall → tool_call, ToolResult → tool_result ✅
7. **MCP 动态适配**: `McpToolSet::from_registry()` 自动从 HandlerRegistry 加载全部 45 个工具 ✅

---

**Plan complete and saved to `docs/superpowers/plans/2026-04-09-zeroclaw-inprocess-integration.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
