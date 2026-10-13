//! Shadow observer（T13）——双跑期等价性的生产观测面（spec「比对期 N=7 天
//! 或 M=50 次 tick」）。
//!
//! 订阅同一 AgentEventBus，同时收：
//! - `HeartbeatResultReady`（旧路径：HeartbeatRunner tick 结果）；
//! - `RunRecorded` 且 trigger 前缀 `heartbeat:`（新路径：统一循环报告）。
//!
//! 按 workspace 配对（各存最新一条，成对即比对并清空），比对**结构白名单**：
//! status 映射 / proposals 工具名集合 / trace 工具名序列。不一致打 `warn!`
//! （带字段级 diff）并递增 `Metrics::shadow_mismatches` 计数。
//!
//! 真实 LLM 下两路径输出天然有方差——summary 文本只记长度级差异提示，不做
//! 逐字断言（逐字等价由 Task 14 的确定性 harness 负责）。observer 是纯观测
//! 面：不消费、不阻断、不重试；Lagged 仅告警（配对窗口内残留槽位下次成对
//! 时自然消化）。

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use tinyiothub_agent::memory::metrics::Metrics;
use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventKind};
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::HeartbeatResult;

/// 每工作区配对槽：两侧各存最新一条，成对即比对并清空。乱序/漏事件
/// （Lagged）下配对精度天然降级——观测面语义，不追求精确对齐。
#[derive(Default)]
struct PairSlot {
    legacy: Option<Box<HeartbeatResult>>,
    unified: Option<Box<RunReport>>,
}

/// 字段级比对结果。`summary_len` 只作长度级差异提示，不参与 `is_match`
/// 判定（真实 LLM 下两路径文本天然有方差）。
#[derive(Debug)]
pub(crate) struct PairDiff {
    /// (旧路径, 新路径) status 映射不一致时的两侧值。
    pub(crate) status: Option<(String, String)>,
    /// proposals 工具名集合差异。
    pub(crate) proposals: Option<(BTreeSet<String>, BTreeSet<String>)>,
    /// trace 工具名序列差异（有序）。
    pub(crate) trace: Option<(Vec<String>, Vec<String>)>,
    /// summary 文本长度（旧, 新）——提示用，断言豁免。
    pub(crate) summary_len: (usize, usize),
}

impl PairDiff {
    pub(crate) fn is_match(&self) -> bool {
        self.status.is_none() && self.proposals.is_none() && self.trace.is_none()
    }
}

/// 结构白名单比对：status 映射（新路径 summary 内嵌 healing JSON 经
/// `parse_healing_report` 还原 HeartbeatStatus，与旧路径结果直接可比；
/// 纯文本 summary 回退 Error，与旧路径 LLM 失败 tick 同向）/ proposals
/// 工具名集合 / trace 工具名序列。pub(crate)：单测直接驱动。
pub(crate) fn compare_pair(legacy: &HeartbeatResult, unified: &RunReport) -> PairDiff {
    let parsed =
        tinyiothub_agent::runtime::heartbeat::report::parse_healing_report(&unified.summary, &unified.workspace_id);

    let status =
        (legacy.status != parsed.status).then(|| (format!("{:?}", legacy.status), format!("{:?}", parsed.status)));

    let legacy_tools: BTreeSet<String> = legacy.proposals.iter().map(|p| p.tool_name.clone()).collect();
    let unified_tools: BTreeSet<String> = parsed.proposals.iter().map(|p| p.tool_name.clone()).collect();
    let proposals = (legacy_tools != unified_tools).then_some((legacy_tools, unified_tools));

    let legacy_trace: Vec<String> = legacy.trace.iter().map(|t| t.name.clone()).collect();
    let unified_trace: Vec<String> = unified.trace.iter().map(|t| t.name.clone()).collect();
    let trace = (legacy_trace != unified_trace).then_some((legacy_trace, unified_trace));

    PairDiff {
        status,
        proposals,
        trace,
        summary_len: (legacy.summary.len(), parsed.summary.len()),
    }
}

/// shadow observer 主循环（T13 生产观测面）：restore 前取得的 receiver +
/// shutdown token，与 persist 订阅者同一接线形态。不一致打 warn!（字段级
/// diff）并递增 `Metrics::shadow_mismatches`。
pub async fn run_shadow_observer(
    mut rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    metrics: Arc<Metrics>,
    shutdown: CancellationToken,
) {
    let slots: DashMap<String, PairSlot> = DashMap::new();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(event) => on_event(&slots, &metrics, event),
                Err(RecvError::Lagged(n)) => {
                    warn!(dropped = n, "shadow observer lagged — pairing window partial");
                }
                Err(RecvError::Closed) => return,
            },
            _ = shutdown.cancelled() => return,
        }
    }
}

fn on_event(slots: &DashMap<String, PairSlot>, metrics: &Metrics, event: AgentEvent) {
    let (workspace_id, slot_update) = match event.kind {
        AgentEventKind::HeartbeatResultReady { result } => {
            let ws = result.workspace_id.clone();
            (ws, SlotUpdate::Legacy(result))
        }
        AgentEventKind::RunRecorded { report, .. } => {
            // 只配对 heartbeat 源 run（trigger 前缀 heartbeat:，含告警分键）。
            if tinyiothub_storage::agent_runs::trigger_type_of(&report.trigger) != "heartbeat" {
                return;
            }
            let ws = report.workspace_id.clone();
            (ws, SlotUpdate::Unified(report))
        }
        _ => return,
    };

    let pair = {
        let mut slot = slots.entry(workspace_id.clone()).or_default();
        match slot_update {
            SlotUpdate::Legacy(result) => slot.legacy = Some(result),
            SlotUpdate::Unified(report) => slot.unified = Some(report),
        }
        // 成对即取出比对并清空槽位（下一 tick 重新配对）。
        match (slot.legacy.take(), slot.unified.take()) {
            (Some(legacy), Some(unified)) => Some((legacy, unified)),
            (legacy, unified) => {
                slot.legacy = legacy;
                slot.unified = unified;
                None
            }
        }
    };

    if let Some((legacy, unified)) = pair {
        let diff = compare_pair(&legacy, &unified);
        if !diff.is_match() {
            metrics.shadow_mismatches.fetch_add(1, Ordering::Relaxed);
            warn!(
                workspace_id = %workspace_id,
                tick_id = %legacy.id,
                run_id = %unified.run_id,
                status_diff = ?diff.status,
                proposals_diff = ?diff.proposals,
                trace_diff = ?diff.trace,
                legacy_summary_len = diff.summary_len.0,
                unified_summary_len = diff.summary_len.1,
                "shadow observer: heartbeat dual-run mismatch"
            );
        }
    }
}

enum SlotUpdate {
    Legacy(Box<HeartbeatResult>),
    Unified(Box<RunReport>),
}

#[cfg(test)]
mod tests {
    use super::*;

    use tinyiothub_agent::runtime::events::AgentEventBus;
    use tinyiothub_core::agent_runs::{Outcome, ToolTraceEntry};
    use tinyiothub_core::heartbeat::HeartbeatStatus;
    use tinyiothub_core::policy::Proposal;

    fn proposal(ws: &str, tool: &str) -> Proposal {
        Proposal {
            id: format!("p-{tool}"),
            workspace_id: ws.into(),
            agent_id: String::new(),
            tool_name: tool.into(),
            thing_id: None,
            summary: "s".into(),
            reason: "r".into(),
            risk: "low".into(),
            parameters: None,
            created_at: "2026-10-12T00:00:00Z".into(),
            status: tinyiothub_core::policy::ProposalStatus::Pending,
        }
    }

    fn trace_entry(name: &str) -> ToolTraceEntry {
        ToolTraceEntry {
            id: format!("c-{name}"),
            name: name.into(),
            args: serde_json::json!({}),
            output: Some("ok".into()),
        }
    }

    /// 旧路径 tick 结果（HeartbeatResultReady 载荷）。
    fn legacy(ws: &str, status: HeartbeatStatus, proposals: &[&str], trace: &[&str], summary: &str) -> HeartbeatResult {
        HeartbeatResult {
            id: format!("tick-{ws}"),
            workspace_id: ws.into(),
            status,
            summary: summary.into(),
            task_count: 1,
            executed_actions: vec![],
            proposals: proposals.iter().map(|t| proposal(ws, t)).collect(),
            error: None,
            trace: trace.iter().map(|n| trace_entry(n)).collect(),
        }
    }

    /// 新路径 run 报告（RunRecorded 载荷）——summary 内嵌 healing report
    /// JSON（与 persist 心跳支路同一解析输入），trace 为框架侧工具记录。
    fn unified(ws: &str, status_json: &str, proposals: &[&str], trace: &[&str], summary: &str) -> RunReport {
        let healing = serde_json::json!({
            "status": status_json,
            "summary": summary,
            "executed_actions": [],
            "proposals": proposals
                .iter()
                .map(|t| serde_json::json!({"tool_name": t, "summary": "s", "reason": "r"}))
                .collect::<Vec<_>>(),
        });
        RunReport {
            run_id: format!("run-{ws}"),
            workspace_id: ws.into(),
            trigger: format!("heartbeat:{ws}"),
            outcome: Outcome::Acted,
            summary: healing.to_string(),
            actions: vec![],
            verified: true,
            duration_ms: 10,
            tool_calls: trace.len() as u32,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: trace.iter().map(|n| trace_entry(n)).collect(),
            task_count: Some(1),
        }
    }

    // ── compare_pair 白名单比对 ────────────────────────────────

    /// 结构一致 → 无 diff；summary 长度差只进提示，不判不一致。
    #[test]
    fn matching_pair_produces_no_diff_despite_summary_length_delta() {
        let old = legacy(
            "ws1",
            HeartbeatStatus::Complete,
            &["write_properties"],
            &["read_property"],
            "短",
        );
        let new = unified(
            "ws1",
            "complete",
            &["write_properties"],
            &["read_property"],
            "明显更长的一段总结文本",
        );
        let diff = compare_pair(&old, &new);
        assert!(diff.is_match(), "结构一致须判 match: {diff:?}");
        assert_ne!(diff.summary_len.0, diff.summary_len.1, "长度级提示仍须记录");
    }

    /// status 映射：新路径 summary 内嵌 JSON 的 status（经 parse_healing_report
    /// 还原 HeartbeatStatus）与旧路径 HeartbeatResult.status 比对。
    #[test]
    fn status_mismatch_reported_with_both_values() {
        let old = legacy("ws1", HeartbeatStatus::Complete, &[], &[], "s");
        let new = unified("ws1", "error", &[], &[], "s");
        let diff = compare_pair(&old, &new);
        assert!(!diff.is_match());
        let (l, u) = diff.status.clone().expect("status diff");
        assert_eq!(l, "Complete");
        assert_eq!(u, "Error");
    }

    /// proposals 比对工具名集合（无序、去重）。
    #[test]
    fn proposals_tool_set_mismatch_reported() {
        let old = legacy(
            "ws1",
            HeartbeatStatus::Complete,
            &["write_properties", "reboot"],
            &[],
            "s",
        );
        let new = unified("ws1", "complete", &["write_properties"], &[], "s");
        let diff = compare_pair(&old, &new);
        assert!(!diff.is_match());
        let (l, u) = diff.proposals.clone().expect("proposals diff");
        assert!(l.contains("reboot") && !u.contains("reboot"));
    }

    /// proposals 顺序不同不算不一致（集合语义）。
    #[test]
    fn proposals_order_difference_is_not_a_mismatch() {
        let old = legacy("ws1", HeartbeatStatus::Complete, &["a", "b"], &[], "s");
        let new = unified("ws1", "complete", &["b", "a"], &[], "s");
        assert!(compare_pair(&old, &new).is_match());
    }

    /// trace 比对工具名序列（有序）。
    #[test]
    fn trace_tool_sequence_mismatch_reported() {
        let old = legacy(
            "ws1",
            HeartbeatStatus::Complete,
            &[],
            &["read_property", "write_properties"],
            "s",
        );
        let new = unified("ws1", "complete", &[], &["write_properties", "read_property"], "s");
        let diff = compare_pair(&old, &new);
        assert!(!diff.is_match(), "trace 是序列比对——顺序不同即不一致");
        assert!(diff.trace.is_some());
    }

    /// 新路径 summary 非 healing JSON（如 agent 不可用早退的纯文本）→
    /// parse 回退 Error，与旧路径 Error tick 配对一致。
    #[test]
    fn unparseable_unified_summary_maps_to_error_status() {
        let old = legacy("ws1", HeartbeatStatus::Error, &[], &[], "s");
        let mut new = unified("ws1", "complete", &[], &[], "s");
        new.summary = "自治 Agent 不可用（connection refused），run 未执行即中止".to_string();
        assert!(compare_pair(&old, &new).is_match());
    }

    // ── run_shadow_observer 配对循环 ──────────────────────────

    /// 成对即比对：不一致递增 shadow_mismatches；一致不递增；非 heartbeat
    /// 触发源的 RunRecorded 不参与配对。
    #[tokio::test]
    async fn observer_pairs_by_workspace_and_counts_mismatches() {
        let bus = Arc::new(AgentEventBus::new(64));
        let rx = bus.subscribe();
        let metrics = Arc::new(Metrics::new());
        let shutdown = tokio_util::sync::CancellationToken::new();
        let handle = tokio::spawn(run_shadow_observer(rx, metrics.clone(), shutdown.clone()));

        // 非 heartbeat 的 run：不得进入配对。
        let mut timer_run = unified("ws1", "error", &["ghost"], &["ghost"], "s");
        timer_run.trigger = "timer:ws1".to_string();
        bus.emit(AgentEventKind::RunRecorded {
            report: Box::new(timer_run),
            problem_key: None,
            dedup_key: None,
        });

        // 一致对：不计数。
        bus.emit(AgentEventKind::HeartbeatResultReady {
            result: Box::new(legacy(
                "ws1",
                HeartbeatStatus::Complete,
                &["write_properties"],
                &[],
                "s",
            )),
        });
        bus.emit(AgentEventKind::RunRecorded {
            report: Box::new(unified("ws1", "complete", &["write_properties"], &[], "s")),
            problem_key: None,
            dedup_key: None,
        });

        // 不一致对（status）：计数 +1。
        bus.emit(AgentEventKind::HeartbeatResultReady {
            result: Box::new(legacy("ws1", HeartbeatStatus::Complete, &[], &[], "s")),
        });
        bus.emit(AgentEventKind::RunRecorded {
            report: Box::new(unified("ws1", "error", &[], &[], "s")),
            problem_key: None,
            dedup_key: None,
        });

        for _ in 0..20_000 {
            if metrics.shadow_mismatches.load(Ordering::Relaxed) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            metrics.shadow_mismatches.load(Ordering::Relaxed),
            1,
            "只有不一致对计数；timer run 与一致对不计"
        );

        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("observer exits on shutdown")
            .expect("observer task");
    }

    /// 单边事件不成对不比对（旧路径先到的结果等待新路径报告）。
    #[tokio::test]
    async fn single_sided_events_do_not_compare() {
        let bus = Arc::new(AgentEventBus::new(64));
        let rx = bus.subscribe();
        let metrics = Arc::new(Metrics::new());
        let shutdown = tokio_util::sync::CancellationToken::new();
        let handle = tokio::spawn(run_shadow_observer(rx, metrics.clone(), shutdown.clone()));

        bus.emit(AgentEventKind::HeartbeatResultReady {
            result: Box::new(legacy("ws1", HeartbeatStatus::Error, &["ghost"], &["ghost"], "s")),
        });
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        assert_eq!(metrics.shadow_mismatches.load(Ordering::Relaxed), 0);

        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("observer exits")
            .expect("observer task");
    }
}
