# AI Agent 架构重构设计

> 日期: 2026-04-11
> 状态: 设计完成，待评审
> 评审模式: SELECTIVE EXPANSION（分阶段实施）

## 背景

当前 AI Agent 系统存在以下问题：

1. **架构混乱**：`zeroclaw_agent.rs`（2655行）和 `zeroclaw_runtime.rs`（725行）混杂了外部网关适配器和内置运行时，职责不清
2. **外部网关未启用**：`ZeroClawAgentClient` 和 `FallbackAgentClient` 设计用于连接外部 OpenClaw Gateway，但生产环境使用的是内置 `TinyIoTHubAgentClient`
3. **命名不统一**：文件和方法名带有 `zeroclaw` 痕迹，但实际是 TinyIoTHub 的内置 Agent
4. **层次不清**：Domain/Application/Infrastructure 三层边界模糊

**决策**：简化架构，只保留内置 zeroclaw runtime 作为底层引擎，上层命名完全去 zeroclaw 化，建立清晰的 DDD 分层。

**参考**：
- Claude Code 源码分析（liuup/claude-code-analysis）：6 层分层、Memory 多层体系、Tool 协议化、Skills 系统
- OpenClaw 架构：Gateway 控制平面、Sessions 管理、A2UI Canvas

---

## 1. 目标架构

### 1.1 模块层次

```
┌─────────────────────────────────────────────────────────────┐
│  API Layer (chat_handler, skills_handler, agents_handler)  │
│  职责：HTTP 请求路由 → 应用服务调用 → SSE 流式响应返回       │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│  Application Layer (chat_service, session_service,         │
│                    agent_memory_service)                   │
│  职责：Query 主循环、会话管理、多层 Memory 管理            │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│  Infrastructure Layer (agent/runtime, agent/prompt,        │
│                       agent/catalog, agent/tools)          │
│  职责：Agent 运行时（zeroclaw 引擎）、工具注册、Prompt 构建│
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│  Domain Layer (skill, device_memory, compact_service)     │
│  职责：纯业务实体和领域逻辑，无外部依赖                    │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 核心组件职责

| 组件 | 所在层 | 文件 | 核心职责 |
|------|--------|------|---------|
| `ChatService` | Application | `application/agent/chat_service.rs` | Query 主循环，多轮对话编排 |
| `SessionService` | Application | `application/agent/session_service.rs` | 会话生命周期管理 |
| `AgentMemoryService` | Application | `application/agent/memory_service.rs` | 整合 Auto/Session/Agent Memory |
| `AgentRuntime` | Infrastructure | `infrastructure/agent/runtime.rs` | zeroclaw 引擎封装 |
| `PromptBuilder` | Infrastructure | `infrastructure/agent/prompt.rs` | System prompt 拼装 |
| `ToolCatalog` | Infrastructure | `infrastructure/agent/catalog.rs` | 工具目录生成 |
| `AgentSkill` | Domain | `domain/agent/skill.rs` | 技能实体 |
| `DeviceMemory` | Domain | `domain/agent/device_memory.rs` | 设备快照实体 |
| `CompactService` | Domain | `domain/agent/compact_service.rs` | 对话压缩逻辑 |

### 1.3 数据流

```
用户消息
    │
    ▼
ChatHandler (HTTP POST /api/v1/chat)
    │
    ▼
ChatService::chat()
    │
    ├──► SessionService::get_or_create_session()  ──► 持久化会话
    │
    ├──► AgentMemoryService::build_context()       ──► Auto + Agent Memory 注入
    │
    ├──► PromptBuilder::build()                   ──► system_prompt 组装
    │
    ├──► AgentRuntime::turn_streamed()            ──► zeroclaw 执行
    │         │
    │         ├──► IoTToolAdapter (MCP → zeroclaw Tool)
    │         │
    │         └──► SSE 事件 (delta / tool_call / final)
    │
    ├──► CompactService::check_and_compact()       ──► 长会话压缩
    │
    └──► 返回 SSE 流
```

---

## 2. Application 层

### 2.1 ChatService

```rust
// application/agent/chat_service.rs

/// ChatService - Query 主循环的核心编排器
pub struct ChatService {
    runtime: Arc<AgentRuntime>,
    session_repo: Arc<dyn SessionRepository>,
    memory_service: Arc<AgentMemoryService>,
    compact_service: Arc<CompactService>,
    config: AgentServiceConfig,
}

/// 聊天请求
pub struct ChatRequest {
    pub session_key: String;      // "agent:workspace_id:agent_id/sess_xxx"
    pub message: String;          // 用户输入
    pub run_id: String;           // 本次运行 ID
    pub system_prompt_override: Option<String>,
}

/// SSE 事件类型
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ChatEvent {
    Delta { message: serde_json::Value },
    Thinking { thinking: String },
    ToolCallStart { tool_name: String, tool_args: String, a2ui: Option<String> },
    ToolResult { tool_name: String, result: String },
    Final { message: serde_json::Value },
    Error { error: String },
}

impl ChatService {
    pub async fn chat(&self, req: ChatRequest) -> Result<SseChatResponse, AgentError> {
        // 1. 获取或创建会话
        let session = self.session_repo.get_or_create(&req.session_key).await?;

        // 2. 构建 Memory 上下文
        let memory_context = self.memory_service
            .build_context(session.workspace_id(), session.agent_id())
            .await?;

        // 3. 追加用户消息到历史
        self.session_repo.append_message(&session, "user", &req.message).await?;

        // 4. 构建 System Prompt
        let system_prompt = self.config.prompt_builder.build(
            &session, &memory_context, req.system_prompt_override,
        )?;

        // 5. 获取历史消息
        let history = self.session_repo
            .get_history(&session, self.config.max_history_messages)
            .await?;

        // 6. 调用 AgentRuntime 执行
        let events = self.runtime
            .turn_streamed(&session, &req.message, &system_prompt, &history)
            .await?;

        // 7. 处理事件流
        let response_events = self.process_events(events, &session, &req.run_id).await?;

        Ok(SseChatResponse { events: response_events })
    }

    async fn process_events(
        &self,
        events: impl Stream<Item = TurnEvent>,
        session: &Session,
        run_id: &str,
    ) -> Result<Vec<ChatEvent>, AgentError> {
        let mut result = Vec::new();
        let mut assistant_content = String::new();

        pin!(events);

        while let Some(evt) = events.next().await {
            match evt {
                TurnEvent::Chunk { delta } => {
                    assistant_content.push_str(&delta);
                    result.push(ChatEvent::Delta {
                        message: serde_json::json!({
                            "role": "assistant",
                            "content": [{ "type": "text", "text": delta }]
                        }),
                    });
                }
                TurnEvent::Thinking { delta } => {
                    result.push(ChatEvent::Thinking { thinking: delta });
                }
                TurnEvent::ToolCall { name, args } => {
                    let a2ui_jsonl = if name == "canvas" {
                        args.get("jsonl").and_then(|v| v.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    };
                    result.push(ChatEvent::ToolCallStart {
                        tool_name: name,
                        tool_args: serde_json::to_string(&args).unwrap_or_default(),
                        a2ui: a2ui_jsonl,
                    });
                }
                TurnEvent::ToolResult { name, output } => {
                    result.push(ChatEvent::ToolResult {
                        tool_name: name,
                        result: output,
                    });
                }
            }
        }

        // 保存 assistant 消息
        if !assistant_content.is_empty() {
            self.session_repo.append_message(&session, "assistant", &assistant_content).await?;
            result.push(ChatEvent::Final {
                message: serde_json::json!({
                    "role": "assistant",
                    "content": [{ "type": "text", "text": assistant_content }]
                }),
            });
        }

        // 8. 检查是否需要压缩
        if let Some(compact_result) = self.compact_service
            .check_and_compact(&session, &result)
            .await?
        {
            self.session_repo.apply_compact(&session, compact_result).await?;
        }

        Ok(result)
    }
}
```

### 2.2 SessionService

```rust
// application/agent/session_service.rs

/// 会话仓库 trait（基础设施层实现）
pub trait SessionRepository: Send + Sync {
    async fn get_or_create(&self, session_key: &str) -> Result<Session, AgentError>;
    async fn append_message(&self, session: &Session, role: &str, content: &str) -> Result<(), AgentError>;
    async fn get_history(&self, session: &Session, limit: usize) -> Result<Vec<ChatMessage>, AgentError>;
    async fn apply_compact(&self, session: &Session, compacted: &CompactedSession) -> Result<(), AgentError>;
}

/// Session 实体
#[derive(Debug, Clone)]
pub struct Session {
    pub session_key: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub title: String,           // 自动生成：首条用户消息前 20 字
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
}

/// 聊天消息
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String;            // "user" | "assistant" | "system"
    pub content: String;         // JSON 字符串 [{ type: "text", text: "..." }]
    pub timestamp: i64;          // Unix ms
    pub run_id: Option<String>,
}

/// 压缩后的会话
pub struct CompactedSession {
    pub summary: String,
    pub preserved_user_messages: Vec<ChatMessage>,
    pub preserved_assistant_messages: Vec<ChatMessage>,
}
```

### 2.3 AgentMemoryService

```rust
// application/agent/memory_service.rs

/// AgentMemoryService - 整合 Auto / Session / Agent Memory
pub struct AgentMemoryService {
    device_memory_repo: Arc<dyn DeviceMemoryRepository>,
    skill_service: Arc<SkillService>,
}

/// Memory 上下文（注入 system prompt）
pub struct MemoryContext {
    /// Auto Memory - 设备状态快照片段
    pub device_snapshots: Vec<DeviceSnapshot>,
    /// Agent Memory - workspace 持久记忆
    pub agent_memories: Vec<AgentMemoryItem>,
    /// Session Memory - 当前会话摘要（后续实现）
    pub session_summary: Option<String>,
}

/// 设备状态快照
pub struct DeviceSnapshot {
    pub device_id: String,
    pub device_name: String,
    pub status: String,
    pub key_metrics: Vec<(String, String)>,
    pub snapshot_time: i64,
}

/// Agent 持久记忆项
pub struct AgentMemoryItem {
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub file_path: String,
    pub updated_at: i64,
}

impl AgentMemoryService {
    pub async fn build_context(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<MemoryContext, AgentError> {
        let device_snapshots = self.get_device_snapshots(workspace_id).await?;
        let agent_memories = self.get_agent_memories(workspace_id, agent_id).await?;

        Ok(MemoryContext {
            device_snapshots,
            agent_memories,
            session_summary: None,
        })
    }

    async fn get_device_snapshots(&self, workspace_id: &str) -> Result<Vec<DeviceSnapshot>, AgentError>;
    async fn get_agent_memories(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<AgentMemoryItem>, AgentError>;
}
```

---

## 3. Infrastructure 层

### 3.1 AgentRuntime

```rust
// infrastructure/agent/runtime.rs

/// AgentRuntime - zeroclaw 引擎封装
/// 命名去 zeroclaw 化，内部使用 zeroclaw 作为底层引擎
pub struct AgentRuntime {
    db_pool: sqlx::SqlitePool,
    provider: Arc<std::sync::Mutex<Option<Box<dyn Provider>>>>,
    model_name: String,
    agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
}

impl AgentRuntime {
    pub fn new(
        db_pool: sqlx::SqlitePool,
        provider: Box<dyn Provider>,
        model_name: String,
    ) -> anyhow::Result<Self>;

    /// 重新构建 Agent（工具注册完成后调用）
    pub async fn refresh_tools(&self) -> anyhow::Result<()>;

    /// 执行单轮对话（流式）
    pub async fn turn_streamed(
        &self,
        session: &Session,
        user_message: &str,
        system_prompt: &str,
        history: &[ChatMessage],
    ) -> Result<impl Stream<Item = TurnEvent>, AgentError>;
}
```

### 3.2 工具适配层

```rust
// infrastructure/agent/tools/mod.rs

/// CanvasTool - A2UI 渲染工具
pub struct CanvasTool;

impl Tool for CanvasTool {
    fn name(&self) -> &str { "canvas" }
    fn description(&self) -> String { "Render A2UI components for IoT visualization".into() }
    async fn call(&self, args: serde_json::Value, _: &dyn ToolCallContext) -> Result<ToolResult, ToolError>;
}

/// IoTToolAdapter - 将 MCP ToolHandler 适配为 zeroclaw Tool
pub struct IoTToolAdapter {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    handler: Box<dyn ToolHandler>,
}

impl Tool for IoTToolAdapter {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> String { self.description.clone() }
    fn input_schema(&self) -> serde_json::Value { self.input_schema.clone() }
    async fn call(&self, args: serde_json::Value, _: &dyn ToolCallContext) -> Result<ToolResult, ToolError> {
        let result = self.handler.execute(args).await?;
        Ok(ToolResult::success(result))
    }
}
```

### 3.3 Prompt 构建器

```rust
// infrastructure/agent/prompt.rs

pub struct PromptBuilder {
    skills_base_path: PathBuf,
}

impl PromptBuilder {
    pub fn new(skills_base_path: PathBuf) -> Self;

    pub fn build(
        &self,
        session: &Session,
        memory_context: &MemoryContext,
        override_prompt: Option<String>,
    ) -> Result<String, AgentError>;

    fn build_device_memory_section(&self, snapshots: &[DeviceSnapshot]) -> String;
    fn build_agent_memory_section(&self, memories: &[AgentMemoryItem]) -> String;
    fn load_skills_prompt(&self, workspace_id: &str, agent_id: &str) -> Result<String, AgentError>;
}

/// 平台基础 prompt（约 250 行中文）
pub fn platform_base_prompt() -> String;

/// 完整 system prompt 组装
pub fn build_full_system_prompt(
    user_persona: Option<&str>,
    workspace_id: Option<&str>,
) -> String;
```

### 3.4 配置与错误类型

```rust
// infrastructure/agent/config.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent API request failed: {0}")]
    RequestFailed(String),
    #[error("Agent API returned error: {0}")]
    ApiError(String),
    #[error("Agent API timeout")]
    Timeout,
    #[error("Agent unavailable: {0}")]
    Unavailable(String),
    #[error("agent not found: {0}")]
    NotFound(String),
}
```

---

## 4. Domain 层

### 4.1 现有文件（保留）

```
domain/agent/
├── skill.rs              ← AgentSkill 实体 + frontmatter 解析（不变）
├── device_memory.rs       ← DeviceMemory 实体（不变）
├── compact_service.rs     ← CompactService 对话压缩（不变）
└── memory_repository.rs  ← 新增，DeviceMemoryRepository trait
```

### 4.2 新增 Repository 接口

```rust
// domain/agent/memory_repository.rs

use async_trait::async_trait;

#[async_trait]
pub trait DeviceMemoryRepository: Send + Sync {
    async fn save(&self, memory: &DeviceMemory) -> Result<(), String>;
    async fn get_latest(&self, workspace_id: &str, agent_id: &str, device_id: &str) -> Result<Option<DeviceMemory>, String>;
    async fn get_all_for_agent(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<DeviceMemory>, String>;
    async fn delete_old(&self, workspace_id: &str, agent_id: &str, device_id: &str, keep_count: i64) -> Result<u64, String>;
}
```

---

## 5. API 层

### 5.1 模块结构

```
api/chat/
├── mod.rs
├── chat_handler.rs      ← SSE 聊天入口
├── skills_handler.rs   ← Skills CRUD
└── agents_handler.rs   ← Agent 配置管理
```

### 5.2 ChatHandler

```rust
// api/chat/chat_handler.rs

/// POST /api/v1/chat
/// body: { session_key, message, run_id, system_prompt? }
/// response: text/event-stream
pub async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let chat_service = ChatService::new(
        state.agent_runtime.clone(),
        state.session_repo.clone(),
        state.agent_memory_service.clone(),
        state.compact_service.clone(),
        state.agent_config.clone(),
    );

    let events = match chat_service.chat(req).await {
        Ok(response) => response.events,
        Err(e) => vec![ChatEvent::Error { error: e.to_string() }],
    };

    // SSE response
    Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .body(events)
}
```

### 5.3 SkillsHandler

```rust
// api/chat/skills_handler.rs

/// GET /api/v1/chat/skills/:workspace_id
pub async fn list_skills(Path(workspace_id): Path<String>) -> impl IntoResponse;

/// POST /api/v1/chat/skills/:workspace_id
pub async fn create_skill(
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateSkillRequest>,
) -> impl IntoResponse;

/// GET /api/v1/chat/skills/:workspace_id/:name
pub async fn get_skill(Path((workspace_id, name)): Path<(String, String)>) -> impl IntoResponse;

/// PUT /api/v1/chat/skills/:workspace_id/:name
pub async fn update_skill(
    Path((workspace_id, name)): Path<(String, String)>,
    Json(req): Json<UpdateSkillRequest>,
) -> impl IntoResponse;

/// DELETE /api/v1/chat/skills/:workspace_id/:name
pub async fn delete_skill(Path((workspace_id, name)): Path<(String, String)>) -> impl IntoResponse;
```

### 5.4 AgentsHandler

```rust
// api/chat/agents_handler.rs

/// GET /api/v1/agents
pub async fn list_agents() -> impl IntoResponse;

/// GET /api/v1/agents/:id/config
pub async fn get_agent_config(Path(agent_id): Path<String>) -> impl IntoResponse;

/// PUT /api/v1/agents/:id/config
pub async fn update_agent_config(
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> impl IntoResponse;

/// GET /api/v1/agents/:id/tools/catalog
pub async fn get_tools_catalog() -> impl IntoResponse;

/// PATCH /api/v1/agents/:id/tools/:tool_name
pub async fn toggle_tool(
    Path((agent_id, tool_name)): Path<(String, String)>,
    Json(req): Json<ToggleToolRequest>,
) -> impl IntoResponse;
```

---

## 6. AppState 依赖注入

```rust
// shared/app_state.rs

#[derive(Clone)]
pub struct AppState {
    // ... 现有字段保留 ...

    // === Agent 相关（新增/重构）===
    pub agent_runtime: Arc<AgentRuntime>,
    pub session_repo: Arc<dyn SessionRepository>,
    pub agent_memory_service: Arc<AgentMemoryService>,
    pub compact_service: Arc<CompactService>,
    pub prompt_builder: Arc<PromptBuilder>,
    pub agent_config: AgentServiceConfig,
}

pub struct AgentServiceConfig {
    pub max_history_messages: usize,
    pub skills_base_path: PathBuf,
}
```

---

## 7. 文件变更清单

### 7.1 新增文件

| 文件 | 说明 |
|------|------|
| `application/agent/mod.rs` | 应用层 module |
| `application/agent/chat_service.rs` | Query 主循环 |
| `application/agent/session_service.rs` | 会话管理 + SessionRepository trait |
| `application/agent/memory_service.rs` | 多层 Memory 整合 |
| `domain/agent/memory_repository.rs` | DeviceMemoryRepository trait |
| `infrastructure/agent/mod.rs` | Infrastructure agent module |
| `infrastructure/agent/runtime.rs` | AgentRuntime |
| `infrastructure/agent/config.rs` | AgentConfig / AgentInfo / AgentError |
| `infrastructure/agent/prompt.rs` | PromptBuilder |
| `infrastructure/agent/tools/mod.rs` | CanvasTool + IoTToolAdapter |
| `api/chat/mod.rs` | Chat API module |
| `api/chat/chat_handler.rs` | SSE handler |
| `api/chat/skills_handler.rs` | Skills CRUD |
| `api/chat/agents_handler.rs` | Agent 配置 |

### 7.2 删除文件

| 文件 | 说明 |
|------|------|
| `infrastructure/zeroclaw_agent.rs` | 外部网关适配器（2655行） |
| `infrastructure/zeroclaw_runtime.rs` | 旧 runtime（725行） |

### 7.3 修改文件

| 文件 | 变更 |
|------|------|
| `infrastructure/mod.rs` | `pub mod zeroclaw_agent` → `pub mod agent` |
| `shared/app_state.rs` | 新增 agent 相关字段 |
| `domain/agent/mod.rs` | 新增 memory_repository 模块 |
| `domain/workspace/service.rs` | `zeroclaw_agent::AgentClient` → `infrastructure::agent` |

---

## 8. 实施路径（分阶段）

### Phase 1: 安全修复 + 代码清理（优先）

**目标**: 解决 CRITICAL 安全问题，删除死代码

**前置审计（Phase 1 开始前必须完成）**:
- 审计 `batch_command` — 确认 IDOR 漏洞存在（`input.workspace_id` 未与 `claims.workspace_id` 校验）
- 逐个检查以下 8 类工具的 workspace 隔离状态，分类为"已修复"或"待修复"
  - `batch.rs`, `device.rs`, `job.rs`, `alarm_mcp.rs`, `schedule_mcp.rs`
  - 其他 tool（见 `2026-04-08-agent-mcp-integration-design.md`）

**工作内容**:

1. **P0 - `batch_command` workspace IDOR 修复**（CRITICAL）
   - `batch.rs:128` + `batch.rs:149`: 添加 `input.workspace_id != claims.workspace_id` 校验
   - 任何跨 workspace 操作必须返回 `ToolError::Forbidden`

2. **删除外部网关代码**
   - 删除 `zeroclaw_agent.rs` 中的 `ZeroClawAgentClient`
   - 删除 `zeroclaw_agent.rs` 中的 `FallbackAgentClient`
   - 删除 `zeroclaw_agent.rs` 中的 `ApiResponse`、`ZeroClawIncoming` 结构体
   - 删除 WebSocket / HTTP 客户端相关代码
   - **注意**: 保留 `#[cfg(test)]` 块中被删除代码对应的测试，后续替换

3. **保留的内容**（迁入 `zeroclaw_runtime.rs` 或新建 `agent.rs`）
   - `AgentConfig` / `AgentInfo` / `AgentError`
   - `build_tools_catalog_json()`
   - `platform_base_prompt()` / `build_full_system_prompt()`
   - `default_agent_config()` / `compute_hash()`
   - `TinyIoTHubAgentClient` 重命名为 `AgentRuntime`（定义明确 API contract，见下）

4. **MCP workspace 隔离修复**（按审计结果分类实施）
   - 对"已修复"工具：验证检查逻辑仍然正确
   - 对"待修复"工具：逐个添加 `workspace_id` 校验
   - `create_device` — 自动绑定 `claims.workspace_id`，不接受 caller 输入

5. **去掉 AgentClient trait**
   - 直接使用 `AgentRuntime` struct
   - 迁移 `workspace.rs` 中所有 `Arc<dyn AgentClient>` 引用为 `Arc<AgentRuntime>`

6. **workspace.rs 清理**
   - 删除 `FallbackAgentClient` 的两处调用

**AgentRuntime API Contract（Phase 2 依赖此合同）**:
```rust
// Phase 1 必须定义并文档化以下 API
impl AgentRuntime {
    /// 执行单轮对话（流式）
    pub async fn turn_streamed(
        &self,
        session: &Session,
        user_message: &str,
        system_prompt: &str,
        history: &[ChatMessage],
    ) -> Result<impl Stream<Item = TurnEvent>, AgentError>;

    /// 重新构建 Agent（工具注册完成后调用）
    pub async fn refresh_tools(&self) -> anyhow::Result<()>;

    /// 获取 Agent 配置
    pub fn config(&self) -> &AgentConfig;
}
```

**Phase 1 新增的显式迁移步骤（来自工程评审）**:

1. **更新 `app_state.rs`**
   - 将 `agent_client: Arc<dyn AgentClient>` 和 `tinyiothub_agent: Arc<TinyIoTHubAgentClient>`
   - 合并为单个字段 `agent_runtime: Arc<AgentRuntime>`
   - 更新 `AppState::new()` 中的初始化逻辑

2. **更新 `workspace.rs`**
   - `create_workspace_handler` (line ~246): 从 `FallbackAgentClient::new()` 改为 `AgentRuntime::new()`
   - `delete_workspace_handler` (line ~458): 从 `FallbackAgentClient::new()` 改为 `AgentRuntime::new()`
   - 注意: `AgentRuntime` 的 `create_agent`/`delete_agent` 返回 stub 值 "default" — 确认这是单 Agent 设计的预期行为

**验收标准**:
- 删除 `ZeroClawAgentClient` + `FallbackAgentClient` + `AgentClient` trait
- `cargo check` 通过
- 所有"待修复"工具通过 workspace 隔离安全测试
  - `batch_command` IDOR 修复（验证：跨 workspace 调用返回 `ToolError::Forbidden`）
  - `alarm_statistics` 添加 `workspace_id` 过滤（验证：仅返回当前 workspace 的统计数据）
  - `alarm_rule_add` 添加 `workspace_id` 绑定（验证：规则创建后仅可在当前 workspace 查询）
- Phase 1 结束后跑通基本 chat SSE 流程

**风险**: Medium — 删除代码量大（含 ~1350 行测试），但有 `cargo check` + 端到端测试保底

**风险**: Medium — 删除代码量大（含 ~670 行测试），但有 `cargo check` + 端到端测试保底

---

### Phase 2: DDD 分层重构 + Memory 增强

**目标**: 建立清晰的架构层次 + 增强 Memory 系统

**工作内容**:

1. 创建 `application/agent/` 目录
2. 创建 `infrastructure/agent/` 目录
3. 将代码按 DDD 分层重新组织
4. 更新 `app_state.rs`
5. 更新所有引用点

**新增（Exp 1+2+3）**:
- **Session Memory**: 在 `CompactService` 中实现会话摘要生成，会话消息数 > 20 时触发
- **Relevant Memory Recall**: 实现轻量选择器，不是把所有记忆都塞进 prompt，而是根据上下文选择最相关的 5 条
- **Skills 内嵌 Shell**: 支持 `!`command`` 语法在 Skill markdown 中执行 Shell 命令并替换结果（需先补充具体语法规范）

**验收标准**:
- 所有新增文件符合 DDD 分层
- `cargo check` 通过
- 现有 API 接口兼容
- Memory 增强通过功能测试

**风险**: Medium — 文件移动 + 新功能

---

## 10. 约束

- 不改变现有 API 接口（兼容前端调用）
- 不改变数据库表结构（复用现有 `chat_sessions`、`chat_messages` 表）
- zeroclaw 作为底层引擎保持不变，仅上层命名去 zeroclaw 化
- SessionRepository 使用 SQLite 实现（复用现有表结构）
