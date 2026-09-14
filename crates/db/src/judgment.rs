//! Judgment 持久化：AI 处置判断（大脑主干化 P0，迁移 20260912000002 +
//! 20260914000001 硬化：dispatch_suppressed 第 9 态 + triage_mode 快照列）。
//!
//! 状态机（所有翻转走条件更新防并发互撞；合法迁移见 [`allowed_transition`]，
//! 加新状态必须更新该矩阵——T-19/S2）：
//!
//! ```text
//! investigating ──verdict=noise─────────→ noise_archived（按 triage_mode 决定是否 suppress 报警）
//!      │───────verdict=self_healable──→ awaiting_approval ─批准→ executing ─verified→ resolved
//!      │                                   │ rejected / 24h 超时 → escalated   │失败/NoActionNeeded/1h SLA→ escalated
//!      │───────verdict=needs_human────→ escalated（转工单，ticket_id 回填）
//!      ├──调查失败/解析失败────────────→ investigation_failed（同样转工单）
//!      ├──超日预算────────────────────→ budget_skipped（报警保持 Active，不计预算）
//!      └──dispatch 被拦（O11/队列满）─→ dispatch_suppressed（不开票，不计预算；
//!                                       迟到 RunRecorded 可按 verdict 恢复路由→三出口）
//! noise_archived ──✕ 反馈「判错了」──→ investigating（重开重调查，T-17/C6）
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
    /// dispatch 被 O11 dedup/队列拦下（调查从未发起）。不开票、不计日预算；
    /// 迟到的 RunRecorded 可按 verdict 恢复路由到三出口（T-7/L1）。
    DispatchSuppressed,
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
            JudgmentStatus::DispatchSuppressed => "dispatch_suppressed",
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
            "dispatch_suppressed" => Some(JudgmentStatus::DispatchSuppressed),
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
}

/// 显式迁移矩阵（T-19/S2）：所有状态翻转的唯一口径。transit_judgment /
/// fail_judgment / judge_judgment 都必须过这张表；加新状态时在此登记。
pub(crate) fn allowed_transition(from: JudgmentStatus, to: JudgmentStatus) -> bool {
    use JudgmentStatus::*;
    matches!(
        (from, to),
        // 调查出口的判定迁移（judge_judgment）
        (Investigating, NoiseArchived)
            | (Investigating, AwaitingApproval)
            | (Investigating, Escalated)
            // 失败/降级终态
            | (Investigating, InvestigationFailed)
            | (Investigating, BudgetSkipped)
            | (Investigating, DispatchSuppressed)
            // 迟到 RunRecorded 恢复路由（T-7/L1）：dispatch 被拦的判定可被判出
            | (DispatchSuppressed, NoiseArchived)
            | (DispatchSuppressed, AwaitingApproval)
            | (DispatchSuppressed, Escalated)
            | (DispatchSuppressed, InvestigationFailed)
            // 审批出口
            | (AwaitingApproval, Executing)
            | (AwaitingApproval, Escalated)
            // 执行出口
            | (Executing, Resolved)
            | (Executing, Escalated)
            // 误判恢复：✕ 反馈噪声判断 → 重开重调查（T-17/C6）
            | (NoiseArchived, Investigating)
            // approve 的补偿回滚（D3）：enqueue 失败时 executing → awaiting_approval
            | (Executing, AwaitingApproval)
    )
}

/// 动作白名单（4A/T-3）：exec prompt 的动作文本由服务端模板按类别生成，
/// LLM 的 suggested_action 只做展示、不进执行指令（注入面收敛到枚举本身）。
pub fn exec_action_template(category: Option<&str>, thing_id: Option<&str>) -> String {
    let thing = thing_id.unwrap_or("目标设备");
    match category {
        Some("device_reboot") => format!("重启设备 {thing}"),
        Some("connection_recovery") => format!("恢复设备 {thing} 的连接（重连/重订阅）"),
        Some("property_adjust") => format!("调整设备 {thing} 的属性设置"),
        Some("threshold_tuning") => "调整报警规则阈值".to_string(),
        _ => "按判断建议处置".to_string(),
    }
}

/// action_category 归一化（白名单外 → other，避免 DB CHECK 失败让判断
/// 卡 investigating，T-3）。
pub fn normalize_action_category(category: Option<&str>) -> Option<String> {
    match category {
        Some(c @ ("device_reboot" | "connection_recovery" | "property_adjust" | "threshold_tuning" | "other")) => {
            Some(c.to_string())
        }
        Some(_) => Some("other".to_string()),
        None => None,
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
    /// 创建时从 workspace 配置快照的 triage 模式（T-20/S4）：verdict 路由按
    /// 快照而非到达时配置，中途切模式不影响在途判断。
    pub triage_mode: String,
    /// 状态进入时刻（SLA 清扫起算点）：每次状态翻转更新（insert/judge/
    /// transit/fail/reopen）。与 judged_at 分开——延迟指标用 judged_at。
    pub state_entered_at: DateTime<Utc>,
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
/// triage_mode：创建时从 workspace 配置快照（'annotate'|'suppress'，T-20/S4）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_judgment(
    pool: &SqlitePool,
    workspace_id: &str,
    alarm_id: Option<&str>,
    run_id: Option<&str>,
    thing_id: Option<&str>,
    triage_mode: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO judgments (id, workspace_id, alarm_id, run_id, thing_id, status, triage_mode, state_entered_at, created_at)
         VALUES (?, ?, ?, ?, ?, 'investigating', ?, ?, ?)",
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(alarm_id)
    .bind(run_id)
    .bind(thing_id)
    .bind(triage_mode)
    .bind(Utc::now().to_rfc3339())
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
        triage_mode: row
            .get::<Option<String>, _>("triage_mode")
            .unwrap_or_else(|| "annotate".to_string()),
        state_entered_at: parse_ts_opt(row.get::<Option<String>, _>("state_entered_at")).unwrap_or_else(Utc::now),
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
/// 条件更新：仅 investigating → 目标态（另允许 dispatch_suppressed → 目标态：
/// 迟到 RunRecorded 的恢复路由，T-7/L1）；0 行 = 并发/重复 RunRecorded，幂等。
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
    debug_assert!(allowed_transition(JudgmentStatus::Investigating, target));
    debug_assert!(allowed_transition(JudgmentStatus::DispatchSuppressed, target));
    let result = sqlx::query(
        "UPDATE judgments SET verdict = ?, reason = ?, evidence_json = ?, suggested_action = ?,
            action_category = ?, proposal_id = ?, status = ?, judged_at = ?, state_entered_at = ?
         WHERE id = ? AND status IN ('investigating','dispatch_suppressed')",
    )
    .bind(verdict.as_str())
    .bind(reason)
    .bind(evidence_json)
    .bind(suggested_action)
    .bind(action_category)
    .bind(proposal_id)
    .bind(target.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 通用条件状态迁移：仅当前态 = expected 时翻转，且 (expected, next) 必须在
/// 迁移矩阵 [`allowed_transition`] 内（非法迁移 = Validation 错误，响亮失败）。
/// 0 行 = 并发互撞/重复触发。
pub(crate) async fn transit_judgment(
    pool: &SqlitePool,
    id: &str,
    expected: JudgmentStatus,
    next: JudgmentStatus,
    ticket_id: Option<i64>,
) -> Result<bool> {
    if !allowed_transition(expected, next) {
        return Err(DbError::Validation {
            message: format!(
                "illegal judgment transition: {} -> {}",
                expected.as_str(),
                next.as_str()
            ),
        });
    }
    let resolved_at = if matches!(next, JudgmentStatus::Resolved) {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };
    let result = sqlx::query(
        "UPDATE judgments SET status = ?, ticket_id = COALESCE(?, ticket_id),
            resolved_at = COALESCE(?, resolved_at), state_entered_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(next.as_str())
    .bind(ticket_id)
    .bind(resolved_at)
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .bind(expected.as_str())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 调查失败/超预算/dispatch 被拦的终态标记（investigating|dispatch_suppressed →
/// investigation_failed|budget_skipped|dispatch_suppressed）。迁移矩阵校验
/// （release 同样生效——T-19/S2，不再是 debug_assert）。
pub(crate) async fn fail_judgment(pool: &SqlitePool, id: &str, next: JudgmentStatus, reason: &str) -> Result<bool> {
    if !matches!(
        next,
        JudgmentStatus::InvestigationFailed | JudgmentStatus::BudgetSkipped | JudgmentStatus::DispatchSuppressed
    ) {
        return Err(DbError::Validation {
            message: format!("fail_judgment target must be a failure terminal, got {}", next.as_str()),
        });
    }
    // 迟到失败恢复（dispatch_suppressed → investigation_failed）合法；
    // budget/dispatch 标记只从 investigating 落。
    let from = if matches!(next, JudgmentStatus::InvestigationFailed) {
        "('investigating','dispatch_suppressed')"
    } else {
        "('investigating')"
    };
    let result = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE judgments SET status = ?, reason = ?, state_entered_at = ? WHERE id = ? AND status IN {from}"
    )))
    .bind(next.as_str())
    .bind(reason)
    .bind(Utc::now().to_rfc3339())
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

/// feed 页查询（F12 契约落地 + T-12/C1 元组游标 + T-13/C2 分页语义）：
/// - workspace 隔离 + 可选 status 筛选 + 48h 窗口（RFC3339 字符串比较）
/// - 首页（before=None）：同 thing+rule 折叠最新（window function）+ 需行动置顶
/// - 后续页（before=Some）：纯时间流（折叠/置顶不参与，组合语义见设计修正案 7）
/// - 游标：(created_at, id) 元组比较，同秒多行不丢（T-12/C1）
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_judgments_feed(
    pool: &SqlitePool,
    workspace_id: &str,
    statuses: Option<&[JudgmentStatus]>,
    before: Option<&str>, // 游标：上一页最后一条 judgment id
    limit: i64,
) -> Result<Vec<Judgment>> {
    let since = (Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    let first_page = before.is_none();

    let mut status_filter = String::new();
    if let Some(ss) = statuses
        && !ss.is_empty()
    {
        let placeholders = vec!["?"; ss.len()].join(",");
        status_filter = format!(" AND status IN ({})", placeholders);
    }

    let sql = if first_page {
        // 折叠：同 thing+rule 只取最新一条（rn=1）；无 thing 的判断各自成组不折叠。
        // 置顶：需行动（awaiting_approval/escalated）优先，其余时间倒序。
        format!(
            "SELECT * FROM (
               SELECT j.*, ROW_NUMBER() OVER (
                   PARTITION BY COALESCE(j.thing_id, j.id),
                                COALESCE((SELECT a.rule_id FROM thing_alarms a WHERE a.id = j.alarm_id), '')
                   ORDER BY j.created_at DESC, j.id DESC
               ) AS rn
               FROM judgments j
               WHERE j.workspace_id = ? AND j.created_at >= ?{status_filter}
             ) WHERE rn = 1
             ORDER BY CASE WHEN status IN ('awaiting_approval','escalated') THEN 0 ELSE 1 END,
                      created_at DESC, id DESC
             LIMIT ?"
        )
    } else {
        // 后续页：纯时间流（不折叠不置顶）。元组游标：同秒行按 id 续翻。
        format!(
            "SELECT * FROM judgments
             WHERE workspace_id = ? AND created_at >= ?{status_filter}
               AND (created_at < (SELECT created_at FROM judgments WHERE id = ?)
                    OR (created_at = (SELECT created_at FROM judgments WHERE id = ?) AND id < ?))
             ORDER BY created_at DESC, id DESC
             LIMIT ?"
        )
    };

    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(workspace_id).bind(since);
    if let Some(ss) = statuses {
        for s in ss {
            q = q.bind(s.as_str());
        }
    }
    if let Some(b) = before {
        q = q.bind(b).bind(b).bind(b);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    rows.into_iter().map(row_to_judgment).collect()
}

/// 审批超时扫描：awaiting_approval 且 judged_at 早于 cutoff。
/// 起算点是 judged_at（进入待审批时刻，与前端审批倒计时同一基准）。
pub(crate) async fn stale_awaiting_approvals(pool: &SqlitePool, cutoff: &str) -> Result<Vec<Judgment>> {
    let rows = sqlx::query("SELECT * FROM judgments WHERE status = 'awaiting_approval' AND judged_at < ?")
        .bind(cutoff)
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(row_to_judgment).collect()
}

/// 三态 SLA 清扫（E2）：investigating/executing 超 cutoff 的扫描。
/// 起算点 = state_entered_at（状态进入时刻；T-16/C4：executing 的 SLA 从
/// 进入执行态起算，不从 judged_at——审批等待时间不该计入执行超时）。
pub(crate) async fn stale_by_status(pool: &SqlitePool, status: JudgmentStatus, cutoff: &str) -> Result<Vec<Judgment>> {
    let rows = sqlx::query("SELECT * FROM judgments WHERE status = ? AND state_entered_at < ?")
        .bind(status.as_str())
        .bind(cutoff)
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(row_to_judgment).collect()
}

/// 摘要端点计数（F-H）：COUNT 替代拉行数长度。
pub(crate) async fn count_by_statuses(
    pool: &SqlitePool,
    workspace_id: &str,
    statuses: &[JudgmentStatus],
) -> Result<i64> {
    let placeholders = vec!["?"; statuses.len()].join(",");
    let sql = format!("SELECT COUNT(*) FROM judgments WHERE workspace_id = ? AND status IN ({placeholders})");
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(workspace_id);
    for s in statuses {
        q = q.bind(s.as_str());
    }
    Ok(q.fetch_one(pool).await?)
}

/// 批量取每判断最新反馈（F-H：替代逐行 latest_feedback 的 N+1）。
pub(crate) async fn latest_feedbacks(
    pool: &SqlitePool,
    judgment_ids: &[String],
) -> Result<std::collections::HashMap<String, JudgmentFeedback>> {
    let mut map = std::collections::HashMap::new();
    if judgment_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = vec!["?"; judgment_ids.len()].join(",");
    let sql = format!(
        "SELECT * FROM judgment_feedback WHERE judgment_id IN ({placeholders})
         ORDER BY created_at DESC, id DESC"
    );
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in judgment_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    for row in rows {
        let fb = JudgmentFeedback {
            id: row.get("id"),
            judgment_id: row.get("judgment_id"),
            workspace_id: row.get("workspace_id"),
            user_id: row.get("user_id"),
            verdict: row.get("verdict"),
            reason: row.get("reason"),
            created_at: parse_ts_opt(row.get::<Option<String>, _>("created_at")).unwrap_or_else(Utc::now),
        };
        // 先出现的是最新（ORDER BY created_at DESC, id DESC）
        map.entry(fb.judgment_id.clone()).or_insert(fb);
    }
    Ok(map)
}

/// flapping 源头防抖（T-9/L3：只对 investigating 生效——「正在查就别重查」；
/// 等审批/执行中不是「正在查」，其间新报警照落行照调查）。
pub(crate) async fn find_investigating_judgment_by_thing_rule(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    rule_id: Option<&str>,
) -> Result<Option<Judgment>> {
    let row = sqlx::query(
        "SELECT j.* FROM judgments j JOIN thing_alarms a ON j.alarm_id = a.id
         WHERE j.workspace_id = ? AND a.thing_id = ? AND COALESCE(a.rule_id, '') = COALESCE(?, '')
           AND j.status = 'investigating'
         ORDER BY j.created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(thing_id)
    .bind(rule_id)
    .fetch_optional(pool)
    .await?;
    row.map(row_to_judgment).transpose()
}

/// subscriber 找回判断 + 迟到恢复（T-7/L1）：同 thing+rule 的最新判断，
/// 不限状态（dispatch_suppressed 的迟到 RunRecorded 需要找回它恢复路由）。
pub(crate) async fn find_latest_judgment_by_thing_rule(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: &str,
    rule_id: Option<&str>,
) -> Result<Option<Judgment>> {
    let row = sqlx::query(
        "SELECT j.* FROM judgments j JOIN thing_alarms a ON j.alarm_id = a.id
         WHERE j.workspace_id = ? AND a.thing_id = ? AND COALESCE(a.rule_id, '') = COALESCE(?, '')
         ORDER BY j.created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(thing_id)
    .bind(rule_id)
    .fetch_optional(pool)
    .await?;
    row.map(row_to_judgment).transpose()
}

/// 误判恢复（T-17/C6）：✕ 反馈噪声判断 → 重开为 investigating 重派调查。
/// 走迁移矩阵（noise_archived → investigating）。
pub(crate) async fn reopen_judgment(pool: &SqlitePool, id: &str) -> Result<bool> {
    if !allowed_transition(JudgmentStatus::NoiseArchived, JudgmentStatus::Investigating) {
        return Err(DbError::Validation {
            message: "reopen not allowed by transition matrix".to_string(),
        });
    }
    let result = sqlx::query(
        "UPDATE judgments SET status = 'investigating', verdict = NULL, judged_at = NULL, state_entered_at = ?
         WHERE id = ? AND status = 'noise_archived'",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// T8 预算：今日已发起判断数。不计入：budget_skipped（超预算标记本身不是
/// LLM 调用）与 dispatch_suppressed（dispatch 被拦，零 LLM 调用——否则防抖
/// 压制循环会耗尽日预算，T-8/L2）。
pub(crate) async fn count_judgments_today(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM judgments WHERE workspace_id = ? AND created_at >= date('now')
           AND status NOT IN ('budget_skipped','dispatch_suppressed')",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// T13 管道指标：verdict 分布 / 反馈对错数（按 workspace）。
/// E2：延迟 p50/p90（created_at→judged_at，秒）；E5：按 action_category 的
/// 反馈对错聚合（P1 转正决策的度量，取每判断最新反馈）。
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

    // 延迟分布：julianday 可解析 RFC3339（含 T 与 +00:00 时区后缀）
    let latency_rows = sqlx::query(
        "SELECT (julianday(judged_at) - julianday(created_at)) * 86400.0 AS secs
         FROM judgments WHERE workspace_id = ? AND judged_at IS NOT NULL ORDER BY secs",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    let latencies: Vec<f64> = latency_rows.iter().map(|r| r.get::<f64, _>("secs")).collect();
    let percentile = |p: f64| -> Option<f64> {
        if latencies.is_empty() {
            return None;
        }
        let idx = ((latencies.len() as f64) * p).ceil() as usize;
        Some(latencies[idx.saturating_sub(1).min(latencies.len() - 1)])
    };

    // 按 action_category 聚合最新反馈（改判取最新：MAX(id) 行）
    let cat_rows = sqlx::query(
        "SELECT j.action_category AS cat, f.verdict AS fb_verdict, COUNT(DISTINCT j.id) AS n
         FROM judgments j
         JOIN judgment_feedback f ON f.judgment_id = j.id
         WHERE j.workspace_id = ? AND j.action_category IS NOT NULL
           AND f.id IN (SELECT MAX(id) FROM judgment_feedback GROUP BY judgment_id)
         GROUP BY j.action_category, f.verdict",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    let mut by_category: std::collections::HashMap<String, CategoryFeedback> = std::collections::HashMap::new();
    for r in &cat_rows {
        let cat: String = r.get("cat");
        let entry = by_category
            .entry(cat)
            .or_insert(CategoryFeedback { right: 0, wrong: 0 });
        match r.get::<String, _>("fb_verdict").as_str() {
            "right" => entry.right = r.get::<i64, _>("n") as u64,
            "wrong" => entry.wrong = r.get::<i64, _>("n") as u64,
            _ => {}
        }
    }

    Ok(JudgmentStats {
        total: row.get::<i64, _>("total") as u64,
        noise: row.get::<Option<i64>, _>("noise").unwrap_or(0) as u64,
        self_healable: row.get::<Option<i64>, _>("self_healable").unwrap_or(0) as u64,
        needs_human: row.get::<Option<i64>, _>("needs_human").unwrap_or(0) as u64,
        resolved: row.get::<Option<i64>, _>("resolved").unwrap_or(0) as u64,
        escalated: row.get::<Option<i64>, _>("escalated").unwrap_or(0) as u64,
        feedback_right: fb.get::<Option<i64>, _>("right_count").unwrap_or(0) as u64,
        feedback_wrong: fb.get::<Option<i64>, _>("wrong_count").unwrap_or(0) as u64,
        latency_p50_secs: percentile(0.50),
        latency_p90_secs: percentile(0.90),
        feedback_by_category: by_category,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CategoryFeedback {
    pub right: u64,
    pub wrong: u64,
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
    /// 判断延迟分布（created_at→judged_at，秒；E2：验收「5min ≥90%」的度量）
    pub latency_p50_secs: Option<f64>,
    pub latency_p90_secs: Option<f64>,
    /// 按 action_category 的最新反馈对错数（E5：P1 转正决策度量）
    pub feedback_by_category: std::collections::HashMap<String, CategoryFeedback>,
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
    let row =
        sqlx::query("SELECT * FROM judgment_feedback WHERE judgment_id = ? ORDER BY created_at DESC, id DESC LIMIT 1")
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
        triage_mode: &str,
    ) -> Result<String> {
        insert_judgment(self.pool(), workspace_id, alarm_id, run_id, thing_id, triage_mode).await
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

    pub async fn list_judgments_feed(
        &self,
        workspace_id: &str,
        statuses: Option<&[JudgmentStatus]>,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Judgment>> {
        list_judgments_feed(self.pool(), workspace_id, statuses, before, limit).await
    }

    pub async fn stale_awaiting_approvals(&self, cutoff: &str) -> Result<Vec<Judgment>> {
        stale_awaiting_approvals(self.pool(), cutoff).await
    }

    pub async fn stale_by_status(&self, status: JudgmentStatus, cutoff: &str) -> Result<Vec<Judgment>> {
        stale_by_status(self.pool(), status, cutoff).await
    }

    pub async fn count_by_statuses(&self, workspace_id: &str, statuses: &[JudgmentStatus]) -> Result<i64> {
        count_by_statuses(self.pool(), workspace_id, statuses).await
    }

    pub async fn latest_feedbacks(
        &self,
        judgment_ids: &[String],
    ) -> Result<std::collections::HashMap<String, JudgmentFeedback>> {
        latest_feedbacks(self.pool(), judgment_ids).await
    }

    pub async fn find_investigating_judgment_by_thing_rule(
        &self,
        workspace_id: &str,
        thing_id: &str,
        rule_id: Option<&str>,
    ) -> Result<Option<Judgment>> {
        find_investigating_judgment_by_thing_rule(self.pool(), workspace_id, thing_id, rule_id).await
    }

    pub async fn find_latest_judgment_by_thing_rule(
        &self,
        workspace_id: &str,
        thing_id: &str,
        rule_id: Option<&str>,
    ) -> Result<Option<Judgment>> {
        find_latest_judgment_by_thing_rule(self.pool(), workspace_id, thing_id, rule_id).await
    }

    pub async fn reopen_judgment(&self, id: &str) -> Result<bool> {
        reopen_judgment(self.pool(), id).await
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

    /// 带系统 seed 的库（tenants 等 FK 依赖；fold 测试需要 workspace→tenant 链）。
    async fn seeded_db() -> Db {
        let db = test_db().await;
        crate::seed::seed_system(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn judgment_lifecycle_happy_path() {
        let db = test_db().await;
        let id = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();

        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Investigating);
        assert!(j.verdict.is_none());

        let ok = db
            .judge_judgment(
                &id,
                JudgmentVerdict::SelfHealable,
                "历史同类事件 87% 由重连恢复",
                "{}",
                Some("重连网关"),
                Some("connection_recovery"),
                Some("prop-1"),
            )
            .await
            .unwrap();
        assert!(ok);
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::AwaitingApproval);
        assert_eq!(j.verdict, Some(JudgmentVerdict::SelfHealable));
        assert!(j.judged_at.is_some());

        // 批准 → executing → resolved
        assert!(
            db.transit_judgment(&id, JudgmentStatus::AwaitingApproval, JudgmentStatus::Executing, None)
                .await
                .unwrap()
        );
        assert!(
            db.transit_judgment(&id, JudgmentStatus::Executing, JudgmentStatus::Resolved, None)
                .await
                .unwrap()
        );
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Resolved);
        assert!(j.resolved_at.is_some());
    }

    #[tokio::test]
    async fn conditional_transit_rejects_wrong_state() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        // investigating 直接跳 resolved 被拒
        assert!(
            !db.transit_judgment(&id, JudgmentStatus::AwaitingApproval, JudgmentStatus::Executing, None)
                .await
                .unwrap()
        );
        // 重复 judge（并发 RunRecorded）幂等
        assert!(
            db.judge_judgment(&id, JudgmentVerdict::Noise, "波动", "{}", None, None, None)
                .await
                .unwrap()
        );
        assert!(
            !db.judge_judgment(&id, JudgmentVerdict::Noise, "波动", "{}", None, None, None)
                .await
                .unwrap()
        );
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::NoiseArchived);
    }

    #[tokio::test]
    async fn fail_and_budget_paths() {
        let db = test_db().await;
        let id1 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        assert!(
            db.fail_judgment(&id1, JudgmentStatus::InvestigationFailed, "LLM 超时")
                .await
                .unwrap()
        );
        let id2 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        assert!(
            db.fail_judgment(&id2, JudgmentStatus::BudgetSkipped, "超日预算")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn feedback_history_and_latest_wins() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.add_judgment_feedback(&id, "ws1", "u1", "wrong", Some("其实该报"))
            .await
            .unwrap();
        // 改判
        db.add_judgment_feedback(&id, "ws1", "u1", "right", None).await.unwrap();
        let latest = db.latest_judgment_feedback(&id).await.unwrap().unwrap();
        assert_eq!(latest.verdict, "right");
    }

    #[tokio::test]
    async fn stale_approvals_scan_and_daily_count() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.judge_judgment(
            &id,
            JudgmentVerdict::SelfHealable,
            "r",
            "{}",
            Some("a"),
            Some("other"),
            None,
        )
        .await
        .unwrap();

        // judged_at 是当下：24h 前的 cutoff 不应命中，未来的 cutoff 应命中
        assert!(db.stale_awaiting_approvals("2000-01-01").await.unwrap().is_empty());
        assert_eq!(db.stale_awaiting_approvals("2999-01-01").await.unwrap().len(), 1);
        assert_eq!(db.count_judgments_today("ws1").await.unwrap(), 1);
        assert_eq!(db.count_judgments_today("ws-other").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_judgments_workspace_isolation_and_cursor() {
        let db = test_db().await;
        let a = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let b = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.insert_judgment("ws2", None, None, None, "annotate").await.unwrap();

        let page1 = db.list_judgments_feed("ws1", None, None, 1).await.unwrap();
        assert_eq!(page1.len(), 1);
        let page2 = db
            .list_judgments_feed("ws1", None, Some(&page1[0].id), 10)
            .await
            .unwrap();
        assert_eq!(page2.len(), 1);
        let ids: Vec<&str> = [page1[0].id.as_str(), page2[0].id.as_str()].into();
        assert!(ids.contains(&a.as_str()) && ids.contains(&b.as_str()));

        // workspace 隔离
        assert!(db.list_judgments_feed("ws2", None, None, 10).await.unwrap().len() == 1);
    }

    /// T-19/S2：非法迁移被矩阵响亮拒绝（release 同样生效）。
    #[tokio::test]
    async fn transition_matrix_rejects_illegal() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        // resolved 是执行出口，investigating 不可直达
        let err = db
            .transit_judgment(&id, JudgmentStatus::Investigating, JudgmentStatus::Resolved, None)
            .await;
        assert!(err.is_err());
        // fail_judgment 目标必须是失败终态
        let err = db.fail_judgment(&id, JudgmentStatus::Resolved, "x").await;
        assert!(err.is_err());
    }

    /// T-7/L1：dispatch_suppressed 生命周期 + 迟到 RunRecorded 恢复路由。
    #[tokio::test]
    async fn dispatch_suppressed_lifecycle_and_late_recovery() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        // dispatch 被拦 → dispatch_suppressed（不开票）
        assert!(
            db.fail_judgment(&id, JudgmentStatus::DispatchSuppressed, "O11 dedup 拦截")
                .await
                .unwrap()
        );
        // 不计入日预算（T-8/L2）
        assert_eq!(db.count_judgments_today("ws1").await.unwrap(), 0);
        // 迟到的调查 verdict 恢复路由到三出口
        assert!(
            db.judge_judgment(
                &id,
                JudgmentVerdict::Noise,
                "迟到的判断：正常波动",
                "{}",
                None,
                None,
                None
            )
            .await
            .unwrap()
        );
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::NoiseArchived);
    }

    /// T-12/C1：同秒多行元组游标不丢行。
    #[tokio::test]
    async fn tuple_cursor_no_loss_same_second() {
        let db = test_db().await;
        let mut ids = vec![];
        for _ in 0..3 {
            ids.push(db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap());
        }
        // 强制同秒（模拟报警风暴/种子脚本）
        sqlx::query("UPDATE judgments SET created_at = '2026-09-14T01:00:00+00:00'")
            .execute(db.pool())
            .await
            .unwrap();
        let page1 = db.list_judgments_feed("ws1", None, None, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db
            .list_judgments_feed("ws1", None, Some(&page1[1].id), 10)
            .await
            .unwrap();
        assert_eq!(page2.len(), 1, "同秒第三行必须可翻页到达");
    }

    /// F12：首页折叠同 thing+rule + 需行动置顶。
    #[tokio::test]
    async fn first_page_folds_and_pins() {
        let db = seeded_db().await;
        // 需要 workspace + thing + thing_alarms 行（FK 链）供折叠 join。
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
        // rule_id FK → thing_alarm_rules：补规则行（折叠键经 alarm.rule_id）
        sqlx::query(
            "INSERT INTO thing_alarm_rules (id, thing_id, rule_name, rule_type, condition_config, alarm_level, workspace_id)
             VALUES ('r1','t1','温度阈值','threshold','{}','warning','ws1')",
        )
        .execute(db.pool())
        .await
        .expect("rule insert");
        // thing_alarms 表 NOT NULL：thing_id/alarm_level/alarm_message/alarm_time
        // flapping 语义：同 thing+rule 三条报警（各是新行，alarm_id 不同）
        for aid in ["a1", "a2", "a3"] {
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
        // 显式递增时间戳（不靠 wall-clock sleep——同文件 tuple_cursor 测试同款）
        sqlx::query("UPDATE judgments SET created_at = '2026-09-14T01:00:00+00:00' WHERE id = ?")
            .bind(&j1)
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE judgments SET created_at = '2026-09-14T01:01:00+00:00' WHERE id = ?")
            .bind(&j2)
            .execute(db.pool())
            .await
            .unwrap();
        let j3 = db
            .insert_judgment("ws1", Some("a3"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        // j3 判为需人工 → escalated（置顶）
        db.judge_judgment(&j3, JudgmentVerdict::NeedsHuman, "要人", "{}", None, None, None)
            .await
            .unwrap();
        let feed = db.list_judgments_feed("ws1", None, None, 10).await.unwrap();
        assert_eq!(feed.len(), 1, "同 thing+rule 折叠为最新一条");
        assert_eq!(feed[0].id, j3);
        assert_eq!(feed[0].status, JudgmentStatus::Escalated);
        let _ = (j1, j2);
    }

    /// F-H：批量反馈取最新（替代 N+1）。
    #[tokio::test]
    async fn latest_feedbacks_batch() {
        let db = test_db().await;
        let j1 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        let j2 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.add_judgment_feedback(&j1, "ws1", "u1", "wrong", Some("判错了"))
            .await
            .unwrap();
        db.add_judgment_feedback(&j1, "ws1", "u1", "right", None).await.unwrap(); // 改判
        db.add_judgment_feedback(&j2, "ws1", "u1", "wrong", Some("不对"))
            .await
            .unwrap();
        let map = db.latest_feedbacks(&[j1.clone(), j2.clone()]).await.unwrap();
        assert_eq!(map.get(&j1).unwrap().verdict, "right", "改判取最新");
        assert_eq!(map.get(&j2).unwrap().verdict, "wrong");
    }

    /// E2：延迟统计（created_at→judged_at 的 p50/p90）。
    #[tokio::test]
    async fn latency_stats_computed() {
        let db = test_db().await;
        let j1 = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.judge_judgment(&j1, JudgmentVerdict::Noise, "r", "{}", None, None, None)
            .await
            .unwrap();
        let stats = db.judgment_stats("ws1").await.unwrap();
        assert!(stats.latency_p50_secs.is_some());
        assert!(stats.latency_p90_secs.is_some());
        assert!(stats.latency_p50_secs.unwrap() >= 0.0);
    }

    /// T-17/C6：✕ 反馈 → 重开 investigating（迁移矩阵 noise_archived → investigating）。
    #[tokio::test]
    async fn reopen_after_wrong_feedback() {
        let db = test_db().await;
        let id = db.insert_judgment("ws1", None, None, None, "annotate").await.unwrap();
        db.judge_judgment(&id, JudgmentVerdict::Noise, "波动", "{}", None, None, None)
            .await
            .unwrap();
        assert!(db.reopen_judgment(&id).await.unwrap());
        let j = db.find_judgment_by_id(&id, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Investigating);
        assert!(j.verdict.is_none());
        // 已重开的不能重复重开
        assert!(!db.reopen_judgment(&id).await.unwrap());
    }
}
