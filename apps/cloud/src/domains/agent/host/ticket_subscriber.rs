//! 工单订阅者（T5/T6）：AgentEvent 广播 → tickets 投影 + SSE 通知。
//!
//! 与 persist.rs 同构：订阅 `AgentEventKind::RunRecorded`，自治失败
//! （outcome∈{Failed, BudgetExceeded, Rejected}）开票。
//!
//! 设计契约（设计文档 Eng Review Addendum）：
//! - 触发按 outcome 不按 TurnEnd（TurnEnd::Failed/TimedOut 是 LLM 基建错误；
//!   Rejected/BudgetExceeded 才是真正的"Agent 处理不了"）。
//! - failure_hash 四源规则：problem_key（UserDirective）> dedup_key 且
//!   thing: 前缀（ThingEvent 粒度）> hash(end_reason || normalize(summary))。
//!   Timer（timer:{ws} 过粗）与 Merged 走 hash 回退。
//! - 撞活跃唯一索引 = 正常去重路径 → 复发折叠事件（带最新 run_id）。
//! - 创建失败（非唯一冲突）→ tracing::error + agent_dead_letters（创建失败
//!   本身必须可见，不得复制"静默失败"）。
//! - 丢事件恢复：`RecvError::Lagged` → 扫 agent_runs 补开票；生产 5 分钟
//!   周期对账（重放幂等——只会再撞一次唯一索引转追加）。
//! - SSE 通知：开票成功即经 notify 域 workspace 广播（复用现有通道，
//!   data 带 workspace_id 参与连接侧过滤）。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventKind};
use tinyiothub_core::agent_runs::{ActionResult, Outcome, RunReport};
use tinyiothub_storage::Db;

use crate::domains::event::sse_manager::SseConnectionManager;

/// 周期全量对账间隔（与 persist.rs 对齐）。
const RECONCILE_INTERVAL: Duration = Duration::from_secs(300);
/// 对账回看窗口（小时）。
const RECONCILE_LOOKBACK_HOURS: i64 = 24;

/// 触发开票的 outcome 集合（设计契约 M1-1）。
pub(crate) fn should_ticket(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Failed | Outcome::BudgetExceeded | Outcome::Rejected)
}

/// failure_hash 四源规则（设计契约 #11）。
pub(crate) fn failure_hash(report: &RunReport, problem_key: Option<&str>, dedup_key: Option<&str>) -> String {
    if let Some(pk) = problem_key {
        return format!("pk:{pk}");
    }
    if let Some(key) = dedup_key
        && key.starts_with("thing:")
    {
        return format!("dk:{key}");
    }
    let kind = report.end_reason.map(|r| r.as_str()).unwrap_or("unknown");
    format!("hash:{}:{}", kind, normalize(&report.summary))
}

/// normalize（token 级）：含数字的 token 整体折叠为 '#'——run_id、错误码、
/// 计数、型号数字都不参与区分（欠 normalize 方向）；纯文字 token（含 CJK，
/// 是语义本体）原样保留并小写化（过 normalize 方向，不同故障不碰撞）。
/// 注意：hash 路径只服务 Timer/Merged/无键源（ThingEvent/UserDirective 走
/// 结构化键），thing 级精确区分不依赖本函数。
pub(crate) fn normalize(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|tok| !tok.is_empty())
        .map(|tok| {
            if tok.chars().any(|c| c.is_numeric()) {
                "#".to_string()
            } else {
                tok.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 简报装配（设计契约 M1-3）：briefing 五字段全部来自 RunReport 结构化
/// 数据，禁止解析 trigger 字符串。
pub(crate) fn build_briefing(report: &RunReport) -> serde_json::Value {
    let steps: Vec<serde_json::Value> = report
        .actions
        .iter()
        .map(|a| {
            let (result, error) = match &a.result {
                ActionResult::Success(_) => ("成功".to_string(), serde_json::Value::Null),
                ActionResult::Failed(e) => ("失败".to_string(), serde_json::Value::String(e.clone())),
                ActionResult::UnknownCancelled => ("已取消（截断时仍在执行）".to_string(), serde_json::Value::Null),
            };
            serde_json::json!({
                "action": format!("{}.{}", a.thing_id, a.action_name),
                "result": result,
                "error": error,
            })
        })
        .collect();
    let last_error = report.actions.iter().rev().find_map(|a| match &a.result {
        ActionResult::Failed(e) => Some(e.clone()),
        _ => None,
    });
    // Rejected（TurnEnd::Text）时 summary 即 LLM 最终原文（runner.rs:457），
    // 其余情形为框架合成摘要，不建议下一步。
    let suggested: Vec<String> = if report.outcome == Outcome::Rejected {
        vec![report.summary.clone()]
    } else {
        vec![]
    };
    serde_json::json!({
        "problem": report.summary,
        "steps_attempted": steps,
        "last_error": last_error,
        "failure_kind": report.end_reason.map(|r| r.as_str()),
        "suggested_next_steps": suggested,
    })
}

/// title 单独合成（评审 F-C2 + 外部声音 #13 附带：synthesize_summary 以
/// "触发:…" 开头且带换行，不能直接截断 problem）。取 summary 首行去前缀，
/// 80 字符截断。
pub(crate) fn synthesize_title(report: &RunReport) -> String {
    let first_line = report.summary.lines().next().unwrap_or("自治任务失败");
    let stripped = first_line
        .strip_prefix("触发: ")
        .or_else(|| first_line.strip_prefix("触发:"))
        .unwrap_or(first_line);
    const MAX: usize = 80;
    if stripped.chars().count() > MAX {
        let truncated: String = stripped.chars().take(MAX).collect();
        format!("{truncated}…")
    } else {
        stripped.to_string()
    }
}

/// 单个 run 的开票路径（事件驱动与对账共用，幂等）。T4：创建走 ticket 域
/// 统一入口 create_escalation（D6），DLQ 兜底留在本路径（run 上下文只有这里有）。
pub(crate) async fn ticket_for_run(
    db: &Db,
    sse: &SseConnectionManager,
    report: &RunReport,
    problem_key: Option<&str>,
    dedup_key: Option<&str>,
) {
    let created = crate::domains::ticket::create_escalation(
        db,
        sse,
        crate::domains::ticket::Escalation {
            workspace_id: report.workspace_id.clone(),
            thing_id: report.thing_id.clone(),
            agent_run_id: report.run_id.clone(),
            title: synthesize_title(report),
            briefing: build_briefing(report),
            failure_hash: failure_hash(report, problem_key, dedup_key),
        },
    )
    .await;
    if created.is_none() {
        // 创建失败必须可见（Success Criteria #5）：error 日志已在入口内，DLQ 持久兜底。
        enqueue_dlq(db, report, "ticket creation failed").await;
    }
}

async fn enqueue_dlq(db: &Db, report: &RunReport, reason: &str) {
    let dlq = super::dlq_repo::SqliteDeadLetterQueue::new(db.pool().clone());
    use tinyiothub_agent::runtime::event::dlq::DeadLetterQueue;
    let payload = serde_json::json!({
        "run_id": report.run_id,
        "outcome": report.outcome.as_str(),
        "summary": report.summary,
    });
    if let Err(e) = dlq
        .enqueue(
            &report.workspace_id,
            "ticket_creation_failed",
            &payload.to_string(),
            reason,
        )
        .await
    {
        error!(error = %e, "DLQ enqueue failed (ticket creation failure now only in logs)");
    }
}

/// 对账扫描：补开漏掉的票（Lagged 恢复 + 周期对账共用）。
async fn resync(db: &Db, sse: &SseConnectionManager) {
    let since = (chrono::Utc::now() - chrono::Duration::hours(RECONCILE_LOOKBACK_HOURS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    match db.unticketed_failed_runs(&since).await {
        Ok(reports) => {
            if !reports.is_empty() {
                info!(count = reports.len(), "ticket resync: backfilling missed runs");
            }
            for report in reports {
                // 对账无事件载荷上下文，problem_key/dedup_key 从 report 不可得
                // ——走 hash 回退；重放撞唯一索引转追加，幂等。
                ticket_for_run(db, sse, &report, None, None).await;
            }
        }
        Err(e) => error!(error = %e, "ticket resync scan failed"),
    }
}

/// 主循环（测试接缝；生产经 [`supervise_ticket_subscriber`]）。
pub async fn run_ticket_subscriber(
    mut rx: Receiver<AgentEvent>,
    db: Arc<Db>,
    sse: Arc<SseConnectionManager>,
    shutdown: CancellationToken,
) {
    let mut reconcile = tokio::time::interval(RECONCILE_INTERVAL);
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                info!("ticket subscriber shutting down");
                break;
            }
            _ = reconcile.tick() => {
                resync(&db, &sse).await;
            }
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        if let AgentEventKind::RunRecorded { report, problem_key, dedup_key } = ev.kind
                            && should_ticket(report.outcome)
                        {
                            ticket_for_run(&db, &sse, &report, problem_key.as_deref(), dedup_key.as_deref()).await;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        warn!(missed = n, "ticket subscriber lagged — resyncing");
                        resync(&db, &sse).await;
                    }
                    Err(RecvError::Closed) => {
                        error!("agent event bus closed — ticket subscriber exiting");
                        break;
                    }
                }
            }
        }
    }
}

/// 生产入口：监管重启循环（对齐 persist.rs 的 supervise_persistence_subscriber
/// 语义——panic/退出不静默，error! + 退避重启）。`first_rx` 是调用方在
/// runtime restore 前取得的 receiver（不丢启动窗口事件），首个循环使用；
/// 重启后经 `rx_factory` 重取。
pub async fn supervise_ticket_subscriber(
    first_rx: Receiver<AgentEvent>,
    rx_factory: impl Fn() -> Receiver<AgentEvent> + Send + 'static,
    db: Arc<Db>,
    sse: Arc<SseConnectionManager>,
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
        let handle = tokio::spawn(async move {
            run_ticket_subscriber(rx, db2, sse2, token).await;
        });
        match handle.await {
            Ok(()) => {
                // 正常退出（shutdown 或 bus closed）——shutdown 则结束，否则重启。
                if shutdown.is_cancelled() {
                    break;
                }
                error!("ticket subscriber exited unexpectedly — restarting");
            }
            Err(e) => {
                error!(error = %e, "ticket subscriber panicked — restarting");
            }
        }
        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}
