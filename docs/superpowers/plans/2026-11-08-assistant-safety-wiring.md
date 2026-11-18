# Assistant Safety & Wiring Implementation Plan (Plan 4A / 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Plan 1–3 的新链路真正收口：每 Session 的权限模式落地并强制执行、审批进入统一时间线与前端卡片、报警触发改走 `TriggerRouter`/Session Loop、`heartbeat_tasks` 迁移为 `inspection` 技能正文；全程不放松 e-stop、联锁、Effect/Decision 与审计账本。

**Architecture:** 权限使用 `PermissionMode` + 会话级持久化，工具层新增 `PermissionGuardTool`（在 `TurnContext` 上读取模式；`ask` 下 Write/Destructive 走新的 `ApprovalBroker` 阻塞等待并写 `approval.requested/resolved` 事件）；报警由 `TriggerRouter` 订阅 `EventBus` 生成 event wake，旧 `HeartbeatBridge` 报警链路被显式关闭；`inspection` 技能由 boot/workspace 创建时从既有任务或缺省步骤种子化，`WakeRunner` 以“schedule.prompt > inspection 技能 > legacy tasks > 默认职责”的顺序构造唤醒消息。

**Tech Stack:** Rust 2024、Axum、Tokio、SQLx/SQLite、`async-trait`、`dashmap`、`tokio-util::CancellationToken`、Lit/TS（前端卡片与选择器）、现有 `SessionRuntime`/`TriggerRouter`/`SessionRunner`/`Tool`/`ToolSafety`。

## Plan Boundary

本计划是 4 份计划中的第 4 份，且只做 **4A 接线与安全收口**：

- **已完成**：Plan 1（Session Loop/原语/事件/API）、Plan 2（Schedule/TriggerRouter/HUD）、Plan 3（Codex 式前端）。
- **Plan 4A（本文件）**：权限模式强制、会话审批（broker/事件/卡片）、报警触发统一到 Session Loop、`inspection` 技能种子化。
- **Plan 4B（后续）**：spec §12 的 Batch 0–6 清理与命名收敛、watch 触发迁移、`heartbeat_tasks` 表下掉、旧前端视图删除、文档收敛。

明确不在本计划内：

- **不删除** `ThingEventTrigger`/`WatchTrigger`/旧 `run_pipeline`/`AutonomousAgentProvider`/`dispatch_thing_task` 等；本计划只切断报警的旧投递路径并新增新路径，删除在 4B。
- **watch 触发仍走旧管线**：`WatchTrigger` 的 `hit_rx` 是单消费者，迁移需要拆分 watch engine 与 trigger（4B）；本计划不双挂新路径，避免双重运行。
- **`heartbeat_tasks` 表保留**（只读语义在 4B 落实）；本计划只新增技能种子与唤醒优先级，不改写/删除用户任务行。
- **`heartbeat_config` 的 interval/enabled 到 `AssistantSchedule` 的写穿**不在本计划（4B 随旧配置 UI 收敛一起做）；Schedule API 仍是调度唯一写入口。
- **Destructive 工具的内层语义不变**：`call_device` 的 `PendingActionStore` token、`require_action_confirm`、Effect/Decision 合同全部保留；`full_access` 只免除本计划新增的逐次审批，不触碰 e-stop/联锁/Effect。

---

## Global Constraints

以下约束来自 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md` §6/§5.4/§11/§12，每个 Task 都隐含包含：

- **安全层不动**：policy / trust / approval / Effect / Decision / unknown fence / 对账 / 恢复合同全部保持；`完全访问` 绝不绕过急停、互锁和 Effect 边界。
- **权限模式**：`read_only` / `ask`（默认） / `full_access`；`read_only` 拒绝一切 Write/Destructive；`ask` 对 Write/Destructive 逐次询问；`full_access` 免除本层逐次询问，但内层设备闸门（token/Effect/审批）照旧。
- **事件**：`aggregate.past_tense`，必须在 spec §5.4.3 白名单内；payload 键 camelCase，Rust/DB 字段 snake_case；`session_events` 是唯一 UI 时间线，审计仍走 `hub_ledger`。
- **命名**：`Workspace → Assistant → Session → Turn`；`Task → Run`；`Turn → Subagent`；禁用词清单（spec §5.4.5）对新代码生效。
- **迁移**：只加列/表/索引，不 drop、不改已有列语义；文件名 `YYYYMMDDNNNNNN_<name>.sql`。
- **TDD**：每 Task 先写失败测试，再实现，再跑测试，再提交。
- **提交**：每 Task 一个 commit；不要提交工作区无关文件（`docs/superpowers/specs/2026-11-02-agent-harness-core-design.md`、`docs/superpowers/plans/2026-11-03-agent-harness-core.md` 不动）。
- **前端**：Node 24（`export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`）；中文文案；沿用现有 tokens。
- **验证**：每 Task 跑对应包测试；最后跑 `cargo fmt --all -- --check`、`cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings`、`cargo test -p tinyiothub-cloud`、`cargo test -p db`、前端 `npm test && npm run type-check && npm run build`。

---

## File Structure

### 新增

- `crates/db/migrations/20261108000001_session_permission_mode.sql` — `chat_sessions.permission_mode`。
- `apps/cloud/src/domains/agent/session/approvals.rs` — `ApprovalBroker`。
- `apps/cloud/src/domains/agent/session/tool_guard.rs` — `PermissionGuardTool`。
- `apps/cloud/src/domains/agent/session/skills_seed.rs` — `inspection` 技能种子化。
- `apps/cloud/templates/skills/inspection.md` — 巡检技能模板（步骤 + 注意事项）。
- `web/src/ui/chat/approval-card.ts` — 审批卡片。
- `web/src/ui/chat/approval-card.test.ts`、`web/src/ui/chat/timeline-model.test.ts`（补充）等测试。

### 修改

- `crates/db/src/session.rs` — `Session.permission_mode` 字段、SQL、测试。
- `apps/cloud/src/domains/agent/session/types.rs` — `PermissionMode::as_str/parse`。
- `apps/cloud/src/domains/agent/session/context.rs` — `TurnContext.permission_mode` + `with_permission_mode`。
- `apps/cloud/src/domains/agent/session/subagent.rs` — `SubagentRequest.permission_mode`。
- `apps/cloud/src/domains/agent/session/subagent` 工具与 `host/chat/subagent.rs` — 传递父模式。
- `apps/cloud/src/domains/agent/session/runner.rs` — 无结构性变化（context 已带模式）。
- `apps/cloud/src/domains/agent/session/mod.rs` — `SessionRuntime.approvals`、模块导出。
- `apps/cloud/src/domains/agent/host/chat/service.rs` — 读取会话模式、注入 TurnContext。
- `apps/cloud/src/domains/agent/session/handler.rs` — 权限 PUT 与审批回答端点。
- `apps/cloud/src/domains/agent/host/tools/service.rs` — 包装 `PermissionGuardTool`。
- `apps/cloud/src/domains/agent/session/events.rs` — 审批事件构造（如需 helper）。
- `apps/cloud/src/domains/agent/session/trigger.rs` — event wake 队列、报警订阅、去重。
- `apps/cloud/src/domains/agent/session/wake.rs` — `execute_event` 与技能优先的唤醒消息。
- `apps/cloud/src/domains/agent/orchestrator/callbacks.rs`、`orchestrator/mod.rs` — 报警旧投递开关。
- `apps/cloud/src/shared/service_manager.rs` — 启动时技能种子化、router 接 EventBus、关闭旧报警桥。
- `apps/cloud/src/domains/tenant/workspace/service.rs` — 新 workspace 技能种子化。
- `apps/cloud/src/api/mod.rs` — 若新端点放在既有 session handler 则无需改动。
- `web/src/api/assistant.ts` — `answerApproval`、`updateSessionPermission`。
- `web/src/ui/chat/timeline-model.ts` — `approval` 状态与块。
- `web/src/ui/views/chat.ts` — 审批卡片渲染、权限选择器。

---

### Task 1: `chat_sessions.permission_mode` 持久化

**Files:**

- Create: `crates/db/migrations/20261108000001_session_permission_mode.sql`
- Modify: `crates/db/src/session.rs`
- Modify: `apps/cloud/src/domains/agent/session/types.rs`
- Test: 上述两个文件的测试模块

**Interfaces:**

- Produces:
  - `Session.permission_mode: String`（默认 `"ask"`）、`Session::with_permission_mode(mode)`。
  - `PermissionMode::as_str(&self) -> &'static str`、`PermissionMode::parse(&str) -> Option<Self>`（`read_only|ask|full_access`）。

- [ ] **Step 1: 写失败测试**

`crates/db/src/session.rs` 测试模块追加：

```rust
#[tokio::test]
async fn test_session_defaults_to_ask_permission() {
    let db = create_test_db().await;
    let session = Session::new(
        "agent:ws:agent1/sess_perm".to_string(),
        "ws".to_string(),
        "agent1".to_string(),
    );
    assert_eq!(session.permission_mode, "ask");
    db.create_session(&session).await.unwrap();
    let found = db.get_session(&session.session_key).await.unwrap().unwrap();
    assert_eq!(found.permission_mode, "ask");
}

#[tokio::test]
async fn test_session_persists_explicit_permission_mode() {
    let db = create_test_db().await;
    let session = Session::new(
        "agent:ws:agent1/sess_ro".to_string(),
        "ws".to_string(),
        "agent1".to_string(),
    )
    .with_permission_mode("read_only");
    db.create_session(&session).await.unwrap();
    let found = db.get_session(&session.session_key).await.unwrap().unwrap();
    assert_eq!(found.permission_mode, "read_only");
}
```

`apps/cloud/src/domains/agent/session/types.rs` 测试模块追加：

```rust
#[test]
fn permission_mode_round_trips() {
    for mode in [PermissionMode::ReadOnly, PermissionMode::Ask, PermissionMode::FullAccess] {
        assert_eq!(PermissionMode::parse(mode.as_str()), Some(mode));
    }
    assert_eq!(PermissionMode::parse("allow_all"), None);
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p db session::tests::test_session_defaults_to_ask_permission -v
cargo test -p tinyiothub-cloud session::types::tests::permission_mode_round_trips -v
```

Expected: FAIL（缺少字段/方法）。

- [ ] **Step 3: 写最小实现**

Migration：

```sql
ALTER TABLE chat_sessions ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'ask';
```

`Session` 在 `status` 之后加：

```rust
    /// 会话权限模式："read_only" | "ask" | "full_access"
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
```

默认函数与 builder：

```rust
fn default_permission_mode() -> String {
    "ask".to_string()
}

    /// 设置会话权限模式；调用方负责校验枚举值。
    pub fn with_permission_mode(mut self, mode: impl Into<String>) -> Self {
        self.permission_mode = mode.into();
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self
    }
```

`Session::new` 加 `permission_mode: default_permission_mode(),`。

`map_session_row` 加：

```rust
    let permission_mode: Option<String> = row.try_get("permission_mode").ok();
```

构造时：

```rust
        permission_mode: permission_mode.unwrap_or_else(default_permission_mode),
```

所有 `SELECT session_key, workspace_id, agent_id, label, kind, status, created_at, updated_at, metadata` 改为包含 `permission_mode`；`create_session` INSERT 与 `update_session` UPDATE 同步加列并按顺序 `.bind(&session.permission_mode)`。

`types.rs`：

```rust
impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Ask => "ask",
            Self::FullAccess => "full_access",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read_only" => Some(Self::ReadOnly),
            "ask" => Some(Self::Ask),
            "full_access" => Some(Self::FullAccess),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p db session::tests -v
cargo test -p tinyiothub-cloud session::types::tests -v
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/db/migrations/20261108000001_session_permission_mode.sql \
  crates/db/src/session.rs apps/cloud/src/domains/agent/session/types.rs
git commit -m "feat(db): persist session permission mode"
```

---

### Task 2: TurnContext 携带权限模式并在聊天中注入

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/context.rs`
- Modify: `apps/cloud/src/domains/agent/session/subagent.rs`
- Modify: `apps/cloud/src/domains/agent/session/tools/spawn_subagent.rs`
- Modify: `apps/cloud/src/domains/agent/host/chat/subagent.rs`
- Modify: `apps/cloud/src/domains/agent/host/chat/service.rs`
- Modify: `apps/cloud/src/domains/agent/session/handler.rs`
- Test: context/subagent/handler 测试模块

**Interfaces:**

- `TurnContext.permission_mode: PermissionMode`；`TurnContext::new(...)` 默认 `Ask`；`with_permission_mode(mode) -> Self`。
- `SubagentRequest.permission_mode: PermissionMode`（子代理继承父模式）。
- `PUT /api/v1/chat/sessions/{session_key}/permission`，body `{ "mode": "read_only"|"ask"|"full_access" }` → 204。

- [ ] **Step 1: 写失败测试**

`context.rs` 测试模块追加：

```rust
#[test]
fn turn_context_defaults_to_ask_and_overrides() {
    let context = ctx("run1");
    assert_eq!(context.permission_mode, PermissionMode::Ask);
    let readonly = context.with_permission_mode(PermissionMode::ReadOnly);
    assert_eq!(readonly.permission_mode, PermissionMode::ReadOnly);
}
```

`handler.rs` 测试模块追加（纯函数）：

```rust
#[test]
fn permission_mode_body_defaults_and_rejects_unknown() {
    assert_eq!(parse_permission_mode("ask"), Some(PermissionMode::Ask));
    assert_eq!(parse_permission_mode("read_only"), Some(PermissionMode::ReadOnly));
    assert_eq!(parse_permission_mode("admin"), None);
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::context::tests -v
cargo test -p tinyiothub-cloud session::handler::tests -v
```

Expected: FAIL（缺少字段/函数）。

- [ ] **Step 3: 写最小实现**

`context.rs`：

```rust
use super::types::PermissionMode;
```

`TurnContext` 增加：

```rust
    /// 本 Turn 的权限模式；由 Session 行注入，子代理继承父 Turn。
    pub permission_mode: PermissionMode,
```

`TurnContext::new` 加 `permission_mode: PermissionMode::Ask,`，并加：

```rust
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }
```

`subagent.rs`：`SubagentRequest` 加

```rust
    pub permission_mode: PermissionMode,
```

（`use super::types::PermissionMode;`）。

`spawn_subagent.rs` 构造 `SubagentRequest` 时加：

```rust
                permission_mode: context.permission_mode,
```

同时更新既有测试中的 `SubagentRequest { ... }` 字面量（`spawn_subagent.rs` 测试与 `host/chat/subagent.rs` 测试）补 `permission_mode: PermissionMode::Ask`（或对应模式），否则编译失败。

`host/chat/subagent.rs` 构造子 TurnContext 时：

```rust
        let context = TurnContext::new(
            request.workspace_id.clone(),
            child_id.clone(),
            request.parent_session_key.clone(),
            uuid::Uuid::new_v4().to_string(),
            request.depth,
        )
        .with_permission_mode(request.permission_mode);
```

`host/chat/service.rs` 的 `send_message` spawn 内、构造 `context` 之前：

```rust
        let permission_mode = tinyiothub_storage::Db::new(db_pool.clone())
            .get_session(&session_key)
            .await
            .ok()
            .flatten()
            .and_then(|session| {
                crate::domains::agent::session::types::PermissionMode::parse(&session.permission_mode)
            })
            .unwrap_or_default();
        let context = crate::domains::agent::session::TurnContext::new(
            workspace_id.clone(),
            agent_id.clone(),
            session_key.clone(),
            run_id.clone(),
            0,
        )
        .with_permission_mode(permission_mode);
```

`session/handler.rs` 增加纯函数与端点：

```rust
use crate::domains::agent::session::types::PermissionMode;

pub(crate) fn parse_permission_mode(value: &str) -> Option<PermissionMode> {
    PermissionMode::parse(value.trim())
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdatePermissionRequest {
    pub mode: String,
}

pub async fn update_permission(
    State(state): State<AgentState>,
    claims: Claims,
    Path(session_key): Path<String>,
    Json(request): Json<UpdatePermissionRequest>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(mode) = parse_permission_mode(&request.mode) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let key = parsed.session_key();
    let Some(mut session) = state.db.get_session(&key).await.ok().flatten() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    session.permission_mode = mode.as_str().to_string();
    session.touch();
    match state.db.update_session(&session).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::warn!(%error, %key, "update session permission failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
```

在 `host/chat/handler/mod.rs` 增加路由：

```rust
        .route(
            "/sessions/{session_key}/permission",
            axum::routing::put(crate::domains::agent::session::handler::update_permission),
        )
```

`handler.rs` 需要 `use tinyiothub_web::security::Claims;` 与 `AgentState`（已有）。

Wake 会话默认 `Ask`（`WakeRunner` 不需要改动；`TurnContext::new` 默认即 Ask）。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::context::tests -v
cargo test -p tinyiothub-cloud session::handler::tests -v
cargo test -p tinyiothub-cloud host::chat::subagent::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/context.rs \
  apps/cloud/src/domains/agent/session/subagent.rs \
  apps/cloud/src/domains/agent/session/tools/spawn_subagent.rs \
  apps/cloud/src/domains/agent/host/chat/subagent.rs \
  apps/cloud/src/domains/agent/host/chat/service.rs \
  apps/cloud/src/domains/agent/session/handler.rs \
  apps/cloud/src/domains/agent/host/chat/handler/mod.rs
git commit -m "feat(session): carry permission mode through turns and subagents"
```

---

### Task 3: `ApprovalBroker` 与回答端点

**Files:**

- Create: `apps/cloud/src/domains/agent/session/approvals.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`（`SessionRuntime.approvals`）
- Modify: `apps/cloud/src/domains/agent/session/handler.rs`（answer 端点）
- Modify: `apps/cloud/src/domains/agent/host/chat/handler/mod.rs`
- Test: `approvals.rs` 与 `handler.rs` 测试模块

**Interfaces:**

- `ApprovalDecision { Approved, Denied }`（serde snake_case：`approved|denied`）。
- `ApprovalError { NotFound, SessionMismatch, Closed }`。
- `ApprovalBroker::{register(session_key, tool, summary) -> (String, oneshot::Receiver<ApprovalDecision>), answer(session_key, approval_id, decision), cancel(id), count_pending_for_workspace(ws) -> usize, list_for_session(session_key) -> Vec<ApprovalSummary>}`。
- `ApprovalSummary { approval_id, tool, summary }`。
- `SessionRuntime.approvals: Arc<ApprovalBroker>`。
- `POST /api/v1/chat/sessions/{session_key}/approvals/{approval_id}/answer` body `{ "decision": "approved"|"denied" }` → 204 / 404 / 403 / 409。

- [ ] **Step 1: 写失败测试**

`approvals.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answer_resolves_registered_approval() {
        let broker = ApprovalBroker::default();
        let (id, rx) = broker.register("agent:ws:a/s1", "set_device", "把 3 号楼空调设为 24℃");
        broker
            .answer("agent:ws:a/s1", &id, ApprovalDecision::Approved)
            .unwrap();
        assert_eq!(rx.await.unwrap(), ApprovalDecision::Approved);
    }

    #[test]
    fn wrong_session_cannot_answer() {
        let broker = ApprovalBroker::default();
        let (id, _rx) = broker.register("agent:ws:a/s1", "set_device", "x");
        let error = broker
            .answer("agent:ws:a/s2", &id, ApprovalDecision::Denied)
            .unwrap_err();
        assert!(matches!(error, ApprovalError::SessionMismatch));
        assert_eq!(broker.count_pending_for_workspace("ws"), 1);
    }

    #[test]
    fn lists_pending_for_session() {
        let broker = ApprovalBroker::default();
        let (_id, _rx) = broker.register("agent:ws:a/s1", "set_device", "x");
        let list = broker.list_for_session("agent:ws:a/s1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].tool, "set_device");
    }
}
```

`handler.rs` 测试追加：

```rust
#[test]
fn approval_decision_parses() {
    assert_eq!(parse_approval_decision("approved"), Some(ApprovalDecision::Approved));
    assert_eq!(parse_approval_decision("denied"), Some(ApprovalDecision::Denied));
    assert_eq!(parse_approval_decision("maybe"), None);
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::approvals::tests -v
```

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`approvals.rs`：

```rust
//! ApprovalBroker — `ask` 模式下 Write/Destructive 工具的逐次审批。
//!
//! 工具注册一个 oneshot，事件 `approval.requested` 把 approvalId 送到前端；
//! 回答端点按 session_key 校验后 resolve。超时由调用方（工具）负责。

use std::sync::Arc;

use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("approval not found")]
    NotFound,
    #[error("approval belongs to another session")]
    SessionMismatch,
    #[error("approval is no longer waiting for an answer")]
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSummary {
    pub approval_id: String,
    pub tool: String,
    pub summary: String,
}

struct PendingApproval {
    session_key: String,
    tool: String,
    summary: String,
    tx: oneshot::Sender<ApprovalDecision>,
}

#[derive(Default)]
pub struct ApprovalBroker {
    pending: DashMap<String, PendingApproval>,
}

impl ApprovalBroker {
    pub fn register(
        &self,
        session_key: &str,
        tool: &str,
        summary: &str,
    ) -> (String, oneshot::Receiver<ApprovalDecision>) {
        let approval_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(
            approval_id.clone(),
            PendingApproval {
                session_key: session_key.to_string(),
                tool: tool.to_string(),
                summary: summary.to_string(),
                tx,
            },
        );
        (approval_id, rx)
    }

    pub fn answer(
        &self,
        session_key: &str,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), ApprovalError> {
        let entry = self
            .pending
            .get_mut(approval_id)
            .ok_or(ApprovalError::NotFound)?;
        if entry.session_key != session_key {
            return Err(ApprovalError::SessionMismatch);
        }
        let pending = entry.remove();
        pending.tx.send(decision).map_err(|_| ApprovalError::Closed)
    }

    pub fn cancel(&self, approval_id: &str) {
        self.pending.remove(approval_id);
    }

    pub fn count_pending_for_workspace(&self, workspace_id: &str) -> usize {
        let prefix = format!("agent:{workspace_id}:");
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key.starts_with(&prefix))
            .count()
    }

    pub fn list_for_session(&self, session_key: &str) -> Vec<ApprovalSummary> {
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key == session_key)
            .map(|entry| ApprovalSummary {
                approval_id: entry.key().clone(),
                tool: entry.value().tool.clone(),
                summary: entry.value().summary.clone(),
            })
            .collect()
    }
}

pub type SharedApprovalBroker = Arc<ApprovalBroker>;
```

`session/mod.rs` 的 `SessionRuntime`：

```rust
pub mod approvals;

pub use approvals::{ApprovalBroker, ApprovalDecision, ApprovalError, ApprovalSummary};
```

`SessionRuntime` 加字段 `pub approvals: Arc<ApprovalBroker>,`，`new` 中 `let approvals = Arc::new(ApprovalBroker::default());` 并放入结构体。

`handler.rs`：

```rust
pub(crate) fn parse_approval_decision(value: &str) -> Option<ApprovalDecision> {
    match value.trim() {
        "approved" => Some(ApprovalDecision::Approved),
        "denied" => Some(ApprovalDecision::Denied),
        _ => None,
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ApprovalAnswerRequest {
    pub decision: String,
}

pub async fn answer_approval(
    State(state): State<AgentState>,
    claims: Claims,
    Path((session_key, approval_id)): Path<(String, String)>,
    Json(request): Json<ApprovalAnswerRequest>,
) -> Response {
    let Some(parsed) = verify_session_workspace(&session_key, &claims.workspace_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(decision) = parse_approval_decision(&request.decision) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match state
        .session_runtime
        .approvals
        .answer(&parsed.session_key(), &approval_id, decision)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(crate::domains::agent::session::ApprovalError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(crate::domains::agent::session::ApprovalError::SessionMismatch) => {
            StatusCode::FORBIDDEN.into_response()
        }
        Err(crate::domains::agent::session::ApprovalError::Closed) => {
            StatusCode::CONFLICT.into_response()
        }
    }
}
```

路由：

```rust
        .route(
            "/sessions/{session_key}/approvals/{approval_id}/answer",
            post(crate::domains::agent::session::handler::answer_approval),
        )
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::approvals::tests -v
cargo test -p tinyiothub-cloud session::handler::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/approvals.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/session/handler.rs \
  apps/cloud/src/domains/agent/host/chat/handler/mod.rs
git commit -m "feat(session): add approval broker and answer endpoint"
```

---

### Task 4: 权限守卫工具与注册接线

**Files:**

- Create: `apps/cloud/src/domains/agent/session/tool_guard.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/service.rs`
- Test: `tool_guard.rs` 测试模块

**Interfaces:**

- `PermissionGuardTool::new(inner, assistant_id, contexts, approvals, events, safety, timeout)`。
- 行为：
  - 无 TurnContext → 直接执行 inner（非聊天路径不受影响）。
  - `ReadOnly` 工具 → 直接执行。
  - `read_only` 模式 + Write/Destructive → 失败 `"只读模式：该操作需要写权限"`，不产生事件。
  - `ask` + Write/Destructive → 注册审批、emit `approval.requested { approvalId, action, summary }`、等待 `APPROVAL_TIMEOUT`（10 分钟）；批准执行 inner；拒绝/超时/关闭 → 失败。
  - `full_access` → 直接执行 inner（内层 token/Effect 闸门照旧）。
  - 审批结果无论批准/拒绝都 emit `approval.resolved { approvalId, resolution: "approved"|"denied" }`。
- 包装发生在 `chat_builtin_tools_provider`：`session_primitive_agent(runtime)` 为 Some 时，对每个 `(tool, safety)` 中 `safety != ReadOnly` 的工具套 `PermissionGuardTool`。

- [ ] **Step 1: 写失败测试**

`tool_guard.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::approvals::{ApprovalBroker, ApprovalDecision};
    use crate::domains::agent::session::context::{TurnContext, TurnContextRegistry};
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::types::PermissionMode;
    use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
    use tinyiothub_agent::port::tool::ToolResult;
    use tinyiothub_skills::trust::ToolSafety;

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct EchoTool { calls: Arc<AtomicUsize> }

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "set_device" }
        fn description(&self) -> &str { "test" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolResult { success: true, output: "ok".into(), error: None, attachments: vec![] })
        }
    }

    fn echo() -> (Box<dyn Tool>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (Box::new(EchoTool { calls: calls.clone() }), calls)
    }

    impl Attributable for EchoTool {
        fn role(&self) -> Role { Role::Tool(ToolKind::Plugin) }
        fn alias(&self) -> &str { "set_device" }
    }

    fn context(mode: PermissionMode) -> TurnContext {
        TurnContext::new("ws".into(), "agent_main".into(), "agent:ws:agent_main/s1".into(), "run1".into(), 0)
            .with_permission_mode(mode)
    }

    #[tokio::test]
    async fn read_only_blocks_write_without_calling_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::ReadOnly));
        let tool = PermissionGuardTool::new(
            inner, "agent_main".into(), contexts,
            Arc::new(ApprovalBroker::default()), Arc::new(RecordingSink::default()),
            ToolSafety::Write, std::time::Duration::from_secs(1),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("只读"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn ask_waits_for_approval_then_executes() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::Ask));
        let broker = Arc::new(ApprovalBroker::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = Arc::new(PermissionGuardTool::new(
            inner, "agent_main".into(), contexts,
            broker.clone(), sink.clone(), ToolSafety::Write, std::time::Duration::from_secs(5),
        ));
        let handle = tokio::spawn({
            let tool = tool.clone();
            async move { tool.execute(serde_json::json!({})).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let pending = broker.list_for_session("agent:ws:agent_main/s1");
        assert_eq!(pending.len(), 1);
        broker
            .answer("agent:ws:agent_main/s1", &pending[0].approval_id, ApprovalDecision::Approved)
            .unwrap();
        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, vec!["approval.requested", "approval.resolved"]);
        assert_eq!(events[1].payload_json["resolution"], "approved");
    }

    #[tokio::test]
    async fn full_access_runs_inner() {
        let (inner, calls) = echo();
        let contexts = Arc::new(TurnContextRegistry::default());
        let _guard = contexts.set(context(PermissionMode::FullAccess));
        let tool = PermissionGuardTool::new(
            inner, "agent_main".into(), contexts,
            Arc::new(ApprovalBroker::default()), Arc::new(RecordingSink::default()),
            ToolSafety::Destructive, std::time::Duration::from_secs(1),
        );
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::tool_guard::tests -v`

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`tool_guard.rs`：

```rust
//! 会话权限守卫：按 TurnContext.permission_mode 决定 ReadOnly/Ask/FullAccess。
//!
//! 只包裹 Write/Destructive 工具；ReadOnly 直接透传。审批事件进入会话时间线。

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_skills::trust::ToolSafety;
use tokio::time::timeout;

use crate::domains::agent::session::approvals::{ApprovalBroker, ApprovalDecision};
use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::types::PermissionMode;

pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct PermissionGuardTool {
    inner: Box<dyn Tool>,
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    approvals: Arc<ApprovalBroker>,
    events: Arc<dyn SessionEventSink>,
    safety: ToolSafety,
    timeout: Duration,
}

impl PermissionGuardTool {
    pub fn new(
        inner: Box<dyn Tool>,
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        approvals: Arc<ApprovalBroker>,
        events: Arc<dyn SessionEventSink>,
        safety: ToolSafety,
        timeout: Duration,
    ) -> Self {
        Self { inner, assistant_id, contexts, approvals, events, safety, timeout }
    }

    fn denied(message: impl Into<String>) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some(message.into()),
            attachments: vec![],
        })
    }
}

#[async_trait::async_trait]
impl Tool for PermissionGuardTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        if self.safety == ToolSafety::ReadOnly {
            return self.inner.execute(args).await;
        }
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return self.inner.execute(args).await;
        };
        match context.permission_mode {
            PermissionMode::FullAccess => self.inner.execute(args).await,
            PermissionMode::ReadOnly => Self::denied("只读模式：该操作需要写权限，已拒绝执行"),
            PermissionMode::Ask => {
                let summary = serde_json::to_string(&args).unwrap_or_default();
                let summary = summary.chars().take(200).collect::<String>();
                let (approval_id, receiver) =
                    self.approvals.register(&context.session_key, self.inner.name(), &summary);
                if let Err(error) = self
                    .events
                    .emit(session_event(
                        &context,
                        "approval.requested",
                        json!({
                            "approvalId": approval_id,
                            "action": self.inner.name(),
                            "summary": summary
                        }),
                    ))
                    .await
                {
                    self.approvals.cancel(&approval_id);
                    return Err(error.into());
                }

                let decision = match timeout(self.timeout, receiver).await {
                    Ok(Ok(decision)) => decision,
                    Ok(Err(_)) => ApprovalDecision::Denied,
                    Err(_) => {
                        self.approvals.cancel(&approval_id);
                        ApprovalDecision::Denied
                    }
                };
                let resolution = match decision {
                    ApprovalDecision::Approved => "approved",
                    ApprovalDecision::Denied => "denied",
                };
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "approval.resolved",
                        json!({ "approvalId": approval_id, "resolution": resolution }),
                    ))
                    .await;
                match decision {
                    ApprovalDecision::Approved => self.inner.execute(args).await,
                    ApprovalDecision::Denied => Self::denied("操作未获批准，已取消"),
                }
            }
        }
    }
}

impl Attributable for PermissionGuardTool {
    fn role(&self) -> Role {
        self.inner.role()
    }

    fn alias(&self) -> &str {
        self.inner.alias()
    }
}
```

`session/mod.rs` 加 `pub mod tool_guard;` 与 `pub use tool_guard::PermissionGuardTool;`。

`host/tools/service.rs`：在 provider 闭包构建完 `tools` 后、返回前加：

```rust
        if runtime.session_tools_enabled && runtime.agent_id.is_some() {
            let assistant_id = runtime.agent_id.clone().unwrap_or_default();
            tools = tools
                .into_iter()
                .map(|(tool, safety)| {
                    if safety == ToolSafety::ReadOnly {
                        (tool, safety)
                    } else {
                        (
                            Box::new(crate::domains::agent::session::PermissionGuardTool::new(
                                tool,
                                assistant_id.clone(),
                                session.contexts.clone(),
                                session.approvals.clone(),
                                session.events.clone(),
                                safety,
                                crate::domains::agent::session::APPROVAL_TIMEOUT,
                            )) as Box<dyn Tool>,
                            safety,
                        )
                    }
                })
                .collect();
        }
```

（`tools` 必须是 `let mut tools`；若 provider 末尾直接返回 `tools`，按现有代码调整。`APPROVAL_TIMEOUT` 添加到 `session/mod.rs`：`pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);`）

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::tool_guard::tests -v
cargo test -p tinyiothub-cloud host::tools::service::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/tool_guard.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/host/tools/service.rs
git commit -m "feat(session): enforce permission mode with approval-gated tools"
```

---

### Task 5: 审批事件的前端卡片与 HUD 计数

**Files:**

- Modify: `web/src/api/assistant.ts`（`answerApproval`）
- Create: `web/src/ui/chat/approval-card.ts`
- Modify: `web/src/ui/chat/timeline-model.ts`
- Modify: `web/src/ui/views/chat.ts`
- Modify: `apps/cloud/src/domains/agent/session/workspace_api.rs`（HUD approvals = harness + broker）
- Test: 上述 TS 测试模块

**Interfaces:**

- `answerApproval(sessionKey, approvalId, decision: 'approved'|'denied'): Promise<void>`。
- TimelineModel：`pendingApproval: { approvalId; tool; summary } | null`；`approval.requested` 设置，`approval.resolved` 清除。
- `<approval-card .sessionKey .pending @answered>` 渲染工具名与摘要，「批准」「拒绝」按钮。

- [ ] **Step 1: 写失败测试**

`timeline-model.test.ts` 追加：

```ts
it('tracks approval lifecycle', () => {
  const model = new TimelineModel();
  model.ingestEvent(event(1, 'approval.requested', { approvalId: 'a1', action: 'set_device', summary: '设 24℃' }));
  expect(model.pendingApproval?.approvalId).toBe('a1');
  model.ingestEvent(event(2, 'approval.resolved', { approvalId: 'a1', resolution: 'approved' }));
  expect(model.pendingApproval).toBeNull();
});
```

`approval-card.test.ts`：

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ answerApproval: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ answerApproval: hoisted.answerApproval }));

import './approval-card.js';
import type { ApprovalCard } from './approval-card.js';

describe('approval-card', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.answerApproval.mockReset();
  });

  it('approves and emits answered', async () => {
    hoisted.answerApproval.mockResolvedValue(undefined);
    const el = document.createElement('approval-card') as ApprovalCard;
    el.sessionKey = 'agent:ws:a/s1';
    el.pending = { approvalId: 'a1', tool: 'set_device', summary: '设 24℃' };
    const answered: string[] = [];
    el.addEventListener('answered', () => answered.push('yes'));
    document.body.appendChild(el);
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.approval-approve') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(hoisted.answerApproval).toHaveBeenCalledWith('agent:ws:a/s1', 'a1', 'approved');
    expect(answered).toHaveLength(1);
  });
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npx vitest run src/ui/chat/timeline-model.test.ts src/ui/chat/approval-card.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`assistant.ts` 追加：

```ts
export async function answerApproval(
  sessionKey: string,
  approvalId: string,
  decision: 'approved' | 'denied',
): Promise<void> {
  const response = await fetch(
    buildUrl(sessionPath(sessionKey, `/approvals/${encodeURIComponent(approvalId)}/answer`)),
    {
      method: 'POST',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      credentials: 'include',
      body: JSON.stringify({ decision }),
    },
  );
  if (response.status === 204) return;
  if (response.status === 404) throw new ApiError(404, '审批不存在或已处理');
  if (response.status === 403) throw new ApiError(403, '审批属于其他会话');
  if (response.status === 409) throw new ApiError(409, '审批已关闭');
  throw new ApiError(response.status, '提交审批失败');
}
```

`timeline-model.ts`：

```ts
export interface PendingApproval {
  approvalId: string;
  tool: string;
  summary: string;
}
```

`TimelineModel` 加字段 `pendingApproval: PendingApproval | null = null;`，`reset()` 中置 null，并在 `switch` 中加：

```ts
      case 'approval.requested':
        this.pendingApproval = {
          approvalId: asString(payload.approvalId),
          tool: asString(payload.action),
          summary: asString(payload.summary),
        };
        break;
      case 'approval.resolved': {
        const approvalId = asString(payload.approvalId);
        if (this.pendingApproval?.approvalId === approvalId) this.pendingApproval = null;
        this.blocks.push({
          kind: 'notice',
          id: approvalId || event.eventId,
          tone: 'answered',
          text: `审批：${asString(payload.resolution) || 'unknown'}`,
          at,
        });
        break;
      }
```

`approval-card.ts`：

```ts
import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerApproval } from '../../api/assistant.js';
import type { PendingApproval } from './timeline-model.js';

@customElement('approval-card')
export class ApprovalCard extends LitElement {
  static styles = css`
    :host { display: block; width: 100%; max-width: 760px; margin: 0 auto 10px; }
    .card { border: 1px solid var(--warn, #f59e0b); border-radius: var(--radius-md); background: var(--card); padding: 12px; }
    .head { font-size: 12px; color: var(--warn, #f59e0b); margin-bottom: 6px; }
    .tool { font-family: var(--mono); font-size: 12.5px; color: var(--text); }
    .summary { margin-top: 6px; font-size: 12px; color: var(--muted); max-height: 120px; overflow: auto; white-space: pre-wrap; }
    .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
    .approval-approve, .approval-deny { border-radius: var(--radius-sm); padding: 6px 14px; cursor: pointer; font-size: 12.5px; }
    .approval-approve { background: var(--accent); color: #fff; border: 0; }
    .approval-deny { background: transparent; color: var(--text); border: 1px solid var(--border); }
    button:disabled { opacity: 0.5; cursor: default; }
    .done { font-size: 12px; color: var(--muted); }
    .error { color: var(--danger, #ef4444); font-size: 12px; margin-top: 6px; }
  `;

  @property({ attribute: false }) sessionKey = '';
  @property({ attribute: false }) pending: PendingApproval | null = null;

  @state() private submitting = false;
  @state() private done = false;
  @state() private error = '';

  protected willUpdate(changed: PropertyValues): void {
    if (changed.has('pending') && this.pending) {
      this.done = false;
      this.error = '';
    }
  }

  private async decide(decision: 'approved' | 'denied'): Promise<void> {
    if (!this.pending || this.submitting) return;
    this.submitting = true;
    this.error = '';
    try {
      await answerApproval(this.sessionKey, this.pending.approvalId, decision);
      this.done = true;
      this.dispatchEvent(new CustomEvent('answered'));
    } catch (error) {
      this.error = (error as Error).message || '提交审批失败';
    } finally {
      this.submitting = false;
    }
  }

  render() {
    if (!this.pending) return html``;
    return html`
      <section class="card" aria-label="需要审批">
        <div class="head">需要你的批准</div>
        <div class="tool">${this.pending.tool}</div>
        <div class="summary">${this.pending.summary}</div>
        ${this.error ? html`<div class="error">${this.error}</div>` : ''}
        <div class="actions">
          ${this.done
            ? html`<span class="done">已处理，等待大脑继续</span>`
            : html`
                <button class="approval-deny" ?disabled=${this.submitting} @click=${() => this.decide('denied')}>
                  拒绝
                </button>
                <button class="approval-approve" ?disabled=${this.submitting} @click=${() => this.decide('approved')}>
                  批准
                </button>
              `}
        </div>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'approval-card': ApprovalCard;
  }
}
```

`chat.ts`：导入 `../chat/approval-card.js`，在 `render()` 中 question card 之后插入：

```ts
            ${this.timeline.pendingApproval
              ? html`<approval-card
                  .sessionKey=${this.sessionKey}
                  .pending=${this.timeline.pendingApproval}
                  @answered=${() => this.onApprovalAnswered()}
                ></approval-card>`
              : ''}
```

并加处理函数：

```ts
  private onApprovalAnswered(): void {
    this.timeline.pendingApproval = null;
    void this.loadTimeline(this.sessionKey);
  }
```

HUD 后端：`workspace_api.rs` 的 `approvals` 计算改为：

```rust
    let harness_approvals = match state.db.count_pending_tool_approvals(&workspace_id).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let broker_approvals = state
        .session_runtime
        .approvals
        .count_pending_for_workspace(&workspace_id) as i64;
    let approvals = harness_approvals + broker_approvals;
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npx vitest run src/ui/chat src/ui/views/chat.test.ts
npm run type-check
cargo test -p tinyiothub-cloud session::workspace_api::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/api/assistant.ts web/src/ui/chat/approval-card.ts \
  web/src/ui/chat/approval-card.test.ts web/src/ui/chat/timeline-model.ts \
  web/src/ui/chat/timeline-model.test.ts web/src/ui/views/chat.ts \
  apps/cloud/src/domains/agent/session/workspace_api.rs
git commit -m "feat(assistant): show and answer tool approvals inline"
```

---

### Task 6: Composer 权限模式选择器

**Files:**

- Modify: `web/src/api/assistant.ts`（`updateSessionPermission`）
- Modify: `web/src/ui/views/chat.ts`
- Test: `web/src/ui/views/chat.test.ts`

**Interfaces:**

- `updateSessionPermission(sessionKey, mode: 'read_only'|'ask'|'full_access'): Promise<void>` → PUT `/sessions/{key}/permission`。
- Chat composer 显示三档选择；当前值来自所选会话（sessions list `permissionMode`），修改时 PUT 并更新本地状态与抽屉数据。
- 说明文案：`只读` / `需要审批` / `完全访问`（只免逐次审批，不绕过联锁）。

- [ ] **Step 1: 写失败测试**

`chat.test.ts` 追加（复用现有 mock，增加 `updateSessionPermission` 到 hoisted 与 assistant mock）：

```ts
it('changes the session permission mode', async () => {
  hoisted.listChatSessions.mockResolvedValue([
    { sessionKey: 'agent:ws-1:default/s1', kind: 'chat', status: 'active', label: '新会话', permissionMode: 'ask', updatedAt: 1 },
  ]);
  hoisted.updateSessionPermission.mockResolvedValue(undefined);
  const el = document.createElement('view-chat') as ChatView;
  document.body.appendChild(el);
  await vi.waitFor(() => expect(el.querySelector('.permission-mode')).toBeTruthy());
  const select = el.querySelector('.permission-mode') as HTMLSelectElement;
  select.value = 'read_only';
  select.dispatchEvent(new Event('change'));
  await el.updateComplete;
  expect(hoisted.updateSessionPermission).toHaveBeenCalledWith('agent:ws-1:default/s1', 'read_only');
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npx vitest run src/ui/views/chat.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`assistant.ts`：

```ts
export async function updateSessionPermission(
  sessionKey: string,
  mode: 'read_only' | 'ask' | 'full_access',
): Promise<void> {
  const response = await fetch(buildUrl(sessionPath(sessionKey, '/permission')), {
    method: 'PUT',
    headers: authHeaders({ 'Content-Type': 'application/json' }),
    credentials: 'include',
    body: JSON.stringify({ mode }),
  });
  if (response.status === 204) return;
  if (response.status === 404) throw new ApiError(404, '会话不存在');
  if (response.status === 400) throw new ApiError(400, '权限模式无效');
  throw new ApiError(response.status, '保存权限模式失败');
}
```

`chat.ts`：

```ts
import { updateSessionPermission, listChatSessions } from '../../api/assistant.js';

type PermissionMode = 'read_only' | 'ask' | 'full_access';

  @state() private permissionMode: PermissionMode = 'ask';

  private async loadPermissionMode(): Promise<void> {
    try {
      const sessions = await listChatSessions(AGENT_ID);
      const current = sessions.find((session) => session.sessionKey === this.sessionKey);
      const mode = current?.permissionMode;
      if (mode === 'read_only' || mode === 'ask' || mode === 'full_access') {
        this.permissionMode = mode;
      }
    } catch {
      // 读取失败保持当前值；发送/审批仍受后端会话模式约束。
    }
  }

  private async handlePermissionChange(mode: PermissionMode): Promise<void> {
    const previous = this.permissionMode;
    this.permissionMode = mode;
    try {
      await updateSessionPermission(this.sessionKey, mode);
      this.refreshDrawer();
    } catch (error) {
      this.permissionMode = previous;
      this.error = (error as Error).message || '保存权限模式失败';
    }
  }
```

在 `selectSession` 末尾调用 `void this.loadPermissionMode();`。

`SessionSummary` 类型补 `permissionMode: 'read_only' | 'ask' | 'full_access' | string;`（Task 1 后端已返回该字段）。

把 Plan 3 composer 中的只读徽标：

```html
<span class="codex-mode" title="权限模式接线在 Plan 4">需要审批</span>
```

替换为：

```html
<select class="permission-mode" .value=${this.permissionMode}
  @change=${(event: Event) => this.handlePermissionChange((event.target as HTMLSelectElement).value as 'read_only'|'ask'|'full_access')}>
  <option value="read_only">只读</option>
  <option value="ask">需要审批</option>
  <option value="full_access">完全访问</option>
</select>
<span class="codex-mode-hint" title="完全访问只免除逐次审批，不绕过急停/联锁/Effect">?</span>
```

样式加到 `codex.css`：

```css
.permission-mode { font-size: 11px; color: var(--text); background: var(--card); border: 1px solid var(--border); border-radius: 999px; padding: 3px 8px; }
.codex-mode-hint { font-size: 11px; color: var(--muted); }
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npx vitest run src/ui/views/chat.test.ts && npm run type-check && npm run lint
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/api/assistant.ts web/src/ui/views/chat.ts \
  web/src/ui/views/chat.test.ts web/src/styles/chat/codex.css
git commit -m "feat(web): add session permission mode selector"
```

---

### Task 7: 报警触发进入 TriggerRouter

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/orchestrator/callbacks.rs`
- Modify: `apps/cloud/src/domains/agent/orchestrator/mod.rs`
- Modify: `apps/cloud/src/shared/service_manager.rs`
- Test: `trigger.rs` / `wake.rs` 测试模块

**Interfaces:**

- `EventWake { workspace_id, trigger: String, source_id: String, summary: String }`（`trigger = "alarm"`）。
- `TriggerRouter::with_event_bus(event_bus: Arc<EventBus>) -> Self`（或 `new` 增加 `Option<Arc<EventBus>>`；保留旧 `new` 供测试）。
- `TriggerRouter::submit_event(&self, wake: EventWake) -> bool`：同 workspace 在途或 3/min 超限则拒绝。
- `WakeExecutor::execute_event(&self, wake: EventWake, cancel: CancellationToken) -> WakeReport`（默认返回 `Skipped`）。
- `WakeRunner` 实现 `execute_event`：创建 `kind="event"` Session，emit `trigger.received { trigger: "alarm", sourceId }` → `wake.dispatched` → `session.started { kind: "event" }` → turn → `session.ended`，消息为：

```
收到告警：<summary>
请按「应急预案」技能（如未加载则按默认职责）评估影响、给出处置建议；涉及写设备必须走审批。
```

- `HeartbeatBridge::set_alarm_dispatch(&self, enabled: bool)`（`AtomicBool`，默认 true）；`dispatch_alarm_investigation` 在关闭时直接返回。`Orchestrator::set_alarm_dispatch` 转发。

- [ ] **Step 1: 写失败测试**

`trigger.rs` 测试追加（复用 Task 5 的 `BlockingExecutor` 模式；`events` 计数 + `started`/`release` 通知）：

```rust
#[tokio::test]
async fn event_wake_dedupes_while_in_flight_then_dispatches() {
    let db = Arc::new(test_db().await);
    let now = ts(1_800_000_000);
    db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now).await.unwrap();

    struct BlockingExecutor {
        calls: AtomicUsize,
        started: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for BlockingExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: tokio_util::sync::CancellationToken) -> WakeReport {
            WakeReport { session_key: "s".into(), outcome: crate::domains::agent::session::wake::WakeOutcome::Completed }
        }
        async fn execute_event(&self, _wake: EventWake, _cancel: tokio_util::sync::CancellationToken) -> WakeReport {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            self.release.notified().await;
            WakeReport { session_key: "e".into(), outcome: crate::domains::agent::session::wake::WakeOutcome::Completed }
        }
    }

    let executor = Arc::new(BlockingExecutor {
        calls: AtomicUsize::new(0),
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));

    assert!(router.submit_event(EventWake {
        workspace_id: "ws-1".into(),
        trigger: "alarm".into(),
        source_id: "alarm-1".into(),
        summary: "3 号楼温度过高".into(),
    }));
    executor.started.notified().await;
    assert!(!router.submit_event(EventWake {
        workspace_id: "ws-1".into(),
        trigger: "alarm".into(),
        source_id: "alarm-2".into(),
        summary: "another".into(),
    }));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    executor.release.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
```

`wake.rs` 测试追加：

```rust
#[tokio::test]
async fn event_wake_writes_event_session_lifecycle() {
    let (wake, agent_pool, sink) = fixture().await;
    let agent = agent_pool
        .create(&assistant_agent_id("ws"), "ws", &AgentRuntimeConfig::default(), vec![], true)
        .unwrap();
    let report = wake
        .run_event_with_agent(
            EventWake { workspace_id: "ws".into(), trigger: "alarm".into(), source_id: "alarm-1".into(), summary: "温度过高".into() },
            agent,
            "收到告警：温度过高",
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert_eq!(report.outcome, WakeOutcome::Completed);
    let events = sink.events.lock().await;
    let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
    assert_eq!(types, vec!["trigger.received", "wake.dispatched", "session.started", "turn.started", "message.created", "turn.completed", "session.ended"]);
    assert_eq!(events[2].payload_json["kind"], "event");
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::trigger::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
```

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`trigger.rs`：

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::VecDeque;
use std::time::Instant;

use axum::async_trait;
use tinyiothub_core::models::event::{ContentElement, EventType};
use tinyiothub_runtime::event_bus::EventBus;

#[derive(Debug, Clone)]
pub struct EventWake {
    pub workspace_id: String,
    pub trigger: String,
    pub source_id: String,
    pub summary: String,
}

struct RateWindow {
    hits: VecDeque<Instant>,
}

pub struct TriggerRouter {
    ...
    event_rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<EventWake>>,
    event_tx: tokio::sync::mpsc::Sender<EventWake>,
    event_bus: Option<Arc<EventBus>>,
    rate: DashMap<String, RateWindow>,
    event_counter: Arc<AtomicU64>,
}
```

`new` 初始化 `mpsc::channel(256)`。`submit_event`：

```rust
    pub fn submit_event(&self, wake: EventWake) -> bool {
        if self.in_flight.contains(&wake.workspace_id) {
            return false;
        }
        let key = format!("{}:{}", wake.workspace_id, wake.trigger);
        let now = Instant::now();
        {
            let mut window = self.rate.entry(key).or_insert_with(|| RateWindow { hits: VecDeque::new() });
            while window.hits.front().is_some_and(|at| now.duration_since(*at).as_secs() >= 60) {
                window.hits.pop_front();
            }
            if window.hits.len() >= 3 {
                return false;
            }
            window.hits.push_back(now);
        }
        self.event_tx.try_send(wake).is_ok()
    }
```

`TriggerRouter` 的完整新结构（替换 Plan 2 的字段与 `new`）：

```rust
use parking_lot::Mutex as ParkingMutex;

pub struct TriggerRouter {
    db: Arc<Db>,
    executor: Arc<dyn WakeExecutor>,
    in_flight: Arc<DashSet<String>>,
    shutdown: CancellationToken,
    event_bus: Option<Arc<EventBus>>,
    event_tx: tokio::sync::mpsc::Sender<EventWake>,
    event_rx: ParkingMutex<Option<tokio::sync::mpsc::Receiver<EventWake>>>,
    rate: DashMap<String, RateWindow>,
}

impl TriggerRouter {
    pub fn new(db: Arc<Db>, executor: Arc<dyn WakeExecutor>) -> Self {
        Self::new_with_event_bus(db, executor, None)
    }

    pub fn new_with_event_bus(
        db: Arc<Db>,
        executor: Arc<dyn WakeExecutor>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);
        Self {
            db,
            executor,
            in_flight: Arc::new(DashSet::new()),
            shutdown: CancellationToken::new(),
            event_bus,
            event_tx,
            event_rx: ParkingMutex::new(Some(event_rx)),
            rate: DashMap::new(),
        }
    }
```

`start` 完整实现（保留原 tick loop，另起一个事件消费循环；先订阅报警再返回）：

```rust
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), crate::shared::error::Error>> {
        if let Some(bus) = self.event_bus.clone() {
            let tx = self.event_tx.clone();
            tokio::spawn(subscribe_alarms(bus, tx, self.shutdown.clone()));
        }
        if let Some(mut receiver) = self.event_rx.lock().take() {
            let router = Arc::clone(&self);
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = router.shutdown.cancelled() => return,
                        wake = receiver.recv() => {
                            let Some(wake) = wake else { return };
                            let workspace_id = wake.workspace_id.clone();
                            if !router.in_flight.insert(workspace_id.clone()) {
                                continue;
                            }
                            let guard = InFlightGuard {
                                set: Arc::clone(&router.in_flight),
                                key: workspace_id,
                            };
                            let router = Arc::clone(&router);
                            tokio::spawn(async move {
                                let _guard = guard;
                                router.dispatch_event(wake).await;
                            });
                        }
                    }
                }
            });
        }
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TRIGGER_TICK_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = self.shutdown.cancelled() => return Ok::<(), crate::shared::error::Error>(()),
                    _ = ticker.tick() => {
                        let _ = self.tick(Utc::now()).await;
                    }
                }
            }
        })
    }
```

`dispatch_event`:

```rust
    async fn dispatch_event(self: Arc<Self>, wake: EventWake) {
        let cancel = self.shutdown.clone();
        let report = self.executor.execute_event(wake.clone(), cancel).await;
        tracing::info!(workspace_id = %wake.workspace_id, source_id = %wake.source_id, outcome = report.outcome.as_str(), "event wake dispatched");
    }
```

先把 `orchestrator/callbacks.rs` 的私有辅助改为可复用（函数已存在，仅改可见性）：

```rust
pub(crate) fn extract_payload(content: &RichContent) -> Option<String> {
    content.elements().iter().find_map(|el| match el {
        ContentElement::Text { content, .. } => Some(content.clone()),
        _ => None,
    })
}
```

`subscribe_alarms`:

```rust
use crate::domains::agent::event::types::AiEvent;
use crate::domains::agent::orchestrator::callbacks::extract_payload;

async fn subscribe_alarms(
    bus: Arc<EventBus>,
    tx: tokio::sync::mpsc::Sender<EventWake>,
    shutdown: CancellationToken,
) {
    let mut rx = bus.subscribe();
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            event = rx.recv() => {
                let Ok(event) = event else { continue };
                if !matches!(event.event_type(), EventType::Ai(_)) { continue; }
                let Some(text) = extract_payload(event.content()) else { continue };
                let Ok(ai) = serde_json::from_str::<AiEvent>(&text) else { continue };
                let AiEvent::AlarmCreated(alarm) = ai else { continue };
                if !matches!(alarm.severity.as_str(), "critical" | "error") { continue; }
                let wake = EventWake {
                    workspace_id: alarm.workspace_id.clone(),
                    trigger: "alarm".to_string(),
                    source_id: alarm.id.clone(),
                    summary: format!("{}（{}，设备 {}）", alarm.message, alarm.severity, alarm.thing_id),
                };
                let _ = tx.try_send(wake);
            }
        }
    }
}
```

`WakeExecutor` 默认方法：

```rust
    async fn execute_event(&self, _wake: EventWake, _cancel: CancellationToken) -> WakeReport {
        WakeReport { session_key: String::new(), outcome: WakeOutcome::Skipped }
    }
```

（trait 定义迁移到 `trigger.rs` 或保持 `wake.rs`；`EventWake` 在 `wake.rs` 定义以免循环：`pub use super::trigger::EventWake;` 视模块组织而定。）

`wake.rs` 加 `run_event` / `run_event_with_agent`，与 `run_with_agent` 共享生命周期写事件，只是 kind 为 `event`、`trigger.received` 的 `trigger` 为 `"alarm"`、`sourceId` 为告警 id。

`callbacks.rs`：

```rust
pub struct HeartbeatBridge {
    ...
    alarm_dispatch: Arc<AtomicBool>,
}

impl HeartbeatBridge {
    pub fn set_alarm_dispatch(&self, enabled: bool) {
        self.alarm_dispatch.store(enabled, Ordering::SeqCst);
    }

    pub async fn dispatch_alarm_investigation(&self, alarm: &AlarmEvent) {
        if !self.alarm_dispatch.load(Ordering::SeqCst) {
            return;
        }
        ...
    }
}
```

`Orchestrator::set_alarm_dispatch` 转发到 handler → bridge（`AiEventHandler` 增加同名字段与方法）。

`service_manager.rs` 启动（构造 router 之后、报警桥关闭）：

```rust
            let event_bus = app_state.event_bus.clone();
            let trigger_router = Arc::new(
                crate::domains::agent::session::TriggerRouter::new_with_event_bus(
                    app_state.db.clone(),
                    wake_runner,
                    event_bus,
                ),
            );
            if let Some(orchestrator) = &self.orchestrator {
                orchestrator.set_alarm_dispatch(false);
            }
```

（顺序保证：router 先订阅，再关闭旧桥，避免窗口内丢失告警。）

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::trigger::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
cargo test -p tinyiothub-cloud --lib thing_agent -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/trigger.rs \
  apps/cloud/src/domains/agent/session/wake.rs \
  apps/cloud/src/domains/agent/orchestrator/callbacks.rs \
  apps/cloud/src/domains/agent/orchestrator/mod.rs \
  apps/cloud/src/shared/service_manager.rs
git commit -m "feat(agent): route alarm wakes into the session trigger router"
```

---

### Task 8: `inspection` 技能种子化与唤醒优先级

**Files:**

- Create: `apps/cloud/templates/skills/inspection.md`
- Create: `apps/cloud/src/domains/agent/session/skills_seed.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/shared/service_manager.rs`
- Modify: `apps/cloud/src/domains/tenant/workspace/service.rs`
- Test: `skills_seed.rs` 与 `wake.rs` 测试模块

**Interfaces:**

- `ensure_inspection_skill(db: &Db, workspace_id: &str) -> anyhow::Result<bool>`：若 `data/agents/<ws>/skills/inspection.md` 不存在，则从当前 active heartbeat tasks 生成步骤（无任务时用缺省步骤）写入；返回是否新建。
- `WakeRunner` 唤醒消息优先级：`schedule.prompt` > `inspection` 技能正文（`load_workspace_skills`）> legacy tasks > 默认职责。函数签名调整为 `build_wake_message(schedule, inspection: Option<&LoadedSkill>, tasks)`。
- Boot 与 workspace 创建时调用 `ensure_inspection_skill`（失败只 warn）。

- [ ] **Step 1: 写失败测试**

`skills_seed.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    #[tokio::test]
    async fn seeds_inspection_skill_from_tasks_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = test_db().await;
        db.replace_heartbeat_tasks(
            "ws-skill",
            &[tinyiothub_core::heartbeat::NewHeartbeatTask {
                priority: "high".into(),
                text: "检查离线设备".into(),
                paused: false,
            }],
        )
        .await
        .unwrap();

        assert!(ensure_inspection_skill_at(dir.path(), &db, "ws-skill").await.unwrap());
        assert!(!ensure_inspection_skill_at(dir.path(), &db, "ws-skill").await.unwrap());
        let body = std::fs::read_to_string(dir.path().join("skills/inspection.md")).unwrap();
        assert!(body.contains("检查离线设备"));
        assert!(body.contains("name: inspection"));
    }
}
```

`wake.rs` 测试追加：

```rust
#[test]
fn wake_message_prefers_inspection_skill_over_tasks() {
    let skill = tinyiothub_skills::LoadedSkill {
        name: "inspection".into(),
        title: "巡检".into(),
        description: String::new(),
        version: "1".into(),
        inject: tinyiothub_skills::InjectMode::Trigger,
        trigger: Some("/inspection".into()),
        body: "# 巡检\n## 执行步骤\n1. 检查离线设备".into(),
    };
    let message = build_wake_message(&schedule(""), Some(&skill), &[task("旧任务", false)]).unwrap();
    assert!(message.contains("检查离线设备"));
    assert!(!message.contains("旧任务"));
    let custom = build_wake_message(&schedule("自定义"), Some(&skill), &[task("旧任务", false)]).unwrap();
    assert_eq!(custom, "自定义");
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::skills_seed::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
```

Expected: FAIL。

- [ ] **Step 3: 写最小实现**

`apps/cloud/templates/skills/inspection.md`：

```markdown
---
name: inspection
version: "1"
trigger: /inspection
inject: index
---

# 巡检

## 执行步骤

1. 检查离线设备：列出离线超过阈值的设备，判断是否可自动重连。
2. 扫描未处理的高优先级告警：按严重程度排序，给出处置建议。
3. 生成状态摘要：聚焦异常与离线设备，正常设备一行带过。

## 注意事项

- 涉及写设备（重连、下发命令、改属性）必须先获得审批，不得自动执行。
- 没有异常时简短说明即可，不要编造进度。
```

`skills_seed.rs`：

```rust
//! 把巡检职责从 heartbeat_tasks 迁移为 workspace 技能正文。
//!
//! 只在技能文件不存在时写入；已存在的用户编辑永不覆盖。

use std::path::Path;

use tinyiothub_storage::Db;

const DEFAULT_BODY: &str = "# 巡检\n\n## 执行步骤\n\n1. 检查离线设备并尝试自动重连。\n2. 扫描未处理的高优先级告警。\n3. 生成设备状态摘要（聚焦异常与离线设备）。\n\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n";

pub async fn ensure_inspection_skill_at(
    workspace_dir: &Path,
    db: &Db,
    workspace_id: &str,
) -> anyhow::Result<bool> {
    let skills_dir = workspace_dir.join("skills");
    let path = skills_dir.join("inspection.md");
    if path.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(&skills_dir)?;
    let tasks = db.list_heartbeat_tasks(workspace_id).await.unwrap_or_default();
    let body = if tasks.iter().any(|task| !task.paused) {
        let mut body = String::from("# 巡检\n\n## 执行步骤\n\n");
        for (index, task) in tasks.iter().filter(|task| !task.paused).enumerate() {
            body.push_str(&format!("{}. {}\n", index + 1, task.text));
        }
        body.push_str("\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n");
        body
    } else {
        DEFAULT_BODY.to_string()
    };
    let content = format!("---\nname: inspection\nversion: \"1\"\ntrigger: /inspection\ninject: index\n---\n\n{body}");
    std::fs::write(&path, content)?;
    Ok(true)
}

pub async fn ensure_inspection_skill(db: &Db, workspace_id: &str) -> anyhow::Result<bool> {
    let dir = crate::domains::agent::prompt::paths::workspace_dir(workspace_id);
    ensure_inspection_skill_at(&dir, db, workspace_id).await
}
```

`wake.rs`：

```rust
use tinyiothub_skills::LoadedSkill;

pub fn build_wake_message(
    schedule: &AssistantSchedule,
    inspection: Option<&LoadedSkill>,
    tasks: &[HeartbeatTask],
) -> Option<String> {
    let custom = schedule.prompt.trim();
    if !custom.is_empty() {
        return Some(custom.to_string());
    }
    if let Some(skill) = inspection {
        return Some(format!(
            "按「{}」技能执行本轮检查。\n\n{}",
            skill.title, skill.body
        ));
    }
    let active: Vec<&HeartbeatTask> = tasks.iter().filter(|task| !task.paused).collect();
    if !tasks.is_empty() && active.is_empty() {
        return None;
    }
    if active.is_empty() {
        return Some(
            "按照你的默认职责，检查当前工作区状态并处理需要关注的事项。若无事可做，简短说明即可。"
                .to_string(),
        );
    }
    let mut message = String::from("本轮默认职责：\n");
    for task in active {
        message.push_str(&format!("- {}\n", task.text));
    }
    message.push_str("按顺序执行；用 todo_write 维护进度，完成后总结结果。");
    Some(message)
}
```

`WakeRunner::run` 中：

```rust
        let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(&schedule.workspace_id);
        let skills = crate::domains::agent::pool::load_workspace_skills(&ws_dir);
        let inspection = skills.iter().find(|skill| skill.name == "inspection");
        let Some(message) = build_wake_message(schedule, inspection, &tasks) else {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        };
```

`service_manager.rs` boot loop（与 heartbeat 补种同处）加：

```rust
                if let Err(error) = crate::domains::agent::session::ensure_inspection_skill(
                    &app_state.db,
                    ws_id,
                )
                .await
                {
                    tracing::warn!(%ws_id, %error, "seed inspection skill failed");
                }
```

`tenant/workspace/service.rs` 的 `create()`（`seed_default_heartbeat_tasks` 之后）加同调用（使用该文件的 db 句柄）。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::skills_seed::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
cargo test -p tinyiothub-cloud session::workspace_api::tests -v
cargo check -p tinyiothub-cloud
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/templates/skills/inspection.md \
  apps/cloud/src/domains/agent/session/skills_seed.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/session/wake.rs \
  apps/cloud/src/shared/service_manager.rs \
  apps/cloud/src/domains/tenant/workspace/service.rs
git commit -m "feat(session): seed inspection skill and prefer it for wakes"
```

---

### Task 9: 端到端与全量验证

**Files:** 仅测试与验证。

- [ ] **Step 1: 后端安全回归测试**

新增 `apps/cloud/src/domains/agent/session/tool_guard.rs` 测试（若 Task 4 未覆盖）：

```rust
#[tokio::test]
async fn ask_denied_does_not_execute_inner() {
    let inner = Arc::new(EchoTool { calls: Default::default() });
    let contexts = Arc::new(TurnContextRegistry::default());
    let _guard = contexts.set(context(PermissionMode::Ask));
    let broker = Arc::new(ApprovalBroker::default());
    let tool = Arc::new(PermissionGuardTool::new(
        inner.clone(), "agent_main".into(), contexts, broker.clone(),
        Arc::new(RecordingSink::default()), ToolSafety::Destructive,
        std::time::Duration::from_secs(5),
    ));
    let handle = tokio::spawn({ let tool = tool.clone(); async move { tool.execute(serde_json::json!({})).await } });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let pending = broker.list_for_session("agent:ws:agent_main/s1");
    broker.answer("agent:ws:agent_main/s1", &pending[0].approval_id, ApprovalDecision::Denied).unwrap();
    let result = handle.await.unwrap().unwrap();
    assert!(!result.success);
    assert_eq!(inner.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}
```

- [ ] **Step 2: 运行全量门禁**

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npm test && npm run type-check && npm run build
```

Expected: 全部通过。若 clippy 报 `too_many_arguments`，沿用现有 allow，不加新的宽松 allow。

- [ ] **Step 3: 提交（仅当新增了测试）**

```bash
git add apps/cloud/src/domains/agent/session/tool_guard.rs
git commit -m "test(session): prove denied approvals never execute tools"
```

---

## 验证清单

- [ ] `chat_sessions.permission_mode` 默认 `ask`；PUT 权限端点跨 workspace 返回 404，非法值 400。
- [ ] `read_only` 下 Write/Destructive 工具返回“只读模式”失败且 inner 未执行。
- [ ] `ask` 下 Write/Destructive 产生 `approval.requested`，前端可批准/拒绝；批准执行、拒绝不执行；`approval.resolved` 两端都写。
- [ ] `full_access` 不弹本层审批，但 `call_device` 的 token/Effect 闸门仍然生效（安全回归用例保留通过）。
- [ ] Critical/Error 告警由 `TriggerRouter` 产生 `kind="event"` Session；旧 `HeartbeatBridge` 不再投递；3/min 与 workspace 在途去重生效。
- [ ] `inspection` 技能在缺省时被种子化（来自现有任务或默认步骤），`WakeRunner` 优先使用技能正文。
- [ ] 全部后端/前端门禁通过；没有删除表/旧端点；没有新增绕过 e-stop/联锁/Effect 的路径。

---

## Self-Review

**Spec coverage（Plan 4A 范围）：**

- §10.3 权限三档：Task 1/2/4/6。
- §11 安全：Task 4（只读拦截、ask 审批、full_access 仅免本层）+ 验证清单的设备闸门回归。
- §5.4.3 事件：`approval.requested/resolved` 在 Task 4；`trigger.received` 报警来源在 Task 7。
- §5.1 触发与执行分离：Task 7 报警 → TriggerRouter → Session Loop。
- §12.1 heartbeat_tasks → 技能：Task 8（种子化 + 唤醒优先级；表保留）。
- §6/§9：零配置巡检技能与默认职责保留。
- **不在本计划**：watch 触发迁移、旧代码删除、表下掉、前端旧视图删除、heartbeat_config 写穿（Plan 4B）。

**Placeholder scan：** 本计划没有 `TBD`/`TODO`/“实现细节略”。所有任务给出完整代码、命令与断言。事件负载解析复用 `callbacks::extract_payload`（本计划将其改为 `pub(crate)`）；`EventWake` 定义在 `session/trigger.rs`，`wake.rs` 以 `use super::trigger::EventWake;` 引用（同 crate 模块互引合法）。

**Type consistency：** `PermissionMode::as_str/parse` 在 Task 1/2/4/6 一致；`ApprovalDecision`/`ApprovalBroker` 在 Task 3/4/5 一致；`EventWake` 在 Task 7 的 trigger/wake 中使用同一字段；`build_wake_message(schedule, inspection, tasks)` 在 Task 8 与既有测试一致；`APPROVAL_TIMEOUT` 单一来源（session/mod.rs）。

**已知风险（接受并记录）：**

1. `full_access` 只免本层审批；`call_device` 的 token/Effect 闸门仍会拦截，用户可能在 `full_access` 下仍被要求确认——这是有意为之（安全边界不合并）。
2. 报警迁移后旧 `HeartbeatBridge` 报警派发被关闭；若新订阅因 EventBus 重启/丢帧漏事件，旧路径不会兜底。Plan 4B 随旧桥删除时补启动重放。
3. `inspection` 技能只在文件缺失时写入；已有 workspace 若用户改过 heartbeat tasks，技能不会自动同步（4B 提供一次性迁移）。
4. `ApprovalBroker` 是进程内状态，重启后未决审批丢失；前端会因 404 提示，用户在下一轮重新触发（后续可持久化到 `decisions`）。
