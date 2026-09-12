//! Judgment 持久化：AI 处置判断（大脑主干化 P0，迁移 20260912000002）。
//!
//! 状态机（所有翻转走条件更新防并发互撞）：
//!
//! ```text
//! investigating ──verdict=noise─────────→ noise_archived（报警 suppress）
//!      │───────verdict=self_healable──→ awaiting_approval ─批准→ executing → resolved
//!      │                                   │ rejected / 24h 超时 → escalated
//!      │───────verdict=needs_human────→ escalated（转工单，ticket_id 回填）
//!      ├──调查失败/解析失败────────────→ investigation_failed（同样转工单）
//!      └──超日预算───────────────────→ budget_skipped（报警保持 Active）
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::database::Db;
use crate::error::{DbError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgmentVerdict {
    Noise,
    SelfHealable,
    NeedsHuman,
}

impl JudgmentVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            JudgmentVerdict::Noise => "noise",
            JudgmentVerdict::SelfHealable => "self_healable",
            JudgmentVerdict::NeedsHuman => "needs_human",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "noise" => Some(JudgmentVerdict::Noise),
            "self_healable" => Some(JudgmentVerdict::SelfHealable),
            "needs_human" => Some(JudgmentVerdict::NeedsHuman),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgmentStatus {
    Investigating,
    NoiseArchived,
    AwaitingApproval,
    Executing,
    Resolved,
    Escalated,
    InvestigationFailed,
    BudgetSkipped,
}

impl JudgmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JudgmentStatus::Investigating => "investigating",
            JudgmentStatus::NoiseArchived => "noise_archived",
            JudgmentStatus::AwaitingApproval => "awaiting_approval",
            JudgmentStatus::Executing => "executing",
            JudgmentStatus::Resolved => "resolved",
            JudgmentStatus::Escalated => "escalated",
            JudgmentStatus::InvestigationFailed => "investigation_failed",
            JudgmentStatus::BudgetSkipped => "budget_skipped",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "investigating" => Some(JudgmentStatus::Investigating),
            "noise_archived" => Some(JudgmentStatus::NoiseArchived),
            "awaiting_approval" => Some(JudgmentStatus::AwaitingApproval),
            "executing" => Some(JudgmentStatus::Executing),
            "resolved" => Some(JudgmentStatus::Resolved),
            "escalated" => Some(JudgmentStatus::Escalated),
            "investigation_failed" => Some(JudgmentStatus::InvestigationFailed),
            "budget_skipped" => Some(JudgmentStatus::BudgetSkipped),
            _ => None,
        }
    }

    /// 未终态（ judgments_active_alarm 部分唯一索引覆盖的三态）
    pub fn is_open(&self) -> bool {
        matches!(
            self,
            JudgmentStatus::Investigating | JudgmentStatus::AwaitingApproval | JudgmentStatus::Executing
        )
    }

    /// feed 页「需要你」tab = 待审批 + 需人工已升级
    pub fn needs_you(&self) -> bool {
        matches!(self, JudgmentStatus::AwaitingApproval | JudgmentStatus::Escalated)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgment {
    pub id: String,
    pub workspace_id: String,
    pub alarm_id: Option<String>,
    pub run_id: Option<String>,
    pub ticket_id: Option<i64>,
    pub proposal_id: Option<String>,
    pub thing_id: Option<String>,
    pub verdict: Option<JudgmentVerdict>,
    pub reason: String,
    pub evidence_json: String,
    pub suggested_action: Option<String>,
    pub action_category: Option<String>,
    pub status: JudgmentStatus,
    pub created_at: DateTime<Utc>,
    pub judged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentFeedback {
    pub id: i64,
    pub judgment_id: String,
    pub workspace_id: String,
    pub user_id: String,
    pub verdict: String, // 'right' | 'wrong'
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 新建调查判断（报警派发调查 run 时落库，verdict 待定）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_judgment(
    pool: &SqlitePool,
    workspace_id: &str,
    alarm_id: Option<&str>,
    run_id: Option<&str>,
    thing_id: Option<&str>,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO judgments (id, workspace_id, alarm_id, run_id, thing_id, status, created_at)
         VALUES (?, ?, ?, ?, ?, 'investigating', ?)",
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(alarm_id)
    .bind(run_id)
    .bind(thing_id)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(id)
}

/// DB 时间戳解析（复用 alarm 模块的容错解析器）。
fn parse_ts_opt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().and_then(|v| crate::alarm::parse_db_datetime(v).ok())
}

fn row_to_judgment(row: sqlx::sqlite::SqliteRow) -> Result<Judgment> {
    let status_str: String = row.get("status");
    let status = JudgmentStatus::parse_str(&status_str).ok_or_else(|| DbError::Validation {
        message: format!("Unknown judgment status: {}", status_str),
    })?;
    let verdict_str: Option<String> = row.get("verdict");
    let verdict = verdict_str.and_then(|v| JudgmentVerdict::parse_str(&v));

    Ok(Judgment {
        id: row.get("id"),
        workspace_id: row.get("workspace_id"),
        alarm_id: row.get("alarm_id"),
        run_id: row.get("run_id"),
        ticket_id: row.get("ticket_id"),
        proposal_id: row.get("proposal_id"),
        thing_id: row.get("thing_id"),
        verdict,
        reason: row.get("reason"),
        evidence_json: row.get("evidence_json"),
        suggested_action: row.get("suggested_action"),
        action_category: row.get("action_category"),
        status,
        created_at: parse_ts_opt(row.get::<Option<String>, _>("created_at")).unwrap_or_else(Utc::now),
        judged_at: parse_ts_opt(row.get("judged_at")),
        resolved_at: parse_ts_opt(row.get("resolved_at")),
    })
}

pub(crate) async fn find_judgment_by_id(pool: &SqlitePool, id: &str, workspace_id: &str) -> Result<Option<Judgment>> {
    let row = sqlx::query("SELECT * FROM judgments WHERE id = ? AND workspace_id = ?")
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?;
    row.map(row_to_judgment).transpose()
}

/// 调查完成：写 verdict + 理由 + 证据，并迁移到 verdict 对应的状态。
/// 条件更新：仅 investigating → 目标态；0 行 = 并发/重复 RunRecorded，调用方按幂等处理。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn judge_judgment(
    pool: &SqlitePool,
    id: &str,
    verdict: JudgmentVerdict,
    reason: &str,
    evidence_json: &str,
    suggested_action: Option<&str>,
    action_category: Option<&str>,
    proposal_id: Option<&str>,
) -> Result<bool> {
    let target = match verdict {
        JudgmentVerdict::Noise => JudgmentStatus::NoiseArchived,
        // 无可执行动作建议的 self_healable 直接升级（无审批对象）
        JudgmentVerdict::SelfHealable => JudgmentStatus::AwaitingApproval,
        JudgmentVerdict::NeedsHuman => JudgmentStatus::Escalated,
    };
    let result = sqlx::query(
        "UPDATE judgments SET verdict = ?, reason = ?, evidence_json = ?, suggested_action = ?,
            action_category = ?, proposal_id = ?, status = ?, judged_at = ?
         WHERE id = ? AND status = 'investigating'",
    )
    .bind(verdict.as_str())
    .bind(reason)
    .bind(evidence_json)
    .bind(suggested_action)
    .bind(action_category)
    .bind(proposal_id)
    .bind(target.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 通用条件状态迁移：仅当前态 = expected 时翻转。0 行 = 并发互撞/重复触发。
pub(crate) async fn transit_judgment(
    pool: &SqlitePool,
    id: &str,
    expected: JudgmentStatus,
    next: JudgmentStatus,
    ticket_id: Option<i64>,
) -> Result<bool> {
    let resolved_at = if matches!(next, JudgmentStatus::Resolved) {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };
    let result = sqlx::query(
        "UPDATE judgments SET status = ?, ticket_id = COALESCE(?, ticket_id),
            resolved_at = COALESCE(?, resolved_at)
         WHERE id = ? AND status = ?",
    )
    .bind(next.as_str())
    .bind(ticket_id)
    .bind(resolved_at)
    .bind(id)
    .bind(expected.as_str())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 调查失败/超预算的终态标记（investigating → investigation_failed|budget_skipped）。
pub(crate) async fn fail_judgment(pool: &SqlitePool, id: &str, next: JudgmentStatus, reason: &str) -> Result<bool> {
    debug_assert!(matches!(
        next,
        JudgmentStatus::InvestigationFailed | JudgmentStatus::BudgetSkipped
    ));
    let result = sqlx::query(
        "UPDATE judgments SET status = ?, reason = ? WHERE id = ? AND status = 'investigating'",
    )
    .bind(next.as_str())
    .bind(reason)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn set_judgment_run_id(pool: &SqlitePool, id: &str, run_id: &str) -> Result<()> {
    sqlx::query("UPDATE judgments SET run_id = ? WHERE id = ? AND run_id IS NULL")
        .bind(run_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Critical/Error 直达工单后把 judgment 关联上（上下文补充在调查完成后进行）。
pub(crate) async fn link_judgment_ticket(pool: &SqlitePool, id: &str, ticket_id: i64) -> Result<()> {
    sqlx::query("UPDATE judgments SET ticket_id = ? WHERE id = ? AND ticket_id IS NULL")
        .bind(ticket_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// feed 页查询：workspace 隔离 + 可选 status/verdict 筛选 + 游标分页（created_at DESC + id 稳定序）。
pub(crate) async fn list_judgments(
    pool: &SqlitePool,
    workspace_id: &str,
    statuses: Option<&[JudgmentStatus]>,
    before: Option<&str>, // 游标：judgment id，取比它更早的
    limit: i64,
) -> Result<Vec<Judgment>> {
    let mut sql = String::from("SELECT * FROM judgments WHERE workspace_id = ?");
    if let Some(ss) = statuses
        && !ss.is_empty()
    {
        let placeholders = vec!["?"; ss.len()].join(",");
        sql.push_str(&format!(" AND status IN ({})", placeholders));
    }
    if before.is_some() {
        sql.push_str(" AND created_at < (SELECT created_at FROM judgments WHERE id = ?)");
    }
    sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ?");

    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(workspace_id);
    if let Some(ss) = statuses {
        for s in ss {
            q = q.bind(s.as_str());
        }
    }
    if let Some(b) = before {
        q = q.bind(b);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    rows.into_iter().map(row_to_judgment).collect()
}

/// 审批超时扫描：awaiting_approval 且 judged_at 早于 cutoff。
pub(crate) async fn stale_awaiting_approvals(pool: &SqlitePool, cutoff: &str) -> Result<Vec<Judgment>> {
    let rows = sqlx::query(
        "SELECT * FROM judgments WHERE status = 'awaiting_approval' AND judged_at < ?",
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(row_to_judgment).collect()
}

/// flapping 源头防抖 + subscriber 找回判断：同 thing+rule 的未终态判断。
///（problem_key=alarm:{thing_id}:{rule_id} 的数据层对应物；按 alarm_id 键控
/// 对抖动永不命中——eng-review 外部视角修正。）
pub(crate) async fn find_open_judgment_by_thing_rule(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    rule_id: Option<&str>,
) -> Result<Option<Judgment>> {
    let row = sqlx::query(
        "SELECT j.* FROM judgments j JOIN thing_alarms a ON j.alarm_id = a.id
         WHERE j.workspace_id = ? AND a.thing_id = ? AND COALESCE(a.rule_id, '') = COALESCE(?, '')
           AND j.status IN ('investigating','awaiting_approval','executing')
         ORDER BY j.created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(thing_id)
    .bind(rule_id)
    .fetch_optional(pool)
    .await?;
    row.map(row_to_judgment).transpose()
}

/// T8 预算：今日已发起判断数（不含 budget_skipped——超预算标记本身不是
/// LLM 调用，不计入额度）。
pub(crate) async fn count_judgments_today(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM judgments WHERE workspace_id = ? AND created_at >= date('now') AND status != 'budget_skipped'",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// T13 管道指标：verdict 分布 / 反馈对错数（按 workspace）。
pub(crate) async fn judgment_stats(pool: &SqlitePool, workspace_id: &str) -> Result<JudgmentStats> {
    let row = sqlx::query(
        "SELECT
            COUNT(*) AS total,
            SUM(CASE WHEN verdict = 'noise' THEN 1 ELSE 0 END) AS noise,
            SUM(CASE WHEN verdict = 'self_healable' THEN 1 ELSE 0 END) AS self_healable,
            SUM(CASE WHEN verdict = 'needs_human' THEN 1 ELSE 0 END) AS needs_human,
            SUM(CASE WHEN status = 'resolved' THEN 1 ELSE 0 END) AS resolved,
            SUM(CASE WHEN status = 'escalated' THEN 1 ELSE 0 END) AS escalated
         FROM judgments WHERE workspace_id = ? AND verdict IS NOT NULL",
    )
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

    Ok(JudgmentStats {
        total: row.get::<i64, _>("total") as u64,
        noise: row.get::<Option<i64>, _>("noise").unwrap_or(0) as u64,
        self_healable: row.get::<Option<i64>, _>("self_healable").unwrap_or(0) as u64,
        needs_human: row.get::<Option<i64>, _>("needs_human").unwrap_or(0) as u64,
        resolved: row.get::<Option<i64>, _>("resolved").unwrap_or(0) as u64,
        escalated: row.get::<Option<i64>, _>("escalated").unwrap_or(0) as u64,
        feedback_right: fb.get::<Option<i64>, _>("right_count").unwrap_or(0) as u64,
        feedback_wrong: fb.get::<Option<i64>, _>("wrong_count").unwrap_or(0) as u64,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentStats {
    pub total: u64,
    pub noise: u64,
    pub self_healable: u64,
    pub needs_human: u64,
    pub resolved: u64,
    pub escalated: u64,
    pub feedback_right: u64,
    pub feedback_wrong: u64,
}

/// 写反馈（全历史保留；「改判取最新」由 latest_feedback 查询实现）。
pub(crate) async fn add_feedback(
    pool: &SqlitePool,
    judgment_id: &str,
    workspace_id: &str,
    user_id: &str,
    verdict: &str,
    reason: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO judgment_feedback (judgment_id, workspace_id, user_id, verdict, reason, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(judgment_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(verdict)
    .bind(reason)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// 每判断最新一条反馈（改判取最新）。
pub(crate) async fn latest_feedback(pool: &SqlitePool, judgment_id: &str) -> Result<Option<JudgmentFeedback>> {
    let row = sqlx::query(
        "SELECT * FROM judgment_feedback WHERE judgment_id = ? ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(judgment_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(JudgmentFeedback {
        id: row.get("id"),
        judgment_id: row.get("judgment_id"),
        workspace_id: row.get("workspace_id"),
        user_id: row.get("user_id"),
        verdict: row.get("verdict"),
        reason: row.get("reason"),
        created_at: parse_ts_opt(row.get::<Option<String>, _>("created_at")).unwrap_or_else(Utc::now),
    }))
}

// ── Db 门面 ──

impl Db {
    pub async fn insert_judgment(
        &self,
        workspace_id: &str,
        alarm_id: Option<&str>,
        run_id: Option<&str>,
        thing_id: Option<&str>,
    ) -> Result<String> {
        insert_judgment(self.pool(), workspace_id, alarm_id, run_id, thing_id).await
    }

    pub async fn find_judgment_by_id(&self, id: &str, workspace_id: &str) -> Result<Option<Judgment>> {
        find_judgment_by_id(self.pool(), id, workspace_id).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn judge_judgment(
        &self,
        id: &str,
        verdict: JudgmentVerdict,
        reason: &str,
        evidence_json: &str,
        suggested_action: Option<&str>,
        action_category: Option<&str>,
        proposal_id: Option<&str>,
    ) -> Result<bool> {
        judge_judgment(
            self.pool(),
            id,
            verdict,
            reason,
            evidence_json,
            suggested_action,
            action_category,
            proposal_id,
        )
        .await
    }

    pub async fn transit_judgment(
        &self,
        id: &str,
        expected: JudgmentStatus,
        next: JudgmentStatus,
        ticket_id: Option<i64>,
    ) -> Result<bool> {
        transit_judgment(self.pool(), id, expected, next, ticket_id).await
    }

    pub async fn fail_judgment(&self, id: &str, next: JudgmentStatus, reason: &str) -> Result<bool> {
        fail_judgment(self.pool(), id, next, reason).await
    }

    pub async fn set_judgment_run_id(&self, id: &str, run_id: &str) -> Result<()> {
        set_judgment_run_id(self.pool(), id, run_id).await
    }

    pub async fn link_judgment_ticket(&self, id: &str, ticket_id: i64) -> Result<()> {
        link_judgment_ticket(self.pool(), id, ticket_id).await
    }

    pub async fn list_judgments(
        &self,
        workspace_id: &str,
        statuses: Option<&[JudgmentStatus]>,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Judgment>> {
        list_judgments(self.pool(), workspace_id, statuses, before, limit).await
    }

    pub async fn stale_awaiting_approvals(&self, cutoff: &str) -> Result<Vec<Judgment>> {
        stale_awaiting_approvals(self.pool(), cutoff).await
    }

    pub async fn find_open_judgment_by_thing_rule(
        &self,
        workspace_id: &str,
        thing_id: &str,
        rule_id: Option<&str>,
    ) -> Result<Option<Judgment>> {
        find_open_judgment_by_thing_rule(self.pool(), workspace_id, thing_id, rule_id).await
    }

    pub async fn count_judgments_today(&self, workspace_id: &str) -> Result<i64> {
        count_judgments_today(self.pool(), workspace_id).await
    }

    pub async fn judgment_stats(&self, workspace_id: &str) -> Result<JudgmentStats> {
        judgment_stats(self.pool(), workspace_id).await
    }

    pub async fn add_judgment_feedback(
        &self,
        judgment_id: &str,
        workspace_id: &str,
        user_id: &str,
        verdict: &str,
        reason: Option<&str>,
    ) -> Result<i64> {
        add_feedback(self.pool(), judgment_id, workspace_id, user_id, verdict, reason).await
    }

    pub async fn latest_judgment_feedback(&self, judgment_id: &str) -> Result<Option<JudgmentFeedback>> {
        latest_feedback(self.pool(), judgment_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    #[tokio::test]
    async fn judgment_lifecycle_happy_path() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, Some("t1")).await.unwrap();

        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Investigating);
        assert!(j.verdict.is_none());

        let ok = db
            .judge_judgment(&id, JudgmentVerdict::SelfHealable, "历史同类事件 87% 由重连恢复", "{}", Some("重连网关"), Some("connection_recovery"), Some("prop-1"))
            .await
            .unwrap();
        assert!(ok);
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::AwaitingApproval);
        assert_eq!(j.verdict, Some(JudgmentVerdict::SelfHealable));
        assert!(j.judged_at.is_some());

        // 批准 → executing → resolved
        assert!(db.transit_judgment(&id, JudgmentStatus::AwaitingApproval, JudgmentStatus::Executing, None).await.unwrap());
        assert!(db.transit_judgment(&id, JudgmentStatus::Executing, JudgmentStatus::Resolved, None).await.unwrap());
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Resolved);
        assert!(j.resolved_at.is_some());
    }

    #[tokio::test]
    async fn conditional_transit_rejects_wrong_state() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None).await.unwrap();
        // investigating 直接跳 resolved 被拒
        assert!(!db.transit_judgment(&id, JudgmentStatus::AwaitingApproval, JudgmentStatus::Executing, None).await.unwrap());
        // 重复 judge（并发 RunRecorded）幂等
        assert!(db.judge_judgment(&id, JudgmentVerdict::Noise, "波动", "{}", None, None, None).await.unwrap());
        assert!(!db.judge_judgment(&id, JudgmentVerdict::Noise, "波动", "{}", None, None, None).await.unwrap());
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::NoiseArchived);
    }

    #[tokio::test]
    async fn fail_and_budget_paths() {
        let db = test_db().await;
        let id1 = db.insert_judgment("ws1", None, None, None).await.unwrap();
        assert!(db.fail_judgment(&id1, JudgmentStatus::InvestigationFailed, "LLM 超时").await.unwrap());
        let id2 = db.insert_judgment("ws1", None, None, None).await.unwrap();
        assert!(db.fail_judgment(&id2, JudgmentStatus::BudgetSkipped, "超日预算").await.unwrap());
    }

    #[tokio::test]
    async fn feedback_history_and_latest_wins() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None).await.unwrap();
        db.add_judgment_feedback(&id, "ws1", "u1", "wrong", Some("其实该报")).await.unwrap();
        // 改判
        db.add_judgment_feedback(&id, "ws1", "u1", "right", None).await.unwrap();
        let latest = db.latest_judgment_feedback(&id).await.unwrap().unwrap();
        assert_eq!(latest.verdict, "right");
    }

    #[tokio::test]
    async fn stale_approvals_scan_and_daily_count() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None).await.unwrap();
        db.judge_judgment(&id, JudgmentVerdict::SelfHealable, "r", "{}", Some("a"), Some("other"), None).await.unwrap();

        // judged_at 是当下：24h 前的 cutoff 不应命中，未来的 cutoff 应命中
        assert!(db.stale_awaiting_approvals("2000-01-01").await.unwrap().is_empty());
        assert_eq!(db.stale_awaiting_approvals("2999-01-01").await.unwrap().len(), 1);
        assert_eq!(db.count_judgments_today("ws1").await.unwrap(), 1);
        assert_eq!(db.count_judgments_today("ws-other").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_judgments_workspace_isolation_and_cursor() {
        let db = test_db().await;
        let a = db.insert_judgment("ws1", None, None, None).await.unwrap();
        let b = db.insert_judgment("ws1", None, None, None).await.unwrap();
        db.insert_judgment("ws2", None, None, None).await.unwrap();

        let page1 = db.list_judgments("ws1", None, None, 1).await.unwrap();
        assert_eq!(page1.len(), 1);
        let page2 = db.list_judgments("ws1", None, Some(&page1[0].id), 10).await.unwrap();
        assert_eq!(page2.len(), 1);
        let ids: Vec<&str> = [page1[0].id.as_str(), page2[0].id.as_str()].into();
        assert!(ids.contains(&a.as_str()) && ids.contains(&b.as_str()));

        // workspace 隔离
        assert!(db.list_judgments("ws2", None, None, 10).await.unwrap().len() == 1);
    }
}
