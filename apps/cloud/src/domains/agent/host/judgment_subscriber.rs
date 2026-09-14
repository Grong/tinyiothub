//! T5：judgment 订阅者——RunRecorded（problem_key="alarm:…" 的调查 run）→
//! judgments 投影 + 三出口路由。
//!
//! 出口（2026-09-14 eng-review 硬化版状态机）：
//! - verdict=noise         → judge 落 noise_archived；抑制双闸门：快照
//!   triage_mode=suppress 且非 Critical/Error 才 suppress_alarm（F-C/T-6/T-24）
//! - verdict=self_healable → awaiting_approval（feed 页审批；执行由 approve 端点派发）
//! - verdict=needs_human   → escalated + create_escalation 工单 + 关联
//! - run 失败/判断解析失败  → investigation_failed + create_escalation 工单
//!   （与 ticket_subscriber 撞同一 failure_hash → 折叠 recurrence，不开双票）
//! - dispatch 被拦（O11/队列满）→ 清扫器标 dispatch_suppressed；迟到
//!   RunRecorded 找回并按 verdict 恢复路由（T-7/L1，真实调查结果永不丢弃）
//!
//! 解析（T-15/C3）：围栏 ```json 块优先；宽松 fallback 保留但 evidence 打
//! parse_fallback 标记（可统计可观测），注入面由 action_category 白名单
//! 与服务端动作模板托底。
//!
//! 调查指令是"只调查不执行"（callbacks.rs alarm_investigation_text），执行
//! 发生在审批通过后由 judgment approve 端点派发新 run——审批权在 judgment 域，
//! 不掺入 thing-agent 的 proposal 体系。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast::Receiver;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventKind};
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_storage::Db;
use tinyiothub_storage::judgment::{Judgment, JudgmentStatus, JudgmentVerdict};

use crate::domains::alarm::service::AlarmService;
use crate::domains::event::sse_manager::SseConnectionManager;

/// problem_key 前缀（callbacks.rs dispatch_alarm_investigation 的键域）。
const ALARM_KEY_PREFIX: &str = "alarm:";

/// 从 problem_key "alarm:{thing_id}:{rule_id|-}" 解析。
fn parse_alarm_key(problem_key: &str) -> Option<(&str, Option<&str>)> {
    let rest = problem_key.strip_prefix(ALARM_KEY_PREFIX)?;
    let (thing_id, rule_id) = rest.rsplit_once(':')?;
    if thing_id.is_empty() {
        return None;
    }
    let rule_id = if rule_id == "-" { None } else { Some(rule_id) };
    Some((thing_id, rule_id))
}

/// 结构化判断载荷（调查指令约定的 ```json 块）。
#[derive(Debug, serde::Deserialize)]
pub(crate) struct VerdictPayload {
    verdict: String,
    reason: String,
    suggested_action: Option<String>,
    action_category: Option<String>,
}

impl VerdictPayload {
    /// eval 套件断言用（T-11：eval 与生产共用解析，保真）。
    /// 仅测试消费（lib 构建下未用）。
    #[allow(dead_code)]
    pub(crate) fn verdict_str(&self) -> &str {
        &self.verdict
    }
}

/// 解析 report.summary 尾部的 ```json verdict 块；宽松 fallback：找最后一个
/// 含 "verdict" 的 {...} 段（eng-review 外部修正 8 + T-15/C3：fallback 保留
/// 但返回 used_fallback 标记——写进 evidence 可统计可审计，格式漂移不转嫁
/// 工单队列；注入面由 action_category 白名单托底而非删除解析路径）。
/// 返回 (payload, used_fallback)。
pub(crate) fn parse_verdict(summary: &str) -> Option<(VerdictPayload, bool)> {
    // 严格路径：```json ... ``` 围栏块
    if let Some(start) = summary.rfind("```json") {
        let block = &summary[start + 7..];
        if let Some(end) = block.find("```")
            && let Ok(v) = serde_json::from_str::<VerdictPayload>(block[..end].trim())
        {
            return Some((v, false));
        }
    }
    // 宽松路径：最后一个含 "verdict" 的 JSON 对象
    if let Some(start) = summary.rfind('{') {
        let candidate = &summary[start..];
        if candidate.contains("\"verdict\"")
            && let Ok(v) = serde_json::from_str::<VerdictPayload>(candidate.trim())
        {
            return Some((v, true));
        }
    }
    None
}

/// 单事件投影。所有错误就地处理（log），不向上传播。
pub(crate) async fn project(event: &AgentEvent, db: &Db, sse: &SseConnectionManager, alarm_service: &AlarmService) {
    let AgentEventKind::RunRecorded {
        report,
        problem_key: Some(pk),
        ..
    } = &event.kind
    else {
        return;
    };

    // 审批执行收尾（approve 端点派发的 exec run）
    if let Some(judgment_id) = pk.strip_prefix("exec:") {
        settle_execution(db, sse, alarm_service, judgment_id, report).await;
        return;
    }

    if !pk.starts_with(ALARM_KEY_PREFIX) {
        return;
    }
    let Some((thing_id, rule_id)) = parse_alarm_key(pk) else {
        warn!(problem_key = %pk, "malformed alarm problem_key");
        return;
    };
    // 正常路径：找 investigating 判断（T-9/L3：只有 investigating 算「在查」）。
    // 迟到恢复（T-7/L1）：判断可能已被清扫器标 dispatch_suppressed（dispatch
    // 当时被拦/误判），RunRecorded 迟到到达时找回它并照常路由 verdict——
    // 真实完成的调查结果永不丢弃。
    let judgment = match db
        .find_investigating_judgment_by_thing_rule(&report.workspace_id, thing_id, rule_id)
        .await
    {
        Ok(Some(j)) => j,
        Ok(None) => match db
            .find_latest_judgment_by_thing_rule(&report.workspace_id, thing_id, rule_id)
            .await
        {
            Ok(Some(j)) if j.status == JudgmentStatus::DispatchSuppressed => {
                tracing::info!(
                    judgment_id = %j.id,
                    problem_key = %pk,
                    "late RunRecorded recovered for dispatch-suppressed judgment"
                );
                j
            }
            _ => {
                debug!(problem_key = %pk, "no open judgment for alarm investigation (flap-deduped?)");
                return;
            }
        },
        Err(e) => {
            error!(problem_key = %pk, error = %e, "find investigating judgment failed");
            return;
        }
    };

    // run_id 回填（调查 run 与判断的关联）
    if let Err(e) = db.set_judgment_run_id(&judgment.id, &report.run_id).await {
        warn!(judgment_id = %judgment.id, error = %e, "backfill run_id failed");
    }

    match report.outcome {
        Outcome::Failed | Outcome::BudgetExceeded | Outcome::Rejected => {
            fail_and_escalate(
                db,
                sse,
                &judgment,
                report,
                pk,
                &format!("调查 run 失败：{}", report.summary),
            )
            .await;
        }
        _ => match parse_verdict(&report.summary) {
            Some((payload, used_fallback)) => {
                route_verdict(db, sse, alarm_service, &judgment, report, pk, payload, used_fallback).await
            }
            None => {
                fail_and_escalate(db, sse, &judgment, report, pk, "判断输出解析失败").await;
            }
        },
    }
}

/// 审批执行收尾（T-4/5A verified 硬门 + T-10/L4 人工确认）：
/// - Acted + verified     → resolved（+报警消除）——执行且验证，唯一消警路径
/// - Acted + !verified    → 停留 executing（正名「等待人工确认」；1h SLA
///   清扫转人工确认工单——没有死等回读，run 已结束）
/// - NoActionNeeded       → 停留 executing（agent 回报「无需动作」与已批准的
///   判断冲突，批准意图不可被 LLM 单方降级；同样由 SLA 转人工确认）
/// - Failed 等            → escalated + 工单
///
/// exec 失败的升级键按 judgment 唯一（执行失败是新事实，不与调查票折叠）。
async fn settle_execution(
    db: &Db,
    sse: &SseConnectionManager,
    alarm_service: &AlarmService,
    judgment_id: &str,
    report: &RunReport,
) {
    let judgment = match db.find_judgment_by_id(judgment_id, &report.workspace_id).await {
        Ok(Some(j)) => j,
        Ok(None) => {
            debug!(judgment_id, "exec run for unknown judgment");
            return;
        }
        Err(e) => {
            error!(judgment_id, error = %e, "find judgment failed");
            return;
        }
    };

    match report.outcome {
        Outcome::Acted if report.verified => {
            match db
                .transit_judgment(judgment_id, JudgmentStatus::Executing, JudgmentStatus::Resolved, None)
                .await
            {
                Ok(true) => {
                    if let Some(alarm_id) = &judgment.alarm_id
                        && let Err(e) = alarm_service.auto_resolve_alarm(alarm_id, &judgment.workspace_id).await
                    {
                        warn!(alarm_id, error = %e, "auto-resolve after execution failed");
                    }
                    broadcast_judgment(sse, &judgment.workspace_id, judgment_id, "judgment_updated").await;
                }
                Ok(false) => {
                    // 迟到 Acted：judgment 可能已被 SLA 转人工（escalated）——
                    // 动作实际执行了但不自动消警（T-16/C4），记录供审计。
                    tracing::warn!(
                        judgment_id,
                        run_id = %report.run_id,
                        "late Acted arrived for non-executing judgment — NOT auto-resolving alarm"
                    );
                }
                Err(e) => error!(judgment_id, error = %e, "settle transit failed"),
            }
        }
        Outcome::Acted => {
            // 执行了但未验证：停留 executing，等 SLA 转人工确认（不消警）
            tracing::info!(
                judgment_id,
                run_id = %report.run_id,
                "exec acted without verification — awaiting human confirmation via SLA sweep"
            );
        }
        Outcome::NoActionNeeded => {
            // 与已批准判断冲突：LLM 不可单方撤销人工批准（T-10/L4）
            tracing::warn!(
                judgment_id,
                run_id = %report.run_id,
                "exec run returned NoActionNeeded after human approval — awaiting human confirmation via SLA sweep"
            );
        }
        Outcome::Failed | Outcome::BudgetExceeded | Outcome::Rejected => {
            match db
                .transit_judgment(judgment_id, JudgmentStatus::Executing, JudgmentStatus::Escalated, None)
                .await
            {
                Ok(true) => {
                    let reason = format!(
                        "批准的动作执行失败：{}",
                        report.summary.chars().take(80).collect::<String>()
                    );
                    escalate_to_ticket(db, sse, &judgment, report, &format!("exec:{}", judgment_id), &reason).await;
                    broadcast_judgment(sse, &judgment.workspace_id, judgment_id, "judgment_updated").await;
                }
                Ok(false) => debug!(judgment_id, "escalate skipped (not executing)"),
                Err(e) => error!(judgment_id, error = %e, "escalate transit failed"),
            }
        }
    }
}

/// 三出口路由。
async fn route_verdict(
    db: &Db,
    sse: &SseConnectionManager,
    alarm_service: &AlarmService,
    judgment: &Judgment,
    report: &RunReport,
    problem_key: &str,
    payload: VerdictPayload,
    used_fallback: bool,
) {
    let reason: String = payload.reason.chars().take(120).collect(); // F13：理由 ≤120 字符
    let verdict = JudgmentVerdict::parse_str(&payload.verdict);
    let Some(verdict) = verdict else {
        fail_and_escalate(
            db,
            sse,
            judgment,
            report,
            problem_key,
            &format!("未知 verdict: {}", payload.verdict),
        )
        .await;
        return;
    };

    match verdict {
        JudgmentVerdict::Noise => {
            // T-3：先落判断（条件迁移），成功后才考虑抑制——抑制是可见副作用，
            // 不能在 judge 输掉竞争时先行发生。
            let judged = judge(db, sse, judgment, report, verdict, &reason, &payload, used_fallback).await;
            if !judged {
                return;
            }
            // F-C + T-6/T-24：抑制双闸门——① judgment 创建时快照的 triage_mode
            // 必须为 suppress（影子期默认 annotate = 只标注不动报警）；
            // ② Critical/Error 永不抑制（严重级切分的全部意义是严重报警必到人）。
            if judgment.triage_mode != "suppress" {
                debug!(judgment_id = %judgment.id, "annotate mode: noise judged without suppressing alarm");
                return;
            }
            let Some(alarm_id) = &judgment.alarm_id else { return };
            let severe = match db.find_alarm_by_id(alarm_id, Some(&judgment.workspace_id)).await {
                Ok(Some(a)) => matches!(
                    a.alarm_level,
                    tinyiothub_storage::alarm::AlarmLevel::Critical | tinyiothub_storage::alarm::AlarmLevel::Error
                ),
                Ok(None) => {
                    warn!(alarm_id, "alarm not found for noise suppression check");
                    true // 查不到报警级别时按严重处理（不抑制 = 安全方向）
                }
                Err(e) => {
                    warn!(alarm_id, error = %e, "alarm level lookup failed, skipping suppression");
                    true
                }
            };
            if severe {
                tracing::info!(alarm_id, "severe alarm judged noise — NOT suppressing (feed mark only)");
                return;
            }
            if let Err(e) = alarm_service.suppress_alarm(alarm_id, &judgment.workspace_id).await {
                warn!(alarm_id, error = %e, "suppress alarm failed (already resolved?)");
            }
        }
        JudgmentVerdict::SelfHealable => {
            judge(db, sse, judgment, report, verdict, &reason, &payload, used_fallback).await;
        }
        JudgmentVerdict::NeedsHuman => {
            judge(db, sse, judgment, report, verdict, &reason, &payload, used_fallback).await;
            escalate_to_ticket(db, sse, judgment, report, problem_key, &reason).await;
        }
    }
}

/// 落判断。返回是否成功迁移（true = 本调用胜出；false = 并发/重复/已终态）。
#[allow(clippy::too_many_arguments)]
async fn judge(
    db: &Db,
    sse: &SseConnectionManager,
    judgment: &Judgment,
    report: &RunReport,
    verdict: JudgmentVerdict,
    reason: &str,
    payload: &VerdictPayload,
    used_fallback: bool,
) -> bool {
    // F11 证据契约 P0 版：调查 run 的摘要截取作为证据来源（属性快照/事件列表
    // 的结构化提取在 P1 再做）。T-15/C3：宽松解析命中时打 parse_fallback
    // 标记——feed 可见、可统计，格式漂移有观测面。
    let summary_excerpt: String = report.summary.chars().take(500).collect();
    let evidence = serde_json::json!({
        "source": "run_summary",
        "run_id": report.run_id,
        "excerpt": summary_excerpt,
        "parse_fallback": used_fallback,
    })
    .to_string();
    // F5：suggested_action 截断（LLM 输出无长度上限会撑爆行）；绑定局部
    // 变量避免临时值借用问题。
    let suggested_action: Option<String> = payload
        .suggested_action
        .as_deref()
        .map(|a| a.chars().take(200).collect());
    let action_category = tinyiothub_storage::judgment::normalize_action_category(payload.action_category.as_deref());
    match db
        .judge_judgment(
            &judgment.id,
            verdict,
            reason,
            &evidence,
            suggested_action.as_deref(),
            action_category.as_deref(),
            None,
        )
        .await
    {
        Ok(true) => {
            broadcast_judgment(sse, &judgment.workspace_id, &judgment.id, "judgment_judged").await;
            true
        }
        Ok(false) => {
            debug!(judgment_id = %judgment.id, "judge skipped (not investigating — duplicate RunRecorded)");
            false
        }
        Err(e) => {
            error!(judgment_id = %judgment.id, error = %e, "judge_judgment failed");
            false
        }
    }
}

async fn fail_and_escalate(
    db: &Db,
    sse: &SseConnectionManager,
    judgment: &Judgment,
    report: &RunReport,
    problem_key: &str,
    reason: &str,
) {
    match db
        .fail_judgment(&judgment.id, JudgmentStatus::InvestigationFailed, reason)
        .await
    {
        Ok(true) => broadcast_judgment(sse, &judgment.workspace_id, &judgment.id, "judgment_judged").await,
        Ok(false) => debug!(judgment_id = %judgment.id, "fail skipped (not investigating)"),
        Err(e) => error!(judgment_id = %judgment.id, error = %e, "fail_judgment failed"),
    }
    escalate_to_ticket(db, sse, judgment, report, problem_key, reason).await;
}

/// 转工单（统一入口；failure_hash 与 ticket_subscriber 同键 → 撞单折叠）。
async fn escalate_to_ticket(
    db: &Db,
    sse: &SseConnectionManager,
    judgment: &Judgment,
    report: &RunReport,
    problem_key: &str,
    reason: &str,
) {
    let Some(ticket_id) = crate::domains::ticket::create_escalation(
        db,
        sse,
        crate::domains::ticket::Escalation {
            workspace_id: judgment.workspace_id.clone(),
            thing_id: judgment.thing_id.clone(),
            agent_run_id: report.run_id.clone(),
            title: format!("需人工：{}", reason.chars().take(70).collect::<String>()),
            briefing: serde_json::json!({
                "problem": reason,
                "source": "judgment_escalation",
                "judgment_id": judgment.id,
                "alarm_id": judgment.alarm_id,
                "run_summary": report.summary,
            }),
            // 与 ticket_subscriber 的 pk:{problem_key} 同形——同一问题的
            // run 失败票与 judgment 升级票折叠为一张
            failure_hash: format!("pk:{}", problem_key),
        },
    )
    .await
    else {
        return;
    };
    // 关联工单（investigation_failed/needs_human 均已落终态，transit 不适用）
    if let Err(e) = db.link_judgment_ticket(&judgment.id, ticket_id).await {
        warn!(judgment_id = %judgment.id, ticket_id, error = %e, "link ticket failed");
    }
}

async fn broadcast_judgment(sse: &SseConnectionManager, workspace_id: &str, judgment_id: &str, kind: &str) {
    let msg = crate::domains::notify::channels::sse_channel::SseMessage::new(
        kind.to_string(),
        serde_json::json!({
            "workspace_id": workspace_id,
            "judgment_id": judgment_id,
        }),
    );
    sse.broadcast_message(msg).await;
}

/// handler 层（approve/reject/feedback）复用的广播出口。
pub async fn broadcast_judgment_pub(sse: &SseConnectionManager, workspace_id: &str, judgment_id: &str) {
    broadcast_judgment(sse, workspace_id, judgment_id, "judgment_updated").await;
}

/// 主循环（测试接缝；生产经 [`supervise_judgment_subscriber`]）。
async fn run_judgment_subscriber(
    mut rx: Receiver<AgentEvent>,
    db: Arc<Db>,
    sse: Arc<SseConnectionManager>,
    alarm_service: Arc<AlarmService>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                debug!("judgment subscriber shutdown");
                return;
            }
            event = rx.recv() => {
                match event {
                    Ok(event) => project(&event, &db, &sse, &alarm_service).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // Lagged：无逐条重放（ticket_subscriber 的 300s 对账
                        // 会兜底开失败的 run 的票；卡在 investigating 的判断
                        // 由 approval_timeout/人工在 feed 可见）。
                        warn!(dropped = n, "judgment subscriber lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}

/// 监管循环（与 ticket_subscriber 同构）：panic/异常退出 → error + 新
/// receiver + 退避重启。
pub async fn supervise_judgment_subscriber(
    first_rx: Receiver<AgentEvent>,
    rx_factory: impl Fn() -> Receiver<AgentEvent> + Send + 'static,
    db: Arc<Db>,
    sse: Arc<SseConnectionManager>,
    alarm_service: Arc<AlarmService>,
    shutdown: CancellationToken,
) {
    let mut backoff = Duration::from_secs(1);
    let mut pending_rx = Some(first_rx);
    loop {
        if shutdown.is_cancelled() {
            break;
        }
        let rx = pending_rx.take().unwrap_or_else(&rx_factory);
        let token = shutdown.clone();
        let db2 = Arc::clone(&db);
        let sse2 = Arc::clone(&sse);
        let alarm2 = Arc::clone(&alarm_service);
        let handle = tokio::spawn(async move {
            run_judgment_subscriber(rx, db2, sse2, alarm2, token).await;
        });
        match handle.await {
            Ok(()) => {
                if shutdown.is_cancelled() {
                    break;
                }
                error!("judgment subscriber exited unexpectedly — restarting");
            }
            Err(e) => {
                error!(error = %e, "judgment subscriber panicked — restarting");
            }
        }
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(backoff) => {},
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::agent_runs::EndReason;

    #[test]
    fn parse_alarm_key_variants() {
        assert_eq!(parse_alarm_key("alarm:t1:rule-9"), Some(("t1", Some("rule-9"))));
        assert_eq!(parse_alarm_key("alarm:t1:-"), Some(("t1", None)));
        assert_eq!(parse_alarm_key("alarm:t1"), None);
        assert_eq!(parse_alarm_key("other:t1:r"), None);
    }

    #[test]
    fn parse_verdict_strict_fenced_block() {
        let summary = "调查过程……\n```json\n{\"verdict\": \"noise\", \"reason\": \"短暂波动\", \"suggested_action\": null, \"action_category\": \"other\"}\n```";
        let (v, fallback) = parse_verdict(summary).unwrap();
        assert!(!fallback);
        assert_eq!(v.verdict, "noise");
        assert_eq!(v.reason, "短暂波动");
    }

    #[test]
    fn parse_verdict_lenient_fallback() {
        // 无围栏的裸 JSON（LLM 常见偷懒输出）——T-15/C3：fallback 保留但打标记
        let summary = "分析完毕。{\"verdict\": \"needs_human\", \"reason\": \"持续恶化\", \"suggested_action\": null, \"action_category\": null}";
        let (v, fallback) = parse_verdict(summary).unwrap();
        assert!(fallback, "裸 JSON 应走 fallback 并打标记");
        assert_eq!(v.verdict, "needs_human");
    }

    #[test]
    fn parse_verdict_unparseable() {
        assert!(parse_verdict("全是自然语言没有结构化输出").is_none());
    }

    fn report(run_id: &str, outcome: Outcome, summary: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: "ws1".to_string(),
            trigger: "alarm".to_string(),
            outcome,
            summary: summary.to_string(),
            actions: vec![],
            verified: false,
            duration_ms: 100,
            tool_calls: 1,
            tokens: 10,
            end_reason: None::<EndReason>,
            thing_id: Some("t1".to_string()),
        }
    }

    async fn fixture() -> (Arc<Db>, Arc<SseConnectionManager>, Arc<AlarmService>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        tinyiothub_storage::seed::seed_system(&tinyiothub_storage::Db::new(pool.clone()))
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thing_alarms (id, thing_id, workspace_id, alarm_level, alarm_message, alarm_time) VALUES ('a1','t1','ws1','warning','温度越限','2025-01-01')")
            .execute(&pool).await.unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let sse = Arc::new(SseConnectionManager::new());
        let alarm = Arc::new(AlarmService::new(db.clone()));
        (db, sse, alarm)
    }

    fn event(run_id: &str, outcome: Outcome, summary: &str) -> AgentEvent {
        AgentEvent {
            seq: 1,
            occurred_at: chrono::Utc::now(),
            kind: AgentEventKind::RunRecorded {
                report: Box::new(report(run_id, outcome, summary)),
                problem_key: Some("alarm:t1:-".to_string()),
                dedup_key: None,
            },
        }
    }

    #[tokio::test]
    async fn needs_human_escalates_to_ticket() {
        let (db, sse, alarm) = fixture().await;
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        let summary = "...\n```json\n{\"verdict\": \"needs_human\", \"reason\": \"冷却系统疑似故障\", \"suggested_action\": null, \"action_category\": \"other\"}\n```";
        project(&event("r1", Outcome::NoActionNeeded, summary), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some(), "ticket linked");
        assert_eq!(j.run_id.as_deref(), Some("r1"));
    }

    #[tokio::test]
    async fn noise_suppresses_alarm_and_archives() {
        let (db, sse, alarm) = fixture().await;
        // suppress 模式（T-6：快照在 judgment 行上；annotate 默认不抑制，
        // 由 alarm_disposition_tests::annotate_mode_noise_does_not_suppress 覆盖）
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "suppress")
            .await
            .unwrap();
        let summary = "```json\n{\"verdict\": \"noise\", \"reason\": \"正常波动\", \"suggested_action\": null, \"action_category\": \"other\"}\n```";
        project(&event("r1", Outcome::NoActionNeeded, summary), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::NoiseArchived);
        let a = db.find_alarm_by_id("a1", Some("ws1")).await.unwrap().unwrap();
        assert_eq!(a.status, tinyiothub_storage::alarm::AlarmStatus::Suppressed);
    }

    #[tokio::test]
    async fn failed_run_marks_investigation_failed_and_escalates() {
        let (db, sse, alarm) = fixture().await;
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        project(&event("r1", Outcome::Failed, "LLM 超时"), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::InvestigationFailed);
        assert!(j.ticket_id.is_some());
    }

    #[tokio::test]
    async fn unparseable_verdict_escalates_not_silent() {
        let (db, sse, alarm) = fixture().await;
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        project(
            &event("r1", Outcome::NoActionNeeded, "没有结构化输出"),
            &db,
            &sse,
            &alarm,
        )
        .await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::InvestigationFailed);
        assert!(j.ticket_id.is_some());
    }

    #[tokio::test]
    async fn unrelated_problem_keys_ignored() {
        let (db, sse, alarm) = fixture().await;
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        let mut e = event("r1", Outcome::NoActionNeeded, "x");
        e.kind = AgentEventKind::RunRecorded {
            report: Box::new(report("r1", Outcome::NoActionNeeded, "x")),
            problem_key: Some("reboot:t1".to_string()),
            dedup_key: None,
        };
        project(&e, &db, &sse, &alarm).await;
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Investigating, "untouched");
    }

    /// 造一个 executing 状态的 judgment（走完整 investigate → judge → approve 链）。
    async fn seed_executing(db: &Db) -> String {
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            JudgmentVerdict::SelfHealable,
            "可自愈",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
        )
        .await
        .unwrap();
        let ok = db
            .transit_judgment(&jid, JudgmentStatus::AwaitingApproval, JudgmentStatus::Executing, None)
            .await
            .unwrap();
        assert!(ok, "awaiting_approval → executing");
        jid
    }

    fn exec_event(jid: &str, run_id: &str, outcome: Outcome, verified: bool) -> AgentEvent {
        let mut r = report(run_id, outcome, "执行完毕");
        r.verified = verified;
        AgentEvent {
            seq: 1,
            occurred_at: chrono::Utc::now(),
            kind: AgentEventKind::RunRecorded {
                report: Box::new(r),
                problem_key: Some(format!("exec:{jid}")),
                dedup_key: None,
            },
        }
    }

    #[tokio::test]
    async fn exec_acted_verified_resolves_and_clears_alarm() {
        let (db, sse, alarm) = fixture().await;
        let jid = seed_executing(&db).await;
        project(&exec_event(&jid, "r-exec", Outcome::Acted, true), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Resolved, "Acted+verified → resolved");
        let a = db.find_alarm_by_id("a1", Some("ws1")).await.unwrap().unwrap();
        assert_eq!(
            a.status,
            tinyiothub_storage::alarm::AlarmStatus::Resolved,
            "verified 执行后自动消警（唯一消警路径）"
        );
    }

    #[tokio::test]
    async fn exec_acted_unverified_stays_executing() {
        let (db, sse, alarm) = fixture().await;
        let jid = seed_executing(&db).await;
        project(&exec_event(&jid, "r-exec", Outcome::Acted, false), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Executing, "未验证不闭环，等 SLA 转人工确认");
        let a = db.find_alarm_by_id("a1", Some("ws1")).await.unwrap().unwrap();
        assert_ne!(a.status, tinyiothub_storage::alarm::AlarmStatus::Resolved, "未验证不消警");
    }

    #[tokio::test]
    async fn exec_failed_escalates_with_ticket() {
        let (db, sse, alarm) = fixture().await;
        let jid = seed_executing(&db).await;
        project(&exec_event(&jid, "r-exec", Outcome::Failed, false), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some(), "执行失败转工单");
    }

    #[tokio::test]
    async fn late_acted_after_escalation_does_not_auto_resolve() {
        let (db, sse, alarm) = fixture().await;
        let jid = seed_executing(&db).await;
        // SLA 清扫先转人工（executing → escalated）
        let ok = db
            .transit_judgment(&jid, JudgmentStatus::Executing, JudgmentStatus::Escalated, None)
            .await
            .unwrap();
        assert!(ok);
        // 迟到的 Acted+verified 到达：不自动消警（T-16/C4）
        project(&exec_event(&jid, "r-exec", Outcome::Acted, true), &db, &sse, &alarm).await;

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, JudgmentStatus::Escalated, "迟到 Acted 不翻转终态");
        let a = db.find_alarm_by_id("a1", Some("ws1")).await.unwrap().unwrap();
        assert_ne!(
            a.status,
            tinyiothub_storage::alarm::AlarmStatus::Resolved,
            "迟到 Acted 不消警"
        );
    }
}
