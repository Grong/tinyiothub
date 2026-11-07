# Assistant Automation & HUD Backend Implementation Plan (Plan 2 / 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给常驻 Assistant 接上统一的自主唤醒链路（每 workspace 一个 `Schedule`，默认 5 分钟节拍，经 `TriggerRouter` → `SessionRunner` 执行，并写入 `trigger.received` / `wake.dispatched` / `session.started` / `session.ended`），同时提供后端 Global HUD 聚合接口（Assistant 状态、设备、告警、待答/待批、活动会话、下次唤醒、最近事件）。

**Architecture:** `TriggerRouter` 只负责产生 wake：每 30 秒扫描 `assistant_schedules` 中到期且启用的行，按 workspace 去重后交给 `WakeRunner`；`WakeRunner` 创建 `kind = scheduled` 的 Session，用 `ensure_agent` 取得 workspace Assistant，再经 `SessionRunner::run_turn_bounded` 在唯一 Session Loop 里执行，结束时把 Session 标记 `ended` 并推进 `next_wake_at`。HUD 是查询层：读取既有设备/告警计数、`QuestionBroker` 待答、Harness `decisions` 待批、活动 `TurnContext`、`session_events` 最近事件与 Schedule 状态，不做新的状态表。

**Tech Stack:** Rust 2024、Axum 0.8、Tokio、SQLx/SQLite、`chrono`、`dashmap`、`tokio-util::CancellationToken`、`async-trait`、现有 `SessionRuntime` / `AgentPool` / `ensure_agent`。

## Plan Boundary

本计划是 4 份计划中的第 2 份：

- **Plan 1（已完成）**：后端 Session Loop、三个原语、`session_events`、事件/答题 API、聊天接线。
- **Plan 2（本文件）**：`Schedule` 持久化、`TriggerRouter`（schedule 触发）、`WakeRunner`、Session 生命周期事件补齐、HUD 聚合接口、Schedule 读写接口、legacy timer/heartbeat 唤醒的接管。**仍然只有后端，没有可见 UI。**
- **Plan 3**：Codex 式前端（全局 HUD 浮层、会话时间线、Todo/提问/子代理卡片）。
- **Plan 4**：旧代码清理与命名收敛（含 event/watch 触发器与 heartbeat 任务迁移到技能）。

明确不在本计划内：

- **事件触发（alarm → wake）不在本计划**：legacy `ThingEventTrigger` / `WatchTrigger` 仍在旧 autonomous 管线上运行；同一报警再走新 Session Loop 会造成双重自主运行。事件触发统一到 Session Loop 放在 Plan 4（随旧触发器退役一起做）。
- **前端与 HUD 视觉**：Plan 3。
- **删除 legacy 表/端点/触发器**：Plan 4。本计划只停止挂载 legacy **定时**触发（`TimerTrigger`、`HeartbeatTrigger`），event/watch 触发器保持原样。
- **`ask_user` 无人在线暂停与通知**：spec §13 的完整语义留待 Plan 3/4；本计划的自主 wake 仍暴露三个原语（`session_tools_enabled = true`），因此模型在自主会话中调用 `ask_user` 仍会等待 10 分钟超时。

---

## Global Constraints

以下约束来自 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md`，每个 Task 都隐含包含：

- **命名基线**：`Workspace → Assistant → Session → Turn`；`Task → Run`；`Session → Todo`；`Turn → Subagent`。禁用词：`ThingAgent`、`AutonomousAgent`、`Brain`（代码标识）、`Conversation`、`Thread`、`Chat`（领域名词）、`Workflow`（新功能）、`Heartbeat`（AI 巡检）、`Manager`/`Service`/`Helper`/`Util`（新领域类型）、`Task`（子代理工具 API 名）、`Job`（Agent 工作单元）、`done`（事件名）。
- **事件命名**：`aggregate.past_tense`，事件名必须来自 spec §5.4.3 白名单。payload 键统一 camelCase；Rust/DB 字段保持 snake_case。
- **事件分层**：`session_events` 是唯一 UI 时间线；`hub_ledger` 的 `runtime.v1` 是安全/审计账本，不混写。
- **SessionKey 例外**：外部格式保持 `agent:<workspace>:<assistant>/<session>`；领域内读作 `assistant_id`。
- **调度**：默认 tick 间隔 5 分钟（`300s`），仅作为唤醒节拍；无人在线也照常唤醒，写设备仍必须审批。
- **安全层不动**：policy / trust / approval / Effect / Decision / unknown 对账 / 恢复合同全部保持；自主运行不得自动放行写动作。
- **迁移**：只加表/列/索引，不 drop 表、不改已有列语义；迁移文件名 `YYYYMMDDNNNNNN_<name>.sql`。
- **TDD**：每个 Task 先写失败测试，再写最小实现，再跑测试，再提交。
- **提交**：每个 Task 一个 commit；不要提交工作区里无关的修改（`docs/superpowers/specs/2026-11-02-agent-harness-core-design.md` 与未跟踪的 `docs/superpowers/plans/2026-11-03-agent-harness-core.md` 不要动）。
- **验证**：每 Task 至少跑对应包的测试；最后跑 `cargo fmt --all -- --check`、`cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings`、`cargo test -p tinyiothub-cloud`、`cargo test -p db`。

---

## File Structure

### 新增

- `crates/db/migrations/20261107000002_assistant_schedules.sql` — `assistant_schedules` 表与到期索引。
- `crates/db/migrations/20261107000003_harness_events_recent_index.sql` — workspace Harness DB 的 `session_events(occurred_at)` 索引。
- `crates/db/src/assistant_schedule.rs` — Schedule 行类型、`Db` 读写与到期扫描。
- `apps/cloud/src/domains/agent/session/hud.rs` — HUD 响应 DTO 与 `assistant_status` 纯函数。
- `apps/cloud/src/domains/agent/session/wake.rs` — `assistant_agent_id`、唤醒消息构造、`WakeOutcome`、`WakeRunner`、`WakeExecutor` trait。
- `apps/cloud/src/domains/agent/session/trigger.rs` — `TriggerRouter` 与 30 秒 tick 循环。
- `apps/cloud/src/domains/agent/session/workspace_api.rs` — Schedule 读写与 HUD HTTP 端点。

### 修改

- `crates/db/src/lib.rs` — 注册 `pub mod assistant_schedule;`。
- `crates/db/src/harness_workspace.rs` — 注册 `20261107000003_harness_events_recent_index` 到 workspace migrator。
- `crates/db/src/harness_event.rs` — `Db::recent_session_events`。
- `crates/db/src/harness_runtime_api.rs` — `Db::count_pending_tool_approvals`。
- `apps/cloud/src/domains/agent/session/mod.rs` — 导出 `hud`、`wake`、`trigger`、`workspace_api`。
- `apps/cloud/src/domains/agent/session/events.rs` — `session_started_payload` 构造器。
- `apps/cloud/src/domains/agent/session/questions.rs` — `count_pending_for_workspace`。
- `apps/cloud/src/domains/agent/session/context.rs` — `count_for_workspace`。
- `apps/cloud/src/domains/agent/host/chat/service.rs` — 聊天 `session.started` payload 改用统一构造器（`{ sessionId, kind: "chat" }`）。
- `apps/cloud/src/domains/agent/thing_agent/manager.rs` — `ThingAgentManagerConfig.legacy_wake_triggers`；默认不再挂载 `TimerTrigger`/`HeartbeatTrigger`。
- `apps/cloud/src/shared/service_manager.rs` — 构造并启动 `TriggerRouter`，句柄注册进 `service_handles`。
- `apps/cloud/src/api/mod.rs` — 挂载 `session::workspace_api::create_workspace_router()`。

---

### Task 1: `assistant_schedules` 持久化

**Files:**

- Create: `crates/db/migrations/20261107000002_assistant_schedules.sql`
- Create: `crates/db/src/assistant_schedule.rs`
- Modify: `crates/db/src/lib.rs`
- Test: `crates/db/src/assistant_schedule.rs` 的 `#[cfg(test)] mod tests`

**Interfaces:**

- Consumes: `Db::pool()`、`crate::error::{DbError, Result}`。
- Produces:
  - `AssistantSchedule { workspace_id, assistant_id, interval_seconds: u32, enabled: bool, prompt: String, next_wake_at: Option<DateTime<Utc>>, last_wake_at: Option<DateTime<Utc>>, last_outcome: Option<String> }`
  - `pub const DEFAULT_ASSISTANT_INTERVAL_SECONDS: u32 = 300;`
  - `pub const MIN_ASSISTANT_INTERVAL_SECONDS: u32 = 60;`
  - `pub const MAX_ASSISTANT_INTERVAL_SECONDS: u32 = 86_400;`
  - `Db::ensure_assistant_schedule(&self, workspace_id: &str, assistant_id: &str, now: DateTime<Utc>) -> Result<AssistantSchedule>`
  - `Db::get_assistant_schedule(&self, workspace_id: &str) -> Result<Option<AssistantSchedule>>`
  - `Db::list_due_assistant_schedules(&self, now: DateTime<Utc>, limit: i64) -> Result<Vec<AssistantSchedule>>`
  - `Db::update_assistant_schedule(&self, workspace_id: &str, enabled: Option<bool>, interval_seconds: Option<u32>, prompt: Option<&str>, next_wake_at: Option<DateTime<Utc>>) -> Result<Option<AssistantSchedule>>`
  - `Db::mark_assistant_wake(&self, workspace_id: &str, next_wake_at: DateTime<Utc>, last_wake_at: DateTime<Utc>, last_outcome: &str) -> Result<()>`

- [ ] **Step 1: 写失败测试**

在 `crates/db/src/assistant_schedule.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[tokio::test]
    async fn ensure_creates_default_row_once() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        let created = db
            .ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        assert_eq!(created.interval_seconds, DEFAULT_ASSISTANT_INTERVAL_SECONDS);
        assert!(created.enabled);
        assert_eq!(created.prompt, "");
        assert_eq!(created.next_wake_at, Some(now));

        let later = ts(1_800_000_100);
        let again = db
            .ensure_assistant_schedule("ws-1", "__assistant__ws-1", later)
            .await
            .unwrap();
        assert_eq!(again.next_wake_at, Some(now), "ensure must not overwrite existing row");
    }

    #[tokio::test]
    async fn due_scan_respects_enabled_and_time() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-due", "__assistant__ws-due", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.ensure_assistant_schedule("ws-future", "__assistant__ws-future", now + chrono::Duration::seconds(300))
            .await
            .unwrap();
        db.ensure_assistant_schedule("ws-off", "__assistant__ws-off", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.update_assistant_schedule("ws-off", Some(false), None, None, None)
            .await
            .unwrap();

        let due = db.list_due_assistant_schedules(now, 10).await.unwrap();
        let ids: Vec<&str> = due.iter().map(|row| row.workspace_id.as_str()).collect();
        assert_eq!(ids, vec!["ws-due"]);
    }

    #[tokio::test]
    async fn mark_wake_advances_and_records_outcome() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let next = now + chrono::Duration::seconds(300);
        db.mark_assistant_wake("ws-1", next, now, "completed").await.unwrap();
        let row = db.get_assistant_schedule("ws-1").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(next));
        assert_eq!(row.last_wake_at, Some(now));
        assert_eq!(row.last_outcome.as_deref(), Some("completed"));
    }

    #[tokio::test]
    async fn update_with_none_leaves_fields_and_validates_interval() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let next = now + chrono::Duration::seconds(600);
        let updated = db
            .update_assistant_schedule("ws-1", Some(false), Some(600), Some("巡检 3 号楼"), Some(next))
            .await
            .unwrap()
            .unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.interval_seconds, 600);
        assert_eq!(updated.prompt, "巡检 3 号楼");
        assert_eq!(updated.next_wake_at, Some(next));

        let untouched = db
            .update_assistant_schedule("ws-1", None, None, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(untouched.interval_seconds, 600);
        assert_eq!(untouched.prompt, "巡检 3 号楼");

        let error = db
            .update_assistant_schedule("ws-1", None, Some(5), None, None)
            .await
            .unwrap_err();
        assert!(matches!(error, DbError::Validation { .. }));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p db assistant_schedule::tests -v`

Expected: FAIL，缺少 `AssistantSchedule` / `ensure_assistant_schedule`。

- [ ] **Step 3: 写最小实现**

`crates/db/migrations/20261107000002_assistant_schedules.sql`：

```sql
CREATE TABLE IF NOT EXISTS assistant_schedules (
    workspace_id     TEXT PRIMARY KEY,
    assistant_id     TEXT NOT NULL,
    interval_seconds INTEGER NOT NULL DEFAULT 300,
    enabled          INTEGER NOT NULL DEFAULT 1,
    prompt           TEXT NOT NULL DEFAULT '',
    next_wake_at     TEXT,
    last_wake_at     TEXT,
    last_outcome     TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_assistant_schedules_due
    ON assistant_schedules(enabled, next_wake_at);
```

`crates/db/src/assistant_schedule.rs`：

```rust
//! Assistant Schedule — 每个 workspace 一行，只保存“什么时候唤醒”。
//!
//! 任务内容来自主 prompt、技能或（迁移期）legacy heartbeat tasks；本表不存任务
//! 正文。时间统一以 UTC RFC3339（秒精度）文本存储，保证字典序 == 时间序。

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::error::{DbError, Result};
use crate::Db;

pub const DEFAULT_ASSISTANT_INTERVAL_SECONDS: u32 = 300;
pub const MIN_ASSISTANT_INTERVAL_SECONDS: u32 = 60;
pub const MAX_ASSISTANT_INTERVAL_SECONDS: u32 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantSchedule {
    pub workspace_id: String,
    pub assistant_id: String,
    pub interval_seconds: u32,
    pub enabled: bool,
    pub prompt: String,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn map_row(row: &SqliteRow) -> Result<AssistantSchedule> {
    Ok(AssistantSchedule {
        workspace_id: row.try_get("workspace_id")?,
        assistant_id: row.try_get("assistant_id")?,
        interval_seconds: row.try_get::<i64, _>("interval_seconds")?.max(0) as u32,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        prompt: row.try_get("prompt")?,
        next_wake_at: row
            .try_get::<Option<String>, _>("next_wake_at")?
            .as_deref()
            .and_then(parse_ts),
        last_wake_at: row
            .try_get::<Option<String>, _>("last_wake_at")?
            .as_deref()
            .and_then(parse_ts),
        last_outcome: row.try_get("last_outcome")?,
    })
}

async fn get_row(pool: &sqlx::SqlitePool, workspace_id: &str) -> Result<Option<AssistantSchedule>> {
    let row = sqlx::query(
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, last_wake_at, last_outcome \
         FROM assistant_schedules WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(map_row).transpose()
}

pub(crate) async fn ensure(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    assistant_id: &str,
    now: DateTime<Utc>,
) -> Result<AssistantSchedule> {
    let stamp = format_ts(now);
    sqlx::query(
        "INSERT OR IGNORE INTO assistant_schedules \
         (workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, created_at, updated_at) \
         VALUES (?, ?, ?, 1, '', ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(assistant_id)
    .bind(i64::from(DEFAULT_ASSISTANT_INTERVAL_SECONDS))
    .bind(&stamp)
    .bind(&stamp)
    .bind(&stamp)
    .execute(pool)
    .await?;
    get_row(pool, workspace_id).await?.ok_or_else(|| DbError::NotFound {
        id: workspace_id.to_string(),
    })
}

pub(crate) async fn due(
    pool: &sqlx::SqlitePool,
    now: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<AssistantSchedule>> {
    let limit = if limit <= 0 { 100 } else { limit.min(500) };
    let rows = sqlx::query(
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, last_wake_at, last_outcome \
         FROM assistant_schedules \
         WHERE enabled = 1 AND next_wake_at IS NOT NULL AND next_wake_at <= ? \
         ORDER BY next_wake_at ASC LIMIT ?",
    )
    .bind(format_ts(now))
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter().map(map_row).collect()
}

pub(crate) async fn update(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    enabled: Option<bool>,
    interval_seconds: Option<u32>,
    prompt: Option<&str>,
    next_wake_at: Option<DateTime<Utc>>,
) -> Result<Option<AssistantSchedule>> {
    if let Some(interval) = interval_seconds {
        if !(MIN_ASSISTANT_INTERVAL_SECONDS..=MAX_ASSISTANT_INTERVAL_SECONDS).contains(&interval) {
            return Err(DbError::Validation {
                message: format!(
                    "interval_seconds must be within {MIN_ASSISTANT_INTERVAL_SECONDS}..={MAX_ASSISTANT_INTERVAL_SECONDS}"
                ),
            });
        }
    }
    sqlx::query(
        "UPDATE assistant_schedules SET \
         enabled = COALESCE(?, enabled), \
         interval_seconds = COALESCE(?, interval_seconds), \
         prompt = COALESCE(?, prompt), \
         next_wake_at = COALESCE(?, next_wake_at), \
         updated_at = ? \
         WHERE workspace_id = ?",
    )
    .bind(enabled.map(|value| if value { 1i64 } else { 0i64 }))
    .bind(interval_seconds.map(i64::from))
    .bind(prompt)
    .bind(next_wake_at.map(format_ts))
    .bind(format_ts(Utc::now()))
    .bind(workspace_id)
    .execute(pool)
    .await?;
    get_row(pool, workspace_id).await
}

pub(crate) async fn mark_wake(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    next_wake_at: DateTime<Utc>,
    last_wake_at: DateTime<Utc>,
    last_outcome: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE assistant_schedules SET next_wake_at = ?, last_wake_at = ?, last_outcome = ?, updated_at = ? \
         WHERE workspace_id = ?",
    )
    .bind(format_ts(next_wake_at))
    .bind(format_ts(last_wake_at))
    .bind(last_outcome)
    .bind(format_ts(Utc::now()))
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn ensure_assistant_schedule(
        &self,
        workspace_id: &str,
        assistant_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AssistantSchedule> {
        ensure(self.pool(), workspace_id, assistant_id, now).await
    }

    pub async fn get_assistant_schedule(&self, workspace_id: &str) -> Result<Option<AssistantSchedule>> {
        get_row(self.pool(), workspace_id).await
    }

    pub async fn list_due_assistant_schedules(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AssistantSchedule>> {
        due(self.pool(), now, limit).await
    }

    pub async fn update_assistant_schedule(
        &self,
        workspace_id: &str,
        enabled: Option<bool>,
        interval_seconds: Option<u32>,
        prompt: Option<&str>,
        next_wake_at: Option<DateTime<Utc>>,
    ) -> Result<Option<AssistantSchedule>> {
        update(self.pool(), workspace_id, enabled, interval_seconds, prompt, next_wake_at).await
    }

    pub async fn mark_assistant_wake(
        &self,
        workspace_id: &str,
        next_wake_at: DateTime<Utc>,
        last_wake_at: DateTime<Utc>,
        last_outcome: &str,
    ) -> Result<()> {
        mark_wake(self.pool(), workspace_id, next_wake_at, last_wake_at, last_outcome).await
    }
}
```

在 `crates/db/src/lib.rs` 的模块列表加入：

```rust
pub mod assistant_schedule;
```

> `enabled` 用显式 `1i64 / 0i64` 映射，避免依赖 `From<bool> for i64` 的平台细节。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p db assistant_schedule::tests -v
```

Expected: 4 tests PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/db/migrations/20261107000002_assistant_schedules.sql \
  crates/db/src/assistant_schedule.rs crates/db/src/lib.rs
git commit -m "feat(db): add assistant schedules"
```

---

### Task 2: HUD 数据读取（最近事件 + 待审批计数）

**Files:**

- Create: `crates/db/migrations/20261107000003_harness_events_recent_index.sql`
- Modify: `crates/db/src/harness_workspace.rs`（workspace migrator 注册）
- Modify: `crates/db/src/harness_event.rs`（`recent_session_events`）
- Modify: `crates/db/src/harness_runtime_api.rs`（`count_pending_tool_approvals`）
- Test: `crates/db/src/harness_event.rs` 与 `crates/db/src/harness_runtime_api.rs` 的测试模块

**Interfaces:**

- Consumes: `Db::workspace_pool`、`SessionEventRow::into_event`、`decisions` 表。
- Produces:
  - `Db::recent_session_events(&self, workspace_id: &str, limit: i64) -> Result<Vec<HarnessEvent>>`（Production、按 `occurred_at DESC, seq DESC`，limit 上限 200）。
  - `Db::count_pending_tool_approvals(&self, workspace_id: &str) -> Result<i64, DbError>`（`kind='tool_approval' AND state='requested'`）。

- [ ] **Step 1: 写失败测试**

在 `crates/db/src/harness_event.rs` 的测试模块追加：

```rust
#[tokio::test]
async fn recent_session_events_returns_newest_first() {
    use chrono::TimeZone;
    use serde_json::json;
    use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
    use crate::test_helpers::fixture_workspace;

    let fixture = fixture_workspace("ws-recent").await;
    for (index, text) in ["first", "second", "third"].iter().enumerate() {
        let occurred_at = Utc.timestamp_opt(1_800_000_000 + index as i64, 0).single().unwrap();
        fixture
            .db
            .append_session_event(&HarnessEventInput {
                event_id: format!("recent-{index}"),
                workspace_id: "ws-recent".to_string(),
                session_id: "agent:ws-recent:a/s1".to_string(),
                execution_mode: ExecutionMode::Production,
                run_id: None,
                task_id: None,
                event_type: "message.created".to_string(),
                occurred_at,
                cause_id: None,
                schema_v: 1,
                payload_digest: format!("digest-{index}"),
                payload_json: json!({ "text": text }),
            })
            .await
            .unwrap();
    }
    let events = fixture.db.recent_session_events("ws-recent", 2).await.unwrap();
    let payloads: Vec<&str> = events
        .iter()
        .map(|event| event.payload_json["text"].as_str().unwrap())
        .collect();
    assert_eq!(payloads, vec!["third", "second"]);
}
```

在 `crates/db/src/harness_runtime_api.rs` 的测试模块追加：

```rust
#[tokio::test]
async fn counts_pending_tool_approvals() {
    use crate::test_helpers::fixture_workspace;

    let fixture = fixture_workspace("ws-approvals").await;
    assert_eq!(fixture.db.count_pending_tool_approvals("ws-approvals").await.unwrap(), 0);
    let pool = fixture.db.workspace_pool_for_testing("ws-approvals").await.unwrap();
    sqlx::query(
        "INSERT INTO decisions \
         (decision_id, kind, state, session_id, run_id, task_id, subject_json, options_json, created_at, updated_at) \
         VALUES ('d1', 'tool_approval', 'requested', 's1', 'r1', 't1', '{}', '[]', '2026-11-07T00:00:00Z', '2026-11-07T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(fixture.db.count_pending_tool_approvals("ws-approvals").await.unwrap(), 1);
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p db recent_session_events_returns_newest_first -v
cargo test -p db counts_pending_tool_approvals -v
```

Expected: FAIL，缺少方法与索引。

- [ ] **Step 3: 写最小实现**

`crates/db/migrations/20261107000003_harness_events_recent_index.sql`：

```sql
CREATE INDEX IF NOT EXISTS idx_session_events_occurred_at
    ON session_events(occurred_at);
```

在 `crates/db/src/harness_workspace.rs` 的 `harness_migrator()` 列表末尾追加：

```rust
        Migration::new(
            20261107000003,
            "harness_events_recent_index".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261107000003_harness_events_recent_index.sql").into_sql_str(),
            false,
        ),
```

在 `crates/db/src/harness_event.rs` 的 `impl Db` 中追加：

```rust
    /// 最近事件（跨会话，Production），按发生时间倒序。
    /// 只用于 HUD/概览；会话回放仍走 `session_event_page`。
    pub async fn recent_session_events(
        &self,
        workspace_id: &str,
        limit: i64,
    ) -> Result<Vec<HarnessEvent>> {
        let limit = if limit <= 0 { 20 } else { limit.min(200) };
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, SessionEventRow>(
            "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, \
             cause_id, schema_v, payload_digest, payload_json \
             FROM session_events WHERE execution_mode = ? \
             ORDER BY occurred_at DESC, seq DESC LIMIT ?",
        )
        .bind(ExecutionMode::Production.as_str())
        .bind(limit)
        .fetch_all(&pool)
        .await?;
        rows.into_iter()
            .map(|row| row.into_event(workspace_id))
            .collect()
    }
```

在 `crates/db/src/harness_runtime_api.rs` 的 `impl Db` 中追加：

```rust
    /// 当前 workspace 中等待回答的 tool_approval 数量（HUD 用）。
    pub async fn count_pending_tool_approvals(&self, workspace_id: &str) -> Result<i64, DbError> {
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM decisions WHERE kind = 'tool_approval' AND state = 'requested'",
        )
        .fetch_one(&pool)
        .await?)
    }
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p db recent_session_events_returns_newest_first -v
cargo test -p db counts_pending_tool_approvals -v
```

Expected: 2 tests PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/db/migrations/20261107000003_harness_events_recent_index.sql \
  crates/db/src/harness_workspace.rs crates/db/src/harness_event.rs \
  crates/db/src/harness_runtime_api.rs
git commit -m "feat(db): add recent session events and pending approval count"
```

---

### Task 3: HUD 计数与状态纯函数

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/questions.rs`
- Modify: `apps/cloud/src/domains/agent/session/context.rs`
- Create: `apps/cloud/src/domains/agent/session/hud.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Test: 上述三个文件的测试模块

**Interfaces:**

- Consumes: `QuestionBroker`、`TurnContextRegistry`、`HarnessEvent`。
- Produces:
  - `QuestionBroker::count_pending_for_workspace(&self, workspace_id: &str) -> usize`
  - `TurnContextRegistry::count_for_workspace(&self, workspace_id: &str) -> usize`
  - `hud::{HudResponse, HudAssistant, HudDevices, HudAlarms, HudPending, HudActivity, HudSchedule, HudStatusInputs, assistant_status, MAX_RECENT_EVENTS}`
  - `pub const MAX_RECENT_EVENTS: i64 = 20;`

- [ ] **Step 1: 写失败测试**

在 `questions.rs` 测试模块追加：

```rust
#[test]
fn counts_pending_questions_by_workspace() {
    let broker = QuestionBroker::default();
    let (_a, _ra) = broker.register("agent:ws-a:a/s1");
    let (_b, _rb) = broker.register("agent:ws-a:a/s2");
    let (_c, _rc) = broker.register("agent:ws-b:a/s1");
    assert_eq!(broker.count_pending_for_workspace("ws-a"), 2);
    assert_eq!(broker.count_pending_for_workspace("ws-b"), 1);
    assert_eq!(broker.count_pending_for_workspace("ws-c"), 0);
}
```

在 `context.rs` 测试模块追加：

```rust
#[test]
fn counts_active_turns_by_workspace() {
    let registry = TurnContextRegistry::default();
    let _a = registry.set(ctx("run1"));
    let mut other_ws = ctx("run2");
    other_ws.workspace_id = "ws-other".to_string();
    let _b = registry.set(other_ws);
    assert_eq!(registry.count_for_workspace("ws"), 1);
    assert_eq!(registry.count_for_workspace("ws-other"), 1);
    assert_eq!(registry.count_for_workspace("ws-missing"), 0);
}
```

在 `hud.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(paused: bool, active: usize, questions: usize, approvals: i64, last: Option<&str>) -> HudStatusInputs<'_> {
        HudStatusInputs {
            paused,
            active_sessions: active,
            pending_questions: questions,
            pending_approvals: approvals,
            last_outcome: last,
        }
    }

    #[test]
    fn paused_wins_over_everything() {
        assert_eq!(assistant_status(inputs(true, 2, 1, 1, Some("failed"))), "paused");
    }

    #[test]
    fn working_beats_waiting() {
        assert_eq!(assistant_status(inputs(false, 1, 1, 0, None)), "working");
    }

    #[test]
    fn waiting_when_blocked_on_user() {
        assert_eq!(assistant_status(inputs(false, 0, 1, 0, None)), "waiting_for_user");
        assert_eq!(assistant_status(inputs(false, 0, 0, 2, None)), "waiting_for_user");
    }

    #[test]
    fn error_only_when_last_wake_failed() {
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, Some("failed"))), "error");
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, Some("completed"))), "idle");
        assert_eq!(assistant_status(inputs(false, 0, 0, 0, None)), "idle");
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::questions::tests -v
cargo test -p tinyiothub-cloud session::context::tests -v
cargo test -p tinyiothub-cloud session::hud::tests -v
```

Expected: FAIL，缺少 `count_pending_for_workspace` / `count_for_workspace` / `hud` 模块。

- [ ] **Step 3: 写最小实现**

在 `questions.rs` 的 `impl QuestionBroker` 中追加：

```rust
    /// HUD 用：当前 workspace 下等待回答的问题数。
    pub fn count_pending_for_workspace(&self, workspace_id: &str) -> usize {
        let prefix = format!("agent:{workspace_id}:");
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key.starts_with(&prefix))
            .count()
    }
```

在 `context.rs` 的 `impl TurnContextRegistry` 中追加：

```rust
    /// HUD 用：当前 workspace 正在执行的 Turn 数。
    pub fn count_for_workspace(&self, workspace_id: &str) -> usize {
        self.inner
            .iter()
            .filter(|entry| entry.value().workspace_id == workspace_id)
            .count()
    }
```

`hud.rs`：

```rust
//! Global HUD 的响应形状与状态判定（纯逻辑，无 I/O）。

use chrono::{DateTime, Utc};
use serde::Serialize;
use tinyiothub_core::harness::HarnessEvent;

pub const MAX_RECENT_EVENTS: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudStatusInputs<'a> {
    pub paused: bool,
    pub active_sessions: usize,
    pub pending_questions: usize,
    pub pending_approvals: i64,
    pub last_outcome: Option<&'a str>,
}

/// 优先级：paused > working > waiting_for_user > error > idle。
pub fn assistant_status(inputs: HudStatusInputs<'_>) -> &'static str {
    if inputs.paused {
        "paused"
    } else if inputs.active_sessions > 0 {
        "working"
    } else if inputs.pending_questions > 0 || inputs.pending_approvals > 0 {
        "waiting_for_user"
    } else if inputs.last_outcome == Some("failed") {
        "error"
    } else {
        "idle"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudResponse {
    pub assistant: HudAssistant,
    pub devices: HudDevices,
    pub alarms: HudAlarms,
    pub pending: HudPending,
    pub activity: HudActivity,
    pub schedule: HudSchedule,
    pub recent_events: Vec<HarnessEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudAssistant {
    pub status: &'static str,
    pub last_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudDevices {
    pub total: i64,
    pub online: i64,
    pub offline: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudAlarms {
    pub high_priority: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudPending {
    pub questions: usize,
    pub approvals: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudActivity {
    pub active_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudSchedule {
    pub enabled: bool,
    pub interval_seconds: u32,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
}
```

在 `session/mod.rs` 加入 `pub mod hud;`（不要 `pub use hud::*;`，避免与 `AskUser` 等无关类型混在一起）。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::questions::tests -v
cargo test -p tinyiothub-cloud session::context::tests -v
cargo test -p tinyiothub-cloud session::hud::tests -v
```

Expected: 全部 PASS（含既有测试）。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/questions.rs \
  apps/cloud/src/domains/agent/session/context.rs \
  apps/cloud/src/domains/agent/session/hud.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add HUD counters and status derivation"
```

---

### Task 4: Session 生命周期 payload 与 WakeRunner

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/events.rs`（`session_started_payload`）
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Create: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/host/chat/service.rs`（改用统一 payload）
- Test: `events.rs` 与 `wake.rs` 的测试模块

**Interfaces:**

- Consumes: `AssistantSchedule`、`Db::list_heartbeat_tasks/create_session/update_session`、`ensure_agent`、`SessionRunner::run_turn_bounded`、`TodoStore`。
- Produces:
  - `events::session_started_payload(session_id: &str, kind: &str) -> serde_json::Value`
  - `wake::{ASSISTANT_AGENT_PREFIX, assistant_agent_id, build_wake_message, WakeOutcome, WakeReport, WakeExecutor, WakeRunner}`
  - `WakeRunner::from_runtime(db: Arc<Db>, pool: Arc<AgentPool>, runtime: &SessionRuntime) -> Self`
  - `WakeRunner::from_parts(db, pool, events: Arc<dyn SessionEventSink>, contexts: Arc<TurnContextRegistry>, todos: Arc<TodoStore>, runner: Arc<SessionRunner>) -> Self`
  - `WakeRunner::run(&self, schedule: &AssistantSchedule) -> WakeReport`
  - `WakeOutcome::as_str(&self) -> &'static str`（`completed|partial|failed|cancelled|skipped`）

> Wake agent 使用 `AgentRuntimeConfig::default()`（与 legacy heartbeat 一致），暂不读取 workspace 聊天模型配置；统一模型配置留待 Plan 3/4。

- [ ] **Step 1: 写失败测试**

在 `events.rs` 测试模块追加：

```rust
#[test]
fn session_started_payload_matches_spec() {
    let payload = session_started_payload("agent:ws:a/s1", "scheduled");
    assert_eq!(payload["sessionId"], "agent:ws:a/s1");
    assert_eq!(payload["kind"], "scheduled");
}
```

在 `wake.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContextRegistry;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::runner::SessionRunner;
    use crate::domains::agent::session::todos::TodoStore;
    use crate::domains::agent::session::types::{Todo, TodoStatus};
    use std::sync::Arc;
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_core::heartbeat::HeartbeatTask;
    use tinyiothub_storage::assistant_schedule::AssistantSchedule;

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
                text: Some("巡检完成".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }

        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    fn task(text: &str, paused: bool) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws".into(),
            priority: "low".into(),
            text: text.into(),
            paused,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn schedule(prompt: &str) -> AssistantSchedule {
        AssistantSchedule {
            workspace_id: "ws".into(),
            assistant_id: assistant_agent_id("ws"),
            interval_seconds: 300,
            enabled: true,
            prompt: prompt.into(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        }
    }

    async fn fixture() -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
            )
            .unwrap(),
        );
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink.clone(), contexts));
        let todos = Arc::new(TodoStore::default());
        let wake = WakeRunner::from_parts(
            db.clone(),
            agent_pool.clone(),
            sink.clone(),
            todos,
            runner,
        );
        (wake, agent_pool, sink)
    }

    #[test]
    fn wake_message_uses_custom_prompt_first() {
        let message = build_wake_message(&schedule("检查 3 号楼"), &[task("离线事件", false)]);
        assert_eq!(message.as_deref(), Some("检查 3 号楼"));
    }

    #[test]
    fn wake_message_lists_active_tasks_and_skips_all_paused() {
        let message = build_wake_message(&schedule(""), &[task("离线事件", false), task("设备异常", true)])
            .unwrap();
        assert!(message.contains("- 离线事件"));
        assert!(!message.contains("设备异常"));
        assert!(build_wake_message(&schedule(""), &[task("离线事件", true)]).is_none());
        assert!(build_wake_message(&schedule(""), &[]).unwrap().contains("默认职责"));
    }

    #[test]
    fn outcome_with_todos_marks_partial() {
        let incomplete = vec![Todo {
            content: "巡检".into(),
            status: TodoStatus::InProgress,
            active_form: "巡检中".into(),
        }];
        assert_eq!(
            outcome_with_todos(TurnOutcome::Completed { text: String::new(), tool_calls: 0 }, &incomplete),
            WakeOutcome::Partial
        );
        assert_eq!(
            outcome_with_todos(TurnOutcome::Completed { text: String::new(), tool_calls: 0 }, &[]),
            WakeOutcome::Completed
        );
    }

    #[tokio::test]
    async fn wake_run_writes_lifecycle_and_ends_session() {
        let (wake, agent_pool, sink) = fixture().await;
        let schedule = schedule("");
        let agent = agent_pool
            .create(&schedule.assistant_id, "ws", &AgentRuntimeConfig::default(), vec![])
            .unwrap();
        let report = wake
            .run_with_agent(&schedule, agent, "按默认职责巡检")
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "message.created",
                "turn.completed",
                "session.ended",
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "scheduled");
        assert_eq!(events.last().unwrap().payload_json["outcome"], "completed");
        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.kind, "scheduled");
        assert_eq!(session.status, "ended");
        let parsed = tinyiothub_agent::session::SessionKey::parse(&report.session_key)
            .expect("wake session key must be a valid agent:<ws>:<assistant>/<session> key");
        assert_eq!(parsed.workspace_id, "ws");
        assert_eq!(parsed.agent_id, assistant_agent_id("ws"));
    }
}
```

> 测试访问 `wake.db` 字段：把 `WakeRunner.db` 设为 `pub(crate)`，并在同模块测试中使用。

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
cargo test -p tinyiothub-cloud session::events::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
```

Expected: FAIL，缺少 `session_started_payload` / `wake` 模块。

- [ ] **Step 3: 写最小实现**

在 `events.rs` 末尾追加：

```rust
/// spec §5.4.3：`session.started { session_id, kind }` 的统一构造。
pub fn session_started_payload(session_id: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({ "sessionId": session_id, "kind": kind })
}
```

`wake.rs`：

```rust
//! 自主唤醒：把一条到期的 `AssistantSchedule` 变成一个 scheduled Session 的 Turn。
//!
//! 触发与执行分离——本节只负责“接到 wake 后怎么跑”，何时唤醒由 `TriggerRouter` 决定。

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::port::runtime::AgentLoopHandle;
use tinyiothub_core::heartbeat::HeartbeatTask;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_storage::session::Session;
use tinyiothub_storage::Db;

use crate::domains::agent::host::chat::service::ensure_agent;
use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::events::{SessionEventSink, session_event, session_started_payload};
use crate::domains::agent::session::runner::{SessionRunner, TurnOutcome};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::{Todo, TodoStatus};
use crate::domains::agent::session::SESSION_TURN_TIMEOUT;

pub const ASSISTANT_AGENT_PREFIX: &str = "__assistant__";

/// 每个 workspace 一个常驻 Assistant 的 agent id。
pub fn assistant_agent_id(workspace_id: &str) -> String {
    format!("{ASSISTANT_AGENT_PREFIX}{workspace_id}")
}

/// 自定义 prompt > 未暂停的 legacy heartbeat tasks > 默认职责。
/// 存在任务但全部暂停时返回 `None`（本轮不唤醒）。
pub fn build_wake_message(schedule: &AssistantSchedule, tasks: &[HeartbeatTask]) -> Option<String> {
    let custom = schedule.prompt.trim();
    if !custom.is_empty() {
        return Some(custom.to_string());
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOutcome {
    Completed,
    Partial,
    Failed { error: String },
    Cancelled,
    Skipped,
}

impl WakeOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeReport {
    pub session_key: String,
    pub outcome: WakeOutcome,
}

#[async_trait::async_trait]
pub trait WakeExecutor: Send + Sync {
    async fn execute(&self, schedule: AssistantSchedule) -> WakeReport;
}

pub struct WakeRunner {
    pub(crate) db: Arc<Db>,
    pool: Arc<AgentPool>,
    events: Arc<dyn SessionEventSink>,
    todos: Arc<TodoStore>,
    runner: Arc<SessionRunner>,
}

impl WakeRunner {
    pub fn from_runtime(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        runtime: &crate::domains::agent::session::SessionRuntime,
    ) -> Self {
        Self {
            db,
            pool,
            events: runtime.events.clone(),
            todos: runtime.todos.clone(),
            runner: runtime.runner.clone(),
        }
    }

    pub fn from_parts(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        events: Arc<dyn SessionEventSink>,
        todos: Arc<TodoStore>,
        runner: Arc<SessionRunner>,
    ) -> Self {
        Self { db, pool, events, todos, runner }
    }

    pub async fn run(&self, schedule: &AssistantSchedule) -> WakeReport {
        if !schedule.enabled {
            return WakeReport { session_key: String::new(), outcome: WakeOutcome::Skipped };
        }
        let tasks = self
            .db
            .list_heartbeat_tasks(&schedule.workspace_id)
            .await
            .unwrap_or_default();
        let Some(message) = build_wake_message(schedule, &tasks) else {
            return WakeReport { session_key: String::new(), outcome: WakeOutcome::Skipped };
        };
        let agent = match ensure_agent(
            &self.pool,
            &schedule.assistant_id,
            &schedule.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
        )
        .await
        {
            Ok(agent) => agent,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed { error: error.to_string() },
                };
            }
        };
        self.run_with_agent(schedule, agent, &message).await
    }

    pub(crate) async fn run_with_agent(
        &self,
        schedule: &AssistantSchedule,
        agent: AgentLoopHandle,
        message: &str,
    ) -> WakeReport {
        let session_key = format!(
            "agent:{}:{}/{}",
            schedule.workspace_id,
            schedule.assistant_id,
            Uuid::new_v4()
        );
        let session = Session::new(
            session_key.clone(),
            schedule.workspace_id.clone(),
            schedule.assistant_id.clone(),
        )
        .with_kind("scheduled");
        if let Err(error) = self.db.create_session(&session).await {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed { error: error.to_string() },
            };
        }
        let context = TurnContext::new(
            schedule.workspace_id.clone(),
            schedule.assistant_id.clone(),
            session_key.clone(),
            Uuid::new_v4().to_string(),
            0,
        );
        let wake_event_id = Uuid::new_v4().to_string();
        self.emit(&context, "trigger.received", json!({ "trigger": "schedule", "sourceId": &wake_event_id }))
            .await;
        self.emit(&context, "wake.dispatched", json!({ "wakeEventId": &wake_event_id }))
            .await;
        self.emit(&context, "session.started", session_started_payload(&session_key, "scheduled"))
            .await;

        let outcome = {
            let guard = agent.lock().await;
            let _ = guard.clear_history().await;
            let turn = self
                .runner
                .run_turn_bounded(&*guard, &context, message, None, SESSION_TURN_TIMEOUT)
                .await;
            drop(guard);
            outcome_with_todos(turn, &self.todos.get(&session_key))
        };

        self.emit(
            &context,
            "session.ended",
            json!({ "sessionId": &session_key, "outcome": outcome.as_str() }),
        )
        .await;

        let mut ended = session;
        ended.status = "ended".to_string();
        ended.touch();
        if let Err(error) = self.db.update_session(&ended).await {
            tracing::warn!(%error, %session_key, "failed to mark wake session ended");
        }
        WakeReport { session_key, outcome }
    }

    async fn emit(&self, context: &TurnContext, event_type: &str, payload: serde_json::Value) {
        if let Err(error) = self.events.emit(session_event(context, event_type, payload)).await {
            tracing::warn!(%error, %event_type, "failed to persist wake session event");
        }
    }
}

#[async_trait::async_trait]
impl WakeExecutor for WakeRunner {
    async fn execute(&self, schedule: AssistantSchedule) -> WakeReport {
        self.run(&schedule).await
    }
}

fn outcome_with_todos(outcome: TurnOutcome, todos: &[Todo]) -> WakeOutcome {
    match outcome {
        TurnOutcome::Completed { .. } if todos.iter().any(|todo| todo.status != TodoStatus::Completed) => {
            WakeOutcome::Partial
        }
        TurnOutcome::Completed { .. } => WakeOutcome::Completed,
        TurnOutcome::Cancelled => WakeOutcome::Cancelled,
        TurnOutcome::Failed { error } => WakeOutcome::Failed { error },
    }
}
```

在 `session/mod.rs` 加入：

```rust
pub mod wake;

pub use wake::{WakeExecutor, WakeOutcome, WakeReport, WakeRunner, assistant_agent_id, build_wake_message};
```

在 `host/chat/service.rs` 把聊天会话事件里的 `"session.started"` payload 改为：

```rust
                    "session.started",
                    crate::domains::agent::session::session_started_payload(&session_key, "chat"),
```

> `session_started_payload` 通过 `session/**` 的 `pub use events::*;` 暴露；若没有通配导出，改用 `crate::domains::agent::session::events::session_started_payload`。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::events::tests -v
cargo test -p tinyiothub-cloud session::wake::tests -v
cargo test -p tinyiothub-cloud host::chat::service::tests -v
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/events.rs \
  apps/cloud/src/domains/agent/session/wake.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/domains/agent/host/chat/service.rs
git commit -m "feat(session): add scheduled wake runner and session lifecycle payloads"
```

---

### Task 5: TriggerRouter 与到期调度

**Files:**

- Create: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Test: `trigger.rs` 的测试模块

**Interfaces:**

- Consumes: `Db::{find_all_workspace_ids, ensure_assistant_schedule, list_due_assistant_schedules, mark_assistant_wake}`、`WakeExecutor`、`WakeReport`。
- Produces:
  - `pub const TRIGGER_TICK_INTERVAL: Duration = Duration::from_secs(30);`
  - `TriggerRouter::new(db: Arc<Db>, executor: Arc<dyn WakeExecutor>) -> Self`
  - `TriggerRouter::tick(self: &Arc<Self>, now: DateTime<Utc>) -> Vec<tokio::task::JoinHandle<()>>`
  - `TriggerRouter::start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), crate::shared::error::Error>>`
  - `TriggerRouter::shutdown(&self)`

- [ ] **Step 1: 写失败测试**

在 `trigger.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct RecordingExecutor {
        calls: AtomicUsize,
        workspaces: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for RecordingExecutor {
        async fn execute(&self, schedule: AssistantSchedule) -> WakeReport {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.workspaces.lock().await.push(schedule.workspace_id);
            WakeReport {
                session_key: "s".into(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
            }
        }
    }

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[tokio::test]
    async fn tick_dispatches_due_and_advances() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now).await.unwrap();
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));

        let handles = router.tick(now).await;
        assert_eq!(handles.len(), 1);
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        let row = db.get_assistant_schedule("ws-1").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
        assert_eq!(row.last_outcome.as_deref(), Some("completed"));

        let handles = router.tick(now + chrono::Duration::seconds(299)).await;
        assert!(handles.is_empty(), "not due yet");
    }

    #[tokio::test]
    async fn shutdown_cancels_tick_loop_token() {
        let db = Arc::new(test_db().await);
        let router = TriggerRouter::new(db, Arc::new(RecordingExecutor::default()));
        router.shutdown();
        assert!(router.shutdown.is_cancelled());
    }
}
```

测试模块顶部需要 `use crate::domains::agent::session::wake::WakeReport;`。

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::trigger::tests -v`

Expected: FAIL，缺少 `TriggerRouter`。

- [ ] **Step 3: 写最小实现**

`trigger.rs`：

```rust
//! TriggerRouter — 只产生 wake，不执行 Turn。
//!
//! 每 30 秒：确保所有 workspace 有 Schedule 行 → 扫描到期且启用的行 →
//! 每个 workspace 至多一个在途 wake → 交给 `WakeExecutor` 并推进 `next_wake_at`。

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dashmap::DashSet;
use tokio_util::sync::CancellationToken;

use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;

use crate::domains::agent::session::wake::{WakeExecutor, assistant_agent_id};

pub const TRIGGER_TICK_INTERVAL: Duration = Duration::from_secs(30);
const DUE_BATCH_LIMIT: i64 = 100;

pub struct TriggerRouter {
    db: Arc<Db>,
    executor: Arc<dyn WakeExecutor>,
    in_flight: Arc<DashSet<String>>,
    shutdown: CancellationToken,
}

impl TriggerRouter {
    pub fn new(db: Arc<Db>, executor: Arc<dyn WakeExecutor>) -> Self {
        Self {
            db,
            executor,
            in_flight: Arc::new(DashSet::new()),
            shutdown: CancellationToken::new(),
        }
    }

    /// 扫描并派发到期的 wake；返回已派发任务的句柄（生产方忽略，测试可 await）。
    pub async fn tick(self: &Arc<Self>, now: DateTime<Utc>) -> Vec<tokio::task::JoinHandle<()>> {
        match self.db.find_all_workspace_ids().await {
            Ok(workspace_ids) => {
                for workspace_id in &workspace_ids {
                    let assistant_id = assistant_agent_id(workspace_id);
                    if let Err(error) = self
                        .db
                        .ensure_assistant_schedule(workspace_id, &assistant_id, now)
                        .await
                    {
                        tracing::warn!(%error, %workspace_id, "assistant schedule: ensure failed");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "assistant schedule: list workspaces failed");
                return Vec::new();
            }
        }

        let due = match self.db.list_due_assistant_schedules(now, DUE_BATCH_LIMIT).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "assistant schedule: due scan failed");
                return Vec::new();
            }
        };

        let mut handles = Vec::new();
        for schedule in due {
            if !schedule.enabled {
                continue;
            }
            if !self.in_flight.insert(schedule.workspace_id.clone()) {
                continue;
            }
            let router = Arc::clone(self);
            handles.push(tokio::spawn(async move {
                router.dispatch(schedule, now).await;
            }));
        }
        handles
    }

    async fn dispatch(self: Arc<Self>, schedule: AssistantSchedule, now: DateTime<Utc>) {
        let report = self.executor.execute(schedule.clone()).await;
        let next_wake_at = now + ChronoDuration::seconds(i64::from(schedule.interval_seconds));
        if let Err(error) = self
            .db
            .mark_assistant_wake(&schedule.workspace_id, next_wake_at, now, report.outcome.as_str())
            .await
        {
            tracing::warn!(%error, workspace_id = %schedule.workspace_id, "assistant schedule: mark wake failed");
        }
        self.in_flight.remove(&schedule.workspace_id);
    }

    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), crate::shared::error::Error>> {
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

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}
```

在 `session/mod.rs` 加入：

```rust
pub mod trigger;

pub use trigger::{TRIGGER_TICK_INTERVAL, TriggerRouter};
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p tinyiothub-cloud session::trigger::tests -v`

Expected: 2 tests PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/trigger.rs \
  apps/cloud/src/domains/agent/session/mod.rs
git commit -m "feat(session): add schedule trigger router"
```

---

### Task 6: Schedule / HUD HTTP API

**Files:**

- Create: `apps/cloud/src/domains/agent/session/workspace_api.rs`
- Modify: `apps/cloud/src/domains/agent/session/mod.rs`
- Modify: `apps/cloud/src/api/mod.rs`
- Test: `workspace_api.rs` 的测试模块

**Interfaces:**

- Consumes: `Db`（assistant schedule / 设备计数 / 告警计数 / 待审批 / 最近事件 / autonomy policy）、`SessionRuntime.questions/contexts`、`Claims`、`AgentState`。
- Produces:
  - `GET /api/v1/workspaces/{workspace_id}/agent/hud` → `HudResponse`
  - `GET /api/v1/workspaces/{workspace_id}/agent/schedule` → `ScheduleDto`
  - `PUT /api/v1/workspaces/{workspace_id}/agent/schedule`（body `UpdateScheduleRequest`）→ `ScheduleDto`
  - `workspace_api::create_workspace_router<S>() -> Router<S>`

- [ ] **Step 1: 写失败测试**

在 `workspace_api.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_bounds_are_enforced() {
        assert!(validate_interval(60).is_ok());
        assert!(validate_interval(300).is_ok());
        assert!(validate_interval(86_400).is_ok());
        assert!(validate_interval(59).is_err());
        assert!(validate_interval(86_401).is_err());
    }

    #[test]
    fn schedule_dto_serializes_camel_case() {
        let dto = ScheduleDto {
            enabled: true,
            interval_seconds: 300,
            prompt: String::new(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        };
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["intervalSeconds"], 300);
        assert!(value.get("nextWakeAt").is_some());
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::workspace_api::tests -v`

Expected: FAIL，缺少模块与函数。

- [ ] **Step 3: 写最小实现**

`workspace_api.rs`：

```rust
//! Workspace 级的 Assistant Schedule 与 Global HUD 端点。
//!
//! 只读聚合：设备/告警/待答/待批/活动会话/最近事件都来自既有存储，不新增状态表。
//! 所有端点先做 workspace 归属校验。

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tinyiothub_core::policy::AutonomyMode;
use tinyiothub_storage::alarm::{AlarmLevel, AlarmQueryCriteria, AlarmStatus};
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_web::security::Claims;

use crate::domains::agent::session::hud::{
    HudActivity, HudAlarms, HudAssistant, HudDevices, HudPending, HudResponse, HudSchedule, HudStatusInputs,
    MAX_RECENT_EVENTS, assistant_status,
};
use crate::domains::agent::session::wake::assistant_agent_id;
use crate::domains::agent::AgentState;

pub fn create_workspace_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{workspace_id}/agent/hud", get(get_hud))
        .route(
            "/{workspace_id}/agent/schedule",
            get(get_schedule).put(update_schedule),
        )
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

async fn verify_workspace_read(
    state: &AgentState,
    claims: &Claims,
    workspace_id: &str,
) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(error_response(StatusCode::FORBIDDEN, "无权访问此工作空间")),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "工作空间不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check workspace failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败"))
        }
    }
}

fn validate_interval(interval_seconds: u32) -> Result<u32, String> {
    if (tinyiothub_storage::assistant_schedule::MIN_ASSISTANT_INTERVAL_SECONDS
        ..=tinyiothub_storage::assistant_schedule::MAX_ASSISTANT_INTERVAL_SECONDS)
        .contains(&interval_seconds)
    {
        Ok(interval_seconds)
    } else {
        Err("intervalSeconds 必须在 60 到 86400 之间".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    pub enabled: bool,
    pub interval_seconds: u32,
    pub prompt: String,
    pub next_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

impl From<&AssistantSchedule> for ScheduleDto {
    fn from(schedule: &AssistantSchedule) -> Self {
        Self {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            prompt: schedule.prompt.clone(),
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScheduleRequest {
    pub enabled: Option<bool>,
    pub interval_seconds: Option<u32>,
    pub prompt: Option<String>,
}

pub async fn get_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => Json(ScheduleDto::from(&schedule)).into_response(),
        Err(error) => internal(error),
    }
}

pub async fn update_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    let current = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let interval_seconds = match request.interval_seconds {
        Some(value) => match validate_interval(value) {
            Ok(value) => value,
            Err(message) => return error_response(StatusCode::BAD_REQUEST, &message),
        },
        None => current.interval_seconds,
    };
    let recompute = request.enabled == Some(true) || request.interval_seconds.is_some();
    let next_wake_at = recompute
        .then(|| now + chrono::Duration::seconds(i64::from(interval_seconds)));

    match state
        .db
        .update_assistant_schedule(
            &workspace_id,
            request.enabled,
            Some(interval_seconds),
            request.prompt.as_deref(),
            next_wake_at,
        )
        .await
    {
        Ok(Some(schedule)) => Json(ScheduleDto::from(&schedule)).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "Schedule 不存在"),
        Err(error) => internal(error),
    }
}

pub async fn get_hud(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    let schedule = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let autonomy_off = match state.db.load_autonomy_policy(&workspace_id).await {
        Ok(Some(policy)) => policy.mode == AutonomyMode::Off,
        Ok(None) => false,
        Err(error) => return internal(error),
    };
    let paused = autonomy_off || !schedule.enabled;

    let total = match state.db.count_things_total(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let online = match state.db.count_online_things(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let criteria = AlarmQueryCriteria {
        workspace_id: Some(workspace_id.clone()),
        alarm_levels: Some(vec![AlarmLevel::Critical, AlarmLevel::Error]),
        statuses: Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]),
        ..Default::default()
    };
    let alarms = match state.db.count_alarms_by_criteria(&criteria).await {
        Ok(value) => value as i64,
        Err(error) => return internal(error),
    };
    let approvals = match state.db.count_pending_tool_approvals(&workspace_id).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let questions = state
        .session_runtime
        .questions
        .count_pending_for_workspace(&workspace_id);
    let active_sessions = state
        .session_runtime
        .contexts
        .count_for_workspace(&workspace_id);
    let recent_events = match state.db.recent_session_events(&workspace_id, MAX_RECENT_EVENTS).await {
        Ok(events) => events,
        Err(error) => return internal(error),
    };

    let last_outcome = schedule.last_outcome.clone();
    let status = assistant_status(HudStatusInputs {
        paused,
        active_sessions,
        pending_questions: questions,
        pending_approvals: approvals,
        last_outcome: last_outcome.as_deref(),
    });

    Json(HudResponse {
        assistant: HudAssistant { status, last_outcome },
        devices: HudDevices {
            total,
            online,
            offline: (total - online).max(0),
        },
        alarms: HudAlarms { high_priority: alarms },
        pending: HudPending { questions, approvals },
        activity: HudActivity { active_sessions },
        schedule: HudSchedule {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome,
        },
        recent_events,
    })
    .into_response()
}

fn internal(error: impl std::fmt::Display) -> Response {
    tracing::warn!(%error, "assistant HUD/schedule query failed");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取 Assistant 状态失败")
}
```

在 `session/mod.rs` 加入：

```rust
pub mod workspace_api;
```

在 `apps/cloud/src/api/mod.rs` 的 workspace 路由组（`agent_tasks` / `harness` 附近）加入：

```rust
        .nest(
            "/workspaces",
            crate::domains::agent::session::workspace_api::create_workspace_router(),
        )
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud session::workspace_api::tests -v
cargo check -p tinyiothub-cloud
```

Expected: 2 tests PASS；`cargo check` 无错误。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/workspace_api.rs \
  apps/cloud/src/domains/agent/session/mod.rs \
  apps/cloud/src/api/mod.rs
git commit -m "feat(session): expose schedule and global HUD APIs"
```

---

### Task 7: 接管 legacy 定时唤醒并接线 TriggerRouter

**Files:**

- Modify: `apps/cloud/src/domains/agent/thing_agent/manager.rs`（config + `start`）
- Modify: `apps/cloud/src/shared/service_manager.rs`
- Test: `manager.rs` 测试模块

**Interfaces:**

- Consumes: `TriggerRouter`、`WakeRunner`、`ThingAgentManagerConfig`。
- Produces:
  - `ThingAgentManagerConfig.legacy_wake_triggers: bool`（默认 `false`）。
  - 默认配置只挂载 forward + event 两个 trigger task；`legacy_wake_triggers = true` 时额外挂载 timer + heartbeat（仅用于回归对比）。
  - `ServiceManager` 启动时构造 `TriggerRouter`，把 `start()` 句柄推入 `service_handles`。

- [ ] **Step 1: 写失败测试**

在 `manager.rs` 测试模块追加（并修改现有的 trigger 计数断言）：

```rust
#[tokio::test(start_paused = true)]
async fn default_config_does_not_mount_legacy_wake_triggers() {
    let manager = stub_manager();
    manager.start("ws-default");
    let tasks = manager.workspace_task_count_for_test("ws-default");
    assert_eq!(tasks, 2, "forward + event only; schedule wakes belong to TriggerRouter");
}

#[tokio::test(start_paused = true)]
async fn legacy_wake_triggers_can_be_opted_in() {
    let manager = stub_manager_with_config(ThingAgentManagerConfig {
        legacy_wake_triggers: true,
        ..Default::default()
    });
    manager.start("ws-legacy");
    let tasks = manager.workspace_task_count_for_test("ws-legacy");
    assert_eq!(tasks, 4, "forward + event + timer + heartbeat");
}
```

在 manager 的 `impl ThingAgentManager` 中增加仅测试可见的辅助：

```rust
    #[cfg(test)]
    fn workspace_task_count_for_test(&self, workspace_id: &str) -> usize {
        self.workspaces
            .get(workspace_id)
            .map(|entry| entry.value().tasks.len())
            .unwrap_or(0)
    }
```

> 现有 `start_mounts_heartbeat_trigger_alongside_timer_and_event` 断言 4 个 task；把它的 manager 配置改为 `legacy_wake_triggers: true`（或按上面两个测试拆开），否则会失败。

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud thing_agent::manager::tests -v`

Expected: FAIL，缺少 `legacy_wake_triggers` 字段 / task 计数不符。

- [ ] **Step 3: 写最小实现**

在 `ThingAgentManagerConfig` 增加字段：

```rust
    /// Plan 2 起 schedule 唤醒由 `TriggerRouter` 接管。为 `true` 时仍挂载
    /// legacy Timer/Heartbeat trigger（仅用于回归对比，生产默认 `false`）。
    pub legacy_wake_triggers: bool,
```

并在 `impl Default` 中加 `legacy_wake_triggers: false`。

把 `start()` 中 timer + heartbeat 的创建与 spawn 包进条件块，并调整 task 向量：

```rust
        let mut tasks = vec![forward, event_task];

        if self.config.legacy_wake_triggers {
            let timer = TimerTrigger {
                workspace_id: ws.clone(),
                interval: self.config.timer_interval,
                policy_repo: Arc::clone(&self.deps.policy_repo),
            };
            let timer_tx = tx.clone();
            let timer_ws = ws.clone();
            let timer_task = tokio::spawn(async move {
                if let Err(e) = timer.run(timer_tx).await {
                    tracing::warn!(workspace_id = %timer_ws, error = %e, "timer trigger exited with error");
                }
            });

            let heartbeat = HeartbeatTrigger {
                workspace_id: workspace_id.to_string(),
                default_interval: self.config.heartbeat_default_interval,
                intervals: Arc::clone(&self.deps.heartbeat_intervals),
                enabled: Arc::clone(&self.deps.heartbeat_enabled),
                tasks_source: Arc::clone(&self.deps.heartbeat_tasks_map),
                policy_repo: Arc::clone(&self.deps.policy_repo),
                governor: self.deps.governor.clone(),
                reload_notify: self.heartbeat_reload_notify(workspace_id),
            };
            let heartbeat_ws = workspace_id.to_string();
            let heartbeat_tx = tx.clone();
            let heartbeat_task = tokio::spawn(async move {
                if let Err(e) = heartbeat.run(heartbeat_tx).await {
                    tracing::warn!(workspace_id = %heartbeat_ws, error = %e, "heartbeat trigger exited with error");
                }
            });

            tasks.push(timer_task);
            tasks.push(heartbeat_task);
        }
```

（`tx` 在 event trigger 之后仍需可用——确保条件块之前没有 move 掉 `tx`；用 `tx.clone()` 即可。）

在 `service_manager.rs` 的 workspace 启动循环之后（`app_state.heartbeat_last_ticks = ...` 附近）加入：

```rust
            let wake_runner = Arc::new(
                crate::domains::agent::session::WakeRunner::from_runtime(
                    app_state.db.clone(),
                    app_state.agent_pool.clone(),
                    &app_state.session_runtime,
                ),
            );
            let trigger_router = Arc::new(
                crate::domains::agent::session::TriggerRouter::new(
                    app_state.db.clone(),
                    wake_runner,
                ),
            );
            let trigger_handle = trigger_router.clone().start();
            self.service_handles.write().await.push(trigger_handle);
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
cargo test -p tinyiothub-cloud thing_agent::manager::tests -v
cargo test -p tinyiothub-cloud --lib host::chat::service::tests -v
cargo check -p tinyiothub-cloud
```

Expected: 全部 PASS。若其他 test 依赖默认挂载 timer/heartbeat，按新语义显式设置 `legacy_wake_triggers: true`。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/thing_agent/manager.rs \
  apps/cloud/src/shared/service_manager.rs
git commit -m "feat(agent): hand scheduled wakes to trigger router"
```

---

### Task 8: 端到端回归与全量验证

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`（追加集成测试）
- Test: 上述文件与全量命令

**Interfaces:**

- Consumes: `TriggerRouter`、`WakeRunner`、`RecordingSink`、`AgentPool`（ScriptedProvider）。
- Produces: 一条从 tick 到 Session 结束的回归保证。

- [ ] **Step 1: 写失败测试**

在 `trigger.rs` 测试模块追加：

```rust
#[tokio::test]
async fn tick_runs_wake_end_to_end() {
    use crate::domains::agent::session::context::TurnContextRegistry;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::runner::SessionRunner;
    use crate::domains::agent::session::todos::TodoStore;
    use crate::domains::agent::session::wake::WakeRunner;
    use crate::domains::agent::pool::AgentPool;
    use std::sync::Arc;
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};

    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(&self, _request: ChatRequest<'_>, _model: &str, _temperature: Option<f64>) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse { text: Some("ok".into()), tool_calls: vec![], usage: None, reasoning_content: None })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role { Role::Provider(ProviderKind::Model(ModelProviderKind::Custom)) }
        fn alias(&self) -> &str { "ScriptedProvider" }
    }

    let db = Arc::new(test_db().await);
    let now = ts(1_800_000_000);
    db.ensure_assistant_schedule("ws-e2e", "__assistant__ws-e2e", now).await.unwrap();
    let agent_pool = Arc::new(
        AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(NoopMemory),
            Arc::new(NoopObserver),
            Arc::new(|| Ok(Box::new(ScriptedProvider))),
        )
        .unwrap(),
    );
    let sink = Arc::new(RecordingSink::default());
    let contexts = Arc::new(TurnContextRegistry::default());
    let runner = Arc::new(SessionRunner::new(sink.clone(), contexts));
    let wake = Arc::new(WakeRunner::from_parts(
        db.clone(),
        agent_pool,
        sink.clone(),
        Arc::new(TodoStore::default()),
        runner,
    ));
    let router = Arc::new(TriggerRouter::new(db.clone(), wake));

    for handle in router.tick(now).await { handle.await.unwrap(); }

    let events = sink.events.lock().await;
    let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
    assert_eq!(
        types,
        vec![
            "trigger.received",
            "wake.dispatched",
            "session.started",
            "turn.started",
            "message.created",
            "turn.completed",
            "session.ended",
        ]
    );
    drop(events);
    let schedule = db.get_assistant_schedule("ws-e2e").await.unwrap().unwrap();
    assert_eq!(schedule.next_wake_at, Some(now + chrono::Duration::seconds(300)));
    assert_eq!(schedule.last_outcome.as_deref(), Some("completed"));
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p tinyiothub-cloud session::trigger::tests::tick_runs_wake_end_to_end -v`

Expected: 如果 Task 1–7 已完成则 PASS；若有失败，按错误修正（不要弱化断言）。

- [ ] **Step 3: 修正实现（仅在失败时）**

常见失败与处理：

- `ensure_agent` 因 workspace assistant 的缓存 flavor 重建：正常行为（Task 12/Plan 1 已实现），无需改。
- Session 行写入失败：确认测试用 `run_all_migrations` 且 `Db::new` 指向平台库。
- `session.ended` 缺失：检查 `WakeRunner::run_with_agent` 是否在 `run_turn_bounded` 之后无条件 emit。

- [ ] **Step 4: 运行全量验证**

Run:

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
```

Expected: 全部通过。若 clippy 报 `too_many_arguments`，沿用现有 `#[allow(clippy::too_many_arguments)]`；不要新增无关 allow。

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/trigger.rs
git commit -m "test(session): cover schedule wake end to end"
```

---

## 验证清单（执行者完成全部 Task 后逐项打勾）

- [ ] 新 workspace 在首次 tick 后拥有 `assistant_schedules` 行（`interval_seconds = 300`、`enabled = 1`），并立即产生一次 scheduled Session。
- [ ] 一次 wake 的事件顺序为 `trigger.received → wake.dispatched → session.started → turn.started → message.created → turn.completed → session.ended`，全部落在 `session_events`。
- [ ] 有未完成 Todo 的 wake 以 `session.ended { outcome: "partial" }` 结束。
- [ ] 全部 legacy heartbeat tasks 暂停时，该轮不产生 Session（`WakeOutcome::Skipped`），`next_wake_at` 仍推进。
- [ ] 默认配置下 `ThingAgentManager` 不再挂载 Timer/Heartbeat trigger；event/watch trigger 行为不变。
- [ ] HUD 返回 camelCase：`assistant.status` 按 `paused > working > waiting_for_user > error > idle` 判定；`devices.offline = total - online`；`pending.questions/approvals` 来自真实存储；`recentEvents` 为最近 20 条。
- [ ] Schedule PUT 校验 `intervalSeconds` 在 60–86400；启用或修改间隔会重算 `nextWakeAt`。
- [ ] 所有端点先做 workspace 归属校验（跨租户 403、不存在 404）。
- [ ] `full_access` 未在本计划接线；设备写操作仍走既有审批/Effect 路径；没有新增任何写设备路径。
- [ ] 没有删除旧表/旧端点；没有改动 Plan 3/4 的前端与清理工作。
- [ ] `git status` 不包含与计划无关的文件（尤其旧的 2026-11-02 spec 与 2026-11-03 plan）。

---

## Self-Review

**Spec coverage（Plan 2 范围内）：**

- §5.1 触发与执行分离：Task 5 `TriggerRouter` 只产 wake；Task 4 `WakeRunner` 执行 Turn。
- §5.3 组件落位：Schedule/Trigger Router 位于 `apps/cloud/src/domains/agent/session/`。
- §5.4.3 事件：Task 4/7 补齐 `session.started`（含 kind）、`session.ended`、`trigger.received`、`wake.dispatched`；payload camelCase。
- §8 事件与可见性：`session_events` 仍是唯一 UI 时间线；`hub_ledger` 未改。
- §9 默认行为：默认 5 分钟 tick；零配置可运行；无匹配技能走通用循环（wake 消息只给默认职责，具体规划由模型决定）。
- §10.2 全局 HUD 后端：Task 3/6 提供状态、设备、告警、待处理、活动、调度、最近事件。
- §13 恢复：`ask_user` 无人在线不在本计划；Todo 未完成 → `partial`（Task 4，`outcome_with_todos`）。
- §14 验证：Task 8 的 E2E 与全量命令。
- **不在本计划**：事件触发（alarm → wake）、前端 HUD 浮层、legacy 表/触发器删除、`ask_user` 在线态感知与通知。

**Placeholder scan：** 本计划没有 `TBD`/`TODO`/“实现细节略”。所有任务给出完整代码与命令。

**Type consistency：** `AssistantSchedule` 字段与 `ScheduleDto` 一致（`workspace_id/assistant_id` 只在后端）；`WakeOutcome::as_str()` 的 `completed|partial|failed|cancelled|skipped` 同时用于 `session.ended.outcome` 与 `assistant_schedules.last_outcome`；`HudStatusInputs` 字段在 Task 3/6 一致；`assistant_agent_id` 在 Task 4/5/6/7 使用同一函数；`WakeExecutor`/`WakeReport` 在 Task 4/5/8 一致；`TRIGGER_TICK_INTERVAL` 只在 Task 5 定义。

**已知风险（接受并记录）：**

1. 由 `TriggerRouter` 接管后，legacy `heartbeat_tasks` 只作为 wake 消息内容，不再有独立节奏；用户为单个任务设置的 interval 在 Plan 4 迁移到技能前不再生效。
2. 自主 wake 暴露 `ask_user`，无人在线时会阻塞至 10 分钟超时（Plan 3/4 处理在线态与通知）。
3. `TriggerRouter` 每 30 秒对全部 workspace 执行 `find_all_workspace_ids` + `ensure`；workspace 数很大时需要改成增量注册（Plan 4 可优化）。
4. HUD 的 `activeSessions` 统计的是内存中的 TurnContext；进程重启后归零，属预期。
