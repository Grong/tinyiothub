# zeroclaw → rig 迁移实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 agent 底层引擎从应用型 zeroclaw（git fork）替换为库优先的 rig 0.42，通过自有的 port trait 反腐败层保证两阶段切换期间行为不变、现有 ScriptedProvider 测试全程绿。

**Architecture:** 在 `crates/agent` 内新建 `port/` 模块定义自有窄接口（Tool / ModelProvider / AgentLoop / Memory / Observer / PromptSection / TurnEvent），全部形状 vendored 自 zeroclaw-api 的对应定义（它们是合理的库级设计，且 17 个工具实现和 4 份 ScriptedProvider 已经长在这个形状上）。Phase 1 用 zeroclaw 适配器实现 port（zeroclaw 退化为黑盒实现细节），Phase 2 用 rig 适配器实现同一组 port 并删除 zeroclaw。ScriptedProvider 测试只面向 port，两个 phase 都是同一套测试当验收门。

**Tech Stack:** Rust 1.95.0、tokio、rig-core 0.42.0 + rig-agent 0.42.0（crates.io 精确锁定）、zeroclaw v0.8.1-patched（phase 1 期间保留）。

## Global Constraints

- **rig 版本锁定 `=0.42.0`**（crates.io）。rig main（d9ed455，#2441 破坏性重构 client 机制）已超前于 0.42 发布，禁止跟 git main；升级必须走正式 release 评估。
- **行为不变式（两 phase 共同验收门）**：`cargo test -p agent` 与 `cargo test -p cloud` 全绿，尤其 `apps/cloud/src/tests/thing_agent_loop_tests.rs`（LoopScriptedProvider 内容感知脚本）与 `agent_loop_e2e_tests.rs`。任何任务不得让该门变红过夜。
- **zeroclaw 源码锚点**：`~/.cargo/git/checkouts/zeroclaw-eaddfbb48bf73718/12f5360/`（v0.8.1-patched，fork 唯一提交 `12f53602`）。vendored 文件从这里复制。
- **crates/agent 设计不变量**（`crates/agent/src/lib.rs` 头注释，CI G9 守卫 + cargo tree 检查）：零 Web 框架依赖、零 SQL/存储实现依赖、不感知 apps/cloud 领域划分。rig 的 reqwest 是 HTTP client 不是 Web 框架，允许；但必须在 Task 1 核对 CI G9 守卫词表不含 `reqwest`/`rig` 意外命中。
- **已裁定的行为裁量**（调研结论，实施中不要再翻转）：
  1. `ResponseCache` **不移植**——zeroclaw 的缓存仅在 `temperature == Some(0.0)` 时生效（zeroclaw-runtime/src/agent/agent.rs:778），两处 builder 均不设 temperature（默认 `None`），缓存是死代码。
  2. `ConversationMessage` 返回元组**不移植**——两个消费点（runner.rs:156 `Ok(Ok((text, _)))`、chat.rs `_conversation`）都丢弃它。port 的 `turn_streamed` 只返回 `Result<String>`。
  3. `AutonomyLevel::Supervised` 不暴露进 port——现状两处恒为 Supervised，且 runner 忽略 `ApprovalRequest` 事件；工具门控由我们自研的 `TrustAwareTool` 承担。zeroclaw 适配器内部硬编码 Supervised 保持现状。
  4. tool call / tool result 在 messages 里的 **canonical 编码**（ScriptedProvider 可见的唯一编码，来自 zeroclaw-runtime/src/agent/dispatcher.rs:229-260 `to_provider_messages`）：
     - assistant 工具调用 → `ChatMessage{role:"assistant", content: '{"content": <text|null>, "tool_calls": [<ToolCall json>], ...}'}`（有 reasoning_content 时多一个 `"reasoning_content"` 键）
     - 工具结果 → 每个调用一条 `ChatMessage{role:"tool", content: '{"tool_call_id": <id>, "content": <output>}'}`（output 经 `canonicalize_tool_result_media_markers`）
  5. rig 侧已知语义陷阱：**`max_turns` 默认 1**（不做工具 followup），builder 必须显式 `.default_max_turns(25)`（对齐 runner 的 `MAX_TOOL_CALLS_PER_RUN=25`）；OpenAI provider 0.42 默认走 Responses API，但 minimax provider 走 OpenAI 兼容 Chat Completions，无需干预。
  6. **可接受行为差异**（写入最终 PR 描述）：跨 turn 记忆注入方面，zeroclaw 用模糊文本 context 注入（memory_strategy.load_context），rig 适配器改为结构化 conversation recall（`ConversationMemory::load` → 20 条 `Message::User` 文本），ScriptedProvider 测试不依赖此行为。
- workspace 依赖现状：`thiserror = "1.0"`、`reqwest = "0.12"`（根 Cargo.toml:62,115）。rig 用 thiserror 2 / reqwest 0.13，**双主版本共存，不升级 workspace**。
- crates/agent 当前**不直接依赖 futures**；Task 2 起新增 `futures = { workspace = true }`（根 workspace 已有 futures 0.3）。
- 每个 Task 完成后 commit；commit message 用项目惯例（`feat(agent): ...` / `refactor(agent): ...` 等，英文小写前缀 + 中文描述均可，参照 git log）。

---

### Task 1: rig 依赖落地 + 工具链/守卫 spike

**Files:**
- Modify: `crates/agent/Cargo.toml`（新增 rig 依赖 + futures）
- Modify: `Cargo.toml`（workspace.dependencies 增加 rig-core / rig-agent 精确版本，可选——若决定只在 crates/agent 直接声明则跳过）
- Test: 无新测试；验证 = `cargo check -p agent` + 读 CI 配置

**Interfaces:**
- Produces: 编译通过的 `rig-core = "=0.42.0"`、`rig-agent = "=0.42.0"` 依赖（后续 Task 全部以此为准）。

- [ ] **Step 1: 声明依赖**

`crates/agent/Cargo.toml` 的 `[dependencies]` 增加：

```toml
futures = { workspace = true }
rig-core = "=0.42.0"
rig-agent = "=0.42.0"
```

（zeroclaw 两行暂时保留；Task 9 才删除。）

- [ ] **Step 2: 核对 CI G9 守卫词表**

Run: `grep -n -i "G9\|purity\|守卫" .github/workflows/ci.yml | head -10`，找到 Agent Loop Purity Guard 步骤的禁止词表，确认不含 `reqwest`、`rig`、`hyper`（含则在该步骤的豁免注释中按既有格式登记，附理由"rig 为 LLM provider 客户端，非 Web 框架"）。
Expected: 词表确认/登记完成。

- [ ] **Step 3: 编译验证 + spike 三个 API 未知点**

Run: `cargo check -p agent 2>&1 | tail -5`
同时在本任务中写一段临时 spike 代码（放 `crates/agent/src/bin/` 不行——crate 无 bin；改为 `#[cfg(test)]` 临时测试，验证后即删）确认：
1. `MultiTurnStreamItem::CompletionCall(c)` 中 usage 的字段路径（`c.usage.input_tokens` 还是别的名字）——打印 `format!("{:?}", item)` 从 `StreamAssistantItem(..)`/`CompletionCall(..)` 的 Debug 输出确认；
2. `rig::completion::PromptResponse` 取最终文本的方法（预期 `.output()` → String）；
3. `ToolCallId` 的 Display（已确认存在，message.rs:428，`.to_string()` 可用）。

Expected: `cargo check` 通过；三个 API 点以实际编译/Debug 输出为准记录到本计划文末"Spike 记录"小节（实施者回填）。

- [ ] **Step 4: Commit**

```bash
git add crates/agent/Cargo.toml Cargo.toml Cargo.lock .github/workflows/ci.yml
git commit -m "chore(agent): 引入 rig-core/rig-agent 0.42.0 依赖（zeroclaw 替换方案 B）"
```

---

### Task 2: port 模块骨架 — attribution / Tool / provider 类型 / TurnEvent / 取消语义

**Files:**
- Create: `crates/agent/src/port/mod.rs`
- Create: `crates/agent/src/port/attribution.rs`
- Create: `crates/agent/src/port/tool.rs`
- Create: `crates/agent/src/port/provider.rs`
- Create: `crates/agent/src/port/events.rs`
- Create: `crates/agent/src/port/outcome.rs`
- Modify: `crates/agent/src/lib.rs`（加 `pub mod port;`）

**Interfaces:**
- Produces（后续 Task 依赖的精确名字）：
  - `port::attribution::{Attributable, Role, ToolKind, ProviderKind, ModelProviderKind, MemoryKind, ...}` + `port::attribution::tool_attribution!` / `mock_tool_attribution!` 宏
  - `port::tool::{Tool, ToolResult, ToolSpec}` — `Tool` 为 `#[async_trait]` trait，超 trait `port::attribution::Attributable`
  - `port::provider::{ModelProvider, ChatRequest<'a>, ChatResponse, ChatMessage, ToolCall, TokenUsage}`
  - `port::events::TurnEvent`（6 variant 与 zeroclaw 完全一致）
  - `port::outcome::{ToolLoopCancelled, is_tool_loop_cancelled}`

- [ ] **Step 1: 写编译测试（失败）**

`crates/agent/src/port/mod.rs`：

```rust
//! 自有 agent 引擎接口面（反腐败层）。形状 vendored 自 zeroclaw-api ——
//! 它们是合理的库级设计，且既有工具实现/ScriptedProvider 已长在该形状上。
pub mod attribution;
pub mod events;
pub mod outcome;
pub mod provider;
pub mod tool;

#[cfg(test)]
mod tests {
    use super::tool::{Tool, ToolResult};
    use super::attribution::{Attributable, Role, ToolKind};

    struct Dummy;
    impl Attributable for Dummy {
        fn role(&self) -> Role { Role::Tool(ToolKind::Plugin) }
        fn alias(&self) -> &str { "dummy" }
    }
    #[async_trait::async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "d" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult { success: true, output: "ok".into(), error: None })
        }
    }

    #[test]
    fn tool_spec_default_method() {
        let t = Dummy;
        let spec = t.spec();
        assert_eq!(spec.name, "dummy");
        assert_eq!(spec.description, "d");
    }
}
```

Run: `cargo test -p agent port:: 2>&1 | tail -3`
Expected: FAIL（子模块不存在 / unresolved imports）。

- [ ] **Step 2: vendor attribution**

复制 `~/.cargo/git/checkouts/zeroclaw-eaddfbb48bf73718/12f5360/crates/zeroclaw-api/src/attribution.rs` 全文 → `crates/agent/src/port/attribution.rs`。该文件自包含（strum/serde derive 若用了 strum，把对应 derive 改为手写 `match` 实现或把 strum 加为 crates/agent 依赖——复制时 `cargo check` 会立刻暴露，二选一处理，优先去 strum 依赖）。

- [ ] **Step 3: vendor tool.rs**

`crates/agent/src/port/tool.rs`（与 zeroclaw-api/src/tool.rs 的 Tool/ToolResult/ToolSpec/mock_tool_attribution! 一致，去掉 zeroclaw 专有部分）：

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Description of a tool for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core tool trait — implement for any capability.
#[async_trait::async_trait]
pub trait Tool: Send + Sync + crate::port::attribution::Attributable {
    /// Tool name (used in LLM function calling)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON schema for parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;

    /// Get the full spec for LLM registration
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
```

（`zeroclaw-api` 的 `tool_attribution!`/`mock_tool_attribution!` 宏也在本文件尾部 vendor，把宏体里的 `$crate::attribution` 路径改为 `crate::port::attribution`。）

- [ ] **Step 4: vendor provider.rs**

`crates/agent/src/port/provider.rs`（形状逐字段来自 zeroclaw-api/src/model_provider.rs:22-162）：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
    pub fn tool(content: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: content.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // JSON string
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

/// An LLM response that may contain text, tool calls, or both.
#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: Option<&'a [crate::port::tool::ToolSpec]>,
}

/// 极简 provider 抽象：agent loop 只走 `chat`。
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync + crate::port::attribution::Attributable {
    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse>;
}
```

说明：zeroclaw 的 `ModelProvider` 有 `chat_with_system`/`chat_with_history`/`capabilities`/`convert_tools` 等一堆成员；我们调用面只用到 `chat`（4 份 ScriptedProvider 也只实现 `chat`），按 Simplicity First 只 port `chat`。zeroclaw 适配器（Task 5）实现 zeroclaw trait 时只用 `chat` 委托。

- [ ] **Step 5: vendor events.rs + outcome.rs**

`crates/agent/src/port/events.rs` — 逐字复制 zeroclaw-api/src/agent.rs 的 `TurnEvent` 枚举（6 个 variant：Chunk/Thinking/ToolCall/ToolResult/ApprovalRequest/Usage，字段类型不变；文档注释可精简）。加 `#[derive(Debug, Clone)]`。

`crates/agent/src/port/outcome.rs`：

```rust
//! Turn-loop 取消语义（vendored 自 zeroclaw-runtime/src/agent/turn/outcome.rs:11-21）

#[derive(Debug)]
pub struct ToolLoopCancelled;

impl std::fmt::Display for ToolLoopCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tool loop cancelled")
    }
}
impl std::error::Error for ToolLoopCancelled {}

pub fn is_tool_loop_cancelled(err: &anyhow::Error) -> bool {
    err.chain().any(|source| source.is::<ToolLoopCancelled>())
}
```

- [ ] **Step 6: 挂模块 + 跑测试**

`crates/agent/src/lib.rs` 的 `pub mod config;` 前加 `pub mod port;`。

Run: `cargo test -p agent port:: 2>&1 | tail -3`
Expected: PASS（tool_spec_default_method）。

- [ ] **Step 7: Commit**

```bash
git add crates/agent/src/port crates/agent/src/lib.rs crates/agent/Cargo.toml
git commit -m "feat(agent): port 模块骨架 — attribution/Tool/provider 类型/TurnEvent/取消语义 vendored"
```

---

### Task 3: port prompt — PromptSection / PromptContext / SystemPromptBuilder + 9 个默认 section 的保真对照测试

**Files:**
- Create: `crates/agent/src/port/prompt.rs`
- Create: `crates/agent/src/port/prompt_sections.rs`
- Modify: `crates/agent/src/port/mod.rs`（加 `pub mod prompt; pub mod prompt_sections;`）

**Interfaces:**
- Produces:
  - `port::prompt::{PromptSection, PromptContext<'a>, SystemPromptBuilder}` — `PromptSection::name(&self) -> &str`、`PromptSection::build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String>`、`SystemPromptBuilder::with_defaults() -> Self`、`add_section(Box<dyn PromptSection>) -> Self`、`build(&self, ctx) -> anyhow::Result<String>`（连接规则与 zeroclaw 一致：逐 section `trim_end` 后加 `"\n\n"`，空 section 跳过——zeroclaw-runtime/src/agent/prompt.rs:73-84）
  - `port::prompt::PromptContext<'a>` 字段：`workspace_dir: &'a Path`、`agent_workspace_dir: &'a Path`、`model_name: &'a str`、`tool_specs: Vec<crate::port::tool::ToolSpec>`、`security_summary: Option<String>`（9 个 vendored section 与 `TinyIoTHubSkillsSection` 实际用到的全集； zeroclaw 原字段更多，用不到的不 port）
  - `port::prompt_sections::{DateTimeSection, IdentitySection, ToolHonestySection, ToolsSection, SafetySection, SkillsSection, WorkspaceSection, RuntimeSection, ChannelMediaSection}`（unit struct，实现 `PromptSection`）

- [ ] **Step 1: 写保真对照测试（失败）**

`crates/agent/src/port/prompt_sections.rs` 的 `#[cfg(test)] mod tests`：

```rust
#[test]
fn vendored_defaults_match_zeroclaw_black_box() {
    // 用 zeroclaw 黑盒（SystemPromptBuilder::with_defaults）与我们的 vendored
    // sections 在同一 PromptContext 下分别渲染，必须逐字符相等。
    // 该测试在 phase 1（zeroclaw 仍在依赖树里）成立，phase 2 删除 zeroclaw 后
    // 变为纯回归锁（期望值已在 phase 1 固化）。
    let ctx_fields = /* 构造 zeroclaw PromptContext 所需最小输入 */;
    let zeroclaw_rendered = zeroclaw_render(&ctx_fields);   // helper，见 Step 3
    let ours = {
        let ctx = crate::port::prompt::PromptContext {
            workspace_dir: &ctx_fields.workspace_dir,
            agent_workspace_dir: &ctx_fields.workspace_dir,
            model_name: "MiniMax-M2",
            tool_specs: vec![/* 一个样例 ToolSpec */],
            security_summary: Some("test security summary".into()),
        };
        crate::port::prompt::SystemPromptBuilder::with_defaults()
            .build(&ctx)
            .unwrap()
    };
    assert_eq!(ours, zeroclaw_rendered);
}
```

Run: `cargo test -p agent port::prompt_sections 2>&1 | tail -3`
Expected: FAIL（模块不存在）。

- [ ] **Step 2: 实现 port::prompt**

`crates/agent/src/port/prompt.rs`：

```rust
use std::path::Path;

pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String>;
}

pub struct PromptContext<'a> {
    pub workspace_dir: &'a Path,
    pub agent_workspace_dir: &'a Path,
    pub model_name: &'a str,
    pub tool_specs: Vec<crate::port::tool::ToolSpec>,
    pub security_summary: Option<String>,
}

pub struct SystemPromptBuilder {
    sections: Vec<Box<dyn PromptSection>>,
}

impl SystemPromptBuilder {
    pub fn with_defaults() -> Self {
        Self {
            sections: vec![
                Box::new(crate::port::prompt_sections::DateTimeSection),
                Box::new(crate::port::prompt_sections::IdentitySection),
                Box::new(crate::port::prompt_sections::ToolHonestySection),
                Box::new(crate::port::prompt_sections::ToolsSection),
                Box::new(crate::port::prompt_sections::SafetySection),
                Box::new(crate::port::prompt_sections::SkillsSection),
                Box::new(crate::port::prompt_sections::WorkspaceSection),
                Box::new(crate::port::prompt_sections::RuntimeSection),
                Box::new(crate::port::prompt_sections::ChannelMediaSection),
            ],
        }
    }

    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    pub fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String> {
        let mut output = String::new();
        for section in &self.sections {
            let part = section.build(ctx)?;
            if part.trim().is_empty() {
                continue;
            }
            output.push_str(part.trim_end());
            output.push_str("\n\n");
        }
        Ok(output)
    }
}
```

- [ ] **Step 3: vendor 9 个 section 文本**

从 `~/.cargo/git/checkouts/zeroclaw-eaddfbb48bf73718/12f5360/crates/zeroclaw-runtime/src/agent/prompt.rs`（unit struct 声明在 87-95 行，各 `impl PromptSection` 的 `build()` 在同一文件）逐段复制 9 个 section 的 `build()` 输出文本到 `port_sections.rs`，替换规则：
- `ctx.tools`（`&[Box<dyn Tool>]`）→ `ctx.tool_specs`（`Vec<ToolSpec>`），工具清单渲染用 `name — description` 行格式保持与 zeroclaw `ToolsSection` 一致；
- 引到 `zeroclaw_config::schema::SkillsPromptInjectionMode`、`IdentityConfig`、`Skill` 的 section（Skills/Identity），按其实际 build 分支：若输入为 None/空时输出固定兜底文本（大概率如此），vendor 该兜底文本并跳过该配置输入；若确实依赖配置内容，在 `PromptContext` 加 `Option<String>` 字段承载，不引 zeroclaw 类型；
- `sends_native_tool_specs` 恒为 true（NativeToolDispatcher::should_send_tool_specs），vendor 该分支的文本。

测试 helper `zeroclaw_render`：构造 zeroclaw 的 `PromptContext`（字段见 zeroclaw-runtime/src/agent/prompt.rs:12-40，未用到的给默认值/None），调 `zeroclaw::agent::prompt::SystemPromptBuilder::with_defaults().build(&zc_ctx)`。

- [ ] **Step 4: 跑对照测试至绿**

Run: `cargo test -p agent port::prompt_sections 2>&1 | tail -5`
Expected: PASS。若不等：diff 两输出，修正 vendored 文本（差异只能来自 vendor 时的文本改动，不允许通过改测试放行）。

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/port
git commit -m "feat(agent): port prompt — 9 默认 section vendored + zeroclaw 黑盒保真对照测试"
```

---

### Task 4: port 运行时接口 — AgentLoop / Memory / Observer

**Files:**
- Create: `crates/agent/src/port/runtime.rs`
- Create: `crates/agent/src/port/memory.rs`
- Create: `crates/agent/src/port/observer.rs`
- Modify: `crates/agent/src/port/mod.rs`

**Interfaces:**
- Produces:
  - `port::runtime::{AgentLoop, AgentLoopConfig, AgentLoopHandle, AgentLoopFactory, MAX_LOOP_TURNS}`
  - `port::runtime::AgentLoopConfig` 字段：`model_name: String`、`prompt_builder: crate::port::prompt::SystemPromptBuilder`、`tools: Vec<Box<dyn crate::port::tool::Tool>>`、`memory: Arc<dyn crate::port::memory::Memory>`、`observer: Arc<dyn crate::port::observer::Observer>`、`workspace_dir: PathBuf`、`security_summary: Option<String>`
  - `port::memory::{Memory, MemoryEntry, MemoryCategory}`（形状 vendored 自 zeroclaw-api/src/memory_traits.rs:149 —— 只 port 方法名清单中 WorkspaceScopedMemory 实际重写的那组：`name/store/recall/get/get_for_agent/list/forget/forget_for_agent/recall_for_agents/store_with_agent/store_with_metadata/recall_namespaced/count/health_check`，其余 zeroclaw 默认方法不 port）
  - `port::observer::{Observer, ObserverEvent, ObserverMetric, NoopObserver}`（vendored 自 zeroclaw-api/src/observability_traits.rs:240 附近：`record_event/record_metric/flush/name`；`ObserverEvent`/`ObserverMetric` 若 dragging 大量 zeroclaw 类型，则 port 为最小空枚举 + 文档注释说明"phase 2 rig 适配器经 hooks 重新产生观测事件"）

- [ ] **Step 1: 写失败测试**

`port/runtime.rs` 内 `#[cfg(test)]`：构造 `AgentLoopConfig`（tools 用 Task 2 的 Dummy、memory 用下文 Step 2 的 `NoopMemory`、observer 用 `NoopObserver`），断言 `MAX_LOOP_TURNS == 25`。

- [ ] **Step 2: 实现 runtime.rs**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 与 runner::MAX_TOOL_CALLS_PER_RUN 对齐的模型调用预算（含初始调用）。
pub const MAX_LOOP_TURNS: usize = 25;

pub struct AgentLoopConfig {
    pub model_name: String,
    pub prompt_builder: crate::port::prompt::SystemPromptBuilder,
    pub tools: Vec<Box<dyn crate::port::tool::Tool>>,
    pub memory: Arc<dyn crate::port::memory::Memory>,
    pub observer: Arc<dyn crate::port::observer::Observer>,
    pub workspace_dir: PathBuf,
    pub security_summary: Option<String>,
}

/// zeroclaw Agent::turn_streamed 的 port 形状（返回去掉 ConversationMessage，
/// 两个消费点均丢弃它 —— Global Constraints #2）。
#[async_trait::async_trait]
pub trait AgentLoop: Send + Sync {
    async fn turn_streamed(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<crate::port::events::TurnEvent>,
        cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String>;

    /// zeroclaw run_single = turn()（zeroclaw-runtime/src/agent/agent.rs:2520），
    /// 即完整工具循环，对应 rig 的 `.prompt()`。
    async fn run_single(&self, message: &str) -> anyhow::Result<String>;
}

pub type AgentLoopHandle = Arc<tokio::sync::Mutex<dyn AgentLoop>>;
pub type AgentLoopFactory =
    Arc<dyn Fn(AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> + Send + Sync>;
```

- [ ] **Step 3: vendor memory.rs / observer.rs**

`port/memory.rs`：从 zeroclaw-api/src/memory_traits.rs 复制 `Memory` trait 的选定方法组（签名原样，含 `#[async_trait]`；`MemoryEntry`、`MemoryCategory` 一并复制）。文件内加 `pub struct NoopMemory;` 实现 `Memory`（所有方法返回 Ok 默认值，供测试与 phase 2 未接线场景）。

`port/observer.rs`：`Observer` trait 四方法（`record_event(&ObserverEvent)`、`record_metric(&ObserverMetric)`、`flush()`、`name()`）+ `ObserverEvent`/`ObserverMetric` 最小枚举 + `pub struct NoopObserver;`。

- [ ] **Step 4: 跑测试 + Commit**

Run: `cargo test -p agent port:: 2>&1 | tail -3`
Expected: PASS。

```bash
git add crates/agent/src/port
git commit -m "feat(agent): port runtime — AgentLoop/Memory/Observer 接口面"
```

---

### Task 5: zeroclaw 适配器（phase 1 引擎实现）

**Files:**
- Create: `crates/agent/src/adapters/mod.rs`
- Create: `crates/agent/src/adapters/zeroclaw/mod.rs`
- Create: `crates/agent/src/adapters/zeroclaw/tools.rs`（port Tool → zeroclaw Tool 桥）
- Create: `crates/agent/src/adapters/zeroclaw/provider.rs`（port ModelProvider → zeroclaw ModelProvider 桥 + canonical 编码 normalizer）
- Create: `crates/agent/src/adapters/zeroclaw/memory.rs`
- Create: `crates/agent/src/adapters/zeroclaw/observer.rs`
- Create: `crates/agent/src/adapters/zeroclaw/prompt.rs`
- Create: `crates/agent/src/adapters/zeroclaw/loop_.rs`（ZeroclawAgentLoop）
- Modify: `crates/agent/src/lib.rs`（加 `pub mod adapters;`）
- Test: `crates/agent/src/adapters/zeroclaw/tests.rs`（canonical 编码 round-trip 测试）

**Interfaces:**
- Consumes: Task 2/4 全部 port 类型。
- Produces:
  - `adapters::zeroclaw::loop_::ZeroclawAgentLoop`（`impl port::runtime::AgentLoop`）
  - `adapters::zeroclaw::loop_::zeroclaw_loop_factory(port::runtime::AgentLoopConfig) -> anyhow::Result<AgentLoopHandle>`
  - `adapters::zeroclaw::provider::normalize_zeroclaw_messages(&[zeroclaw ChatMessage]) -> Vec<port::provider::ChatMessage>`（canonical 编码断言的被测对象）

- [ ] **Step 1: 写 canonical 编码 round-trip 测试（失败）**

`adapters/zeroclaw/tests.rs`：

```rust
use crate::adapters::zeroclaw::provider::normalize_zeroclaw_messages;
use crate::port::provider::ChatMessage;

#[test]
fn normalizes_flat_tool_encoding_to_canonical() {
    let raw = vec![
        ChatMessage::system("sys"),
        ChatMessage::user("do it"),
        ChatMessage::assistant(r#"{"content": null, "tool_calls": [{"id":"call_1","name":"read_property","arguments":"{\"thingId\":\"t1\"}","extra_content":null}]}"#),
        ChatMessage::tool(r#"{"tool_call_id":"call_1","content":"currentValue=23"}"#),
    ];
    // normalize 是恒等映射（zeroclaw 的 flat 编码即 canonical），
    // 测试锁死格式契约，防止后续误改。
    let out = normalize_zeroclaw_messages(&raw);
    assert_eq!(out, raw);
}
```

注：若 Step 3 发现 zeroclaw 实发的 assistant 工具调用 JSON 键序/空白与上述字面量不同（以 dispatcher.rs:229-260 的 `serde_json::json!` 为准），修正本测试字面量即可——canonical 以 zeroclaw 实际输出为准，此测试是它的回归锁。

Run: `cargo test -p agent adapters::zeroclaw 2>&1 | tail -3`
Expected: FAIL（模块不存在）。

- [ ] **Step 2: tools / memory / observer / prompt 桥**

`tools.rs`：`struct PortToolAsZeroclaw(Box<dyn port::tool::Tool>);` 实现 `zeroclaw::tools::Tool`——`name/description/parameters_schema/spec` 直通；`execute(args)` 直通；`Attributable` 直通（port Attributable → zeroclaw Attributable，两枚举同形状，写 `match` 转换或经字符串中转）。反向不需要（工具只从 loop  outward）。

`memory.rs`：`struct PortMemoryAsZeroclaw(Arc<dyn port::memory::Memory>);` 实现 zeroclaw `Memory`——方法直通，仅 `Attributable` 做枚举转换。`MemoryEntry`/`MemoryCategory` 两形状字段相同则逐字段搬，不同则 `serde_json` 中转。

`observer.rs`：同上模式委托；`ObserverEvent`/`ObserverMetric` 到 phase 1 先映射为 Noop（不丢 `record_metric` 的调用计数语义——我们的代码从不直接调 observer，桥只需满足类型）。

`prompt.rs`：`fn to_zeroclaw_builder(ours: port::prompt::SystemPromptBuilder) -> zeroclaw::agent::prompt::SystemPromptBuilder`——**默认 9 section 不逐段映射**（zeroclaw 的 section struct 可见性未保证），直接 `SystemPromptBuilder::with_defaults()`；仅把 `ours` 中追加的自定义 section（`name() != 9 个内置名`）包成 zeroclaw `PromptSection` 桥追加。同时提供 `fn build_prompt_ctx(ours: &port::prompt::PromptContext) -> zeroclaw::agent::prompt::PromptContext`（字段按 zeroclaw PromptContext:12-40 填默认，security_summary 传入）。

- [ ] **Step 3: provider 桥 + normalizer**

`provider.rs`：

```rust
use std::sync::Arc;
use crate::port::provider as port;

/// port ModelProvider → zeroclaw ModelProvider。
/// zeroclaw loop 发出的 ChatRequest 先经 normalize 固化 canonical 编码，
/// ScriptedProvider 永远只看到 port 形状。
pub struct PortProviderAsZeroclaw {
    inner: Arc<dyn port::ModelProvider>,
}

#[async_trait::async_trait]
impl zeroclaw::providers::traits::ModelProvider for PortProviderAsZeroclaw {
    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let msgs = vec![
            system_prompt.map(|s| port::ChatMessage::system(s)),
            Some(port::ChatMessage::user(message)),
        ].into_iter().flatten().collect::<Vec<_>>();
        let req = port::ChatRequest { messages: &msgs, tools: None };
        Ok(self.inner.chat(req, model, temperature).await?.text.unwrap_or_default())
    }

    async fn chat(
        &self,
        request: zeroclaw::providers::traits::ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<zeroclaw::providers::traits::ChatResponse> {
        let normalized = normalize_zeroclaw_messages(request.messages);
        let tools = request.tools.map(|ts| ts.iter().map(|t| port::ToolSpec {
            name: t.name.clone(), description: t.description.clone(), parameters: t.parameters.clone(),
        }).collect::<Vec<_>>());
        let port_req = port::ChatRequest { messages: &normalized, tools: tools.as_deref() };
        let resp = self.inner.chat(port_req, model, temperature).await?;
        Ok(zeroclaw::providers::traits::ChatResponse {
            text: resp.text,
            tool_calls: resp.tool_calls.into_iter().map(|tc| zeroclaw::providers::traits::ToolCall {
                id: tc.id, name: tc.name, arguments: tc.arguments, extra_content: None,
            }).collect(),
            usage: resp.usage.map(|u| zeroclaw::providers::traits::TokenUsage {
                input_tokens: u.input_tokens, output_tokens: u.output_tokens, cached_input_tokens: u.cached_input_tokens,
            }),
            reasoning_content: resp.reasoning_content,
        })
    }
}

/// zeroclaw flat 编码 → port canonical 编码。当前为恒等（见 Global Constraints #4），
/// 函数保留以便 zeroclaw 侧格式漂移时单点修正。
pub fn normalize_zeroclaw_messages(
    msgs: &[zeroclaw::providers::traits::ChatMessage],
) -> Vec<port::ChatMessage> {
    msgs.iter().map(|m| port::ChatMessage { role: m.role.clone(), content: m.content.clone() }).collect()
}
```

（zeroclaw `chat_with_system` 在该 trait 中是必须实现项——检查 zeroclaw-api/src/model_provider.rs:491 是否无默认体，无则如上实现；有默认体则可省略，以编译为准。`Attributable` 同样需要转换实现。）

- [ ] **Step 4: ZeroclawAgentLoop**

`loop_.rs`：

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ZeroclawAgentLoop {
    agent: zeroclaw::agent::Agent,
}

#[async_trait::async_trait]
impl crate::port::runtime::AgentLoop for ZeroclawAgentLoop {
    async fn turn_streamed(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<crate::port::events::TurnEvent>,
        cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        // zeroclaw Agent::turn_streamed 是 &mut self，handle 层的 Mutex 由
        // AgentLoopHandle 提供；此处经内部 RefCell 不可行（Send 要求），
        // 故 ZeroclawAgentLoop 持 tokio::sync::Mutex<zeroclaw::agent::Agent>。
        let mut ag = self.agent.lock().await;
        let (text, _conversation) = ag.turn_streamed(user_message, event_tx, cancel_token).await
            .map_err(map_cancelled)?;
        Ok(text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        let mut ag = self.agent.lock().await;
        ag.run_single(message).await.map_err(map_cancelled)
    }
}

fn map_cancelled(e: anyhow::Error) -> anyhow::Error {
    // zeroclaw 的取消错误归一化到 port 的 ToolLoopCancelled，
    // runner 的 is_tool_loop_cancelled 判断对两个适配器行为一致。
    if zeroclaw::agent::loop_::is_tool_loop_cancelled(&e) {
        anyhow::Error::new(crate::port::outcome::ToolLoopCancelled)
    } else {
        e
    }
}

pub fn zeroclaw_loop_factory(
    cfg: crate::port::runtime::AgentLoopConfig,
) -> anyhow::Result<crate::port::runtime::AgentLoopHandle> {
    let provider = /* (provider_factory)() 由调用方注入？不——见下 */ ;
    todo!()
}
```

工厂依赖 provider：**设计裁定**——`AgentLoopConfig` 增加字段 `provider_factory: crate::pool::provider::ProviderFactory`（即现有 `Arc<dyn Fn() -> Result<Box<dyn port::ModelProvider>>>`，Task 6 时 ProviderFactory 的 trait 对象从 zeroclaw 换成 port）。在本 Task 就把 `crates/agent/src/pool/provider.rs` 的 `ProviderFactory` 类型别名与 `create_minimax_provider` 迁到 port（机械替换：zeroclaw ModelProvider → port ModelProvider，zeroclaw provider 工厂 `create_model_provider_with_url` 仍由它内部调用并包一层 `PortProviderAsZeroclaw`——不，方向反了：pool 的 provider 要能被两种 loop 用，必须 speak port。`create_minimax_provider` 返回 `Box<dyn port::ModelProvider>`，内部 zeroclaw 原生 provider 包 `ZeroclawProviderAsPort` 反向桥（zeroclaw ModelProvider → port ModelProvider，直接调其 `chat`）。该反向桥同样放 `adapters/zeroclaw/provider.rs`。）

`zeroclaw_loop_factory` 完整实现：

```rust
pub fn zeroclaw_loop_factory(
    cfg: crate::port::runtime::AgentLoopConfig,
) -> anyhow::Result<crate::port::runtime::AgentLoopHandle> {
    let provider = (cfg.provider_factory)()?;
    let zc_provider = PortProviderAsZeroclaw { inner: provider };
    let zc_memory = PortMemoryAsZeroclaw::new(Arc::clone(&cfg.memory));
    let zc_observer = PortObserverAsZeroclaw::new(Arc::clone(&cfg.observer));
    let agent = zeroclaw::agent::Agent::builder()
        .model_provider(Box::new(zc_provider))
        .tools(cfg.tools.into_iter().map(|t| Box::new(PortToolAsZeroclaw(t)) as Box<dyn zeroclaw::tools::Tool>).collect())
        .memory(Arc::new(zc_memory))
        .observer(Arc::new(zc_observer))
        .tool_dispatcher(Box::new(zeroclaw::agent::dispatcher::NativeToolDispatcher))
        .model_name(cfg.model_name.clone())
        .security_summary(cfg.security_summary.clone())
        .autonomy_level(zeroclaw::security::AutonomyLevel::Supervised)
        .prompt_builder(super::prompt::to_zeroclaw_builder(cfg.prompt_builder))
        .workspace_dir(cfg.workspace_dir.clone())
        .build()
        .map_err(|e| anyhow::anyhow!("Agent build failed: {}", e))?;
    Ok(Arc::new(tokio::sync::Mutex::new(ZeroclawAgentLoop {
        agent: tokio::sync::Mutex::new(agent),
    })))
}
```

（`.response_cache(...)` 调用**删除**——Global Constraints #1。zeroclaw builder 其余参数名以编译为准。）

- [ ] **Step 5: 跑测试至绿 + Commit**

Run: `cargo test -p agent adapters::zeroclaw 2>&1 | tail -3` 然后 `cargo test -p agent 2>&1 | tail -3`
Expected: PASS（既有测试尚未切到 port，应全绿）。

```bash
git add crates/agent/src/adapters crates/agent/src/port crates/agent/src/pool/provider.rs crates/agent/src/lib.rs
git commit -m "feat(agent): zeroclaw 适配器 — port 接口的首个引擎实现（canonical 编码 round-trip 锁定）"
```

---

### Task 6: 调用点迁移到 port（行为不变，全测试绿）

**Files:**
- Modify: `crates/agent/src/pool/pool.rs`（L11-20 导入、L126-157 memory/observer 创建、L241-292 create、L326-355 build_agent、L372-414 测试 ScriptedModelProvider）
- Modify: `crates/agent/src/pool/chat.rs`（L91-148 run_streaming、L71-81 run_single）
- Modify: `crates/agent/src/runtime/thing_agent/runner.rs`（L20-21 导入、L44 AgentHandle、L144-185 execute、L156-161 match 分支）
- Modify: `crates/agent/src/memory/workspace_memory.rs`（L15 导入，Memory trait 换 port）
- Modify: `crates/agent/src/tools/{registry,trust,external}.rs`（Tool/ToolResult 换 port）
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`（L31-37、L128-144、L260-314 ScriptedModelProvider）
- Modify: `apps/cloud/src/domains/agent/host/tools/` 全部 13 个工具文件（`use zeroclaw::tools::{Tool, ToolResult}` → `use agent::port::tool::{Tool, ToolResult}`，`zeroclaw_api::attribution` → `agent::port::attribution`）
- Modify: `apps/cloud/src/tests/thing_agent_loop_tests.rs`（L60-145 LoopScriptedProvider、L152 HangingProvider、L447 scripted_provider_factory）
- Modify: `apps/cloud/src/tests/agent_loop_e2e_tests.rs`（L62-107 E2eScriptedProvider）
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（tool_ok/tool_err 不变，仅导入）
- Test: 上述既有测试即验收门

**Interfaces:**
- Consumes: Task 2-5 全部。
- Produces: 全仓库除 `adapters/zeroclaw/` 与 Cargo.toml 外不再出现 `zeroclaw`/`zeroclaw_api` 字样（用 `grep -rn "zeroclaw" crates/ apps/ --include="*.rs"` 验证，命中应只剩 adapters/zeroclaw 与注释）。

- [ ] **Step 1: 迁移 tools（机械）**

把 13 个 cloud 工具 + crates/agent 的 registry/trust/external 中的 `zeroclaw::tools::{Tool, ToolResult}` 与 `zeroclaw_api::attribution::*` 导入替换为 port 路径。工具 impl 体一字不改（port trait 形状一致）。`TrustAwareTool` 与 `ToolRegistry` 的 `Box<dyn Tool>` 签名同步替换。

Run: `cargo check -p cloud 2>&1 | grep -E "^error" | head -20`
Expected: 仅剩 pool/runner/chat/autonomous_factory 相关错误（下一步处理）；若工具文件报错说明 trait 形状有出入，按编译器提示微调 port trait（不得改工具 impl 体）。

- [ ] **Step 2: 迁移 pool / runner / chat / workspace_memory**

- `pool.rs`：`PoolEntry.zeroclaw_agent` → `pub agent: port::runtime::AgentLoopHandle`；`AgentPool::create` 构造 `port::runtime::AgentLoopConfig`（含 provider_factory 与 prompt_builder），调 `adapters::zeroclaw::loop_::zeroclaw_loop_factory(cfg)`；memory/observer 创建段的 `zeroclaw::config::schema::MemoryConfig`/`create_memory`/`create_response_cache`/`create_observer` 调用整体替换——**裁定**：pool 现有的 memory 创建由谁承担？现状 zeroclaw `create_memory` 建 SQLite memory。port 化后：crates/agent 不得有存储实现（设计不变量），故 pool 改为接收 `Arc<dyn port::memory::Memory>`（由 apps/cloud 组合层创建并注入，`AgentPool::new` 签名加参）；`create_response_cache` 调用删除（死代码裁定）；observer 同理注入或由 config 字符串在 adapters/zeroclaw 内构造（保留现有 `create_observer` 行为——它变成 adapter 内部细节，签名 `adapters::zeroclaw::observer::create_observer(backend: &str) -> Arc<dyn port::observer::Observer>`，内部仍调 zeroclaw 工厂再包桥）。
- `runner.rs`：`use zeroclaw::agent::TurnEvent` → `use agent::port::events::TurnEvent`；`use zeroclaw::agent::loop_::is_tool_loop_cancelled` → `use agent::port::outcome::is_tool_loop_cancelled`；`pub type AgentHandle = Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>` → `pub type AgentHandle = agent::port::runtime::AgentLoopHandle`；execute 的 `ag.turn_streamed(&prompt, event_tx, Some(cancel))` 不变（AgentLoop trait 同签名）；match 分支 `Ok(Ok((text, _)))` → `Ok(Ok(text))`（port 返回 String）。
- `chat.rs`：`run_streaming` 的 `zeroclaw::agent::TurnEvent` → port；`ag.turn_streamed(message, event_tx, None).await` 返回 `Result<String>`，`Ok(final_text)` 直接得；`run_single` 不变。
- `workspace_memory.rs`：impl 的 trait 换 port；文件头注释更新（"替代 zeroclaw v0.8.1 移除的 NamespacedMemory"保留为历史说明）。

- [ ] **Step 3: 迁移 autonomous_factory + 测试 ScriptedProviders**

- `autonomous_factory.rs`：builder 段改为构造 `AgentLoopConfig`（无 skills section、security_summary 用原文案、response_cache 不传）调同一个 `zeroclaw_loop_factory`；`ScriptedModelProvider` 实现 port `ModelProvider`（`chat` 方法体不变——request 类型现在是 port ChatRequest，字段同名）。
- 两份测试文件的 ScriptedProvider 同上机械替换；`scripted_provider_factory` 返回类型已是 port ProviderFactory（Task 5 已迁 provider.rs）。

- [ ] **Step 4: 全量测试门**

Run: `cargo test -p agent 2>&1 | tail -3 && cargo test -p cloud 2>&1 | tail -3`
Expected: 全绿。红的优先怀疑桥接层（Task 5）而非调用点。

- [ ] **Step 5: grep 验证 + Commit**

Run: `grep -rn "zeroclaw" crates/ apps/ --include="*.rs" -l | grep -v adapters/zeroclaw`
Expected: 无输出。

```bash
git add crates apps
git commit -m "refactor(agent): 全部调用点迁移到 port 接口 — zeroclaw 收敛为 adapter 内部细节"
```

---

### Task 7: rig 适配器（phase 2 引擎实现）

**Files:**
- Create: `crates/agent/src/adapters/rig/mod.rs`
- Create: `crates/agent/src/adapters/rig/tools.rs`（port Tool → rig DynamicTool 桥）
- Create: `crates/agent/src/adapters/rig/provider.rs`（port ModelProvider → rig CompletionModel 桥）
- Create: `crates/agent/src/adapters/rig/memory.rs`（port Memory → rig ConversationMemory 桥）
- Create: `crates/agent/src/adapters/rig/loop_.rs`（RigAgentLoop + TurnEvent 翻译 + 预算 hook）
- Test: `crates/agent/src/adapters/rig/tests.rs`

**Interfaces:**
- Consumes: Task 1-4 port 全部；rig-core/rig-agent 0.42（API 以 Task 1 Spike 记录为准）。
- Produces:
  - `adapters::rig::loop_::RigAgentLoop`（`impl port::runtime::AgentLoop`）
  - `adapters::rig::loop_::rig_loop_factory(port::runtime::AgentLoopConfig) -> anyhow::Result<AgentLoopHandle>`

- [ ] **Step 1: 写 TurnEvent 翻译测试（失败）**

`adapters/rig/tests.rs`：用 rig 的 `test-utils`（rig-agent feature `test-utils`，或手写最小 `CompletionModel` mock——优先手写 `struct ScriptModel` 实现 `rig_core::completion::CompletionModel`，按 FIFO 返回预置 `CompletionResponse`）驱动 `RigAgentLoop::turn_streamed`，断言事件序列。最小用例：模型先回一个 tool_call 再回文本（两轮），断言事件序：

```text
ToolCall { id 稳定, name, args }
ToolResult { 同 id, name, output 含工具返回文本 }
Usage { input/cached/output 为 Some }
最终返回文本 == 第二轮文本
```

Run: `cargo test -p agent adapters::rig 2>&1 | tail -3`
Expected: FAIL（模块不存在）。

- [ ] **Step 2: tools.rs — DynamicTool 桥**

```rust
use std::sync::Arc;

/// port Tool → rig DynamicTool。每个 port 工具在构建期包一个 DynamicTool。
pub fn to_dynamic_tool(tool: Arc<dyn crate::port::tool::Tool>) -> rig_agent::tool::DynamicTool {
    let name = tool.name().to_string();
    let description = tool.description().to_string();
    let parameters = tool.parameters_schema();
    rig_agent::tool::DynamicTool::new(
        name,
        description,
        parameters,
        move |_ctx, args| {
            let tool = Arc::clone(&tool);
            Box::pin(async move {
                match tool.execute(args).await {
                    Ok(res) if res.success => {
                        Ok(rig_core::tool::ToolOutput::text(res.output))
                    }
                    Ok(res) => {
                        // zeroclaw 语义：success=false 也作为 tool result 回传（error 字段携带原因），
                        // 不让 loop 重试。以文本形式回传，JSON 编码与 zeroclaw 的
                        // tool result message 内容保持一致（dispatcher 编码）。
                        Ok(rig_core::tool::ToolOutput::text(serde_json::json!({
                            "success": false, "error": res.error.unwrap_or_default(),
                        }).to_string()))
                    }
                    Err(e) => Err(rig_agent::tool::ToolExecutionError::new(
                        rig_core::tool::ToolErrorKind::Other,
                        e.to_string(),
                    )),
                }
            })
        },
    )
}
```

（`DynamicTool::new` 回调类型以 Task 1 编译验证为准；`ToolOutput::text` 与 `ToolExecutionError::new` 已在 v0.42 源码确认存在。）

- [ ] **Step 3: provider.rs — CompletionModel 桥**

```rust
use std::sync::Arc;

/// port ModelProvider → rig CompletionModel。
/// 关键：把 rig 结构化 Message 拍平成 port canonical 编码（Global Constraints #4），
/// ScriptedProvider 因此与 zeroclaw 适配器看到完全相同的文本。
pub struct PortModelAsRig {
    inner: Arc<dyn crate::port::provider::ModelProvider>,
    model: String,
}

impl rig_core::completion::CompletionModel for PortModelAsRig {
    async fn completion(
        &self,
        request: rig_core::completion::CompletionRequest,
    ) -> Result<rig_core::completion::CompletionResponse, rig_core::completion::CompletionError> {
        // 1) 拍平 chat_history 到 port ChatMessage（canonical 编码）：
        //    Message::System{content} → system 消息
        //    Message::User{content} → 每条 UserContent::Text 一条 user 消息；
        //        UserContent::ToolResult{tool_result} → ChatMessage::tool(
        //            json!({"tool_call_id": tool_result.call.to_string(), "content": 文本化 content}).to_string())
        //    Message::Assistant{content} → AssistantContent::Text 拼成一条 assistant；
        //        AssistantContent::ToolCall(tc) → 收集到 assistant JSON 的 "tool_calls" 数组
        //        （同轮多条 ToolCall 合并为一条 assistant 消息，编码 =
        //         {"content": 文本或 null, "tool_calls": [...]}，ToolCall 序列化字段名
        //         id/name/arguments(String)，arguments 由 tc.function.arguments(Value) to_string）
        // 2) request.tools → port ToolSpec 列表
        // 3) 调 inner.chat(...) 得 port ChatResponse
        // 4) 组装 rig CompletionResponse：
        //    choice = [AssistantContent::Text(...)]（text 有值时）
        //      ++ tool_calls.map(|tc| AssistantContent::ToolCall(rig ToolCall::from_wire(
        //             Some(tc.id), ToolFunction{ name: tc.name, arguments: serde_json::from_str(&tc.arguments)? })))
        //    usage = Usage{ input_tokens: u.input_tokens.unwrap_or(0), output_tokens: ...,
        //                   total_tokens: 两者之和, cached_input_tokens: ..., cache_creation_input_tokens: 0,
        //                   tool_use_prompt_tokens: 0, ...其余字段 0 }
        todo!("按上方注释逐步实现，错误映射 CompletionError::BackendError")
    }
}
```

（`CompletionError` 的具体 variant 名以 Task 1 Spike/编译为准；`rig ToolCall::from_wire(provider_id, ToolFunction)` 已在 v0.42 message.rs 确认存在。reasoning_content 有值时作为 `AssistantContent::Reasoning` 追加，保持 round-trip。）

`stream()` 方法：非流式代理——对 `ScriptModel`/MiniMax 非流式路径可直接 `Err(CompletionError::...unsupported)` **不行**（rig agent streaming 走 stream()）。实现：调 `completion()` 后把整份 `CompletionResponse` 包装成一个单元素 `StreamingCompletionResponse`（rig-core streaming 提供从静态响应构造的方式——若不便，用 `async_stream`/`futures::stream::once` 构造官方要求的流类型；具体构造入口以 Task 1 Spike 记录为准，本任务第一步先在 spike 记录里确认）。

- [ ] **Step 4: memory.rs — ConversationMemory 桥**

```rust
pub struct PortMemoryAsConversation {
    inner: Arc<dyn crate::port::memory::Memory>,
}

const RECALL_LIMIT: usize = 20; // zeroclaw effective_memory_recall_limit 默认值量级，作为可接受差异文档化

impl rig_core::memory::ConversationMemory for PortMemoryAsConversation {
    async fn load(&self, conversation_id: &str) -> Result<Vec<rig_core::message::Message>, rig_core::memory::MemoryError> {
        let entries = self.inner.recall("", RECALL_LIMIT, Some(conversation_id), None, None).await
            .map_err(|e| rig_core::memory::MemoryError::Backend(e.to_string().into()))?;
        Ok(entries.into_iter().map(|e| rig_core::message::Message::user(e.content)).collect())
    }
    async fn append(&self, conversation_id: &str, messages: Vec<rig_core::message::Message>) -> Result<(), rig_core::memory::MemoryError> {
        for m in messages {
            if let rig_core::message::Message::User { content } = m {
                for block in content {
                    if let rig_core::message::UserContent::Text(t) = block {
                        self.inner.store("msg", t.text, MemoryCategory::Conversation, Some(conversation_id)).await...;
                    }
                }
            }
        }
        Ok(())
    }
    async fn clear(&self, conversation_id: &str) -> Result<(), rig_core::memory::MemoryError> { Ok(()) }
}
```

（具体 `MemoryError` 构造与 `Message`/`UserContent` 字段路径以编译为准；这是 Global Constraints #6 可接受差异的落点。）

- [ ] **Step 5: loop_.rs — RigAgentLoop**

核心结构：

```rust
pub struct RigAgentLoop {
    agent: rig_agent::agent::Agent,   // rig Agent 方法均 &self，无需内部 Mutex
    max_tool_calls: usize,            // = port::runtime::MAX_LOOP_TURNS
    max_duration: std::time::Duration, // runner 外层还有 timeout 兜底；hook 内做软预算
}

#[async_trait::async_trait]
impl crate::port::runtime::AgentLoop for RigAgentLoop {
    async fn turn_streamed(&self, user_message, event_tx, cancel_token) -> anyhow::Result<String> {
        let mut stream = self.agent.stream_prompt(user_message).await;
        // per-request 覆盖：.tool_concurrency(最大(工具数,1)) .max_turns(MAX_LOOP_TURNS) .add_hook(BudgetHook)
        let mut final_text = String::new();
        let start = std::time::Instant::now();
        let cancelled = cancel_token.clone();
        loop {
            tokio::select! {
                _ = async { match &cancelled { Some(t) => t.cancelled().await, None => std::future::pending::<()>().await } } => {
                    drop(stream); // rig 取消语义 = drop（Task 1 spike 已验证）
                    return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                }
                item = stream.next() => {
                    let Some(item) = item else { break };
                    let item = item?;
                    match item {
                        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(t)) => {
                            let _ = event_tx.send(TurnEvent::Chunk { delta: t.text.clone() }).await;
                            final_text.push_str(&t.text);
                        }
                        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ReasoningDelta(r)) => {
                            let _ = event_tx.send(TurnEvent::Thinking { delta: r.reasoning }).await;
                        }
                        // 工具调用事件取自 ToolExecutionCommitted（hook 后、每调用恰一次），
                        // 避免与 StreamAssistantItem::ToolCall 双计（runner 按 ToolCall 计数做预算）。
                        MultiTurnStreamItem::ToolExecutionCommitted { tool_call, .. } => {
                            let _ = event_tx.send(TurnEvent::ToolCall {
                                id: tool_call.id.to_string(),
                                name: tool_call.function.name.clone(),
                                args: tool_call.function.arguments.clone(),
                            }).await;
                        }
                        MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult { tool_result, .. }) => {
                            let output = tool_result.content.iter().map(|c| match c {
                                rig_core::message::ToolResultContent::Text(t) => t.text.clone(),
                                other => serde_json::to_string(other).unwrap_or_default(),
                            }).collect::<Vec<_>>().join("\n");
                            let _ = event_tx.send(TurnEvent::ToolResult {
                                id: tool_result.call.to_string(),
                                name: tool_result.name.clone(),
                                output,
                            }).await;
                        }
                        MultiTurnStreamItem::CompletionCall(call) => {
                            let _ = event_tx.send(TurnEvent::Usage {
                                input_tokens: Some(call.usage.input_tokens),
                                cached_input_tokens: Some(call.usage.cached_input_tokens),
                                output_tokens: Some(call.usage.output_tokens),
                                cost_usd: None,
                            }).await;
                            if start.elapsed() > self.max_duration {
                                drop(stream);
                                return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                            }
                        }
                        MultiTurnStreamItem::FinalResponse(resp) => {
                            final_text = resp.output().to_string(); // 以 spike 记录的方法名为准
                        }
                        _ => {}
                    }
                }
            }
        }
        drop(event_tx);
        Ok(final_text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        self.agent.prompt(message.to_string()).await.map_err(|e| anyhow::anyhow!(e))
    }
}
```

`BudgetHook`（工具数预算的 hook 实现，作为第二道防线；主防线是 runner 的 CancellationToken）：

```rust
struct BudgetHook { budget: usize, count: std::sync::atomic::AtomicUsize, cancel: Option<CancellationToken> }
impl rig_agent::agent::AgentHook for BudgetHook {
    fn on_tool_call(&self, _ctx, _event) -> impl Future<Output = ToolCallAction> + Send {
        let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if n > self.budget {
                if let Some(t) = &self.cancel { t.cancel(); }
                ToolCallAction::stop("tool call budget exceeded")
            } else {
                ToolCallAction::Continue
            }
        }
    }
}
```

`rig_loop_factory`：

```rust
pub fn rig_loop_factory(cfg: crate::port::runtime::AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> {
    let provider = (cfg.provider_factory)()?;
    let model = PortModelAsRig { inner: provider, model: cfg.model_name.clone() };
    let preamble = cfg.prompt_builder.build(&crate::port::prompt::PromptContext {
        workspace_dir: &cfg.workspace_dir,
        agent_workspace_dir: &cfg.workspace_dir,
        model_name: &cfg.model_name,
        tool_specs: cfg.tools.iter().map(|t| t.spec()).collect(),
        security_summary: cfg.security_summary.clone(),
    })?;
    let mut builder = rig_agent::agent::AgentBuilder::new(model)  // 或 client.agent(model) —— 以 spike 记录为准
        .preamble(preamble)
        .default_max_turns(crate::port::runtime::MAX_LOOP_TURNS)
        .memory(PortMemoryAsConversation::new(Arc::clone(&cfg.memory)))
        .conversation("default"); // conversation_id：现状 zeroclaw 各 turn 独立、无显式会话，取固定值即可
    for tool in cfg.tools {
        builder = builder.dynamic_tool(to_dynamic_tool(tool));
    }
    let agent = builder.build();
    Ok(Arc::new(tokio::sync::Mutex::new(RigAgentLoop {
        agent,
        max_tool_calls: crate::port::runtime::MAX_LOOP_TURNS,
        max_duration: std::time::Duration::from_secs(300),
    })))
}
```

（`.memory()` 要求 `ConversationMemory`；`AgentBuilder` typestate 有工具/无工具两条 build 路径——零工具时走 `AgentBuilder::new(model).build()`，以编译为准。observer 的 forward：phase 2 经 `BudgetHook` 之外的观测 hook（`on_completion_call` → port `ObserverMetric`）实现，若 Task 4 的 port Observer 已是 Noop 语义则保留 NoopObserver 等价物并在 PR 描述中记录观测差异。）

- [ ] **Step 6: 跑翻译测试至绿 + Commit**

Run: `cargo test -p agent adapters::rig 2>&1 | tail -5`
Expected: PASS。

```bash
git add crates/agent/src/adapters/rig
git commit -m "feat(agent): rig 适配器 — DynamicTool/CompletionModel/ConversationMemory 桥 + TurnEvent 翻译 + 预算 hook"
```

---

### Task 8: 切换默认引擎到 rig + 全量回归

**Files:**
- Modify: `crates/agent/src/pool/pool.rs`（`create` 内 `zeroclaw_loop_factory` → `rig_loop_factory`）
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`（同）
- Modify: `crates/agent/src/pool/provider.rs`（`create_minimax_provider` 内部 zeroclaw provider → rig minimax provider）

**Interfaces:**
- Consumes: Task 5-7。
- Produces: 运行时默认引擎 = rig；zeroclaw 适配器仍在代码树但不再被默认路径引用。

- [ ] **Step 1: 切换 minimax provider 到 rig**

`create_minimax_provider` 改为：

```rust
pub fn create_minimax_provider() -> anyhow::Result<Box<dyn crate::port::provider::ModelProvider>> {
    let cfg = minimax_settings().ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
    // rig minimax provider：OpenAI 兼容协议，base_url/auth_token 直接对应
    let client = rig_core::providers::minimax::Client::builder()
        .api_key(&cfg.auth_token)
        .base_url(&cfg.base_url)
        .build()?;
    Ok(Box::new(RigMinimaxAsPort::new(client.completion_model(&cfg.model))))
}
```

`RigMinimaxAsPort`（同文件或 `adapters/rig/provider.rs`）：包装 rig minimax completion model，实现 port `ModelProvider::chat`——把 port ChatRequest 转换为 rig `CompletionRequest`（Task 7 Step 3 拍平的逆变换：canonical 编码解析回结构化 Message；`{"tool_calls":[...]}` 的 assistant JSON 解析回 AssistantContent；role=tool 的 JSON 解析回 UserContent::ToolResult），调 `CompletionModel::completion()`，响应按 Task 7 Step 3 的逆映射回 port ChatResponse。**此逆变换与 Task 7 的拍平互为往返，必须在 `adapters::rig::tests` 加 round-trip 测试**（port ChatRequest → rig 请求 → 回到 port 形状，含 tool_call/tool_result 用例）。

Run: `cargo test -p agent adapters::rig 2>&1 | tail -3`
Expected: PASS。

- [ ] **Step 2: 切换两处 loop factory**

`pool.rs` 与 `autonomous_factory.rs` 的 `zeroclaw_loop_factory` 调用替换为 `adapters::rig::loop_::rig_loop_factory`。

- [ ] **Step 3: 全量测试门（决定性验收）**

Run: `cargo test -p agent 2>&1 | tail -3 && cargo test -p cloud 2>&1 | tail -3`
Expected: 全绿。若有 ScriptedProvider 测试红：说明其匹配依赖了 zeroclaw 编码细节，先把匹配改为语义子串（如 `contains("currentValue")`），不得改桥接编码去迁就测试。

- [ ] **Step 4: Commit**

```bash
git add crates apps
git commit -m "feat(agent): 默认引擎切换到 rig — zeroclaw 适配器退居非默认"
```

---

### Task 9: 删除 zeroclaw

**Files:**
- Modify: `crates/agent/Cargo.toml`、`apps/cloud/Cargo.toml`（删 zeroclaw/zeroclaw-api 两行）
- Modify: `deny.toml`、`.cargo/audit.toml`（zeroclaw 相关豁免注释清理）
- Delete: `crates/agent/src/adapters/zeroclaw/`（整目录）
- Modify: `Cargo.lock`（`cargo update` 自动收敛）

- [ ] **Step 1: 删适配器目录与依赖行**

- [ ] **Step 2: 编译 + 全量测试**

Run: `cargo build -p agent -p cloud 2>&1 | tail -3 && cargo test -p agent 2>&1 | tail -3 && cargo test -p cloud 2>&1 | tail -3 && cargo deny check 2>&1 | tail -5`
Expected: 全绿；deny 检查通过（rig 的 MIT/Apache 依赖树无新违规——若有遗漏的 license/advisory 例外按既有格式登记 deny.toml）。

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore(agent): 移除 zeroclaw 依赖与 fork — agent 引擎完成 rig 迁移"
```

---

### Task 10: 文档收尾

**Files:**
- Modify: `AGENTS.md`（zeroclaw 相关段落 → rig + port 架构说明）
- Modify: `crates/agent/src/lib.rs` 头注释（引擎描述更新）
- Create: `docs/designs/agent-engine-rig-migration.md`（迁移决策记录：动机、port 设计、canonical 编码、可接受差异清单 Global Constraints #6、回滚方式=切回 zeroclaw_loop_factory 的 git revert 路径）

- [ ] **Step 1: 更新三处文档**
- [ ] **Step 2: Commit**

```bash
git add AGENTS.md crates/agent/src/lib.rs docs/designs/agent-engine-rig-migration.md
git commit -m "docs(agent): rig 迁移决策记录与 AGENTS.md 引擎说明更新"
```

---

## Self-Review 记录

- **Spec 覆盖**：调研报告 6 类耦合点逐一对应——Tool trait（Task 2/6）、ModelProvider（Task 2/5/6/8）、agent loop（Task 4/5/7）、SystemPromptBuilder（Task 3）、Memory/Observer（Task 4/5/7）、attribution（Task 2/6）、AutonomyLevel/ResponseCache/ConversationMessage（Global Constraints 裁定不移植）。删除 zeroclaw（Task 9）、文档（Task 10）。
- **占位符扫描**：Task 7 Step 3 留了 `todo!()` 骨架但附带逐步实现注释与 spike 记录回指——这是有意为之（实现步骤依赖 Task 1 spike 的两个字段名确认，注释给出全部决策）；其余步骤均含完整代码。Task 7 Step 3/4 的"以编译为准"回指 spike，符合"Task 1 先行消除 API 未知"的全局设计。
- **类型一致性**：`AgentLoopConfig.provider_factory` 在 Task 5 Step 4 引入、Task 6 Step 2 使用、Task 7/8 消费——同名同型。`AgentLoopHandle` 在 Task 4 定义、Task 5/7 生产、Task 6 消费。`MAX_LOOP_TURNS` Task 4 定义、Task 7 消费。canonical 编码在 Global Constraints #4 定义、Task 5 Step 3 与 Task 7 Step 3/Task 8 Step 1 双向使用。
- **已知风险**：Task 7 Step 3 的 `stream()` 非流式代理构造方式、Step 4 的 `MemoryError` 构造、Step 5 的 `FinalResponse` 文本取值方法——全部收敛到 Task 1 Spike 记录，spike 未完成不得开始 Task 7。
