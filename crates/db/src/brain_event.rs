//! brain_events 投影查询（AI 大脑 P0 Task 2）：feed/summary/detail 全部读
//! Task 1 的 brain_events 只读视图（judgments ∪ agent_actions ∪ agent_runs）。
//!
//! 语义移植自 judgment::list_judgments_feed（48h 窗 / 折叠 / 置顶 / 元组游标），
//! 差异点：
//! - 折叠仅 alarm 源生效（thing+rule join thing_alarms via alarm_id）；
//!   patrol/directive 按 id 各自成组不折叠
//! - 「需要你」口径含工单认领态：awaiting_approval，或 escalated 且
//!   （无工单 / 工单仍 open）；已被认领（claimed 及以后）不算
//! - list 不载 evidence_json（性能评审裁决）；证据由 detail 单独取

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::database::Db;
use crate::error::Result;

/// feed 页签：需要你 / 全部 / 巡检 / 报警 / 指令。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrainEventTab {
    NeedsYou,
    All,
    Patrol,
    Alarm,
    Directive,
}

/// 列表行（19 列；无 evidence_json/action_params——列表不载证据与动作参数）。
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
    pub ticket_id: Option<i64>,
    pub status: String,
    pub triage_mode: String,
    pub created_at: String,
    pub judged_at: Option<String>,
    pub state_entered_at: Option<String>,
    pub resolved_at: Option<String>,
}

/// 详情 = 列表行 + 完整证据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainEventDetail {
    #[serde(flatten)]
    pub event: BrainEvent,
    pub evidence_json: String,
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
    suggested_action, action_category, risk, run_id, ticket_id, status, triage_mode, \
    created_at, judged_at, state_entered_at, resolved_at";

/// 「需要你」口径（仅在 FROM brain_events 的作用域内使用）。
/// 口径决定：needs_you 无时间窗——陈旧未认领恰恰最该被看见；其余 tab 保持 48h 窗防膨胀
/// （见 list_brain_events 的 window 分支，与 summary.needs_you 计数对齐）。
const NEEDS_YOU_COND: &str = "(status = 'awaiting_approval' OR (status = 'escalated' AND \
    (ticket_id IS NULL OR (SELECT t.state FROM tickets t WHERE t.id = brain_events.ticket_id) = 'open')))";

fn tab_filter(tab: BrainEventTab) -> &'static str {
    match tab {
        BrainEventTab::NeedsYou => {
            " AND (status = 'awaiting_approval' OR (status = 'escalated' AND \
            (ticket_id IS NULL OR (SELECT t.state FROM tickets t WHERE t.id = brain_events.ticket_id) = 'open')))"
        }
        BrainEventTab::All => "",
        BrainEventTab::Patrol => " AND source IN ('patrol','patrol_tick')",
        BrainEventTab::Alarm => " AND source = 'alarm'",
        BrainEventTab::Directive => " AND source = 'directive'",
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
/// - 首页（before=None）：alarm 源同 thing+rule 折叠最新（window function）+ 需要你置顶
/// - 后续页（before=Some）：纯时间流（折叠/置顶不参与，同 judgment feed 语义）
/// - 游标：(created_at, id) 元组比较，同秒多行不丢
pub(crate) async fn list_brain_events(
    pool: &SqlitePool,
    workspace_id: &str,
    tab: BrainEventTab,
    before: Option<&str>, // 游标：上一页最后一条 brain event id
    limit: i64,
) -> Result<Vec<BrainEvent>> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    let filter = tab_filter(tab);
    // needs_you 无时间窗——陈旧未认领恰恰最该被看见；其余 tab 保持 48h 窗防膨胀。
    let window = if tab == BrainEventTab::NeedsYou {
        ""
    } else {
        " AND created_at >= ?"
    };

    let sql = if before.is_none() {
        // 折叠：仅 alarm 源按 thing+rule 分组取最新；非 alarm 行按 id 各自成组（rn 恒 1）。
        // 置顶：需要你（pin=0）在前，其余按 created_at DESC, id DESC。
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
             ORDER BY pin, created_at DESC, id DESC
             LIMIT ?"
        )
    } else {
        // 后续页：纯时间流（不折叠不置顶）。元组游标：同秒行按 id 续翻。
        format!(
            "SELECT {LIST_COLS} FROM brain_events
             WHERE workspace_id = ?{window}{filter}
               AND (created_at < (SELECT created_at FROM brain_events WHERE id = ?)
                    OR (created_at = (SELECT created_at FROM brain_events WHERE id = ?) AND id < ?))
             ORDER BY created_at DESC, id DESC
             LIMIT ?"
        )
    };

    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(workspace_id);
    if tab != BrainEventTab::NeedsYou {
        q = q.bind(since);
    }
    if let Some(b) = before {
        q = q.bind(b).bind(b).bind(b);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    rows.into_iter().map(row_to_brain_event).collect()
}

/// 详情：列表行 + evidence_json（证据只在这里载）。
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
    Ok(Some(BrainEventDetail {
        event: row_to_brain_event(row)?,
        evidence_json,
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
         FROM brain_events WHERE workspace_id = ?"
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
    pub async fn list_brain_events(
        &self,
        workspace_id: &str,
        tab: BrainEventTab,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<BrainEvent>> {
        list_brain_events(self.pool(), workspace_id, tab, before, limit).await
    }

    pub async fn brain_event_detail(&self, workspace_id: &str, id: &str) -> Result<Option<BrainEventDetail>> {
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
    use chrono::Utc;

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

        // 两条 patrol 提案（不同 proposalId，同 thing）——不应折叠
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
        let patrol_rows: Vec<_> = feed.iter().filter(|e| e.source == "patrol").collect();
        assert_eq!(patrol_rows.len(), 2, "patrol 不折叠");
        assert_eq!(feed.len(), 3);
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

    /// list 行无证据字段（结构层面无 evidence_json）；detail 返回完整证据。
    #[tokio::test]
    async fn list_does_not_carry_evidence_detail_does() {
        let db = test_db().await;
        let j = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        let evidence = "{\"run\":\"r1\",\"steps\":[]}";
        db.judge_judgment(&j, JudgmentVerdict::Noise, "波动", evidence, None, None, None)
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

    /// directive 分类规则：problem_key IS NULL 且 trigger_type='user' 的 run 才出现。
    #[tokio::test]
    async fn directive_classification_rule() {
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
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, problem_key, created_at)
             VALUES ('r-alarm','ws1','user','success','报警调查','{}','alarm:x',?)",
        )
        .bind(&now)
        .execute(db.pool())
        .await
        .unwrap();
        let feed = db
            .list_brain_events("ws1", BrainEventTab::Directive, None, 10)
            .await
            .unwrap();
        let ids: Vec<&str> = feed.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"directive:r-user"), "用户指令 run 出现在 directive tab");
        assert!(!ids.contains(&"directive:r-alarm"), "problem_key 非空的不出现");
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
}
