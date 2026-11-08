//! brain_events 投影查询（AI 大脑 P0 Task 2）：feed/summary/detail 全部读
//! Task 1 的 brain_events 只读视图。
//!
//! Legacy cleanup (Batch 4) narrowed the feed to two live sources:
//! - `judgments` (view rows with `source = 'alarm'`)
//! - `session` (recent Harness events read from the workspace store and merged
//!   in [`Db::list_brain_events`])
//!
//! `agent_actions` / `agent_runs` view branches are no longer surfaced by list
//! queries. The view itself stays in place for one version so legacy
//! `source:id` detail lookups (`patrol:`, `directive:`, `tick:`) keep working.
//!
//! 语义移植自 judgment::list_judgments_feed（48h 窗 / 折叠 / 置顶 / 元组游标），
//! 差异点：
//! - 折叠仅 alarm 源生效（thing+rule join thing_alarms via alarm_id）
//! - 「需要你」口径含工单认领态：awaiting_approval，或 escalated 且
//!   （无工单 / 工单仍 open）；已被认领（claimed 及以后）不算
//! - list 不载 evidence_json（性能评审裁决）；证据由 detail 单独取

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::database::Db;
use crate::error::{DbError, Result};

/// feed 页签：需要你 / 全部 / 会话 / 报警。
///
/// `Patrol` / `Directive` remain as one-version aliases for `Session`; they no
/// longer read their old `agent_actions` / `agent_runs` sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrainEventTab {
    NeedsYou,
    All,
    Patrol,
    Alarm,
    Directive,
    Session,
}

impl BrainEventTab {
    /// Whether this tab merges recent session (Harness) events.
    pub fn includes_session(self) -> bool {
        matches!(
            self,
            BrainEventTab::All | BrainEventTab::Patrol | BrainEventTab::Directive | BrainEventTab::Session
        )
    }
}

/// 列表行（20 列；无 evidence_json/action_params——列表不载证据与动作参数）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainEvent {
    pub id: String,
    pub workspace_id: String,
    pub source: String,
    pub alarm_id: Option<String>,
    pub thing_id: Option<String>,
    pub title: String,
    pub verdict: Option<String>,
    pub reason: String,
    pub suggested_action: Option<String>,
    pub action_category: Option<String>,
    pub risk: Option<String>,
    pub run_id: Option<String>,
    pub tick_id: Option<String>,
    pub ticket_id: Option<i64>,
    pub status: String,
    pub triage_mode: String,
    pub created_at: String,
    pub judged_at: Option<String>,
    pub state_entered_at: Option<String>,
    pub resolved_at: Option<String>,
}

impl BrainEvent {
    /// Project one Harness/Session event into the brain feed row shape.
    ///
    /// Session events are informational: they carry no verdict, alarm, ticket or
    /// judgment timestamps, and always use `status = "info"`.
    pub fn from_session(workspace_id: &str, event: tinyiothub_core::harness::HarnessEvent) -> Self {
        Self {
            id: format!("session:{}", event.event_id),
            workspace_id: workspace_id.to_string(),
            source: "session".to_string(),
            alarm_id: None,
            thing_id: None,
            title: event.event_type.clone(),
            verdict: None,
            reason: String::new(),
            suggested_action: None,
            action_category: None,
            risk: None,
            run_id: event.run_id.clone(),
            tick_id: None,
            ticket_id: None,
            status: "info".to_string(),
            triage_mode: String::new(),
            created_at: event.occurred_at.to_rfc3339(),
            judged_at: None,
            state_entered_at: None,
            resolved_at: None,
        }
    }
}

/// 详情 = 列表行 + 完整证据 + 工具调用轨迹 + judgment 叙述。
/// trace 透出（judgment 类走 evidence.trace_run_id → agent_runs.report，
/// Task 6；patrol 提案类走 tick_id → 同 workspace tick 锚行 content.trace，
/// Task 10；同一响应字段，两类来源互斥）。narrative 懒加载（T6 评审 N1：
/// 列表不载，detail 才取；NULL/非 judgment 行降级 None）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainEventDetail {
    #[serde(flatten)]
    pub event: BrainEvent,
    pub evidence_json: String,
    #[serde(default)]
    pub trace: Vec<crate::agent_runs::ToolTraceEntry>,
    #[serde(default)]
    pub narrative: Option<String>,
}

/// 摘要：今日消化 / 需要你 / alarm 判定延迟 p50 / 反馈对错（judgment_feedback 现状表）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BrainEventsSummary {
    pub digested_today: u64,
    pub needs_you: u64,
    pub latency_p50_secs: Option<f64>,
    pub feedback_right: u64,
    pub feedback_wrong: u64,
}

/// 列表列清单（不含 evidence_json/action_params）。
const LIST_COLS: &str = "id, workspace_id, source, alarm_id, thing_id, title, verdict, reason, \
    suggested_action, action_category, risk, run_id, tick_id, ticket_id, status, triage_mode, \
    created_at, judged_at, state_entered_at, resolved_at";

/// 「需要你」口径（仅在 FROM brain_events 的作用域内使用）。
/// 口径决定：needs_you 无时间窗——陈旧未认领恰恰最该被看见；其余 tab 保持 48h 窗防膨胀
/// （见 list_brain_events 的 window 分支，与 summary.needs_you 计数对齐）。
const NEEDS_YOU_COND: &str = "(status = 'awaiting_approval' OR (status = 'escalated' AND \
    (ticket_id IS NULL OR (SELECT t.state FROM tickets t WHERE t.id = brain_events.ticket_id) = 'open')))";

fn tab_filter(tab: BrainEventTab) -> &'static str {
    match tab {
        BrainEventTab::NeedsYou => {
            " AND source = 'alarm' AND (status = 'awaiting_approval' OR (status = 'escalated' AND \
            (ticket_id IS NULL OR (SELECT t.state FROM tickets t WHERE t.id = brain_events.ticket_id) = 'open')))"
        }
        // Judgments are the only live legacy source.
        BrainEventTab::All | BrainEventTab::Alarm => " AND source = 'alarm'",
        // Legacy patrol/directive tabs are aliases for session events; their
        // old agent_actions/agent_runs rows are no longer listed.
        BrainEventTab::Patrol | BrainEventTab::Directive | BrainEventTab::Session => " AND 1 = 0",
    }
}

fn row_to_brain_event(row: sqlx::sqlite::SqliteRow) -> Result<BrainEvent> {
    Ok(BrainEvent {
        id: row.get("id"),
        workspace_id: row.get("workspace_id"),
        source: row.get("source"),
        alarm_id: row.get("alarm_id"),
        thing_id: row.get("thing_id"),
        title: row.get("title"),
        verdict: row.get("verdict"),
        reason: row.get("reason"),
        suggested_action: row.get("suggested_action"),
        action_category: row.get("action_category"),
        risk: row.get("risk"),
        run_id: row.get("run_id"),
        tick_id: row.get("tick_id"),
        ticket_id: row.get("ticket_id"),
        status: row.get("status"),
        triage_mode: row.get("triage_mode"),
        created_at: row.get("created_at"),
        judged_at: row.get("judged_at"),
        state_entered_at: row.get("state_entered_at"),
        resolved_at: row.get("resolved_at"),
    })
}

/// feed 页查询：workspace 隔离 + tab 筛选 + 48h 窗口（RFC3339 字符串比较；needs_you tab 除外）。
/// - 首页（before=None）：alarm 源同 thing+rule 折叠最新（window function）
/// - 非合并 tab（needs_you/alarm）：需要你置顶（X6：escalated 先于
///   awaiting_approval）
/// - 合并 tab（all/session/legacy patrol/directive）：严格按 `(created_at, id)`
///   倒序，与 [`Db::list_brain_events`] 的 session 合并顺序一致——否则分页
///   游标会跨过被置顶挤出首页的行
/// - 后续页（before=Some）：纯时间流（折叠/置顶不参与，同 judgment feed 语义）
/// - 游标：(created_at, id) 元组由 [`Db::list_brain_events`] 在 workspace 作用域
///   内解析后传入，同秒多行不丢；session 游标同样适用
pub(crate) async fn list_brain_events(
    pool: &SqlitePool,
    workspace_id: &str,
    tab: BrainEventTab,
    before: Option<(&str, &str)>, // 游标：(created_at, id)，由调用方按 workspace 解析
    limit: i64,
) -> Result<Vec<BrainEvent>> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    let filter = tab_filter(tab);
    let merged = tab.includes_session();
    // needs_you 无时间窗——陈旧未认领恰恰最该被看见；其余 tab 保持 48h 窗防膨胀。
    let window = if tab == BrainEventTab::NeedsYou {
        ""
    } else {
        " AND created_at >= ?"
    };

    let sql = if before.is_none() {
        // 折叠：仅 alarm 源按 thing+rule 分组取最新；非 alarm 行按 id 各自成组（rn 恒 1）。
        // 非合并 tab 保留置顶顺序（需要你 pin=0 在前，escalated 再先于
        // awaiting_approval）；合并 tab 必须与 merge 顺序一致，只按时间倒序。
        let order = if merged {
            "created_at DESC, id DESC"
        } else {
            "pin, CASE WHEN pin = 0 AND status = 'escalated' THEN 0 ELSE 1 END, created_at DESC, id DESC"
        };
        format!(
            "SELECT * FROM (
               SELECT {LIST_COLS},
                 CASE WHEN {NEEDS_YOU_COND} THEN 0 ELSE 1 END AS pin,
                 ROW_NUMBER() OVER (
                   PARTITION BY CASE WHEN source = 'alarm' THEN COALESCE(thing_id, id) ELSE id END,
                                CASE WHEN source = 'alarm' THEN
                                  COALESCE((SELECT a.rule_id FROM thing_alarms a WHERE a.id = brain_events.alarm_id), '')
                                  ELSE '' END
                   ORDER BY created_at DESC, id DESC
                 ) AS rn
               FROM brain_events
               WHERE workspace_id = ?{window}{filter}
             ) WHERE rn = 1
             ORDER BY {order}
             LIMIT ?"
        )
    } else {
        // 后续页：纯时间流（不折叠不置顶）。元组游标：同秒行按 id 续翻。
        // 游标时间戳由调用方在 workspace 作用域内解析（session 游标来自
        // workspace store，legacy 游标来自本视图）——他 workspace 的 id 解析
        // 不到时间戳，调用方直接返回空页。
        format!(
            "SELECT {LIST_COLS} FROM brain_events
             WHERE workspace_id = ?{window}{filter}
               AND (created_at < ? OR (created_at = ? AND id < ?))
             ORDER BY created_at DESC, id DESC
             LIMIT ?"
        )
    };

    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(workspace_id);
    if tab != BrainEventTab::NeedsYou {
        q = q.bind(since);
    }
    if let Some((cursor_created_at, cursor_id)) = before {
        q = q.bind(cursor_created_at).bind(cursor_created_at).bind(cursor_id);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    rows.into_iter().map(row_to_brain_event).collect()
}

/// 详情：列表行 + evidence_json（证据只在这里载）+ trace（Task 6 透出）。
pub(crate) async fn brain_event_detail(
    pool: &SqlitePool,
    workspace_id: &str,
    id: &str,
) -> Result<Option<BrainEventDetail>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {LIST_COLS}, evidence_json FROM brain_events WHERE id = ? AND workspace_id = ?"
    )))
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let evidence_json: String = row.get("evidence_json");
    let event = row_to_brain_event(row)?;
    // Task 6（eng D2）：judgment 类证据存 trace 引用（trace_run_id）不存副本——
    // detail 时按引用回 agent_runs.report 取 trace 透出。引用缺失（老数据）/
    // 报告丢失/report 解析失败均降级为空 trace：证据主体 evidence_json 仍在，
    // 不让 detail 整体失败。
    // Task 10：patrol 提案类走 tick_id——join 同 workspace 的 tick 锚行
    // （action_type IN ('summary','error')，部分唯一索引保证至多一行）取
    // content.trace。两类来源互斥（judgment 行 tick_id 恒 NULL，提案行证据
    // 无 trace_run_id）；锚行缺失/content 解析失败同样降级空 trace。
    let trace_run_id = serde_json::from_str::<serde_json::Value>(&evidence_json)
        .ok()
        .and_then(|v| v.get("trace_run_id")?.as_str().map(str::to_string));
    let trace = match trace_run_id {
        Some(run_id) => match crate::agent_runs::find_run_report_json(pool, workspace_id, &run_id).await {
            Ok(Some(report_json)) => serde_json::from_str::<crate::agent_runs::RunReport>(&report_json)
                .map(|r| r.trace)
                .unwrap_or_default(),
            _ => vec![],
        },
        None => match &event.tick_id {
            Some(tick_id) => {
                let content: Option<String> = sqlx::query_scalar(
                    "SELECT content FROM agent_actions
                     WHERE workspace_id = ? AND tick_id = ? AND action_type IN ('summary','error')
                     LIMIT 1",
                )
                .bind(workspace_id)
                .bind(tick_id)
                .fetch_optional(pool)
                .await?;
                content
                    .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                    .and_then(|v| v.get("trace").cloned())
                    .and_then(|t| serde_json::from_value::<Vec<crate::agent_runs::ToolTraceEntry>>(t).ok())
                    .unwrap_or_default()
            }
            None => vec![],
        },
    };
    // T6 评审 N1：judgment 的 narrative 在 detail 懒加载透出（列表不载）。
    // judgment 类行 id 形如 'alarm:<judgment_id>'；非 judgment 行/NULL 降级 None。
    let narrative = match event.id.strip_prefix("alarm:") {
        Some(judgment_id) => sqlx::query_scalar("SELECT narrative FROM judgments WHERE id = ? AND workspace_id = ?")
            .bind(judgment_id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?
            .flatten(),
        None => None,
    };
    Ok(Some(BrainEventDetail {
        event,
        evidence_json,
        trace,
        narrative,
    }))
}

/// 摘要：今日消化（created_at 今日且 status 终态）、需要你计数、alarm 判定延迟
/// p50（judged_at-created_at，仅 alarm 源、judged_at 非空；30 天窗口防全表扫，
/// 同 judgment_stats F3）、反馈对错（judgment_feedback——P0 不迁）。
pub(crate) async fn brain_events_summary(pool: &SqlitePool, workspace_id: &str) -> Result<BrainEventsSummary> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT
           SUM(CASE WHEN created_at >= date('now')
                     AND status IN ('resolved','self_closed','dismissed','investigation_failed','budget_skipped')
                    THEN 1 ELSE 0 END) AS digested_today,
           SUM(CASE WHEN {NEEDS_YOU_COND} THEN 1 ELSE 0 END) AS needs_you
         FROM brain_events WHERE workspace_id = ? AND source = 'alarm'"
    )))
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let fb = sqlx::query(
        "SELECT
           SUM(CASE WHEN verdict = 'right' THEN 1 ELSE 0 END) AS right_count,
           SUM(CASE WHEN verdict = 'wrong' THEN 1 ELSE 0 END) AS wrong_count
         FROM judgment_feedback WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let window = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    let latency_rows = sqlx::query(
        "SELECT (julianday(judged_at) - julianday(created_at)) * 86400.0 AS secs
         FROM brain_events
         WHERE workspace_id = ? AND source = 'alarm' AND judged_at IS NOT NULL AND created_at >= ?
         ORDER BY secs",
    )
    .bind(workspace_id)
    .bind(&window)
    .fetch_all(pool)
    .await?;
    let latencies: Vec<f64> = latency_rows.iter().map(|r| r.get::<f64, _>("secs")).collect();
    let latency_p50_secs = if latencies.is_empty() {
        None
    } else {
        let idx = ((latencies.len() as f64) * 0.50).ceil() as usize;
        Some(latencies[idx.saturating_sub(1).min(latencies.len() - 1)])
    };

    Ok(BrainEventsSummary {
        digested_today: row.get::<Option<i64>, _>("digested_today").unwrap_or(0) as u64,
        needs_you: row.get::<Option<i64>, _>("needs_you").unwrap_or(0) as u64,
        latency_p50_secs,
        feedback_right: fb.get::<Option<i64>, _>("right_count").unwrap_or(0) as u64,
        feedback_wrong: fb.get::<Option<i64>, _>("wrong_count").unwrap_or(0) as u64,
    })
}

// ── Db 门面 ──

impl Db {
    /// List the feed. Legacy judgment rows are read from the `brain_events`
    /// view first (source-narrowed to `alarm`); when the tab opts in, recent
    /// Harness/Session events are merged, sorted deterministically by
    /// `(created_at, id)` descending and sliced to `limit`.
    ///
    /// The invariant that makes per-source `limit` fetches lossless is that
    /// both sources use the *same* `(created_at, id)` descending order; there
    /// is no pin/priority reordering on a merged tab. A page cursor is the
    /// same tuple, so each source's next page continues exactly where the
    /// merged page ended.
    pub async fn list_brain_events(
        &self,
        workspace_id: &str,
        tab: BrainEventTab,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<BrainEvent>> {
        let limit = limit.max(1);
        // Resolve the cursor inside this workspace. Unknown/foreign cursors
        // yield an empty page (the old SQL sub-query returned NULL).
        let cursor = match before {
            Some(cursor) => match self.brain_event_cursor_created_at(workspace_id, cursor).await? {
                Some(created_at) => Some((created_at, cursor)),
                None => return Ok(Vec::new()),
            },
            None => None,
        };
        let before_pair = cursor.as_ref().map(|(created_at, id)| (created_at.as_str(), *id));
        let mut events = list_brain_events(self.pool(), workspace_id, tab, before_pair, limit).await?;
        if !tab.includes_session() {
            return Ok(events);
        }

        // A workspace without a registered harness store simply has no session
        // events; keep the judgment feed readable in that case. With a cursor,
        // paginate the session source by the same tuple so a limit page is
        // filled even when many newer session rows exist.
        let session_page = match &cursor {
            Some((cursor_created_at, cursor_id)) => {
                self.recent_session_events_before(workspace_id, cursor_created_at, cursor_id, limit)
                    .await
            }
            None => self.recent_session_events(workspace_id, limit).await,
        };
        let session_events = match session_page {
            Ok(events) => events,
            Err(DbError::NotFound { .. }) => Vec::new(),
            Err(error) => return Err(error),
        };

        let mut session_events: Vec<BrainEvent> = session_events
            .into_iter()
            .map(|event| BrainEvent::from_session(workspace_id, event))
            .collect();

        if let Some((cursor_created_at, cursor_id)) = &cursor {
            session_events.retain(|event| {
                event.created_at < *cursor_created_at
                    || (event.created_at == *cursor_created_at && event.id.as_str() < *cursor_id)
            });
        }

        events.extend(session_events);
        events.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        events.truncate(limit as usize);
        Ok(events)
    }

    /// Resolve the `(created_at, id)` cursor timestamp for either a legacy
    /// `source:id` view row or a `session:<event_id>` Harness event.
    async fn brain_event_cursor_created_at(&self, workspace_id: &str, cursor: &str) -> Result<Option<String>> {
        if let Some(event_id) = cursor.strip_prefix("session:") {
            return Ok(self
                .session_event_by_id(workspace_id, event_id)
                .await?
                .map(|event| event.occurred_at.to_rfc3339()));
        }
        // Legacy view ids keep their `source:id` shape (alarm:, patrol:,
        // directive:, tick:).
        Ok(
            sqlx::query_scalar("SELECT created_at FROM brain_events WHERE id = ? AND workspace_id = ?")
                .bind(cursor)
                .bind(workspace_id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn brain_event_detail(&self, workspace_id: &str, id: &str) -> Result<Option<BrainEventDetail>> {
        // Session rows are not in the platform view; project them directly.
        if let Some(event_id) = id.strip_prefix("session:") {
            let Some(event) = self.session_event_by_id(workspace_id, event_id).await? else {
                return Ok(None);
            };
            let evidence_json = serde_json::to_string(&event.payload_json).unwrap_or_else(|_| "{}".to_string());
            let brain_event = BrainEvent::from_session(workspace_id, event);
            return Ok(Some(BrainEventDetail {
                event: brain_event,
                evidence_json,
                trace: Vec::new(),
                narrative: None,
            }));
        }
        // Legacy `source:id` prefixes still resolve through the view.
        brain_event_detail(self.pool(), workspace_id, id).await
    }

    pub async fn brain_events_summary(&self, workspace_id: &str) -> Result<BrainEventsSummary> {
        brain_events_summary(self.pool(), workspace_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judgment::{JudgmentStatus, JudgmentVerdict};
    use chrono::{DateTime, Utc};
    use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

    /// Workspace store fixture for session-event merge tests.
    struct SessionFeedFixture {
        root: std::path::PathBuf,
        db: Db,
    }

    impl SessionFeedFixture {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-brain-session-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let db = Db::new(pool).with_workspace_store_root(root.join("data/agents"));
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-session', 'workspace', 'tenant-default-001', 'now', 'now')",
            )
            .execute(db.pool())
            .await
            .expect("insert workspace");
            db.create_workspace_store("ws-session")
                .await
                .expect("create workspace store");
            Self { root, db }
        }

        async fn close(self) {
            self.db
                .close_workspace_store("ws-session")
                .await
                .expect("close workspace store");
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn session_input(event_id: &str, occurred_at: DateTime<Utc>) -> HarnessEventInput {
        HarnessEventInput {
            event_id: event_id.to_string(),
            workspace_id: "ws-session".to_string(),
            session_id: "session-merge".to_string(),
            execution_mode: ExecutionMode::Production,
            run_id: Some("run-session".to_string()),
            task_id: Some("task-session".to_string()),
            event_type: "run.started".to_string(),
            occurred_at,
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("digest-{event_id}"),
            payload_json: serde_json::json!({"event": event_id}),
        }
    }

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    /// 带系统 seed 的库（tenants 等 FK 依赖；fold 测试需要 workspace→tenant 链）。
    async fn seeded_db() -> Db {
        let db = test_db().await;
        crate::seed::seed_system(&db).await.unwrap();
        db
    }

    async fn insert_ticket(db: &Db, state: &str, failure_hash: &str) -> i64 {
        let r = sqlx::query(
            "INSERT INTO tickets (workspace_id, agent_run_id, title, briefing, failure_hash, state)
             VALUES ('ws1','run-x','t','{}',?,?)",
        )
        .bind(failure_hash)
        .bind(state)
        .execute(db.pool())
        .await
        .unwrap();
        r.last_insert_rowid()
    }

    /// 需要你口径：awaiting_approval + escalated 未认领（无工单或工单 open）。
    #[tokio::test]
    async fn needs_you_counts_approval_and_unclaimed_escalated() {
        let db = test_db().await;
        // 1 条 awaiting_approval
        let j1 = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &j1,
            JudgmentVerdict::SelfHealable,
            "r",
            "{}",
            Some("a"),
            Some("other"),
            None,
            None,
        )
        .await
        .unwrap();
        // 1 条 escalated 且 ticket open（未认领→算）
        let j2 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let open_ticket = insert_ticket(&db, "open", "h-open").await;
        db.transit_judgment(
            &j2,
            JudgmentStatus::Investigating,
            JudgmentStatus::Escalated,
            Some(open_ticket),
        )
        .await
        .unwrap();
        // 1 条 escalated 且 ticket claimed（已认领→不算）
        let j3 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let claimed_ticket = insert_ticket(&db, "claimed", "h-claimed").await;
        db.transit_judgment(
            &j3,
            JudgmentStatus::Investigating,
            JudgmentStatus::Escalated,
            Some(claimed_ticket),
        )
        .await
        .unwrap();

        let feed = db
            .list_brain_events("ws1", BrainEventTab::NeedsYou, None, 10)
            .await
            .unwrap();
        assert_eq!(feed.len(), 2, "claimed 的不算需要你");
        let summary = db.brain_events_summary("ws1").await.unwrap();
        assert_eq!(summary.needs_you, 2);

        // summary 其余字段冒烟：反馈计数 + alarm 源延迟 p50
        db.add_judgment_feedback(&j1, "ws1", "u1", "right", None).await.unwrap();
        let summary = db.brain_events_summary("ws1").await.unwrap();
        assert_eq!(summary.feedback_right, 1);
        assert_eq!(summary.feedback_wrong, 0);
        assert!(summary.latency_p50_secs.is_some(), "j1 已判定（alarm 源）");
    }

    /// X6：需要你内 escalated（要人动手）排在 awaiting_approval（等点头）前——
    /// 即使 awaiting_approval 更新（时间倒序会让它靠前）。
    #[tokio::test]
    async fn needs_you_escalated_sorts_before_awaiting_approval() {
        let db = test_db().await;
        // 先建 escalated（较早），后建 awaiting_approval（较新）
        let j_old = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.transit_judgment(&j_old, JudgmentStatus::Investigating, JudgmentStatus::Escalated, None)
            .await
            .unwrap();
        let j_new = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &j_new,
            JudgmentVerdict::SelfHealable,
            "r",
            "{}",
            Some("a"),
            Some("other"),
            None,
            None,
        )
        .await
        .unwrap();
        // 确保 created_at 有秒级差（awaiting_approval 更新）
        sqlx::query("UPDATE judgments SET created_at = datetime('now', '-1 hour') WHERE id = ?")
            .bind(&j_old)
            .execute(db.pool())
            .await
            .unwrap();

        let feed = db
            .list_brain_events("ws1", BrainEventTab::NeedsYou, None, 10)
            .await
            .unwrap();
        assert_eq!(feed.len(), 2);
        assert_eq!(
            feed[0].status, "escalated",
            "escalated 必须排在 awaiting_approval 前（即使更旧）"
        );
        assert_eq!(feed[1].status, "awaiting_approval");
    }

    /// 折叠仅 alarm 源生效：同 thing+rule 两条 alarm 事件折 1 行；
    /// 两条不同 proposalId 的 patrol 事件不折叠。
    #[tokio::test]
    async fn alarm_rows_fold_by_thing_rule_others_dont() {
        let db = seeded_db().await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')",
        )
        .execute(db.pool())
        .await
        .expect("workspace insert");
        sqlx::query(
            "INSERT INTO things (id, workspace_id, name, thing_type, state, created_at, updated_at)
             VALUES ('t1','ws1','dev1','sensor',0,'2025-01-01','2025-01-01')",
        )
        .execute(db.pool())
        .await
        .expect("things insert");
        sqlx::query(
            "INSERT INTO thing_alarm_rules (id, thing_id, rule_name, rule_type, condition_config, alarm_level, workspace_id)
             VALUES ('r1','t1','温度阈值','threshold','{}','warning','ws1')",
        )
        .execute(db.pool())
        .await
        .expect("rule insert");
        for aid in ["a1", "a2"] {
            sqlx::query(
                "INSERT INTO thing_alarms (id, thing_id, rule_id, alarm_level, alarm_message, alarm_time, workspace_id)
                 VALUES (?,'t1','r1','warning','m1','2026-09-14','ws1')",
            )
            .bind(aid)
            .execute(db.pool())
            .await
            .expect("alarm insert");
        }
        let j1 = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        let j2 = db
            .insert_judgment("ws1", Some("a2"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        // 显式递增时间戳（以 Utc::now() 为锚——feed 有 48h 窗口，不硬编码历史日期）
        let t1 = (Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
        let t2 = (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
            .bind(&t1)
            .bind(&j1)
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
            .bind(&t2)
            .bind(&j2)
            .execute(db.pool())
            .await
            .unwrap();

        // 两条 patrol 提案（不同 proposalId，同 thing）——Batch 4 收窄后
        // 不再出现在 list 源里（视图仍在，detail 一版兼容）。
        for p in ["p1", "p2"] {
            sqlx::query(
                "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at)
                 VALUES (?,'ws1','agent1','patrol','proposal',?,?)",
            )
            .bind(format!("act-{p}"))
            .bind(format!(
                "{{\"proposalId\":\"{p}\",\"thingId\":\"t1\",\"summary\":\"s-{p}\",\"status\":\"pending\"}}"
            ))
            .bind(Utc::now().to_rfc3339())
            .execute(db.pool())
            .await
            .expect("proposal insert");
        }

        let feed = db.list_brain_events("ws1", BrainEventTab::All, None, 10).await.unwrap();
        let alarm_rows: Vec<_> = feed.iter().filter(|e| e.source == "alarm").collect();
        assert_eq!(alarm_rows.len(), 1, "同 thing+rule 折叠为一条");
        assert_eq!(alarm_rows[0].id, format!("alarm:{j2}"), "折叠保留最新");
        assert_eq!(feed.len(), 1, "patrol/directive 源不再进入 list");
    }

    /// 同秒多行元组游标不丢行（Utc::now() 锚定，48h 窗口不漂移）。
    #[tokio::test]
    async fn tuple_cursor_no_loss_same_second() {
        let db = test_db().await;
        let mut ids = vec![];
        for _ in 0..3 {
            ids.push(db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap());
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE judgments SET created_at = ?")
            .bind(&now)
            .execute(db.pool())
            .await
            .unwrap();
        let page1 = db.list_brain_events("ws1", BrainEventTab::All, None, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db
            .list_brain_events("ws1", BrainEventTab::All, Some(&page1[1].id), 10)
            .await
            .unwrap();
        assert_eq!(page2.len(), 1, "同秒第三行必须可翻页到达");
    }

    /// 游标跨 workspace 隔离回归（2026-10-16 ship 评审补）：游标子查询带
    /// workspace 过滤——用 ws-B 的行 id 作 ws-A 的 before 游标时，子查询不
    /// 回行（NULL），ws-A 得空页而不是借到 ws-B 的时间戳翻页（弱 oracle
    /// 锁定：修复前会返回 ws-A 中早于 ws-B 游标行时间戳的行）。
    #[tokio::test]
    async fn cursor_from_other_workspace_yields_empty_page() {
        let db = test_db().await;
        // ws-A 两行：一行早于、一行晚于 ws-B 游标行的时间戳。
        let j_a_old = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let j_a_new = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        // ws-B 游标行。
        let j_b = db.insert_judgment("ws2", None, None, None, "annotate").await.unwrap();
        let t_old = (Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
        let t_mid = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let t_new = Utc::now().to_rfc3339();
        for (id, ts) in [(&j_a_old, &t_old), (&j_b, &t_mid), (&j_a_new, &t_new)] {
            sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
                .bind(ts)
                .bind(id)
                .execute(db.pool())
                .await
                .unwrap();
        }

        let page = db
            .list_brain_events("ws1", BrainEventTab::All, Some(&format!("alarm:{j_b}")), 10)
            .await
            .unwrap();
        assert!(
            page.is_empty(),
            "ws-B 的行 id 作 ws-A 游标不得借到 ws-B 时间戳（修复前会返回 j_a_old）: {:?}",
            page.iter().map(|e| &e.id).collect::<Vec<_>>()
        );

        // 对照：ws-A 自己的游标正常翻页（锁定子查询未塌缩为恒空）。
        let page1 = db.list_brain_events("ws1", BrainEventTab::All, None, 1).await.unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].id, format!("alarm:{j_a_new}"));
        let page2 = db
            .list_brain_events("ws1", BrainEventTab::All, Some(&page1[0].id), 10)
            .await
            .unwrap();
        assert_eq!(page2.len(), 1, "本 workspace 游标翻页不受影响");
        assert_eq!(page2[0].id, format!("alarm:{j_a_old}"));
    }

    /// list 行无证据字段（结构层面无 evidence_json）；detail 返回完整证据。
    #[tokio::test]
    async fn list_does_not_carry_evidence_detail_does() {
        let db = test_db().await;
        let j = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        let evidence = "{\"run\":\"r1\",\"steps\":[]}";
        db.judge_judgment(&j, JudgmentVerdict::Noise, "波动", evidence, None, None, None, None)
            .await
            .unwrap();
        let feed = db.list_brain_events("ws1", BrainEventTab::All, None, 10).await.unwrap();
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].id, format!("alarm:{j}"));
        assert_eq!(feed[0].status, "self_closed", "noise_archived 在视图层改名");
        let detail = db.brain_event_detail("ws1", &feed[0].id).await.unwrap().unwrap();
        assert_eq!(detail.evidence_json, evidence);
        assert_eq!(detail.event.id, feed[0].id);
        // workspace 隔离
        assert!(db.brain_event_detail("ws2", &feed[0].id).await.unwrap().is_none());
    }

    /// Batch 4 收窄：旧 directive/patrol 源不再出现在 list；旧 `source:id`
    /// detail 仍可解析（一版兼容）。
    #[tokio::test]
    async fn legacy_directive_hidden_from_list_but_detail_resolves() {
        let db = test_db().await;
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, problem_key, created_at)
             VALUES ('r-user','ws1','user','success','查一下温度','{}',NULL,?)",
        )
        .bind(&now)
        .execute(db.pool())
        .await
        .unwrap();

        // 旧 tab=directive 现在读 session（本库无 harness store → 空）。
        for tab in [BrainEventTab::Directive, BrainEventTab::Patrol, BrainEventTab::Session] {
            let feed = db.list_brain_events("ws1", tab, None, 10).await.unwrap();
            assert!(feed.is_empty(), "{tab:?} 不再返回 legacy directive/patrol 行: {feed:?}");
        }

        // 旧 id 前缀仍可解析。
        let detail = db.brain_event_detail("ws1", "directive:r-user").await.unwrap().unwrap();
        assert_eq!(detail.event.source, "directive");
        assert_eq!(detail.event.id, "directive:r-user");
    }

    /// 提案行投影 tick_id（Task 2 视图重建）：同 tick 锚行（summary）插 tick_id=t1、
    /// 提案行手工带 tick_id=t1（Task 9 才由写入方填——此处手工 INSERT 模拟）。
    /// Batch 4 起 list 不再返回 patrol 源，改用 legacy detail 断言投影。
    #[tokio::test]
    async fn legacy_proposal_detail_projects_tick_id() {
        let db = test_db().await;
        let now = Utc::now().to_rfc3339();
        // tick 锚行（action_type='summary'）。
        // 注：tick_id 上有部分唯一索引（20260824000002），同库两行不能同值——
        // 锚行用 't0'，提案行用 't1'（简报草图同值 t1 与真实约束不符，按真实代码修正）。
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
             VALUES ('act-sum','ws1','agent1','patrol','summary','{\"result\":\"ok\"}','t0',?)",
        )
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("summary anchor insert");
        // 提案行（Task 9 才写 tick_id——此处手工 INSERT 模拟）
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
             VALUES ('act-prop','ws1','agent1','patrol','proposal',?,'t1',?)",
        )
        .bind("{\"proposalId\":\"p1\",\"thingId\":\"t1\",\"summary\":\"s-p1\",\"status\":\"pending\"}")
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("proposal insert");

        // list 不再包含 patrol 源。
        let feed = db.list_brain_events("ws1", BrainEventTab::All, None, 10).await.unwrap();
        assert!(feed.iter().all(|e| e.source == "alarm"), "仅 judgment 源: {feed:?}");

        // legacy detail 仍透出 tick_id 投影。
        let proposal = db.brain_event_detail("ws1", "patrol:p1").await.unwrap().unwrap();
        assert_eq!(proposal.event.tick_id.as_deref(), Some("t1"), "提案行投影 tick_id");

        let tick_id: String = sqlx::query_scalar("SELECT id FROM brain_events WHERE source = 'patrol_tick' LIMIT 1")
            .fetch_one(db.pool())
            .await
            .expect("tick 聚合行存在");
        let tick_row = db.brain_event_detail("ws1", &tick_id).await.unwrap().unwrap();
        assert!(tick_row.event.tick_id.is_none(), "tick 聚合行 tick_id 保持 NULL");
    }

    /// Task 6：judgment 类 detail 透出 trace——evidence.trace_run_id（引用式，
    /// eng D2）→ agent_runs.report.trace；引用缺失/报告丢失/报告解析失败均降级
    /// 为空 trace，不让 detail 失败。
    #[tokio::test]
    async fn detail_exposes_trace_for_judgment_event() {
        let db = test_db().await;
        // 调查 run 的 report（带 1 条 trace）
        let report = crate::agent_runs::RunReport {
            run_id: "r-trace".to_string(),
            workspace_id: "ws1".to_string(),
            trigger: "alarm".to_string(),
            outcome: crate::agent_runs::Outcome::NoActionNeeded,
            summary: "调查完毕".to_string(),
            actions: vec![],
            verified: false,
            duration_ms: 100,
            tool_calls: 1,
            tokens: 10,
            end_reason: None,
            thing_id: Some("t1".to_string()),
            trace: vec![crate::agent_runs::ToolTraceEntry {
                id: "c1".to_string(),
                name: "read_property".to_string(),
                args: serde_json::json!({"p": "temp"}),
                output: Some("26.5".to_string()),
                evidence: vec![],
            }],
            task_count: None,
            snapshot: None,
            task_id: None,
            wake_event_id: None,
        };
        db.insert_agent_run(&report, Some("alarm:t1:-"), None).await.unwrap();

        let j = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        let evidence = serde_json::json!({
            "source": "run_summary",
            "run_id": "r-trace",
            "trace_run_id": "r-trace",
            "summary": "调查完毕",
        })
        .to_string();
        db.judge_judgment(&j, JudgmentVerdict::Noise, "波动", &evidence, None, None, None, None)
            .await
            .unwrap();

        let detail = db
            .brain_event_detail("ws1", &format!("alarm:{j}"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.trace.len(), 1, "detail 透出 report.trace");
        assert_eq!(detail.trace[0].name, "read_property");
        assert_eq!(detail.trace[0].output.as_deref(), Some("26.5"));

        // 老数据：evidence 无 trace_run_id → 空 trace，detail 照常返回
        let j2 = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(&j2, JudgmentVerdict::Noise, "波动", "{}", None, None, None, None)
            .await
            .unwrap();
        let detail2 = db
            .brain_event_detail("ws1", &format!("alarm:{j2}"))
            .await
            .unwrap()
            .unwrap();
        assert!(detail2.trace.is_empty(), "无 trace_run_id 引用 → 空 trace 不炸");

        // 引用悬空：trace_run_id 指向不存在的 run → 空 trace，detail 照常返回
        let j3 = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        let dangling = serde_json::json!({"trace_run_id": "r-gone"}).to_string();
        db.judge_judgment(&j3, JudgmentVerdict::Noise, "波动", &dangling, None, None, None, None)
            .await
            .unwrap();
        let detail3 = db
            .brain_event_detail("ws1", &format!("alarm:{j3}"))
            .await
            .unwrap()
            .unwrap();
        assert!(detail3.trace.is_empty(), "引用悬空 → 空 trace 不炸");
    }

    /// Task 10：patrol 提案行 detail 透出同 tick 锚行的全量 trace（按 tick_id
    /// join 同 workspace 的 summary/error 锚行 content.trace）；无 tick_id 的
    /// 提案、tick 聚合行（tick_id 恒 NULL）均降级为空 trace 不炸。
    #[tokio::test]
    async fn detail_exposes_anchor_trace_for_patrol_proposal() {
        let db = test_db().await;
        let now = Utc::now().to_rfc3339();
        // tick 锚行（action_type='summary'，content 含全量 trace——Task 9 形状）。
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
             VALUES ('act-sum','ws1','agent1','patrol','summary',?,'tk1',?)",
        )
        .bind(serde_json::json!({
            "taskCount": 1,
            "result": "ok",
            "trace": [{"id":"c1","name":"read_property","args":{"p":"temp"},"output":"26.5"}],
        }).to_string())
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("anchor insert");
        // 提案行携带同一 tick_id（Task 9 写入方行为，手工 INSERT 模拟）。
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
             VALUES ('act-prop','ws1','agent1','patrol','proposal',?,'tk1',?)",
        )
        .bind("{\"proposalId\":\"p1\",\"thingId\":\"t1\",\"summary\":\"s-p1\",\"status\":\"pending\"}")
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("proposal insert");

        let detail = db.brain_event_detail("ws1", "patrol:p1").await.unwrap().unwrap();
        assert_eq!(detail.trace.len(), 1, "提案 detail 透出锚行 trace");
        assert_eq!(detail.trace[0].name, "read_property");
        assert_eq!(detail.trace[0].output.as_deref(), Some("26.5"));

        // 无 tick_id 的提案行（历史数据）→ 空 trace 不炸。
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at)
             VALUES ('act-prop2','ws1','agent1','patrol','proposal',?,?)",
        )
        .bind("{\"proposalId\":\"p2\",\"thingId\":\"t1\",\"summary\":\"s-p2\",\"status\":\"pending\"}")
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("proposal2 insert");
        let detail2 = db.brain_event_detail("ws1", "patrol:p2").await.unwrap().unwrap();
        assert!(detail2.trace.is_empty(), "无 tick_id 提案 → 空 trace 不炸");

        // tick_id 悬空（锚行不存在）→ 空 trace 不炸。
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
             VALUES ('act-prop3','ws1','agent1','patrol','proposal',?,'tk-gone',?)",
        )
        .bind("{\"proposalId\":\"p3\",\"thingId\":\"t1\",\"summary\":\"s-p3\",\"status\":\"pending\"}")
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("proposal3 insert");
        let detail3 = db.brain_event_detail("ws1", "patrol:p3").await.unwrap().unwrap();
        assert!(detail3.trace.is_empty(), "tick_id 悬空 → 空 trace 不炸");

        // tick 聚合行（tick_id 恒 NULL）→ 空 trace 不炸。
        // Batch 4 起 list 不再返回 patrol_tick；从视图取 id 走 legacy detail。
        let tick_id: String = sqlx::query_scalar("SELECT id FROM brain_events WHERE source = 'patrol_tick' LIMIT 1")
            .fetch_one(db.pool())
            .await
            .expect("tick 聚合行存在");
        let detail4 = db.brain_event_detail("ws1", &tick_id).await.unwrap().unwrap();
        assert!(detail4.trace.is_empty(), "tick 聚合行 trace 为空不炸");
    }

    /// Task 10（T6 评审 N1 结转）：detail 透出 judgment 的 narrative（懒加载，
    /// 列表不载）；narrative 为 NULL（未填充/非 judgment 行）时透出 None 不炸。
    #[tokio::test]
    async fn detail_exposes_judgment_narrative() {
        let db = test_db().await;
        let j = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &j,
            JudgmentVerdict::Noise,
            "波动",
            "{}",
            None,
            None,
            None,
            Some("温度短暂波动后自行恢复，无需处理"),
        )
        .await
        .unwrap();
        let detail = db
            .brain_event_detail("ws1", &format!("alarm:{j}"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.narrative.as_deref(), Some("温度短暂波动后自行恢复，无需处理"));

        // narrative NULL（老数据）→ None 降级不炸。
        let j2 = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(&j2, JudgmentVerdict::Noise, "波动", "{}", None, None, None, None)
            .await
            .unwrap();
        let detail2 = db
            .brain_event_detail("ws1", &format!("alarm:{j2}"))
            .await
            .unwrap()
            .unwrap();
        assert!(detail2.narrative.is_none(), "narrative NULL → None 降级");

        // 非 judgment 行（patrol 提案）→ None。
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at)
             VALUES ('act-np','ws1','agent1','patrol','proposal',?,?)",
        )
        .bind("{\"proposalId\":\"np\",\"thingId\":\"t1\",\"summary\":\"s\",\"status\":\"pending\"}")
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("proposal insert");
        let detail3 = db.brain_event_detail("ws1", "patrol:np").await.unwrap().unwrap();
        assert!(detail3.narrative.is_none(), "非 judgment 行 narrative 为 None");
    }

    /// needs_you tab 无时间窗：72h 前 escalated + 工单 open 的事件仍可见（与 badge 计数口径一致）；
    /// all tab 保持 48h 窗：同一事件不出现。
    #[tokio::test]
    async fn needs_you_no_time_window_all_keeps_48h() {
        let db = test_db().await;
        let j = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let open_ticket = insert_ticket(&db, "open", "h-stale").await;
        db.transit_judgment(
            &j,
            JudgmentStatus::Investigating,
            JudgmentStatus::Escalated,
            Some(open_ticket),
        )
        .await
        .unwrap();
        let old = (Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
            .bind(&old)
            .bind(&j)
            .execute(db.pool())
            .await
            .unwrap();

        let feed = db
            .list_brain_events("ws1", BrainEventTab::NeedsYou, None, 10)
            .await
            .unwrap();
        assert_eq!(feed.len(), 1, "needs_you 无时间窗——陈旧未认领仍可见");
        assert_eq!(feed[0].id, format!("alarm:{j}"));
        let summary = db.brain_events_summary("ws1").await.unwrap();
        assert_eq!(summary.needs_you, 1, "badge 计数与 needs_you 列表口径一致");

        let all = db.list_brain_events("ws1", BrainEventTab::All, None, 10).await.unwrap();
        assert!(all.is_empty(), "all tab 保持 48h 窗——72h 前事件不出现");
    }

    /// Batch 4 review regression: a merged tab must fetch judgments in the same
    /// `(created_at, id)` order it merges with. A pinned (awaiting_approval)
    /// oldest row must not displace the 20th-newest row from page 1, or the
    /// cursor skips a reachable row forever.
    #[tokio::test]
    async fn merged_feed_preserves_every_row_when_oldest_is_pinned() {
        let db = test_db().await;
        let base = Utc::now() - chrono::Duration::minutes(60);

        // Oldest row is the only pinned one (awaiting_approval).
        let pinned = db
            .insert_judgment("ws1", None, None, Some("oldest"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &pinned,
            JudgmentVerdict::SelfHealable,
            "r",
            "{}",
            Some("a"),
            Some("other"),
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
            .bind(base.to_rfc3339())
            .bind(&pinned)
            .execute(db.pool())
            .await
            .unwrap();

        // 20 newer, resolved rows, each with its own thing so folding keeps them.
        let mut newer = Vec::new();
        for index in 0..20 {
            let id = db
                .insert_judgment("ws1", None, None, Some(&format!("t{index}")), "annotate")
                .await
                .unwrap();
            db.judge_judgment(&id, JudgmentVerdict::Noise, "r", "{}", None, None, None, None)
                .await
                .unwrap();
            let created_at = (base + chrono::Duration::minutes(index + 1)).to_rfc3339();
            sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
                .bind(&created_at)
                .bind(&id)
                .execute(db.pool())
                .await
                .unwrap();
            newer.push(format!("alarm:{id}"));
        }

        let page1 = db.list_brain_events("ws1", BrainEventTab::All, None, 20).await.unwrap();
        let expected: Vec<String> = newer.iter().rev().cloned().collect();
        assert_eq!(
            page1.iter().map(|event| event.id.clone()).collect::<Vec<_>>(),
            expected,
            "merged page 1 is strictly the 20 newest, regardless of pin"
        );

        let page2 = db
            .list_brain_events("ws1", BrainEventTab::All, Some(&page1[19].id), 20)
            .await
            .unwrap();
        assert_eq!(
            page2.iter().map(|event| event.id.clone()).collect::<Vec<_>>(),
            vec![format!("alarm:{pinned}")],
            "the pinned oldest row is reachable on page 2"
        );
        assert!(
            page1.iter().all(|left| page2.iter().all(|right| left.id != right.id)),
            "pages must not overlap"
        );

        // Non-merging tab keeps the pin-first experience.
        let alarm = db
            .list_brain_events("ws1", BrainEventTab::Alarm, None, 20)
            .await
            .unwrap();
        assert_eq!(
            alarm[0].id,
            format!("alarm:{pinned}"),
            "alarm tab pins awaiting_approval"
        );
    }

    /// The session source must use the merged id order (`'session:' || event_id`)
    /// as its tiebreak, not insertion `seq`. Inserting ids in reverse lexical
    /// order makes the difference observable.
    #[tokio::test]
    async fn session_same_timestamp_tiebreak_matches_merged_id_order() {
        let fixture = SessionFeedFixture::new().await;
        let ts = Utc::now();
        for event_id in ["event-e", "event-d", "event-c", "event-b", "event-a"] {
            fixture
                .db
                .append_session_event(&session_input(event_id, ts))
                .await
                .unwrap();
        }

        let page1 = fixture
            .db
            .list_brain_events("ws-session", BrainEventTab::Session, None, 2)
            .await
            .unwrap();
        assert_eq!(
            page1.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:event-e", "session:event-d"],
            "same timestamp sorts by merged id, not seq"
        );
        let page2 = fixture
            .db
            .list_brain_events("ws-session", BrainEventTab::Session, Some(&page1[1].id), 2)
            .await
            .unwrap();
        assert_eq!(
            page2.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:event-c", "session:event-b"]
        );
        let page3 = fixture
            .db
            .list_brain_events("ws-session", BrainEventTab::Session, Some(&page2[1].id), 2)
            .await
            .unwrap();
        assert_eq!(
            page3.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:event-a"]
        );

        fixture.close().await;
    }

    /// Batch 4 merge contract: judgments (alarm) + recent session events are
    /// merged, sorted deterministically by `(created_at, id)` descending and
    /// sliced after the merge. Legacy `patrol`/`directive` tabs alias session.
    #[tokio::test]
    async fn session_events_merge_slice_and_alias_tabs() {
        let fixture = SessionFeedFixture::new().await;
        let db = &fixture.db;
        let ws = "ws-session";
        let now = Utc::now();

        // Judgment is the oldest row.
        let j = db
            .insert_judgment(ws, None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        sqlx::query("UPDATE judgments SET created_at = ? WHERE id = ?")
            .bind((now - chrono::Duration::minutes(10)).to_rfc3339())
            .bind(&j)
            .execute(db.pool())
            .await
            .unwrap();

        // Session events at -3m, -2m, -1m.
        for (event_id, minutes) in [("session-e1", 3), ("session-e2", 2), ("session-e3", 1)] {
            db.append_session_event(&session_input(event_id, now - chrono::Duration::minutes(minutes)))
                .await
                .unwrap();
        }

        // All: merge + newest-first, then slice.
        let all = db.list_brain_events(ws, BrainEventTab::All, None, 10).await.unwrap();
        let ids: Vec<String> = all.iter().map(|event| event.id.clone()).collect();
        assert_eq!(
            ids,
            vec![
                "session:session-e3".to_string(),
                "session:session-e2".to_string(),
                "session:session-e1".to_string(),
                format!("alarm:{j}"),
            ],
            "merged deterministic ordering"
        );
        let limited = db.list_brain_events(ws, BrainEventTab::All, None, 2).await.unwrap();
        assert_eq!(
            limited.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:session-e3", "session:session-e2"],
            "limit applies after merge"
        );

        // Session tab and legacy patrol/directive aliases return the same rows.
        for tab in [BrainEventTab::Session, BrainEventTab::Patrol, BrainEventTab::Directive] {
            let feed = db.list_brain_events(ws, tab, None, 2).await.unwrap();
            assert_eq!(
                feed.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
                vec!["session:session-e3", "session:session-e2"],
                "{tab:?} aliases session"
            );
            assert!(feed.iter().all(|event| event.source == "session"));
        }

        // Alarm tab never merges session rows.
        let alarm = db.list_brain_events(ws, BrainEventTab::Alarm, None, 10).await.unwrap();
        assert_eq!(alarm.len(), 1);
        assert_eq!(alarm[0].id, format!("alarm:{j}"));

        // Tuple cursor for a session row: only strictly older merged rows.
        let page = db
            .list_brain_events(ws, BrainEventTab::All, Some("session:session-e2"), 10)
            .await
            .unwrap();
        assert_eq!(
            page.iter().map(|event| event.id.clone()).collect::<Vec<_>>(),
            vec!["session:session-e1".to_string(), format!("alarm:{j}")],
            "session cursor slices the merged stream"
        );

        // Backward pagination fills a limit-sized page even though the newest
        // session rows (e3/e2) are all newer than the cursor.
        let first_page = db.list_brain_events(ws, BrainEventTab::All, None, 2).await.unwrap();
        assert_eq!(
            first_page.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:session-e3", "session:session-e2"]
        );
        let second_page = db
            .list_brain_events(ws, BrainEventTab::All, Some(&first_page[1].id), 2)
            .await
            .unwrap();
        assert_eq!(
            second_page.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["session:session-e1", &format!("alarm:{j}")],
            "cursor-aware session page must not be emptied by newer rows"
        );

        // Session detail projects payload evidence.
        let detail = db.brain_event_detail(ws, "session:session-e1").await.unwrap().unwrap();
        assert_eq!(detail.event.source, "session");
        assert_eq!(detail.event.status, "info");
        assert_eq!(detail.event.run_id.as_deref(), Some("run-session"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&detail.evidence_json).unwrap(),
            serde_json::json!({"event": "session-e1"})
        );

        fixture.close().await;
    }
}
