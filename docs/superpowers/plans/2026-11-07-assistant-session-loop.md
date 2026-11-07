# Assistant Session Loop 实现计划（Plan 1 / 4）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把聊天循环升级为唯一的 Assistant Session Loop，并在其中提供 `todo_write`、`ask_user`、`spawn_subagent` 三个 Claude Code 式原语、统一的 `session_events` 时间线，以及可查询的事件/待答问题 API。

**Architecture:** 以 `Workspace → Assistant → Session → Turn` 为唯一模型。`SessionRunner` 负责一次 Turn 的执行、事件落库和 `TurnEvent` 观察；三个原语作为普通 `Tool` 注入 chat Agent，通过 `TurnContextRegistry` 读取当前 `session_key`/`workspace_id`。`session_events` 继续复用 2026-11-03 已有的每 workspace Harness DB 表，不新建事件存储。调度器、前端、旧代码清理不在本计划内。

**Tech Stack:** Rust 2024、Axum、Tokio、SQLx/SQLite、`async-trait`、`dashmap`、`uuid`、`sha2`、现有 rig AgentLoop 适配器。

## Plan Boundary

本计划是 4 份计划中的第 1 份：

- **Plan 1（本文件）**：后端 Session Loop + 三个原语 + `session_events` + 查询/答题 API + 聊天接线。完成后聊天 Assistant 可零配置启动，模型可以规划 Todo、向用户提问、派生子代理，所有过程可在事件时间线里回放。
- **Plan 2**：Trigger Router / Schedule / 全局 HUD 事件流（`trigger.received`、`wake.dispatched`、最近事件索引）。
- **Plan 3**：Codex 式前端（AppShell、会话时间线、Todo 面板、问题卡片、子代理卡片）。
- **Plan 4**：旧代码清理与命名收敛（spec §12 的 Batch 1–5）。

不要在本计划中提前做 Plan 2–4 的删除或重命名；只做本计划列出的新增与必要接线。

---

## Global Constraints

以下约束来自 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md`，每个 Task 都隐含包含：

- **命名基线**：`Workspace → Assistant → Session → Turn`；`Task → Run`；`Session → Todo`；`Turn → Subagent`。新代码不出现 `ThingAgent`、`AutonomousAgent`、`Brain`（作为代码标识符）、`Conversation`、`Thread`、`Chat`（作为领域名词）、`Workflow`（新功能）、`Heartbeat`（AI 巡检）、`Job`（Agent 工作）。
- **事件命名**：`aggregate.past_tense`，例如 `subagent.completed`；禁止 `done`。本计划使用的事件名必须来自 spec §5.4.3 白名单。
- **SessionKey 例外**：保持 `agent:<workspace>:<assistant>/<session>` 格式；`agent` 是协议前缀，领域内读作 `assistant_id`。不要改 `crates/agent/src/session.rs` 的解析规则。
- **事件分层**：`session_events` 是唯一 UI 时间线（`SessionEvent`）；`hub_ledger` 的 `runtime.v1` 是安全/审计账本（`RuntimeEvent`）。不要把审计事实写进聊天时间线，也不要把聊天内容写进审计账本。
- **权限模式**：`read_only` / `ask`（默认） / `full_access`。`full_access` 也绝不绕过急停、互锁、Effect 边界；设备写操作仍走现有审批/Effect 路径。
- **存储**：`session_events` 位于每 workspace 独立的 `data/agents/<workspace>/harness/harness.db`，不需要 `workspace_id` 列；`HarnessEvent.workspace_id` 由调用方从 DB 归属补齐。
- **迁移**：只加列/加索引，不 drop 表、不改已有列语义；新迁移文件名使用 `YYYYMMDDNNNNNN_<name>.sql`。
- **TDD**：每个 Task 先写失败测试，再写最小实现，再跑测试，再提交。
- **提交**：每个 Task 一个 commit；不要提交工作区里无关的修改（现有 `docs/superpowers/specs/2026-11-02-agent-harness-core-design.md` 和未跟踪的 `docs/superpowers/plans/2026-11-03-agent-harness-core.md` 不要动）。
- **验证**：每个 Task 至少跑对应包的测试；最后跑 `cargo fmt --all -- --check`、`cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings`、`cargo test -p tinyiothub-cloud`。

---

## File Structure

### 新增

- `crates/db/migrations/20261107000001_assistant_sessions.sql` — `chat_sessions.kind/status` 列与索引。
- `apps/cloud/src/domains/agent/session/mod.rs` — 模块导出、`SessionRuntime`、超时常量。
- `apps/cloud/src/domains/agent/session/types.rs` — `SessionKind`、`PermissionMode`、`Todo`、`Question`、`Answer` 等值类型与校验。
- `apps/cloud/src/domains/agent/session/context.rs` — `TurnContext`、`TurnContextRegistry`、`TurnGuard`。
- `apps/cloud/src/domains/agent/session/events.rs` — `SessionEventSink`、`DbSessionEventSink`、`session_event()`、测试用 `RecordingSink`。
- `apps/cloud/src/domains/agent/session/todos.rs` — `TodoStore`。
- `apps/cloud/src/domains/agent/session/questions.rs` — `QuestionBroker`。
- `apps/cloud/src/domains/agent/session/subagent.rs` — `SubagentRunner` trait、`SubagentRequest/Report`、`SubagentSlot`。
- `apps/cloud/src/domains/agent/session/runner.rs` — `SessionRunner`、`TurnObserver`、`TurnOutcome`。
- `apps/cloud/src/domains/agent/session/tools/mod.rs` — 原语工具导出。
- `apps/cloud/src/domains/agent/session/tools/todo_write.rs` — `todo_write`。
- `apps/cloud/src/domains/agent/session/tools/ask_user.rs` — `ask_user`。
- `apps/cloud/src/domains/agent/session/tools/spawn_subagent.rs` — `spawn_subagent`。
- `apps/cloud/src/domains/agent/session/handler.rs` — 事件、Todo、问题回答 HTTP 端点。
- `apps/cloud/src/domains/agent/host/chat/subagent.rs` — `PoolSubagentRunner`（组合层实现）。

### 修改

- `apps/cloud/src/domains/agent/mod.rs` — 注册 `pub mod session;`；`AgentState` 增加 `session_runtime`。
- `apps/cloud/src/state.rs` — `AppState` 增加 `session_runtime`；`AppState::new` 构造；`FromRef` 派生。
- `apps/cloud/src/shared/service_manager.rs` — 注入 `SessionRuntime`、注册 `PoolSubagentRunner`、传给工具 provider。
- `apps/cloud/src/domains/agent/tools/context.rs` — `ToolRuntimeContext` 增加 `agent_id`、`session_tools_enabled`。
- `apps/cloud/src/domains/agent/host/tools/service.rs` — provider 增加 `session` 参数并注入三个工具。
- `apps/cloud/src/domains/agent/host/chat/service.rs` — `ensure_agent` 设置工具上下文；`send_message` 改走 `SessionRunner`；超时改为可答题的 15 分钟。
- `apps/cloud/src/domains/agent/host/chat/handler/proxy.rs` — 传入 `session_runtime`。
- `apps/cloud/src/domains/agent/host/chat/handler/mod.rs` — 挂载事件/答题/Todo 路由。
- `apps/cloud/src/domains/agent/pool/pool.rs` — `AgentPool::agent_config()` 读取父 Agent 配置。
- `crates/db/src/session.rs` — `Session` 增加 `kind/status` 字段与 SQL。

---

### Task 1: Session 持久化增加 `kind` / `status`

**Files:**

- Create: `crates/db/migrations/20261107000001_assistant_sessions.sql`
- Modify: `crates/db/src/session.rs`（`Session` 结构、`Session::new`、`map_session_row`、`get_session`、`create_session`、`update_session`、`get_or_create_session` 的 SQL）
- Test: `crates/db/src/session.rs` 的 `#[cfg(test)] mod tests`

**Interfaces:**

- Consumes: 现有 `Db::create_session/get_session/list_sessions`。
- Produces: `Session.kind: String`（默认 `"chat"`）、`Session.status: String`（默认 `"active"`）、`Session::with_kind(kind)`。

- [ ] **Step 1: 写失败测试**

在 `crates/db/src/session.rs` 的测试模块追加：

```rust
#[tokio::test]
pub async fn test_session_defaults_to_chat_active() {
    let db = create_test_db().await;
    let session = Session::new(
        "agent:ws:agent1/sess_kind".to_string(),
        "ws".to_string(),
        "agent1".to_string(),
    );
    assert_eq!(session.kind, "chat");
    assert_eq!(session.status, "active");

    db.create_session(&session).await.unwrap();
    let found = db.get_session(&session.session_key).await.unwrap().unwrap();
    assert_eq!(found.kind, "chat");
    assert_eq!(found.status, "active");
}

#[tokio::test]
pub async fn test_session_persists_explicit_kind() {
    let db = create_test_db().await;
    let session = Session::new(
        "agent:ws:agent1/sess_scheduled".to_string(),
        "ws".to_string(),
        "agent1".to_string(),
    )
    .with_kind("scheduled");
    db.create_session(&session).await.unwrap();
    let found = db.get_session(&session.session_key).await.unwrap().unwrap();
    assert_eq!(found.kind, "scheduled");
    assert_eq!(found.status, "active");
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p db session::tests::test_session_defaults_to_chat_active -v`

Expected: FAIL，编译错误 `no field kind on type Session`。

- [ ] **Step 3: 写最小实现**

创建 `crates/db/migrations/20261107000001_assistant_sessions.sql`：

```sql
ALTER TABLE chat_sessions ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';
ALTER TABLE chat_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
CREATE INDEX IF NOT EXISTS idx_chat_sessions_workspace_updated
    ON chat_sessions(workspace_id, updated_at);
```

在 `crates/db/src/session.rs` 中：

1. 给 `Session` 增加字段（放在 `label` 后）：

```rust
    /// 会话类型："chat" | "scheduled" | "event"
    #[serde(default = "default_session_kind")]
    pub kind: String,
    /// 会话状态："active" | "ended"
    #[serde(default = "default_session_status")]
    pub status: String,
```

2. 增加默认函数：

```rust
fn default_session_kind() -> String {
    "chat".to_string()
}

fn default_session_status() -> String {
    "active".to_string()
}
```

3. `Session::new` 中设置：

```rust
            kind: default_session_kind(),
            status: default_session_status(),
```

4. 增加 builder：

```rust
    /// 设置会话类型（用于 scheduled/event 会话；默认 chat）。
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self
    }
```

5. `map_session_row` 读取列并给出旧库兜底：

```rust
    let kind: Option<String> = row.try_get("kind").ok();
    let status: Option<String> = row.try_get("status").ok();
```

并在构造 `Session` 时加：

```rust
        kind: kind.unwrap_or_else(default_session_kind),
        status: status.unwrap_or_else(default_session_status),
```

6. 所有 `SELECT session_key, workspace_id, agent_id, label, created_at, updated_at, metadata` 改为：

```sql
SELECT session_key, workspace_id, agent_id, label, kind, status, created_at, updated_at, metadata
```

7. `create_session` 的 INSERT 改为，并按下列顺序绑定：

```rust
    sqlx::query(
        "INSERT INTO chat_sessions (session_key, workspace_id, agent_id, label, kind, status, created_at, updated_at, metadata) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&session.session_key)
    .bind(&session.workspace_id)
    .bind(&session.agent_id)
    .bind(&session.label)
    .bind(&session.kind)
    .bind(&session.status)
    .bind(session.created_at)
    .bind(session.updated_at)
    .bind(&metadata_str)
    .execute(pool)
    .await
    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
```

8. `ensure_session`（`INSERT OR IGNORE INTO chat_sessions`，用于聊天历史前置）不需要改：迁移给 `kind/status` 的 `NOT NULL DEFAULT`，旧 INSERT 会得到 `chat`/`active`。`get_or_create_session` 复用 `create_session`，无需单独处理。

9. `update_session` 的 UPDATE 改为，并按下列顺序绑定：

```rust
    let result = sqlx::query(
        "UPDATE chat_sessions \
         SET workspace_id = ?, agent_id = ?, label = ?, kind = ?, status = ?, updated_at = ?, metadata = ? \
         WHERE session_key = ?",
    )
    .bind(&session.workspace_id)
    .bind(&session.agent_id)
    .bind(&session.label)
    .bind(&session.kind)
    .bind(&session.status)
    .bind(session.updated_at)
    .bind(&metadata_str)
    .bind(&session.session_key)
    .execute(pool)
    .await
    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p db session::tests::test_session_defaults_to_chat_active -v
cargo test -p db session::tests::test_session_persists_explicit_kind -v
cargo test -p db session::tests::test_session_crud -v
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/db/migrations/20261107000001_assistant_sessions.sql crates/db/src/session.rs
git commit -m "feat(db): persist assistant session kind and status"
```

---

### Task 2: Session 值类型与校验

**Files:**

- Create: `apps/cloud/src/domains/agent/session/mod.rs`
- Create: `apps/cloud/src/domains/agent/session/types.rs`
- Modify: `apps/cloud/src/domains/agent/mod.rs`（只加 `pub mod session;`）
- Test: `apps/cloud/src/domains/agent/session/types.rs` 的 `#[cfg(test)] mod tests`

**Interfaces:**

- Consumes: `serde`。
- Produces:
  - `SessionKind::{Chat, Scheduled, Event}`，`as_str()`、`parse(&str) -> Option<Self>`。
  - `PermissionMode::{ReadOnly, Ask, FullAccess}`，`Default = Ask`。
  - `Todo { content, status, active_form }`、`TodoStatus::{Pending, InProgress, Completed}`、`TodoList { todos }`、`TodoList::validate() -> Result<(), String>`。
  - `Question { question, header, options, multi_select }`、`QuestionOption { label, description }`、`Question::validate_set(&[Question]) -> Result<(), String>`。
  - `QuestionAnswer { answers }`、`QuestionResponse { question_index, selected, text }`。

- [ ] **Step 1: 写失败测试**

创建 `apps/cloud/src/domains/agent/session/mod.rs`（先让模块存在）：

```rust
pub mod types;

pub use types::*;
```

在 `apps/cloud/src/domains/agent/mod.rs` 的模块列表中加入：

```rust
pub mod session;
```

在 `types.rs` 末尾写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_defaults_to_ask() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    }

    #[test]
    fn session_kind_round_trips() {
        for kind in [SessionKind::Chat, SessionKind::Scheduled, SessionKind::Event] {
            assert_eq!(SessionKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(SessionKind::parse("workflow"), None);
    }

    #[test]
    fn todo_list_rejects_two_in_progress() {
        let list = TodoList {
            todos: vec![
                Todo {
                    content: "检查离线事件".into(),
                    status: TodoStatus::InProgress,
                    active_form: "检查离线事件".into(),
                },
                Todo {
                    content: "检查设备异常".into(),
                    status: TodoStatus::InProgress,
                    active_form: "检查设备异常".into(),
                },
            ],
        };
        assert!(list.validate().is_err());
    }

    #[test]
    fn todo_list_accepts_one_in_progress() {
        let list = TodoList {
            todos: vec![Todo {
                content: "检查离线事件".into(),
                status: TodoStatus::InProgress,
                active_form: "检查离线事件".into(),
            }],
        };
        assert!(list.validate().is_ok());
    }

    #[test]
    fn question_set_rejects_out_of_range_options() {
        let question = Question {
            question: "要检查哪些设备？".into(),
            header: "检查范围".into(),
            options: vec![QuestionOption {
                label: "全部".into(),
                description: "检查全部设备".into(),
            }],
            multi_select: false,
        };
        assert!(Question::validate_set(std::slice::from_ref(&question)).is_err());
    }

    #[test]
    fn question_set_accepts_valid_question() {
        let question = Question {
            question: "要检查哪些设备？".into(),
            header: "检查范围".into(),
            options: vec![
                QuestionOption {
                    label: "全部".into(),
                    description: "检查全部设备".into(),
                },
                QuestionOption {
                    label: "仅离线".into(),
                    description: "只检查离线设备".into(),
                },
            ],
            multi_select: true,
        };
        assert!(Question::validate_set(&[question]).is_ok());
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::types::tests -v`

Expected: FAIL，编译错误缺少类型。

- [ ] **Step 3: 写最小实现**

在 `types.rs` 中写：

```rust
//! Assistant Session 的值类型 — 纯数据，无 I/O。
//!
//! 这些类型是 Session Loop、原语工具、HTTP DTO 的共享词汇。wire 用 camelCase，
//! Rust 字段用 snake_case。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Chat,
    Scheduled,
    Event,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Scheduled => "scheduled",
            Self::Event => "event",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "scheduled" => Some(Self::Scheduled),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    ReadOnly,
    #[default]
    Ask,
    FullAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub content: String,
    pub status: TodoStatus,
    pub active_form: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoList {
    pub todos: Vec<Todo>,
}

impl TodoList {
    /// Claude Code 语义：非空、最多 50 项、最多一个 in_progress。
    pub fn validate(&self) -> Result<(), String> {
        if self.todos.is_empty() {
            return Err("todo list must not be empty".to_string());
        }
        if self.todos.len() > 50 {
            return Err("todo list exceeds 50 items".to_string());
        }
        let in_progress = self
            .todos
            .iter()
            .filter(|todo| todo.status == TodoStatus::InProgress)
            .count();
        if in_progress > 1 {
            return Err("at most one todo may be in_progress".to_string());
        }
        for todo in &self.todos {
            if todo.content.trim().is_empty() {
                return Err("todo content must not be empty".to_string());
            }
            if todo.active_form.trim().is_empty() {
                return Err("todo activeForm must not be empty".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

impl Question {
    /// Claude Code 语义：1–4 个问题，每题 2–4 个选项，header 不超过 12 字符。
    pub fn validate_set(questions: &[Self]) -> Result<(), String> {
        if questions.is_empty() || questions.len() > 4 {
            return Err("ask_user requires 1 to 4 questions".to_string());
        }
        for question in questions {
            if question.question.trim().is_empty() {
                return Err("question text must not be empty".to_string());
            }
            if question.header.trim().is_empty() {
                return Err("question header must not be empty".to_string());
            }
            if question.header.chars().count() > 12 {
                return Err("question header must be at most 12 characters".to_string());
            }
            if question.options.len() < 2 || question.options.len() > 4 {
                return Err("each question requires 2 to 4 options".to_string());
            }
            for option in &question.options {
                if option.label.trim().is_empty() {
                    return Err("question option label must not be empty".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionResponse {
    pub question_index: usize,
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub answers: Vec<QuestionResponse>,
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud session::types::tests -v`

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/mod.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/session/types.rs
git commit -m "feat(session): add assistant session value types"
```

---

### Task 3: TurnContext 与 TurnContextRegistry

**Files:**

- Create: `apps/cloud/src/domains/agent/session/context.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`（`pub mod context; pub use context::*;`）
- Test: `apps/cloud/src/domains/agent/session/context.rs` 的测试模块

**Interfaces:**

- Consumes: `uuid::Uuid`、`dashmap::DashMap`。
- Produces:
  - `TurnContext { workspace_id, assistant_id, session_key, turn_id, run_id, depth }`，`TurnContext::new(workspace_id, assistant_id, session_key, run_id, depth) -> TurnContext`（自动生成 `turn_id`）。
  - `TurnContextRegistry::set(ctx) -> TurnGuard`、`get(assistant_id) -> Option<TurnContext>`；内部 `clear_if_current(assistant_id, turn_id)` 由 `TurnGuard::drop` 调用。
  - `TurnGuard` 在 Drop 时只清除自己安装的那次 Turn。

- [ ] **Step 1: 写失败测试**

在 `context.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(run_id: &str) -> TurnContext {
        TurnContext::new(
            "ws".to_string(),
            "agent_main".to_string(),
            "agent:ws:agent_main/s1".to_string(),
            run_id.to_string(),
            0,
        )
    }

    #[test]
    fn set_and_get_round_trip() {
        let registry = TurnContextRegistry::default();
        let context = ctx("run1");
        let _guard = registry.set(context.clone());
        let found = registry.get("agent_main").expect("context installed");
        assert_eq!(found.session_key, context.session_key);
        assert_eq!(found.turn_id, context.turn_id);
    }

    #[test]
    fn guard_clears_installed_turn() {
        let registry = TurnContextRegistry::default();
        {
            let _guard = registry.set(ctx("run1"));
        }
        assert!(registry.get("agent_main").is_none());
    }

    #[test]
    fn stale_guard_does_not_clear_newer_turn() {
        let registry = TurnContextRegistry::default();
        let first = ctx("run1");
        let guard = registry.set(first);
        let second = ctx("run2");
        let _second_guard = registry.set(second.clone());
        drop(guard);
        let found = registry.get("agent_main").expect("newer context survives");
        assert_eq!(found.turn_id, second.turn_id);
    }

    #[test]
    fn contexts_are_isolated_by_assistant() {
        let registry = TurnContextRegistry::default();
        let _a = registry.set(ctx("run_a"));
        let mut other = ctx("run_b");
        other.assistant_id = "agent_other".to_string();
        let _b = registry.set(other);
        assert_eq!(registry.get("agent_main").unwrap().run_id, "run_a");
        assert_eq!(registry.get("agent_other").unwrap().run_id, "run_b");
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::context::tests -v`

Expected: FAIL，缺少 `TurnContext`。

- [ ] **Step 3: 写最小实现**

在 `context.rs` 中写：

```rust
//! TurnContext — 当前 Turn 的会话身份，供工具在 execute 时读取。
//!
//! AgentLoop 按 assistant_id 缓存并串行执行 Turn，因此每个 assistant 在任一时刻
//! 最多只有一个 TurnContext。Registry 以 assistant_id 为键；TurnGuard 在 Drop
//! 时只清除自己安装的 turn_id，避免慢 Turn 清掉后来者的上下文。

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnContext {
    pub workspace_id: String,
    /// 领域内读作 assistant_id；协议层仍叫 agent_id。
    pub assistant_id: String,
    /// "agent:<workspace>:<assistant>/<session>"
    pub session_key: String,
    pub turn_id: String,
    pub run_id: String,
    /// 0 = 主 Assistant；1 = 子代理；>=2 拒绝。
    pub depth: u8,
}

impl TurnContext {
    pub fn new(
        workspace_id: String,
        assistant_id: String,
        session_key: String,
        run_id: String,
        depth: u8,
    ) -> Self {
        Self {
            workspace_id,
            assistant_id,
            session_key,
            turn_id: Uuid::new_v4().to_string(),
            run_id,
            depth,
        }
    }
}

#[derive(Clone, Default)]
pub struct TurnContextRegistry {
    inner: Arc<DashMap<String, TurnContext>>,
}

pub struct TurnGuard {
    registry: TurnContextRegistry,
    assistant_id: String,
    turn_id: String,
}

impl TurnContextRegistry {
    pub fn set(&self, context: TurnContext) -> TurnGuard {
        let assistant_id = context.assistant_id.clone();
        let turn_id = context.turn_id.clone();
        self.inner.insert(assistant_id.clone(), context);
        TurnGuard {
            registry: self.clone(),
            assistant_id,
            turn_id,
        }
    }

    pub fn get(&self, assistant_id: &str) -> Option<TurnContext> {
        self.inner.get(assistant_id).map(|entry| entry.value().clone())
    }

    fn clear_if_current(&self, assistant_id: &str, turn_id: &str) {
        let is_current = self
            .inner
            .get(assistant_id)
            .map(|entry| entry.value().turn_id == turn_id)
            .unwrap_or(false);
        if is_current {
            self.inner.remove(assistant_id);
        }
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        self.registry.clear_if_current(&self.assistant_id, &self.turn_id);
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod context;

pub use context::*;
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud session::context::tests -v`

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/context.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add turn context registry"
```

---

### Task 4: SessionEventSink 与事件构造

**Files:**

- Create: `apps/cloud/src/domains/agent/session/events.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Test: `apps/cloud/src/domains/agent/session/events.rs` 的测试模块

**Interfaces:**

- Consumes: `tinyiothub_core::harness::{HarnessEvent, HarnessEventInput, ExecutionMode}`、`tinyiothub_storage::{Db, DbError}`、`HarnessStreamHub`、`TurnContext`。
- Produces:
  - `trait SessionEventSink: emit(HarnessEventInput) -> Result<HarnessEvent, DbError>`。
  - `DbSessionEventSink::new(db: Arc<Db>, hub: Arc<HarnessStreamHub>)`。
  - `session_event(ctx, event_type, payload) -> HarnessEventInput`。
  - `#[cfg(test)] events::test_support::RecordingSink`（供后续 Task 复用）。

- [ ] **Step 1: 写失败测试**

在 `events.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use chrono::Utc;
    use serde_json::json;

    fn ctx() -> TurnContext {
        TurnContext::new(
            "ws".to_string(),
            "agent_main".to_string(),
            "agent:ws:agent_main/s1".to_string(),
            "run1".to_string(),
            0,
        )
    }

    #[test]
    fn event_carries_turn_identity_and_schema() {
        let context = ctx();
        let input = session_event(&context, "todo.updated", json!({"todos": []}));
        assert_eq!(input.workspace_id, "ws");
        assert_eq!(input.session_id, "agent:ws:agent_main/s1");
        assert_eq!(input.run_id, Some("run1".to_string()));
        assert_eq!(input.event_type, "todo.updated");
        assert_eq!(input.schema_v, 1);
        assert_eq!(input.execution_mode, ExecutionMode::Production);
        assert_eq!(input.task_id, None);
        assert_eq!(input.cause_id, None);
        assert!(!input.event_id.is_empty());
        assert!(input.occurred_at <= Utc::now());
    }

    #[test]
    fn same_payload_has_stable_digest_but_new_event_id() {
        let context = ctx();
        let a = session_event(&context, "todo.updated", json!({"todos": []}));
        let b = session_event(&context, "todo.updated", json!({"todos": []}));
        assert_eq!(a.payload_digest, b.payload_digest);
        assert_ne!(a.event_id, b.event_id);
    }

    #[tokio::test]
    async fn recording_sink_captures_events() {
        let sink = test_support::RecordingSink::default();
        let context = ctx();
        sink.emit(session_event(&context, "turn.started", json!({})))
            .await
            .unwrap();
        let events = sink.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "turn.started");
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::events::tests -v`

Expected: FAIL，缺少 `SessionEventSink`。

- [ ] **Step 3: 写最小实现**

在 `events.rs` 中写：

```rust
//! SessionEvent — 唯一 UI 时间线的写入面。
//!
//! 复用 2026-11-03 的 `session_events` 表（每 workspace Harness DB）。本模块只
//! 负责：生成 `HarnessEventInput`、写入 DB、唤醒 SSE tail。事件名必须来自
//! spec §5.4.3 白名单。

use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent, HarnessEventInput};
use tinyiothub_storage::{Db, DbError};
use uuid::Uuid;

use super::context::TurnContext;
use crate::domains::agent::runtime::harness_stream::HarnessStreamHub;

pub const SESSION_EVENT_SCHEMA_V: i64 = 1;

#[async_trait::async_trait]
pub trait SessionEventSink: Send + Sync {
    async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError>;
}

pub struct DbSessionEventSink {
    db: Arc<Db>,
    hub: Arc<HarnessStreamHub>,
}

impl DbSessionEventSink {
    pub fn new(db: Arc<Db>, hub: Arc<HarnessStreamHub>) -> Self {
        Self { db, hub }
    }
}

#[async_trait::async_trait]
impl SessionEventSink for DbSessionEventSink {
    async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError> {
        let event = self.db.append_session_event(&input).await?;
        self.hub.notify(&input.workspace_id);
        Ok(event)
    }
}

/// 构造一条 Session 事件。`event_id` 每次新生成；同一 payload 的 digest 稳定，
/// 因此重放/重试不会产生冲突（DB 以 event_id 幂等）。
pub fn session_event(
    ctx: &TurnContext,
    event_type: &str,
    payload: serde_json::Value,
) -> HarnessEventInput {
    HarnessEventInput {
        event_id: Uuid::new_v4().to_string(),
        workspace_id: ctx.workspace_id.clone(),
        session_id: ctx.session_key.clone(),
        execution_mode: ExecutionMode::Production,
        run_id: Some(ctx.run_id.clone()),
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: Utc::now(),
        cause_id: None,
        schema_v: SESSION_EVENT_SCHEMA_V,
        payload_digest: payload_digest(&payload),
        payload_json: payload,
    }
}

fn payload_digest(payload: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use tokio::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct RecordingSink {
        pub(crate) events: Mutex<Vec<HarnessEventInput>>,
    }

    #[async_trait::async_trait]
    impl SessionEventSink for RecordingSink {
        async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError> {
            let mut events = self.events.lock().await;
            let event = HarnessEvent {
                seq: events.len() as i64 + 1,
                event_id: input.event_id.clone(),
                workspace_id: input.workspace_id.clone(),
                session_id: input.session_id.clone(),
                execution_mode: input.execution_mode,
                run_id: input.run_id.clone(),
                task_id: input.task_id.clone(),
                event_type: input.event_type.clone(),
                occurred_at: input.occurred_at,
                cause_id: input.cause_id,
                schema_v: input.schema_v,
                payload_digest: input.payload_digest.clone(),
                payload_json: input.payload_json.clone(),
            };
            events.push(input);
            Ok(event)
        }
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod events;

pub use events::*;
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud session::events::tests -v`

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/events.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add session event sink and constructors"
```

---

### Task 5: TodoStore 与 `todo_write`

**Files:**

- Create: `apps/cloud/src/domains/agent/session/todos.rs`
- Create: `apps/cloud/src/domains/agent/session/tools/mod.rs`
- Create: `apps/cloud/src/domains/agent/session/tools/todo_write.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Test: `todos.rs` 与 `tools/todo_write.rs` 的测试模块

**Interfaces:**

- Consumes: `TurnContextRegistry`、`SessionEventSink`、`session_event`、`TodoList`。
- Produces:
  - `TodoStore::replace(session_key, Vec<Todo>) -> Vec<Todo>`、`get(session_key) -> Vec<Todo>`。
  - `TodoWriteTool::new(assistant_id, contexts, store, events)`，工具名 `todo_write`。
  - 事件 `todo.updated { todos }`。

- [ ] **Step 1: 写失败测试**

在 `todos.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::types::{Todo, TodoStatus};

    #[test]
    fn replace_and_get_are_session_scoped() {
        let store = TodoStore::default();
        let todo = Todo {
            content: "检查离线事件".into(),
            status: TodoStatus::Pending,
            active_form: "检查离线事件".into(),
        };
        store.replace("s1", vec![todo.clone()]);
        assert_eq!(store.get("s1"), vec![todo]);
        assert!(store.get("s2").is_empty());
    }
}
```

在 `tools/todo_write.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn todo_write_persists_and_emits() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let store = Arc::new(TodoStore::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = TodoWriteTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            store.clone(),
            sink.clone(),
        );
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool
            .execute(json!({
                "todos": [{
                    "content": "检查离线事件",
                    "status": "in_progress",
                    "activeForm": "检查离线事件"
                }]
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(store.get("agent:ws:agent_main/s1").len(), 1);
        let events = sink.events.lock().await;
        assert_eq!(events.last().unwrap().event_type, "todo.updated");
        assert_eq!(events.last().unwrap().payload_json["todos"][0]["content"], "检查离线事件");
    }

    #[tokio::test]
    async fn todo_write_rejects_two_in_progress_without_mutating_store() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let store = Arc::new(TodoStore::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = TodoWriteTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            store.clone(),
            sink,
        );
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool
            .execute(json!({
                "todos": [
                    {"content": "a", "status": "in_progress", "activeForm": "a"},
                    {"content": "b", "status": "in_progress", "activeForm": "b"}
                ]
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(store.get("agent:ws:agent_main/s1").is_empty());
    }

    #[tokio::test]
    async fn todo_write_fails_without_turn_context() {
        let tool = TodoWriteTool::new(
            "agent_main".to_string(),
            Arc::new(TurnContextRegistry::default()),
            Arc::new(TodoStore::default()),
            Arc::new(RecordingSink::default()),
        );
        let result = tool
            .execute(json!({"todos": [{"content": "a", "status": "pending", "activeForm": "a"}]}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("turn context"));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::todos::tests -v && cargo test -p tinyiothub-cloud session::tools::todo_write::tests -v`

Expected: FAIL，缺少模块/类型。

- [ ] **Step 3: 写最小实现**

`todos.rs`：

```rust
//! TodoStore — 当前会话的 Todo 列表缓存。
//!
//! DB 里的 `todo.updated` 事件是真相；这里只保存最近一次写入，供同进程内的
//! `/sessions/{key}/todos` 快速读取。重启后由事件重放恢复。

use std::sync::Arc;

use dashmap::DashMap;

use super::types::Todo;

#[derive(Default)]
pub struct TodoStore {
    by_session: DashMap<String, Vec<Todo>>,
}

impl TodoStore {
    pub fn replace(&self, session_key: &str, todos: Vec<Todo>) -> Vec<Todo> {
        self.by_session.insert(session_key.to_string(), todos.clone());
        todos
    }

    pub fn get(&self, session_key: &str) -> Vec<Todo> {
        self.by_session
            .get(session_key)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }
}

pub type SharedTodoStore = Arc<TodoStore>;
```

`tools/mod.rs`（本 Task 只声明已存在的 `todo_write`；`ask_user`/`spawn_subagent` 分别在 Task 6/7 追加）：

```rust
pub mod todo_write;

pub use todo_write::TodoWriteTool;
```

`tools/todo_write.rs`：

```rust
use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::TodoList;

pub struct TodoWriteTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    store: Arc<TodoStore>,
    events: Arc<dyn SessionEventSink>,
}

impl TodoWriteTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        store: Arc<TodoStore>,
        events: Arc<dyn SessionEventSink>,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            store,
            events,
        }
    }

    async fn run(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return Ok(failed(format!(
                "turn context missing for assistant {}",
                self.assistant_id
            )));
        };
        let list: TodoList = serde_json::from_value(args)
            .map_err(|error| anyhow::anyhow!("invalid todo list: {error}"))?;
        if let Err(message) = list.validate() {
            return Ok(failed(message));
        }
        let todos = self.store.replace(&context.session_key, list.todos.clone());
        self.events
            .emit(session_event(&context, "todo.updated", json!({ "todos": &todos })))
            .await?;
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string(&json!({ "ok": true, "todos": todos }))?,
            error: None,
            attachments: vec![],
        })
    }
}

fn failed(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        attachments: vec![],
    }
}

impl Attributable for TodoWriteTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }

    fn alias(&self) -> &str {
        "todo_write"
    }
}

#[async_trait::async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Create or replace the session todo list. Use it to plan multi-step work and keep exactly one item in_progress."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 50,
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
                            "activeForm": {"type": "string"}
                        },
                        "required": ["content", "status", "activeForm"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod todos;
pub mod tools;

pub use todos::TodoStore;
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::todos::tests -v
cargo test -p tinyiothub-cloud session::tools::todo_write::tests -v
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/todos.rs \
  apps/cloud/src/domains/agent/session/tools/mod.rs \
  apps/cloud/src/domains/agent/session/tools/todo_write.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add todo_write primitive"
```

---

### Task 6: QuestionBroker 与 `ask_user`

**Files:**

- Create: `apps/cloud/src/domains/agent/session/questions.rs`
- Create: `apps/cloud/src/domains/agent/session/tools/ask_user.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`（导出 `QuestionBroker`、`ASK_USER_TIMEOUT`）
- Modify: `apps/cloud/src/domains/agent/session/tools/mod.rs`（追加 `ask_user`）
- Test: `questions.rs` 与 `tools/ask_user.rs` 的测试模块

**Interfaces:**

- Consumes: `Question`、`QuestionAnswer`、`QuestionResponse`、`TurnContextRegistry`、`SessionEventSink`。
- Produces:
  - `QuestionBroker::register(session_key) -> (question_id, oneshot::Receiver<QuestionAnswer>)`。
  - `QuestionBroker::answer(session_key, question_id, QuestionAnswer) -> Result<(), QuestionError>`。
  - `QuestionBroker::cancel(question_id)`。
  - `AskUserTool::new(assistant_id, contexts, broker, events, timeout)`；工具名 `ask_user`。
  - 事件 `question.asked { questionId, questions }`、`question.answered { questionId, answers, timedOut }`（payload 键统一 camelCase，与现有 `session_events` 一致）。
  - `pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(10 * 60);`

- [ ] **Step 1: 写失败测试**

在 `questions.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::types::{QuestionAnswer, QuestionResponse};

    #[tokio::test]
    async fn answer_resolves_registered_question() {
        let broker = QuestionBroker::default();
        let (id, rx) = broker.register("agent:ws:a/s1");
        let answer = QuestionAnswer {
            answers: vec![QuestionResponse {
                question_index: 0,
                selected: vec!["全部".into()],
                text: None,
            }],
        };
        broker.answer("agent:ws:a/s1", &id, answer.clone()).unwrap();
        assert_eq!(rx.await.unwrap(), answer);
    }

    #[test]
    fn wrong_session_cannot_answer() {
        let broker = QuestionBroker::default();
        let (id, _rx) = broker.register("agent:ws:a/s1");
        let error = broker
            .answer("agent:ws:a/s2", &id, QuestionAnswer { answers: vec![] })
            .unwrap_err();
        assert!(matches!(error, QuestionError::SessionMismatch));
    }

    #[test]
    fn unknown_question_is_not_found() {
        let broker = QuestionBroker::default();
        let error = broker
            .answer("agent:ws:a/s1", "missing", QuestionAnswer { answers: vec![] })
            .unwrap_err();
        assert!(matches!(error, QuestionError::NotFound));
    }
}
```

在 `tools/ask_user.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::questions::QuestionBroker;
    use crate::domains::agent::session::types::{QuestionAnswer, QuestionResponse};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    fn args() -> serde_json::Value {
        json!({
            "questions": [{
                "question": "要检查哪些设备？",
                "header": "检查范围",
                "options": [
                    {"label": "全部", "description": "检查全部设备"},
                    {"label": "仅离线", "description": "只检查离线设备"}
                ],
                "multiSelect": false
            }]
        })
    }

    #[tokio::test]
    async fn ask_user_blocks_until_answered() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let broker = Arc::new(QuestionBroker::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = Arc::new(AskUserTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            broker.clone(),
            sink.clone(),
            Duration::from_secs(5),
        ));
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let handle = tokio::spawn({
            let tool = tool.clone();
            async move { tool.execute(args()).await }
        });

        // 等 question.asked 落进 sink，再回答。
        let mut question_id = None;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let events = sink.events.lock().await;
            if let Some(event) = events.iter().find(|event| event.event_type == "question.asked") {
                question_id = event.payload_json["questionId"].as_str().map(ToString::to_string);
                break;
            }
        }
        let question_id = question_id.expect("question.asked emitted");
        broker
            .answer(
                "agent:ws:agent_main/s1",
                &question_id,
                QuestionAnswer {
                    answers: vec![QuestionResponse {
                        question_index: 0,
                        selected: vec!["仅离线".into()],
                        text: None,
                    }],
                },
            )
            .unwrap();

        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
        assert!(result.output.contains("仅离线"));
        let events = sink.events.lock().await;
        assert!(events.iter().any(|event| event.event_type == "question.answered"));
    }

    #[tokio::test]
    async fn ask_user_times_out_without_answer() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let tool = AskUserTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            Arc::new(QuestionBroker::default()),
            Arc::new(RecordingSink::default()),
            Duration::from_millis(10),
        );
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool.execute(args()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("未在时限内回答"));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::questions::tests -v && cargo test -p tinyiothub-cloud session::tools::ask_user::tests -v`

Expected: FAIL，缺少 `QuestionBroker` / `AskUserTool`。

- [ ] **Step 3: 写最小实现**

`questions.rs`：

```rust
//! QuestionBroker — 把 `ask_user` 的阻塞等待与 HTTP 回答端点解耦。
//!
//! 工具注册一个 oneshot，事件 `question.asked` 把 question_id 送到前端；
//! 回答端点按 session_key 校验后 resolve。

use std::sync::Arc;

use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::oneshot;

use super::types::QuestionAnswer;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuestionError {
    #[error("question not found")]
    NotFound,
    #[error("question belongs to another session")]
    SessionMismatch,
    #[error("question is no longer waiting for an answer")]
    Closed,
}

struct PendingQuestion {
    session_key: String,
    tx: oneshot::Sender<QuestionAnswer>,
}

#[derive(Default)]
pub struct QuestionBroker {
    pending: DashMap<String, PendingQuestion>,
}

impl QuestionBroker {
    pub fn register(&self, session_key: &str) -> (String, oneshot::Receiver<QuestionAnswer>) {
        let question_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(
            question_id.clone(),
            PendingQuestion {
                session_key: session_key.to_string(),
                tx,
            },
        );
        (question_id, rx)
    }

    pub fn answer(
        &self,
        session_key: &str,
        question_id: &str,
        answer: QuestionAnswer,
    ) -> Result<(), QuestionError> {
        let (_, pending) = self
            .pending
            .remove(question_id)
            .ok_or(QuestionError::NotFound)?;
        if pending.session_key != session_key {
            self.pending.insert(question_id.to_string(), pending);
            return Err(QuestionError::SessionMismatch);
        }
        pending.tx.send(answer).map_err(|_| QuestionError::Closed)
    }

    pub fn cancel(&self, question_id: &str) {
        self.pending.remove(question_id);
    }
}

pub type SharedQuestionBroker = Arc<QuestionBroker>;
```

`tools/ask_user.rs`：

```rust
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tokio::time::timeout;

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::questions::QuestionBroker;
use crate::domains::agent::session::types::{Question, QuestionAnswer};

pub struct AskUserTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    broker: Arc<QuestionBroker>,
    events: Arc<dyn SessionEventSink>,
    timeout: Duration,
}

impl AskUserTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        broker: Arc<QuestionBroker>,
        events: Arc<dyn SessionEventSink>,
        timeout: Duration,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            broker,
            events,
            timeout,
        }
    }

    async fn run(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return Ok(failed(format!(
                "turn context missing for assistant {}",
                self.assistant_id
            )));
        };
        let questions: Vec<Question> = match args.get("questions").cloned() {
            Some(value) => match serde_json::from_value(value) {
                Ok(questions) => questions,
                Err(error) => return Ok(failed(format!("invalid questions: {error}"))),
            },
            None => return Ok(failed("missing questions")),
        };
        if let Err(message) = Question::validate_set(&questions) {
            return Ok(failed(message));
        }

        let (question_id, receiver) = self.broker.register(&context.session_key);
        if let Err(error) = self
            .events
            .emit(session_event(
                &context,
                "question.asked",
                json!({
                    "questionId": question_id.clone(),
                    "questions": questions.clone()
                }),
            ))
            .await
        {
            self.broker.cancel(&question_id);
            return Err(error.into());
        }

        match timeout(self.timeout, receiver).await {
            Ok(Ok(answer)) => {
                let output = format_answers(&questions, &answer);
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question.answered",
                        json!({
                            "questionId": question_id,
                            "answers": answer.answers,
                            "timedOut": false
                        }),
                    ))
                    .await;
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                    attachments: vec![],
                })
            }
            Ok(Err(_)) => {
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question.answered",
                        json!({
                            "questionId": question_id,
                            "answers": [],
                            "timedOut": false
                        }),
                    ))
                    .await;
                Ok(failed("用户未回答（连接已关闭）"))
            }
            Err(_) => {
                self.broker.cancel(&question_id);
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question.answered",
                        json!({
                            "questionId": question_id,
                            "answers": [],
                            "timedOut": true
                        }),
                    ))
                    .await;
                Ok(failed("用户未在时限内回答"))
            }
        }
    }
}

fn failed(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        attachments: vec![],
    }
}

fn format_answers(questions: &[Question], answer: &QuestionAnswer) -> String {
    let mut output = String::from("用户回答：\n");
    for (index, question) in questions.iter().enumerate() {
        let response = answer
            .answers
            .iter()
            .find(|response| response.question_index == index);
        let selected = response
            .map(|response| response.selected.join("、"))
            .filter(|selected| !selected.is_empty())
            .unwrap_or_else(|| "（未选择）".to_string());
        output.push_str(&format!("{}. {}\n   选择：{}\n", index + 1, question.question, selected));
        if let Some(text) = response.and_then(|response| response.text.as_deref())
            && !text.trim().is_empty()
        {
            output.push_str(&format!("   补充：{}\n", text.trim()));
        }
    }
    output
}

impl Attributable for AskUserTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }

    fn alias(&self) -> &str {
        "ask_user"
    }
}

#[async_trait::async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user 1-4 blocking questions with 2-4 options each. Use this when a decision cannot be inferred safely."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string"},
                            "header": {"type": "string", "maxLength": 12},
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {"type": "string"},
                                        "description": {"type": "string"}
                                    },
                                    "required": ["label", "description"]
                                }
                            },
                            "multiSelect": {"type": "boolean"}
                        },
                        "required": ["question", "header", "options"]
                    }
                }
            },
            "required": ["questions"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}
```

在 `session/mod.rs` 顶部增加：

```rust
use std::time::Duration;

pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const SESSION_TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub mod questions;

pub use questions::QuestionBroker;
```

在 `session/tools/mod.rs` 追加：

```rust
pub mod ask_user;

pub use ask_user::AskUserTool;
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::questions::tests -v
cargo test -p tinyiothub-cloud session::tools::ask_user::tests -v
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/questions.rs \
  apps/cloud/src/domains/agent/session/tools/ask_user.rs \
  apps/cloud/src/domains/agent/session/tools/mod.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add ask_user primitive"
```

---

### Task 7: Subagent 类型与 `spawn_subagent` 工具

**Files:**

- Create: `apps/cloud/src/domains/agent/session/subagent.rs`
- Create: `apps/cloud/src/domains/agent/session/tools/spawn_subagent.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Modify: `apps/cloud/src/domains/agent/session/tools/mod.rs`（追加 `spawn_subagent`）
- Test: `subagent.rs` 与 `tools/spawn_subagent.rs` 的测试模块

**Interfaces:**

- Consumes: `TurnContextRegistry`、`SessionEventSink`、`ToolKind::SpawnSubagent`（已存在）。
- Produces:
  - `SubagentStatus::{Completed, Failed, Cancelled}`。
  - `SubagentRequest { workspace_id, parent_agent_id, parent_session_key, subagent_id, subagent_type, description, prompt, depth }`。
  - `SubagentReport { subagent_id, status, summary, tool_calls, evidence_refs, recommended_actions }`。
  - `#[async_trait] trait SubagentRunner: run(SubagentRequest) -> Result<SubagentReport, String>`。
  - `SubagentSlot::{set, get}`（`parking_lot::RwLock` 内部可变）。
  - `SpawnSubagentTool::new(assistant_id, contexts, subagents, events)`；工具名 `spawn_subagent`。
  - 工具参数 `description` / `prompt` / `subagent_type`（可选，默认 `general-purpose`）。
  - 事件 `subagent.started { subagentId, description, subagentType }`、`subagent.completed { subagentId, status, summary, toolCalls, evidenceRefs, recommendedActions }`（payload 键 camelCase）。
  - 深度限制：`ctx.depth >= 1` 直接拒绝，不派生子代理。

- [ ] **Step 1: 写失败测试**

在 `subagent.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_starts_empty_and_accepts_runner() {
        let slot = SubagentSlot::default();
        assert!(slot.get().is_none());
        let runner: Arc<dyn SubagentRunner> = Arc::new(super::tests::StubRunner);
        slot.set(runner);
        assert!(slot.get().is_some());
    }

    pub(super) struct StubRunner;

    #[async_trait::async_trait]
    impl SubagentRunner for StubRunner {
        async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
            Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: "subagent done".to_string(),
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            })
        }
    }
}
```

在 `tools/spawn_subagent.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::{TurnContext, TurnGuard};
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::subagent::{
        SubagentReport, SubagentRequest, SubagentRunner, SubagentSlot, SubagentStatus,
    };
    use serde_json::json;
    use std::sync::Arc;

    struct StubRunner;

    #[async_trait::async_trait]
    impl SubagentRunner for StubRunner {
        async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
            assert_eq!(request.prompt, "检查全部设备");
            assert_eq!(request.subagent_type, "general-purpose");
            Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: "检查完成：无异常".to_string(),
                tool_calls: 3,
                evidence_refs: vec!["event:42".to_string()],
                recommended_actions: vec![],
            })
        }
    }

    fn make_tool(
        sink: Arc<RecordingSink>,
        contexts: Arc<TurnContextRegistry>,
        depth: u8,
    ) -> (SpawnSubagentTool, TurnGuard) {
        let slot = Arc::new(SubagentSlot::default());
        slot.set(Arc::new(StubRunner));
        let tool = SpawnSubagentTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            slot,
            sink,
        );
        let guard = contexts.set(TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            depth,
        ));
        (tool, guard)
    }

    #[tokio::test]
    async fn spawn_returns_report_and_emits_lifecycle() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let (tool, _guard) = make_tool(sink.clone(), contexts, 0);
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("检查完成"));
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, vec!["subagent.started", "subagent.completed"]);
        assert_eq!(events[1].payload_json["toolCalls"], 3);
    }

    #[tokio::test]
    async fn spawn_rejects_second_level() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let (tool, _guard) = make_tool(sink.clone(), contexts, 1);
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("depth"));
        assert!(sink.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn spawn_fails_when_runner_not_wired() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let tool = SpawnSubagentTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            Arc::new(SubagentSlot::default()),
            Arc::new(RecordingSink::default()),
        );
        let _guard = contexts.set(TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        ));
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not configured"));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::subagent::tests -v && cargo test -p tinyiothub-cloud session::tools::spawn_subagent::tests -v`

Expected: FAIL，缺少 `SubagentSlot` / `SpawnSubagentTool`。

- [ ] **Step 3: 写最小实现**

`subagent.rs`：

```rust
//! Subagent — Turn 级同步子代理。
//!
//! 子代理拥有隔离的上下文，执行完只把最终报告返回父 Turn；不创建独立 Task。
//! 只允许一层：父 Turn depth=0，子代理 depth=1；depth>=1 的父 Turn 不得再派生。

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct SubagentRequest {
    pub workspace_id: String,
    pub parent_agent_id: String,
    pub parent_session_key: String,
    pub subagent_id: String,
    pub subagent_type: String,
    pub description: String,
    pub prompt: String,
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentReport {
    pub subagent_id: String,
    pub status: SubagentStatus,
    pub summary: String,
    pub tool_calls: u32,
    /// 子代理引用的证据（事件 id / 设备 id / 媒体 ref）。
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// 子代理建议的后续动作；Plan 1 允许为空。
    #[serde(default)]
    pub recommended_actions: Vec<String>,
}

#[async_trait::async_trait]
pub trait SubagentRunner: Send + Sync {
    async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String>;
}

#[derive(Default)]
pub struct SubagentSlot {
    inner: RwLock<Option<Arc<dyn SubagentRunner>>>,
}

impl SubagentSlot {
    pub fn set(&self, runner: Arc<dyn SubagentRunner>) {
        *self.inner.write() = Some(runner);
    }

    pub fn get(&self) -> Option<Arc<dyn SubagentRunner>> {
        self.inner.read().clone()
    }
}
```

`tools/spawn_subagent.rs`：

```rust
use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::subagent::{
    SubagentRequest, SubagentSlot, SubagentStatus,
};

pub struct SpawnSubagentTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    subagents: Arc<SubagentSlot>,
    events: Arc<dyn SessionEventSink>,
}

impl SpawnSubagentTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        subagents: Arc<SubagentSlot>,
        events: Arc<dyn SessionEventSink>,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            subagents,
            events,
        }
    }

    async fn run(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return Ok(failed(format!(
                "turn context missing for assistant {}",
                self.assistant_id
            )));
        };
        if context.depth >= 1 {
            return Ok(failed("subagent depth exceeded: only one level is allowed"));
        }
        let description = args
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or("subagent task")
            .to_string();
        let subagent_type = args
            .get("subagent_type")
            .and_then(|value| value.as_str())
            .unwrap_or("general-purpose")
            .to_string();
        let Some(prompt) = args.get("prompt").and_then(|value| value.as_str()) else {
            return Ok(failed("missing prompt"));
        };
        let prompt = prompt.to_string();
        let Some(runner) = self.subagents.get() else {
            return Ok(failed("subagent runner not configured"));
        };

        let subagent_id = uuid::Uuid::new_v4().to_string();
        self.events
            .emit(session_event(
                &context,
                "subagent.started",
                json!({
                    "subagentId": subagent_id.clone(),
                    "description": description.clone(),
                    "subagentType": subagent_type.clone()
                }),
            ))
            .await?;

        let report = match runner
            .run(SubagentRequest {
                workspace_id: context.workspace_id.clone(),
                parent_agent_id: context.assistant_id.clone(),
                parent_session_key: context.session_key.clone(),
                subagent_id: subagent_id.clone(),
                subagent_type,
                description,
                prompt,
                depth: context.depth + 1,
            })
            .await
        {
            Ok(report) => report,
            Err(error) => {
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "subagent.completed",
                        json!({
                            "subagentId": subagent_id,
                            "status": "failed",
                            "summary": "",
                            "toolCalls": 0,
                            "error": &error
                        }),
                    ))
                    .await;
                return Ok(failed(format!("subagent failed: {error}")));
            }
        };

        self.events
            .emit(session_event(
                &context,
                "subagent.completed",
                json!({
                    "subagentId": report.subagent_id.clone(),
                    "status": report.status,
                    "summary": report.summary.clone(),
                    "toolCalls": report.tool_calls,
                    "evidenceRefs": report.evidence_refs.clone(),
                    "recommendedActions": report.recommended_actions.clone()
                }),
            ))
            .await?;

        Ok(ToolResult {
            success: report.status == SubagentStatus::Completed,
            output: serde_json::to_string(&report)?,
            error: (report.status != SubagentStatus::Completed)
                .then(|| format!("subagent status: {:?}", report.status)),
            attachments: vec![],
        })
    }
}

fn failed(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        attachments: vec![],
    }
}

impl Attributable for SpawnSubagentTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::SpawnSubagent)
    }

    fn alias(&self) -> &str {
        "spawn_subagent"
    }
}

#[async_trait::async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Run a synchronous subagent with isolated context and return its final report. Use for focused, parallelizable investigation. One level only."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "description": {"type": "string", "description": "Short task label"},
                "prompt": {"type": "string", "description": "Self-contained task for the subagent"},
                "subagent_type": {
                    "type": "string",
                    "description": "Focus label, e.g. general-purpose or Explore",
                    "default": "general-purpose"
                }
            },
            "required": ["description", "prompt"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod subagent;

pub use subagent::{SubagentReport, SubagentRequest, SubagentRunner, SubagentSlot, SubagentStatus};
```

在 `session/tools/mod.rs` 追加：

```rust
pub mod spawn_subagent;

pub use spawn_subagent::SpawnSubagentTool;
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::subagent::tests -v
cargo test -p tinyiothub-cloud session::tools::spawn_subagent::tests -v
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/subagent.rs \
  apps/cloud/src/domains/agent/session/tools/spawn_subagent.rs \
  apps/cloud/src/domains/agent/session/tools/mod.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add spawn_subagent primitive"
```

---

### Task 8: SessionRunner

**Files:**

- Create: `apps/cloud/src/domains/agent/session/runner.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Test: `runner.rs` 的测试模块

**Interfaces:**

- Consumes: `AgentLoop`、`TurnEvent`、`CancellationToken`、`SessionEventSink`、`TurnContextRegistry`。
- Produces:
  - `TurnObserver` trait：`async fn on_turn_event(&self, event: &TurnEvent)`。
  - `TurnOutcome::{Completed { text, tool_calls }, Cancelled, Failed { error }}`。
  - `SessionRunner::new(events, contexts)`。
  - `SessionRunner::run_turn(agent, ctx, message, observer, cancel) -> TurnOutcome`。
  - 事件序列：`turn.started` → `tool.called`/`tool.returned`（按 TurnEvent）→ `message.created`（assistant）→ `turn.completed`。

- [ ] **Step 1: 写失败测试**

在 `runner.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tinyiothub_agent::port::provider::ChatMessage;

    struct FakeLoop;

    #[async_trait::async_trait]
    impl AgentLoop for FakeLoop {
        async fn turn_streamed(
            &self,
            _message: &str,
            tx: tokio::sync::mpsc::Sender<TurnEvent>,
            _cancel: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            tx.send(TurnEvent::Chunk { delta: "hi".into() }).await.unwrap();
            tx.send(TurnEvent::ToolCall {
                id: "call1".into(),
                name: "todo_write".into(),
                args: json!({"todos": []}),
            })
            .await
            .unwrap();
            tx.send(TurnEvent::ToolResult {
                id: "call1".into(),
                name: "todo_write".into(),
                output: "ok".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
            Ok("done".into())
        }

        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingObserver {
        events: Mutex<Vec<TurnEvent>>,
    }

    #[async_trait::async_trait]
    impl TurnObserver for RecordingObserver {
        async fn on_turn_event(&self, event: &TurnEvent) {
            self.events.lock().await.push(event.clone());
        }
    }

    #[tokio::test]
    async fn run_turn_persists_lifecycle_and_tools() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = SessionRunner::new(sink.clone(), contexts);
        let observer = Arc::new(RecordingObserver::default());
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );

        let outcome = runner
            .run_turn(&FakeLoop, &ctx, "hello", Some(observer.clone()), None)
            .await;

        assert_eq!(
            outcome,
            TurnOutcome::Completed {
                text: "done".into(),
                tool_calls: 1
            }
        );
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "turn.started",
                "tool.called",
                "tool.returned",
                "message.created",
                "turn.completed"
            ]
        );
        assert_eq!(events[4].payload_json["outcome"], "completed");
        assert_eq!(observer.events.lock().await.len(), 3);
        assert!(contexts.get("agent_main").is_none(), "guard clears turn context");
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::runner::tests -v`

Expected: FAIL，缺少 `SessionRunner`。

- [ ] **Step 3: 写最小实现**

`runner.rs`：

```rust
//! SessionRunner — 一次 Assistant Turn 的唯一执行入口。
//!
//! 它只负责：安装 TurnContext、驱动 AgentLoop、把 TurnEvent 写入 session_events、
//! 通知可选 observer（聊天 SSE 适配器）、收尾。消息持久化/反思仍由 chat 组合层
//! 负责，避免 SessionRunner 依赖 chat 的存储历史。

use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::runtime::AgentLoop;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::context::{TurnContext, TurnContextRegistry};
use super::events::{SessionEventSink, session_event};

#[async_trait::async_trait]
pub trait TurnObserver: Send + Sync {
    async fn on_turn_event(&self, event: &TurnEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed { text: String, tool_calls: u32 },
    Cancelled,
    Failed { error: String },
}

pub struct SessionRunner {
    events: Arc<dyn SessionEventSink>,
    contexts: Arc<TurnContextRegistry>,
}

/// Accumulates per-model-call usage into one `turn.completed` payload.
#[derive(Default)]
struct UsageTotals {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    seen: bool,
}

impl UsageTotals {
    fn add(
        &mut self,
        input_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_usd: Option<f64>,
    ) {
        self.seen = true;
        self.input_tokens += input_tokens.unwrap_or(0);
        self.cached_input_tokens += cached_input_tokens.unwrap_or(0);
        self.output_tokens += output_tokens.unwrap_or(0);
        self.cost_usd += cost_usd.unwrap_or(0.0);
    }

    fn to_json(&self) -> serde_json::Value {
        if !self.seen {
            return serde_json::Value::Null;
        }
        json!({
            "inputTokens": self.input_tokens,
            "cachedInputTokens": self.cached_input_tokens,
            "outputTokens": self.output_tokens,
            "costUsd": self.cost_usd
        })
    }
}

impl SessionRunner {
    pub fn new(events: Arc<dyn SessionEventSink>, contexts: Arc<TurnContextRegistry>) -> Self {
        Self { events, contexts }
    }

    pub async fn run_turn(
        &self,
        agent: &dyn AgentLoop,
        context: &TurnContext,
        message: &str,
        observer: Option<Arc<dyn TurnObserver>>,
        cancel: Option<CancellationToken>,
    ) -> TurnOutcome {
        let _guard = self.contexts.set(context.clone());
        self.emit(context, "turn.started", json!({ "turnId": &context.turn_id }))
            .await;

        let (event_tx, mut event_rx) = mpsc::channel::<TurnEvent>(64);
        let forward = tokio::spawn({
            let events = self.events.clone();
            let context = context.clone();
            let observer = observer.clone();
            async move {
                let mut tool_calls = 0u32;
                let mut usage = UsageTotals::default();
                while let Some(event) = event_rx.recv().await {
                    match &event {
                        TurnEvent::ToolCall { id, name, args } => {
                            tool_calls += 1;
                            let _ = events
                                .emit(session_event(
                                    &context,
                                    "tool.called",
                                    json!({ "toolCallId": id, "tool": name, "args": args }),
                                ))
                                .await;
                        }
                        TurnEvent::ToolResult {
                            id,
                            name,
                            output,
                            attachments,
                        } => {
                            let evidence: Vec<tinyiothub_core::media::EvidenceRef> =
                                attachments.iter().map(Into::into).collect();
                            let _ = events
                                .emit(session_event(
                                    &context,
                                    "tool.returned",
                                    json!({
                                        "toolCallId": id,
                                        "tool": name,
                                        "result": truncate(output, 2000),
                                        "evidence": evidence
                                    }),
                                ))
                                .await;
                        }
                        TurnEvent::Usage {
                            input_tokens,
                            cached_input_tokens,
                            output_tokens,
                            cost_usd,
                        } => usage.add(
                            *input_tokens,
                            *cached_input_tokens,
                            *output_tokens,
                            *cost_usd,
                        ),
                        _ => {}
                    }
                    if let Some(observer) = &observer {
                        observer.on_turn_event(&event).await;
                    }
                }
                (tool_calls, usage.to_json())
            }
        });

        let result = agent.turn_streamed(message, event_tx, cancel.clone()).await;
        let (tool_calls, usage) = forward.await.unwrap_or((0, serde_json::Value::Null));

        match result {
            Ok(text) => {
                self.emit(
                    context,
                    "message.created",
                    json!({
                        "messageId": uuid::Uuid::new_v4().to_string(),
                        "role": "assistant",
                        "text": &text
                    }),
                )
                .await;
                self.emit(
                    context,
                    "turn.completed",
                    json!({
                        "turnId": &context.turn_id,
                        "outcome": "completed",
                        "text": &text,
                        "toolCalls": tool_calls,
                        "usage": &usage
                    }),
                )
                .await;
                TurnOutcome::Completed { text, tool_calls }
            }
            Err(error) => {
                let cancelled = cancel.as_ref().is_some_and(|token| token.is_cancelled());
                let error = error.to_string();
                self.emit(
                    context,
                    "turn.completed",
                    json!({
                        "turnId": &context.turn_id,
                        "outcome": if cancelled { "cancelled" } else { "failed" },
                        "error": &error,
                        "toolCalls": tool_calls,
                        "usage": &usage
                    }),
                )
                .await;
                if cancelled {
                    TurnOutcome::Cancelled
                } else {
                    TurnOutcome::Failed { error }
                }
            }
        }
    }

    async fn emit(&self, context: &TurnContext, event_type: &str, payload: serde_json::Value) {
        if let Err(error) = self.events.emit(session_event(context, event_type, payload)).await {
            tracing::warn!(%error, event_type, "session event write failed");
        }
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod runner;

pub use runner::{SessionRunner, TurnObserver, TurnOutcome};
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud session::runner::tests -v`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/runner.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add unified session turn runner"
```

---

### Task 9: PoolSubagentRunner（组合层实现）

**Files:**

- Create: `apps/cloud/src/domains/agent/host/chat/subagent.rs`
- Modify: `apps/cloud/src/domains/agent/host/chat/mod.rs`（`pub mod subagent;`）
- Modify: `apps/cloud/src/domains/agent/pool/pool.rs`（`pub fn agent_config`）
- Test: `apps/cloud/src/domains/agent/host/chat/subagent.rs` 的测试模块

**Interfaces:**

- Consumes: `AgentPool`、`SessionRunner`、`TurnContextRegistry`、`AgentRuntimeConfig`、`SubagentRunner`。
- Produces: `PoolSubagentRunner::new(pool: Arc<AgentPool>, runner: Arc<SessionRunner>)`，实现 `SubagentRunner`。

- [ ] **Step 1: 写失败测试**

在 `host/chat/subagent.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::{
        SessionRunner, SubagentRequest, SubagentRunner, SubagentStatus, TurnContextRegistry,
    };
    use std::sync::Arc;
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ProviderKind, Role};
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;

    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("subagent done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(
                tinyiothub_agent::port::attribution::ModelProviderKind::Custom,
            ))
        }

        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    #[tokio::test]
    async fn pool_subagent_runs_and_reports() {
        let pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
            )
            .unwrap(),
        );
        let config = AgentRuntimeConfig::default();
        let _parent = pool
            .create("agent_main", "ws-test", &config, vec![])
            .unwrap();
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink, contexts));
        let subagents = PoolSubagentRunner::new(pool.clone(), runner);

        let report = subagents
            .run(SubagentRequest {
                workspace_id: "ws-test".into(),
                parent_agent_id: "agent_main".into(),
                parent_session_key: "agent:ws-test:agent_main/s1".into(),
                subagent_id: "sub1".into(),
                subagent_type: "general-purpose".into(),
                description: "检查".into(),
                prompt: "检查全部设备".into(),
                depth: 1,
            })
            .await
            .unwrap();

        assert_eq!(report.status, SubagentStatus::Completed);
        assert_eq!(report.summary, "subagent done");
        assert_eq!(pool.pool_size(), 1, "child agent must be invalidated");
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud host::chat::subagent::tests -v`

Expected: FAIL，缺少 `PoolSubagentRunner` / `AgentPool::agent_config`。

- [ ] **Step 3: 写最小实现**

在 `apps/cloud/src/domains/agent/pool/pool.rs` 的 `impl AgentPool` 中增加：

```rust
    /// 读取已缓存 Agent 的运行时配置（子代理继承父配置）。
    pub fn agent_config(&self, agent_id: &str) -> Option<AgentRuntimeConfig> {
        self.agents
            .get(agent_id)
            .map(|entry| entry.metadata.config.clone())
    }
```

在 `host/chat/mod.rs` 增加：

```rust
pub mod subagent;
```

`host/chat/subagent.rs`：

```rust
//! PoolSubagentRunner — 用 AgentPool 创建一次性子 Agent，执行后立即清除。
//!
//! 子代理继承父 Agent 的模型配置与工具集，但过滤掉 `ask_user` 与
//! `spawn_subagent`（只允许一层）。工具解析时设置 `agent_id` 与
//! `session_tools_enabled`，让原语工具读得到子 Turn 的上下文。

use std::sync::Arc;

use tinyiothub_agent::port::provider::ChatMessage;

use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::runner::{SessionRunner, TurnOutcome};
use crate::domains::agent::session::subagent::{
    SubagentReport, SubagentRequest, SubagentRunner, SubagentStatus,
};

pub struct PoolSubagentRunner {
    pool: Arc<AgentPool>,
    runner: Arc<SessionRunner>,
}

impl PoolSubagentRunner {
    pub fn new(pool: Arc<AgentPool>, runner: Arc<SessionRunner>) -> Self {
        Self { pool, runner }
    }

    async fn run_inner(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
        let config = self
            .pool
            .agent_config(&request.parent_agent_id)
            .ok_or_else(|| format!("parent agent not found: {}", request.parent_agent_id))?;
        let child_id = format!(
            "{}_sub_{}",
            request.parent_agent_id,
            request.subagent_id.replace('-', "_")
        );

        let mut runtime = self.pool.runtime_context().await;
        runtime.agent_id = Some(child_id.clone());
        runtime.session_tools_enabled = true;
        let tools = self
            .pool
            .tool_registry()
            .resolve_tools_for_agent(
                &config,
                &request.workspace_id,
                self.pool.trust_config(&request.workspace_id),
                &runtime,
            )
            .await
            .into_iter()
            .filter(|tool| !matches!(tool.name(), "ask_user" | "spawn_subagent"))
            .collect();

        let agent = self
            .pool
            .create(&child_id, &request.workspace_id, &config, tools)
            .map_err(|error| error.to_string())?;

        let system_prompt = format!(
            "You are a focused {} subagent. Complete the assigned task and return a concise final report. Never ask the user questions.\n\nTask: {}",
            request.subagent_type, request.description
        );
        // 事件挂在父 Session 下（spec §7.3）；assistant_id 用子代理 id，
        // 让 TurnContextRegistry 与父 Turn 隔离。
        let context = TurnContext::new(
            request.workspace_id.clone(),
            child_id.clone(),
            request.parent_session_key.clone(),
            uuid::Uuid::new_v4().to_string(),
            request.depth,
        );

        let result = {
            let guard = agent.lock().await;
            guard.clear_history().await;
            if let Err(error) = guard
                .seed_history(&[ChatMessage::system(system_prompt)])
                .await
            {
                self.pool.invalidate(&child_id);
                return Err(error.to_string());
            }
            self.runner
                .run_turn(&*guard, &context, &request.prompt, None, None)
                .await
        };

        self.pool.invalidate(&child_id);

        match result {
            TurnOutcome::Completed { text, tool_calls } => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: text,
                tool_calls,
                // Plan 1 的模型只返回自由文本；结构化证据是 Plan 2 的扩展点。
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
            TurnOutcome::Cancelled => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Cancelled,
                summary: "subagent cancelled".to_string(),
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
            TurnOutcome::Failed { error } => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Failed,
                summary: error,
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
        }
    }
}

#[async_trait::async_trait]
impl SubagentRunner for PoolSubagentRunner {
    async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
        self.run_inner(request).await
    }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud host::chat::subagent::tests -v`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/host/chat/subagent.rs \
  apps/cloud/src/domains/agent/host/chat/mod.rs \
  apps/cloud/src/domains/agent/pool/pool.rs
git commit -m "feat(session): add pool-backed subagent runner"
```

---

### Task 10: SessionRuntime 与 AppState 接线

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/mod.rs`（`SessionRuntime`）
- Modify: `apps/cloud/src/domains/agent/mod.rs`（`AgentState.session_runtime`）
- Modify: `apps/cloud/src/state.rs`（`AppState.session_runtime`、`new`、`FromRef`）
- Modify: `apps/cloud/src/domains/agent/tools/context.rs`（`agent_id`、`session_tools_enabled`）
- Test: `session/mod.rs` 的测试模块

**Interfaces:**

- Consumes: `Db`、`HarnessStreamHub`、`SessionRunner`、`TodoStore`、`QuestionBroker`、`SubagentSlot`。
- Produces:
  - `SessionRuntime { events, contexts, todos, questions, subagents, runner }`，`SessionRuntime::new(db, hub)`。
  - `AgentState.session_runtime: Arc<SessionRuntime>`。
  - `AppState.session_runtime: Arc<SessionRuntime>`。
  - `ToolRuntimeContext.agent_id: Option<String>`、`session_tools_enabled: bool`。

- [ ] **Step 1: 写失败测试**

在 `session/mod.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_wires_shared_components() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let hub = Arc::new(crate::domains::agent::runtime::harness_stream::HarnessStreamHub::default());
        let runtime = SessionRuntime::new(db, hub);

        let context = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let guard = runtime.contexts.set(context);
        assert!(runtime.contexts.get("agent_main").is_some());
        drop(guard);
        assert!(runtime.contexts.get("agent_main").is_none());
        assert!(runtime.todos.get("agent:ws:agent_main/s1").is_empty());
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::tests::runtime_wires_shared_components -v`

Expected: FAIL，缺少 `SessionRuntime`。

- [ ] **Step 3: 写最小实现**

在 `session/mod.rs` 追加：

```rust
use std::sync::Arc;

use crate::domains::agent::runtime::harness_stream::HarnessStreamHub;

#[derive(Clone)]
pub struct SessionRuntime {
    pub events: Arc<dyn SessionEventSink>,
    pub contexts: Arc<TurnContextRegistry>,
    pub todos: Arc<TodoStore>,
    pub questions: Arc<QuestionBroker>,
    pub subagents: Arc<SubagentSlot>,
    pub runner: Arc<SessionRunner>,
}

impl SessionRuntime {
    pub fn new(db: Arc<tinyiothub_storage::Db>, hub: Arc<HarnessStreamHub>) -> Self {
        let events: Arc<dyn SessionEventSink> = Arc::new(DbSessionEventSink::new(db, hub));
        let contexts = Arc::new(TurnContextRegistry::default());
        let todos = Arc::new(TodoStore::default());
        let questions = Arc::new(QuestionBroker::default());
        let subagents = Arc::new(SubagentSlot::default());
        let runner = Arc::new(SessionRunner::new(events.clone(), contexts.clone()));
        Self {
            events,
            contexts,
            todos,
            questions,
            subagents,
            runner,
        }
    }
}
```

在 `apps/cloud/src/domains/agent/tools/context.rs` 的 `ToolRuntimeContext` 增加：

```rust
    /// 当前解析工具所属的 assistant/agent id（Session 工具读取 TurnContext 用）。
    pub agent_id: Option<String>,
    /// 只有聊天/子代理 Turn 才暴露 todo_write / ask_user / spawn_subagent。
    pub session_tools_enabled: bool,
```

`#[derive(Clone, Default)]` 会自动给 `None/false`。

在 `apps/cloud/src/domains/agent/mod.rs` 的 `AgentState` 增加：

```rust
    /// 统一 Session 运行时（事件、Todo、待答问题、子代理槽）。
    pub session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
```

在 `apps/cloud/src/state.rs` 的 `AppState` 增加：

```rust
    /// 统一 Session 运行时（事件、Todo、待答问题、子代理槽）。
    pub session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
```

在 `AppState::new` 中，`AgentPool::new(...)` 之后加入：

```rust
        let session_runtime = Arc::new(crate::domains::agent::session::SessionRuntime::new(
            database.clone(),
            harness_stream.clone(),
        ));
```

在 `AppState { ... }` 构造字面量中加入：

```rust
            session_runtime,
```

在 `impl FromRef<AppState> for AgentState` 中加入：

```rust
            session_runtime: state.session_runtime.clone(),
```

> 如果 `harness_stream` 在 `AgentPool::new` 之后才创建，把 `SessionRuntime::new` 移到它之后；两个变量都必须在作用域内。

`apps/cloud/src/main.rs` 与 `apps/cloud/src/router.rs` 里有两处 `ToolRuntimeContext { ... }` 完整字面量，新增字段后必须补默认值：

```rust
                ..crate::domains::agent::tools::ToolRuntimeContext::default()
```

（两处都加在 `tool_policy: None,` 之后。）

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::tests::runtime_wires_shared_components -v
cargo check -p tinyiothub-cloud
```

Expected: 测试 PASS，`cargo check` 无错误。若有 `AppState` 其他构造点，按编译器提示补 `session_runtime`。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/mod.rs \
  apps/cloud/src/state.rs \
  apps/cloud/src/domains/agent/tools/context.rs
git commit -m "feat(session): wire session runtime into app state"
```

---

### Task 11: 把三个原语注入工具 provider

**Files:**

- Modify: `apps/cloud/src/domains/agent/host/tools/service.rs`
- Modify: `apps/cloud/src/shared/service_manager.rs`
- Test: `apps/cloud/src/domains/agent/host/tools/service.rs` 的测试模块

**Interfaces:**

- Consumes: `SessionRuntime`、`ToolRuntimeContext.agent_id/session_tools_enabled`。
- Produces: chat 工具列表新增 `todo_write`、`ask_user`、`spawn_subagent`（仅当 `runtime.session_tools_enabled == true`）。

- [ ] **Step 1: 写失败测试**

在 `host/tools/service.rs` 的测试模块写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_primitives_require_flag_and_agent() {
        let mut runtime =
            crate::domains::agent::tools::context::ToolRuntimeContext::default();
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.agent_id = Some("agent_main".to_string());
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.session_tools_enabled = true;
        assert_eq!(
            session_primitive_agent(&runtime),
            Some("agent_main".to_string())
        );
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud host::tools::service::tests::session_primitives_require_flag_and_agent -v`

Expected: FAIL，缺少 `session_primitive_agent`。

- [ ] **Step 3: 写最小实现**

在 `host/tools/service.rs`：

1. 给 `chat_builtin_tools_provider` 增加最后一个参数：

```rust
    session: std::sync::Arc<crate::domains::agent::session::SessionRuntime>,
```

2. 在 `tools.extend(create_thing_tools(...))` 之后、`DispatchThingTaskTool` 之前加入：

```rust
        if let Some(agent_id) = session_primitive_agent(runtime) {
            use crate::domains::agent::session::tools::{
                AskUserTool, SpawnSubagentTool, TodoWriteTool,
            };
            tools.push((
                Box::new(TodoWriteTool::new(
                    agent_id.clone(),
                    session.contexts.clone(),
                    session.todos.clone(),
                    session.events.clone(),
                )),
                ToolSafety::ReadOnly,
            ));
            tools.push((
                Box::new(AskUserTool::new(
                    agent_id.clone(),
                    session.contexts.clone(),
                    session.questions.clone(),
                    session.events.clone(),
                    crate::domains::agent::session::ASK_USER_TIMEOUT,
                )),
                ToolSafety::ReadOnly,
            ));
            tools.push((
                Box::new(SpawnSubagentTool::new(
                    agent_id,
                    session.contexts.clone(),
                    session.subagents.clone(),
                    session.events.clone(),
                )),
                ToolSafety::ReadOnly,
            ));
        }
```

在 provider 函数之后加入纯判定 helper（让测试不需要构造 provider 的全部依赖）：

```rust
fn session_primitive_agent(
    runtime: &crate::domains::agent::tools::context::ToolRuntimeContext,
) -> Option<String> {
    if runtime.session_tools_enabled {
        runtime.agent_id.clone()
    } else {
        None
    }
}
```

3. 在 `service_manager.rs` 的 provider 注册调用末尾加：

```rust
                app_state.session_runtime.clone(),
```

并在注册之前（`app_state.agent_pool` 已存在的位置）加：

```rust
            let subagent_runner: Arc<dyn crate::domains::agent::session::SubagentRunner> =
                Arc::new(crate::domains::agent::host::chat::subagent::PoolSubagentRunner::new(
                    app_state.agent_pool.clone(),
                    app_state.session_runtime.runner.clone(),
                ));
            app_state.session_runtime.subagents.set(subagent_runner);
```

> 注意：`agent_pool` 是 `Arc<AgentPool>`；`PoolSubagentRunner::new` 需要 `Arc<AgentPool>`，直接 clone。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud host::tools::service::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS / 无错误。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/host/tools/service.rs \
  apps/cloud/src/shared/service_manager.rs
git commit -m "feat(session): expose session primitives to chat agents"
```

---

### Task 12: 聊天循环改走 SessionRunner

**Files:**

- Modify: `apps/cloud/src/domains/agent/host/chat/service.rs`
- Modify: `apps/cloud/src/domains/agent/host/chat/handler/proxy.rs`
- Modify: `apps/cloud/src/domains/agent/host/pool_adapter.rs`（两处 `ensure_agent` 传 `false`）
- Test: `apps/cloud/src/domains/agent/host/chat/service.rs` 的现有测试 + 新增测试

**Interfaces:**

- Consumes: `SessionRuntime`、`SessionRunner`、`TurnContext`、`SESSION_TURN_TIMEOUT`。
- Produces:
  - `ensure_agent(pool, agent_id, workspace_id, config, session_tools_enabled)`；聊天传 `true`，心跳/自治传 `false`。
  - `send_with_pool(..., session_runtime: Arc<SessionRuntime>)`。
  - `send_message(..., session_runtime: Arc<SessionRuntime>)`。
  - 聊天 Turn 超时改为 `SESSION_TURN_TIMEOUT`（15 分钟），以容纳 `ask_user` 的 10 分钟等待。

- [ ] **Step 1: 写失败测试**

在 `service.rs` 测试模块追加：

```rust
#[test]
fn chat_timeout_allows_question_answer() {
    use crate::domains::agent::session::{ASK_USER_TIMEOUT, SESSION_TURN_TIMEOUT};
    assert!(SESSION_TURN_TIMEOUT > ASK_USER_TIMEOUT);
    assert!(SESSION_TURN_TIMEOUT >= std::time::Duration::from_secs(15 * 60));
}
```

并把现有 `send_with_pool_rejects_session_from_other_workspace` 整体替换为：

```rust
    #[tokio::test]
    async fn send_with_pool_rejects_session_from_other_workspace() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&db)
            .await
            .unwrap();
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            tinyiothub_agent::provider::minimax_provider_factory(),
        )
        .expect("test AgentPool");
        let session_runtime = Arc::new(
            crate::domains::agent::session::SessionRuntime::new(
                Arc::new(tinyiothub_storage::Db::new(db.clone())),
                Arc::new(
                    crate::domains::agent::runtime::harness_stream::HarnessStreamHub::default(),
                ),
            ),
        );
        let err = send_with_pool(
            &pool,
            &db,
            "agent_main",
            "agent:ws_other:agent_main/s1",
            "hi",
            "r1",
            "",
            "ws_mine",
            &AgentRuntimeConfig::default(),
            None,
            session_runtime,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
        // The workspace check must run before any agent is built.
        assert_eq!(pool.pool_size(), 0);
    }
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud host::chat::service::tests -v`

Expected: FAIL，签名不匹配。

- [ ] **Step 3: 写最小实现**

1. `ensure_agent` 增加一个参数，并且只对聊天 Turn 打开会话原语：

```rust
pub async fn ensure_agent(
    pool: &AgentPool,
    agent_id: &str,
    workspace_id: &str,
    config: &AgentRuntimeConfig,
    session_tools_enabled: bool,
) -> Result<AgentLoopHandle, AgentError> {
    if let Some(agent) = pool.get_cached(agent_id) {
        return Ok(agent);
    }
    let trust_config = pool.trust_config(workspace_id);
    let mut runtime = pool.runtime_context().await;
    runtime.agent_id = Some(agent_id.to_string());
    runtime.session_tools_enabled = session_tools_enabled;
    let tools = pool
        .tool_registry()
        .resolve_tools_for_agent(config, workspace_id, trust_config, &runtime)
        .await;
    pool.create(agent_id, workspace_id, config, tools)
}
```

`send_with_pool` 里调用 `ensure_agent(..., true)`；`apps/cloud/src/domains/agent/host/pool_adapter.rs` 的两处调用改为 `ensure_agent(..., false)`（心跳/自治 agent 不暴露 `todo_write`/`ask_user`/`spawn_subagent`，它们没有 TurnContext）。

> **缓存隔离约束：** `AgentPool` 按 `agent_id` 缓存 agent，工具集在首次创建时定型。聊天与心跳/自治必须使用不同的 `agent_id`，否则先创建的一方的工具集会泄漏给另一方。现有心跳 id 为 `heartbeat_agent_id(workspace_id)`（`__heartbeat__:<ws>`），不会与聊天冲突；`get_or_create_agent` 目前用 `workspace_id` 作为 agent_id 且在当前 cloud 代码中无调用方，若后续接线自治路径，必须改用独立前缀。把这条约束写进 `ensure_agent` 的 doc comment。

2. `send_with_pool` 增加最后一个参数：

```rust
    session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
```

并在调用 `send_message` 时把 `session_runtime` 传进去。

3. `send_message` 增加最后一个参数：

```rust
    session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
```

4. 在 `send_message` 的 clone 区（`let db_pool = db_pool.clone();` 之后）加入：

```rust
    let session_runtime = session_runtime.clone();
```

5. 在模块级 `turn_event_to_chat_event` 函数之后定义 observer（不要写在 `send_message` 函数体内）：

```rust
struct ChatEventObserver {
    tx: mpsc::Sender<ChatEvent>,
    run_id: String,
    session_key: String,
}

#[async_trait::async_trait]
impl crate::domains::agent::session::TurnObserver for ChatEventObserver {
    async fn on_turn_event(&self, event: &TurnEvent) {
        if matches!(event, TurnEvent::Usage { .. }) {
            return;
        }
        let chat = turn_event_to_chat_event(event, &self.run_id, &self.session_key);
        let _ = self.tx.send(chat).await;
    }
}
```

6. 在 `send_message` 的 `tokio::spawn` 内，删除原来的 `event_tx/event_rx` 创建与 forward 任务；保留 `tokio::spawn` 外层、history、reflection、`chat_handles` 清理。

在 `ensure_session` 调用之前探测是否为新会话，把结果带到后面写 `session.started`：

```rust
        let is_new_session = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM chat_sessions WHERE session_key = ?",
        )
        .bind(&session_key)
        .fetch_one(&db_pool)
        .await
        {
            Ok(count) => count == 0,
            Err(error) => {
                tracing::warn!(%error, %session_key, "Failed to probe chat session");
                false
            }
        };
```

7. 在 `ag` 锁之后、`append_message`（user）之后构造上下文并写用户消息事件：

```rust
        let context = crate::domains::agent::session::TurnContext::new(
            workspace_id.clone(),
            agent_id.clone(),
            session_key.clone(),
            run_id.clone(),
            0,
        );
        if is_new_session {
            if let Err(error) = session_runtime
                .events
                .emit(crate::domains::agent::session::session_event(
                    &context,
                    "session.started",
                    serde_json::json!({
                        "sessionKey": context.session_key.as_str(),
                        "assistantId": context.assistant_id.as_str()
                    }),
                ))
                .await
            {
                tracing::warn!(%error, %session_key, "Failed to persist session.started event");
            }
        }
        if let Err(error) = session_runtime
            .events
            .emit(crate::domains::agent::session::session_event(
                &context,
                "message.created",
                serde_json::json!({
                    "messageId": uuid::Uuid::new_v4().to_string(),
                    "role": "user",
                    "text": reflect_message.as_str()
                }),
            ))
            .await
        {
            tracing::warn!(%error, %session_key, "Failed to persist user session event");
        }
```

8. 让 trigger 解析同时返回命中的技能名，并在用户消息事件之后写 `skill.loaded`。把函数开头的

```rust
    let (turn_message, reflect_message) = {
```

改为：

```rust
    let (turn_message, reflect_message, trigger_skill) = {
```

`Some(hit)` 分支改为：

```rust
            Some(hit) => {
                let user_text = if hit.cleaned_message.is_empty() {
                    "综合情况".to_string()
                } else {
                    hit.cleaned_message
                };
                let version = skills
                    .iter()
                    .find(|skill| skill.name == hit.name)
                    .map(|skill| skill.version.clone())
                    .unwrap_or_default();
                let turn = tinyiothub_skills::wrap_injected_skill(&hit.name, &hit.body, &user_text);
                (turn, user_text, Some((hit.name, version)))
            }
            None => (message.to_string(), message.to_string(), None),
```

`trigger_skill` 的类型是 `Option<(String, String)>`，记录技能名与版本。

然后在用户 `message.created` 事件之后加：

```rust
        if let Some((skill_name, skill_version)) = trigger_skill {
            if let Err(error) = session_runtime
                .events
                .emit(crate::domains::agent::session::session_event(
                    &context,
                    "skill.loaded",
                    serde_json::json!({
                        "skillId": skill_name,
                        "name": skill_name,
                        "version": skill_version
                    }),
                ))
                .await
            {
                tracing::warn!(%error, %session_key, "Failed to persist skill.loaded event");
            }
        }
```

9. 把原来的 `let result = tokio::time::timeout(120s, ag.turn_streamed(...))` 整块（含 `drop(ag)` 与其后的 `match result`）替换为：

```rust
        let observer: Arc<dyn crate::domains::agent::session::TurnObserver> =
            Arc::new(ChatEventObserver {
                tx: tx.clone(),
                run_id: run_id.clone(),
                session_key: session_key.clone(),
            });
        let cancel = tokio_util::sync::CancellationToken::new();
        let timer = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(crate::domains::agent::session::SESSION_TURN_TIMEOUT).await;
                cancel.cancel();
            })
        };
        let outcome = session_runtime
            .runner
            .run_turn(&*ag, &context, &turn_message, Some(observer), Some(cancel))
            .await;
        timer.abort();
        drop(ag);

        let final_text = match outcome {
            crate::domains::agent::session::TurnOutcome::Completed { text, .. } => {
                let _ = tx
                    .send(ChatEvent::Final {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        message: serde_json::json!({
                            "role": "assistant",
                            "content": [{ "type": "text", "text": &text }],
                        }),
                    })
                    .await;
                Some(text)
            }
            crate::domains::agent::session::TurnOutcome::Cancelled => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: "Agent execution cancelled".to_string(),
                    })
                    .await;
                None
            }
            crate::domains::agent::session::TurnOutcome::Failed { error } => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error,
                    })
                    .await;
                None
            }
        };
```

10. `proxy.rs` 的 `send_with_pool` 调用末尾加：

```rust
        state.session_runtime.clone(),
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud host::chat::service::tests -v
cargo test -p tinyiothub-cloud host::tools::service::tests -v
cargo check -p tinyiothub-cloud
```

Expected: 全部 PASS。若 `chat_event` 测试仍引用旧的 forward 逻辑，按新断言更新；保留 `turn_event_to_chat_event` 的单元测试。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/host/chat/service.rs \
  apps/cloud/src/domains/agent/host/chat/handler/proxy.rs \
  apps/cloud/src/domains/agent/host/pool_adapter.rs
git commit -m "feat(chat): route chat turns through session runner"
```

---

### Task 13: 事件 / Todo / 问题回答 API

**Files:**

- Create: `apps/cloud/src/domains/agent/session/handler.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`（`pub mod handler;`）
- Modify: `apps/cloud/src/domains/agent/host/chat/handler/mod.rs`（挂路由）
- Test: `handler.rs` 的纯函数测试

**Interfaces:**

- Consumes: `AgentState.db`、`AgentState.session_runtime`、`Claims`、`SessionKey`、`EventFilter`。
- Produces:
  - `GET /api/v1/chat/sessions/{session_key}/events?after=<seq>&limit=<n>` → `{ events, highWatermark }`。
  - `GET /api/v1/chat/sessions/{session_key}/events/stream?after=<seq>` → SSE（`event: session`）。
  - `GET /api/v1/chat/sessions/{session_key}/todos` → `[Todo]`。
  - `POST /api/v1/chat/sessions/{session_key}/questions/{question_id}/answer`（body `QuestionAnswer`）→ 204。

- [ ] **Step 1: 写失败测试**

在 `handler.rs` 写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_mismatch_is_rejected() {
        let key = "agent:ws-a:agent_main/s1";
        assert!(verify_session_workspace(key, "ws-a").is_some());
        assert!(verify_session_workspace(key, "ws-b").is_none());
    }

    #[test]
    fn malformed_session_key_is_rejected() {
        assert!(verify_session_workspace("chat:ws:agent/s1", "ws").is_none());
        assert!(verify_session_workspace("agent:ws:agent", "ws").is_none());
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::handler::tests -v`

Expected: FAIL，缺少 `verify_session_workspace`。

- [ ] **Step 3: 写最小实现**

`session/handler.rs`：

```rust
//! Session 事件、Todo、问题回答的 HTTP 端点。
//!
//! 所有端点先校验 session_key 的 workspace 归属，再访问 DB / broker。

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tinyiothub_agent::session::SessionKey;
use tinyiothub_core::harness::ExecutionMode;
use tinyiothub_storage::harness_event::EventFilter;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::session::QuestionAnswer;

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub after: i64,
    pub limit: Option<i64>,
}

pub(crate) fn verify_session_workspace(
    session_key: &str,
    authorized_workspace: &str,
) -> Option<SessionKey> {
    let parsed = SessionKey::parse(session_key).ok()?;
    if parsed.workspace_id != authorized_workspace {
        return None;
    }
    Some(parsed)
}

fn event_filter(session_id: &str) -> EventFilter {
    EventFilter {
        session_id: Some(session_id.to_string()),
        run_id: None,
        task_id: None,
        execution_mode: ExecutionMode::Production,
    }
}

pub async fn list_events(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Query(query): Query<EventQuery>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let filter = event_filter(&parsed.session_key());
    match state
        .db
        .session_event_page(
            &parsed.workspace_id,
            &filter,
            query.after,
            query.limit.unwrap_or(200),
        )
        .await
    {
        Ok(page) => Json(serde_json::json!({
            "events": page.events,
            "highWatermark": page.high_watermark,
        }))
        .into_response(),
        Err(error) => {
            tracing::warn!(%error, %session_key, "session event page failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn stream_events(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Query(query): Query<EventQuery>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let filter = event_filter(&parsed.session_key());
    let db = state.db.clone();

    let event_stream = stream! {
        let mut cursor = query.after;
        loop {
            match db
                .session_event_page(&parsed.workspace_id, &filter, cursor, 200)
                .await
            {
                Ok(page) => {
                    for event in page.events {
                        cursor = event.seq;
                        if let Ok(frame) = Event::default()
                            .event("session")
                            .id(event.seq.to_string())
                            .json_data(event)
                        {
                            yield Ok::<_, Infallible>(frame);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, %session_key, "session event tail failed");
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

pub async fn todos(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    Json(state.session_runtime.todos.get(&parsed.session_key())).into_response()
}

pub async fn answer_question(
    State(state): State<AgentState>,
    claims: Claims,
    Path((session_key, question_id)): Path<(String, String)>,
    Json(answer): Json<QuestionAnswer>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state
        .session_runtime
        .questions
        .answer(&parsed.session_key(), &question_id, answer)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(crate::domains::agent::session::questions::QuestionError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(crate::domains::agent::session::questions::QuestionError::SessionMismatch) => {
            StatusCode::FORBIDDEN.into_response()
        }
        Err(crate::domains::agent::session::questions::QuestionError::Closed) => {
            StatusCode::CONFLICT.into_response()
        }
    }
}
```

在 `session/mod.rs` 增加：

```rust
pub mod handler;
```

在 `host/chat/handler/mod.rs` 的路由中增加：

```rust
        .route(
            "/sessions/{session_key}/events",
            get(crate::domains::agent::session::handler::list_events),
        )
        .route(
            "/sessions/{session_key}/events/stream",
            get(crate::domains::agent::session::handler::stream_events),
        )
        .route(
            "/sessions/{session_key}/todos",
            get(crate::domains::agent::session::handler::todos),
        )
        .route(
            "/sessions/{session_key}/questions/{question_id}/answer",
            post(crate::domains::agent::session::handler::answer_question),
        )
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::handler::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/handler.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/host/chat/handler/mod.rs
git commit -m "feat(session): expose events, todos, and question answer APIs"
```

---

### Task 14: 端到端回归与零配置验证

**Files:**

- Modify: `apps/cloud/src/domains/agent/prompt/mod.rs`（只加测试）
- Modify: `apps/cloud/src/domains/agent/session/runner.rs`（只加测试）
- Test: 上述两处

**Interfaces:**

- Consumes: `build_full_system_prompt`、`SessionRunner`。
- Produces: 两条回归保证：
  1. 空 workspace 也能得到身份/规则/能力模板（零配置启动）；
  2. 一次 Turn 的事件顺序稳定，且 `turn.started` 在 `tool.called` 之前、`message.created` 在 `turn.completed` 之前。

- [ ] **Step 1: 写失败测试**

在 `prompt/mod.rs` 测试模块追加：

```rust
#[tokio::test]
async fn empty_workspace_still_gets_identity_and_rules() {
    let config = tinyiothub_core::config::SystemPromptsConfig::default();
    let prompt = build_full_system_prompt(&config, Some("ws-empty"), None, None).await;
    assert!(prompt.contains("Identity"), "identity template must load");
    assert!(prompt.contains("Agent Rules"), "agent rules template must load");
    assert!(prompt.contains("Capabilities"), "tools template must load");
}
```

在 `runner.rs` 测试模块追加：

```rust
#[tokio::test]
async fn lifecycle_order_is_stable() {
    let sink = Arc::new(RecordingSink::default());
    let contexts = Arc::new(TurnContextRegistry::default());
    let runner = SessionRunner::new(sink.clone(), contexts);
    let ctx = TurnContext::new(
        "ws".into(),
        "agent_main".into(),
        "agent:ws:agent_main/s1".into(),
        "run1".into(),
        0,
    );
    runner.run_turn(&FakeLoop, &ctx, "hello", None, None).await;
    let events = sink.events.lock().await;
    let index = |name: &str| {
        events
            .iter()
            .position(|event| event.event_type == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    assert!(index("turn.started") < index("tool.called"));
    assert!(index("tool.returned") < index("message.created"));
    assert!(index("message.created") < index("turn.completed"));
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::runner::tests::lifecycle_order_is_stable -v`

Expected: 如果 Task 8 已实现则 PASS；如果模板测试失败，说明模板路径/默认配置有缺口。

- [ ] **Step 3: 写最小实现**

这两条测试不应需要生产代码改动。如果 `empty_workspace_still_gets_identity_and_rules` 失败，检查：

- `SystemPromptsConfig::default().workspace_dir` 是否为空；为空时 `get_workspace_dir` 使用 `paths::workspace_dir(ws)`，模板应走 embedded fallback。
- `load_workspace_prompt` 是否因为 `shared_base` 存在空文件而跳过 embedded 模板；若是，属于既有 bug，按最小改动让空文件回退到 embedded 模板。

如果 `lifecycle_order_is_stable` 失败，按 Task 8 的顺序修正，不要改测试。

- [ ] **Step 4: 运行全量验证**

Run:

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
```

Expected: 全部通过。若 clippy 报 `too_many_arguments`，在 `chat_builtin_tools_provider` / `send_with_pool` / `send_message` 上保留现有 `#[allow(clippy::too_many_arguments)]`。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/prompt/mod.rs \
  apps/cloud/src/domains/agent/session/runner.rs
git commit -m "test(session): lock zero-config prompt and turn event order"
```

---

## 验证清单（执行者完成全部 Task 后逐项打勾）

- [ ] 新会话第一次发消息无需配置 Assistant/Task/Skill：`build_full_system_prompt` 返回身份、规则、能力模板。
- [ ] 模型调用 `todo_write` 后，`session_events` 出现 `todo.updated`，`/todos` 返回同一列表。
- [ ] 模型调用 `ask_user` 后，Turn 阻塞；`question.asked` 可见；回答端点 resolve 后 `question.answered` 出现，Turn 继续。
- [ ] 模型调用 `spawn_subagent` 后，`subagent.started`/`subagent.completed` 出现；子代理不能再次派生（depth 拒绝）。
- [ ] 聊天 SSE 仍收到现有 `ChatEvent`（delta/thinking/tool start/tool result/final），行为不回退。
- [ ] `session_events` 按 `seq` 可回放 `turn.started → tool.called → tool.returned → message.created → turn.completed`。
- [ ] 新会话首条消息写入 `session.started`，后续消息不重复写。
- [ ] 所有新写入的事件 payload 键为 camelCase（`turnId`/`toolCallId`/`questionId`/`subagentId`/`messageId`/`toolCalls`/`evidenceRefs`/`recommendedActions`），Rust/DB 字段保持 snake_case。
- [ ] 设备写操作仍走既有审批/Effect 路径；`full_access` 未在本计划中被接线，且没有绕过现有安全检查。
- [ ] 没有删除任何旧表/旧端点；没有改动 Plan 2–4 的代码。
- [ ] `git status` 不包含与计划无关的文件（尤其旧的 2026-11-02 spec 和 2026-11-03 plan）。

---

## Self-Review

**Spec coverage（Plan 1 范围内）：**

- §5.1 零配置 Assistant：Task 14 的模板回归 + 现有 `build_full_system_prompt`。
- §5.2 Session/Turn 模型：Task 2/3/8/10。
- §5.3 / §7.1–7.3 三个原语：Task 5（todo_write）、Task 6（ask_user）、Task 7/9（spawn_subagent，含 `subagent_type` 与 `evidence_refs`/`recommended_actions` 字段）。
- §6 权限模式：Task 2 定义 `PermissionMode`；本计划不接线 `full_access`，设备写操作继续走现有 trust/approval，默认行为等价于 `ask`。模式选择 UI 与强制入口在 Plan 3。
- §5.4 事件分层与命名：Task 4/8/12/13；`session_events` 复用现有表；不新增 `workspace_id` 列；聊天 Turn 额外写 `session.started`（新会话首条消息）与 `skill.loaded`。payload 键统一 camelCase，与现有 `session_events`（如 `deviceFacts`/`observedAt`）一致；DB/Rust 字段保持 snake_case。
- §5.4.3 白名单中 `session.ended`、`approval.requested/resolved`、`trigger.received`、`wake.dispatched` 不在 Plan 1 写入：`session.ended` 依赖 Plan 2 的调度/触发器生命周期；审批时间线依赖现有 approval 路径改造；trigger/wake 属 Plan 2。白名单本身不限制写入（现有测试已覆盖），Plan 1 只保证自己写入的事件名合法。
- §5.4.4 清理映射：不在 Plan 1 执行；Plan 4。
- §12 清理批次：不在 Plan 1 执行；Plan 4。
- Trigger/Schedule/HUD：Plan 2。
- 前端：Plan 3。

**Placeholder scan：** 本计划没有 `TBD`/`TODO`/“实现细节略”。Task 11 的测试直接验证纯函数 `session_primitive_agent`，不依赖 provider 的全部构造参数；provider 注入代码也已给出完整调用。

**Type consistency：** `TurnContext.assistant_id` 与 `agent_id` 的映射在 Task 2/3/8/9/10/12 一致；`SubagentRequest.subagent_type` 在 Task 7 工具入参、Task 9 子代理系统提示、Task 11 过滤逻辑中使用同一字段名；`SubagentReport.subagent_id/status/summary/tool_calls/evidence_refs/recommended_actions` 在 Task 7/9 一致；`QuestionAnswer.answers[].question_index` 在 Task 2/6/13 一致；`SessionRuntime` 字段在 Task 10/11/13 一致。
