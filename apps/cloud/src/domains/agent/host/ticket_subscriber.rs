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
///
/// 2026-09-27 起：框架合成 summary 的首行剥掉「触发:」后可能只剩裸触发
/// 标签（user:heartbeat / timer:… / thing:…:event:… / merged:…）——对人
/// 零信息量（实测工单列表整列 "user:heartbeat"）。裸标签时改用
/// 「触发源人话 + 失败细节」合成。
pub(crate) fn synthesize_title(report: &RunReport) -> String {
    let first_line = report.summary.lines().next().unwrap_or("自治任务失败");
    let stripped = first_line
        .strip_prefix("触发: ")
        .or_else(|| first_line.strip_prefix("触发:"))
        .unwrap_or(first_line);
    if is_bare_trigger_label(stripped) {
        return truncate_title(&format!(
            "{}失败: {}",
            describe_trigger(stripped),
            failure_detail(report)
        ));
    }
    truncate_title(stripped)
}

fn truncate_title(s: &str) -> String {
    const MAX: usize = 80;
    if s.chars().count() > MAX {
        let truncated: String = s.chars().take(MAX).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

/// trigger_label 的产出形态（thing_agent/manager.rs）——裸标签对人类零
/// 信息量。仅匹配标签形态，LLM 原文首行不受影响。
fn is_bare_trigger_label(s: &str) -> bool {
    s.starts_with("user:")
        || s.starts_with("timer:")
        || s.starts_with("merged:")
        || (s.starts_with("thing:") && s.contains(":event:"))
}

/// 触发标签人话化（仅工单 title 展示用；trigger_type 解析不经过这里）。
fn describe_trigger(label: &str) -> String {
    if label == "user:heartbeat" {
        "心跳巡检".to_string()
    } else if let Some(user) = label.strip_prefix("user:") {
        format!("用户指令({user})")
    } else if label.starts_with("timer:") {
        "定时巡检".to_string()
    } else if let Some(rest) = label.strip_prefix("thing:") {
        format!("设备事件({rest})")
    } else if label.starts_with("merged:") {
        "合并信号".to_string()
    } else {
        label.to_string()
    }
}

/// 失败细节：首个失败动作优先，其次 end_reason 人话。
fn failure_detail(report: &RunReport) -> String {
    /// 错误文本截断上限——必须明显小于标题上限（80），给触发源前缀留位置。
    const DETAIL_MAX: usize = 40;
    for a in &report.actions {
        if let ActionResult::Failed(e) = &a.result {
            let short: String = e.chars().take(DETAIL_MAX).collect();
            return format!("{}.{} 失败（{}）", a.thing_id, a.action_name, short);
        }
    }
    let reason = match report.end_reason {
        Some(tinyiothub_core::agent_runs::EndReason::Budget) => "预算超限",
        Some(tinyiothub_core::agent_runs::EndReason::Policy) => "策略拒绝",
        Some(tinyiothub_core::agent_runs::EndReason::Llm) => "LLM 错误",
        Some(tinyiothub_core::agent_runs::EndReason::Timeout) => "执行超时",
        Some(tinyiothub_core::agent_runs::EndReason::Tool) => "工具执行失败",
        Some(tinyiothub_core::agent_runs::EndReason::AgentUnavailable) => "Agent 不可用",
        None => "未执行动作",
    };
    reason.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::agent_runs::{ActionRecord, EndReason};

    fn report(summary: &str, end_reason: Option<EndReason>, actions: Vec<ActionRecord>) -> RunReport {
        RunReport {
            run_id: "run_1".into(),
            workspace_id: "ws".into(),
            trigger: "user:heartbeat".into(),
            outcome: Outcome::Failed,
            summary: summary.to_string(),
            actions,
            verified: false,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
            end_reason,
            thing_id: None,
        }
    }

    fn failed_action(thing: &str, action: &str, err: &str) -> ActionRecord {
        ActionRecord {
            thing_id: thing.into(),
            action_name: action.into(),
            params: serde_json::json!({}),
            result: ActionResult::Failed(err.to_string()),
            verified: false,
        }
    }

    /// 回归（2026-09-27 实测）：心跳桥 run 的 trigger 标签是
    /// "user:heartbeat"，框架合成 summary 首行剥前缀后只剩裸标签，
    /// 工单列表整列 "user:heartbeat"——对人零信息量。裸标签必须改用
    /// 触发源人话 + 失败细节合成。
    #[test]
    fn heartbeat_bare_label_title_describes_failure() {
        let r = report(
            "触发: user:heartbeat\n动作: dev_1.reboot 失败: device offline\nllm error",
            Some(EndReason::Tool),
            vec![failed_action("dev_1", "reboot", "device offline")],
        );
        let title = synthesize_title(&r);
        assert!(title.contains("心跳巡检"), "触发源人话: {title}");
        assert!(title.contains("dev_1.reboot"), "失败动作: {title}");
        assert!(!title.contains("user:heartbeat"), "裸标签不得上标题: {title}");
    }

    #[test]
    fn bare_label_without_actions_uses_end_reason() {
        let r = report("触发: user:heartbeat\n动作:\nbudget", Some(EndReason::Budget), vec![]);
        let title = synthesize_title(&r);
        assert_eq!(title, "心跳巡检失败: 预算超限");
    }

    #[test]
    fn thing_event_label_title_humanized() {
        let r = report(
            "触发: thing:dev_1:event:offline\n动作:\n",
            Some(EndReason::Timeout),
            vec![],
        );
        let title = synthesize_title(&r);
        assert!(title.starts_with("设备事件(dev_1:event:offline)失败"), "{title}");
    }

    #[test]
    fn llm_text_first_line_used_verbatim() {
        // Rejected/正常结束的 summary 是 LLM 原文——首行直接做标题（原有行为）。
        let r = report("温度超限，建议检查冷机\n第二行", None, vec![]);
        assert_eq!(synthesize_title(&r), "温度超限，建议检查冷机");
    }
}
