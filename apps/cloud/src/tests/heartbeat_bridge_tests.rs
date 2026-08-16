//! X6 心跳桥 dedup 集成测试（T18 / Task 6 内存化）—— RunRegistry 内存
//! dedup 真源 + mock `DirectiveSink`。
//!
//! 覆盖 O11 规则：全 outcome 矩阵、窗口内计数（acted+未 verified 仅放行
//! 一次）、超 6h 复发放行、ack 抑制 7 天（6h 内/6h 外/超 7 天三档）、无
//! proposals 不投递、心跳 directive 形态（Normal / source=heartbeat /
//! 不参与合并）。
//!
//! Task 6 起 dedup 依据从 `AgentRunsRepository` SQL 查询迁移到
//! `RunRegistry` 的 problem_key 元数据映射（等价性论证见 registry.rs
//! 模块文档）；DB 落库由 Task 8 的 RunRecorded 订阅者承接。

use std::sync::{Arc, Mutex};

use crate::domains::agent::loop_::{
    heartbeat::types::{HeartbeatResult, HeartbeatStatus},
    orchestrator::callbacks::HeartbeatBridge,
    thing_agent::{DirectiveSink, EnqueueError, Priority, TriggerSource, WakeSignal},
    thing_agent::registry::RunRegistry,
    thing_agent::types::Outcome,
};
use chrono::Utc;
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

const WS: &str = "ws_bridge";

#[derive(Default)]
struct RecordingSink {
    signals: Mutex<Vec<WakeSignal>>,
}

impl DirectiveSink for RecordingSink {
    fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
        self.signals.lock().unwrap().push(signal);
        Ok(())
    }
}

/// 写入一条指定年龄/结果的 problem run（显式时间戳，无 I/O）。
fn record_run(
    registry: &RunRegistry,
    outcome: Outcome,
    verified: bool,
    problem_key: &str,
    age: chrono::Duration,
) {
    registry.record_problem_run(WS, problem_key, outcome, verified, Utc::now() - age);
}

fn proposal(tool_name: &str, device_id: Option<&str>) -> Proposal {
    Proposal {
        id: "p1".into(),
        workspace_id: WS.into(),
        agent_id: "hb".into(),
        tool_name: tool_name.into(),
        device_id: device_id.map(str::to_string),
        summary: "车间温度超过阈值".into(),
        reason: "连续采样超限".into(),
        risk: "medium".into(),
        parameters: None,
        created_at: "2026-08-03T00:00:00Z".into(),
        status: ProposalStatus::Pending,
    }
}

fn result_with(proposals: Vec<Proposal>) -> HeartbeatResult {
    HeartbeatResult {
        workspace_id: WS.into(),
        status: HeartbeatStatus::Complete,
        summary: "tick done".into(),
        task_count: 1,
        executed_actions: vec![],
        proposals,
        error: None,
    }
}

fn bridge(registry: RunRegistry) -> (HeartbeatBridge, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    (HeartbeatBridge::new(registry, sink.clone()), sink)
}

/// problem_key 为 `{tool}:dev-1` 的提案经 dedup 后是否投递。
async fn dispatched_count(registry: RunRegistry, tool: &str) -> usize {
    let (bridge, sink) = bridge(registry);
    bridge
        .dispatch_proposals(WS, &result_with(vec![proposal(tool, Some("dev-1"))]))
        .await;
    sink.signals.lock().unwrap().len()
}

// O11 全 outcome 矩阵：窗口内最近一次 run 决定抑制/放行。
#[tokio::test]
async fn outcome_matrix_against_in_memory_dedup() {
    for (tool, outcome, verified, expect_dispatch) in [
        ("t_failed", Outcome::Failed, false, false),
        ("t_rejected", Outcome::Rejected, false, false),
        ("t_budget", Outcome::BudgetExceeded, false, false),
        ("t_noaction", Outcome::NoActionNeeded, false, false),
        ("t_acted_verified", Outcome::Acted, true, false),
        ("t_acted_unverified", Outcome::Acted, false, true), // 窗口内仅 1 次 → 放行一次
    ] {
        let registry = RunRegistry::new();
        let key = format!("{tool}:dev-1");
        record_run(&registry, outcome, verified, &key, chrono::Duration::hours(1));
        assert_eq!(
            dispatched_count(registry, tool).await,
            usize::from(expect_dispatch),
            "{outcome:?} (verified={verified}) dispatch expectation"
        );
    }
}

// acted+未 verified：窗口内仅放行一次重试，第二次起跳过（窗口内计数）。
#[tokio::test]
async fn acted_unverified_retry_only_once_in_memory() {
    let registry = RunRegistry::new();
    let key = "set_hvac:dev-1";

    record_run(&registry, Outcome::Acted, false, key, chrono::Duration::hours(1));
    assert_eq!(dispatched_count(registry, "set_hvac").await, 1, "first retry allowed");

    let registry = RunRegistry::new();
    record_run(&registry, Outcome::Acted, false, key, chrono::Duration::hours(2));
    record_run(&registry, Outcome::Acted, false, key, chrono::Duration::minutes(30));
    assert_eq!(
        dispatched_count(registry, "set_hvac").await,
        0,
        "two acted+unverified runs in window suppress the second retry"
    );
}

// 超 6h 旧 Run 不抑制：7h 前 acted+verified 的问题复发 → 放行。
#[tokio::test]
async fn recurrence_beyond_6h_dispatches_in_memory() {
    let registry = RunRegistry::new();
    record_run(&registry, Outcome::Acted, true, "set_hvac:dev-1", chrono::Duration::hours(7));
    assert_eq!(dispatched_count(registry, "set_hvac").await, 1);
}

// ack 抑制 7 天：6h 内 acked → 跳；6h 外 7 天内 acked → 跳；
// 超 7 天 acked → 抑制过期放行。
#[tokio::test]
async fn ack_suppression_windows_in_memory() {
    let registry = RunRegistry::new();
    record_run(&registry, Outcome::Acted, true, "k1:dev-1", chrono::Duration::hours(1));
    registry.mark_problem_acked(WS, "k1:dev-1", Utc::now());
    assert_eq!(dispatched_count(registry, "k1").await, 0, "acked within 6h suppressed");

    let registry = RunRegistry::new();
    record_run(&registry, Outcome::Acted, true, "k2:dev-1", chrono::Duration::days(3));
    registry.mark_problem_acked(WS, "k2:dev-1", Utc::now() - chrono::Duration::days(2));
    assert_eq!(dispatched_count(registry, "k2").await, 0, "acked within 7d suppressed");

    let registry = RunRegistry::new();
    record_run(&registry, Outcome::Acted, true, "k3:dev-1", chrono::Duration::days(8));
    registry.mark_problem_acked(WS, "k3:dev-1", Utc::now() - chrono::Duration::days(8) + chrono::Duration::hours(1));
    assert_eq!(
        dispatched_count(registry, "k3").await,
        1,
        "ack older than 7d no longer suppresses"
    );
}

// HeartbeatCompleted 无 proposals → 不投递。
#[tokio::test]
async fn no_proposals_dispatches_nothing_in_memory() {
    let (bridge, sink) = bridge(RunRegistry::new());
    bridge.dispatch_proposals(WS, &result_with(vec![])).await;
    assert!(sink.signals.lock().unwrap().is_empty());
}

// 心跳 directive 形态（O5/O24）：Normal、source=Some("heartbeat")、
// problem_key 随指令携带、dedup_key=None 不参与合并。
#[tokio::test]
async fn heartbeat_directive_shape_in_memory() {
    let (bridge, sink) = bridge(RunRegistry::new());
    bridge
        .dispatch_proposals(WS, &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
        .await;

    let signals = sink.signals.lock().unwrap();
    assert_eq!(signals.len(), 1);
    let sig = &signals[0];
    assert_eq!(sig.priority, Priority::Normal);
    assert_eq!(sig.dedup_key, None);
    match &sig.source {
        TriggerSource::UserDirective {
            user_id,
            text,
            source,
            problem_key,
            ..
        } => {
            assert_eq!(user_id, "heartbeat");
            assert_eq!(source.as_deref(), Some("heartbeat"));
            assert_eq!(problem_key.as_deref(), Some("set_hvac:dev-1"));
            assert!(text.contains("set_hvac:dev-1"));
            assert!(text.contains("请诊断并处置"));
        }
        other => panic!("expected UserDirective, got {other:?}"),
    }
}
