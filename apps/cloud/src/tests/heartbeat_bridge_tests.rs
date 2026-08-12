//! X6 心跳桥 dedup 集成测试（T18）——真实 SQLite（thing_agent_loop 迁移）
//! + 真实 `SqliteAgentRunsRepository` + mock `DirectiveSink`。
//!
//! 覆盖 O11 规则：全 outcome 矩阵、窗口内计数（acted+未 verified 仅放行
//! 一次）、超 6h 复发放行、ack 抑制 7 天（6h 内/6h 外/超 7 天三档）、无
//! proposals 不投递、心跳 directive 形态（Normal / source=heartbeat /
//! 不参与合并）。

use std::sync::{Arc, Mutex};

use crate::domains::agent::loop_::{
    heartbeat::types::{HeartbeatResult, HeartbeatStatus},
    orchestrator::callbacks::HeartbeatBridge,
    thing_agent::{AgentRunsRepository, DirectiveSink, EnqueueError, Priority, TriggerSource, WakeSignal},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

const WS: &str = "ws_bridge";

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("create in-memory sqlite");
    let migration = include_str!("../../../../crates/db/migrations/20260729000001_thing_agent_loop.sql");
    for stmt in migration.split(';') {
        let stmt = stmt.trim();
        // Skip the events ALTER — the events table is not part of this pool.
        if !stmt.is_empty() && !stmt.starts_with("ALTER TABLE") {
            sqlx::query(stmt).execute(&pool).await.expect("apply migration");
        }
    }
    pool
}

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

/// 插入一条指定年龄/结果的 run；acked=true 时同时写 acked_at/acked_by。
async fn insert_run(
    pool: &SqlitePool,
    id: &str,
    outcome: &str,
    verified: bool,
    acked: bool,
    problem_key: &str,
    age_modifier: &str,
) {
    sqlx::query(
        "INSERT INTO agent_runs
             (id, workspace_id, trigger_type, outcome, summary, report, verified,
              tokens, problem_key, created_at, acked_at, acked_by)
         VALUES (?, ?, 'user', ?, ?, '{}', ?, 0, ?, datetime('now', ?),
                 CASE WHEN ? THEN datetime('now') ELSE NULL END,
                 CASE WHEN ? THEN 'u1' ELSE NULL END)",
    )
    .bind(id)
    .bind(WS)
    .bind(outcome)
    .bind(format!("summary of {id}"))
    .bind(verified)
    .bind(problem_key)
    .bind(age_modifier)
    .bind(acked)
    .bind(acked)
    .execute(pool)
    .await
    .expect("insert agent_run");
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

fn bridge(pool: &SqlitePool) -> (HeartbeatBridge, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let repo: Arc<AgentRunsRepository> =
        Arc::new(tinyiothub_storage::agent_runs::AgentRunsRepository::new(pool.clone()));
    (HeartbeatBridge::new(repo, sink.clone()), sink)
}

/// problem_key 为 `tool:dev-1` 的提案经 dedup 后是否投递。
async fn dispatched_count(pool: &SqlitePool, tool: &str) -> usize {
    let (bridge, sink) = bridge(pool);
    bridge
        .dispatch_proposals(WS, &result_with(vec![proposal(tool, Some("dev-1"))]))
        .await;
    sink.signals.lock().unwrap().len()
}

// O11 全 outcome 矩阵（真实 DB）：窗口内最近一次 run 决定抑制/放行。
#[tokio::test]
async fn outcome_matrix_against_real_db() {
    let pool = test_pool().await;

    for (tool, outcome, verified, expect_dispatch) in [
        ("t_failed", "failed", false, false),
        ("t_rejected", "rejected", false, false),
        ("t_budget", "budget_exceeded", false, false),
        ("t_noaction", "no_action_needed", false, false),
        ("t_acted_verified", "acted", true, false),
        ("t_acted_unverified", "acted", false, true), // 窗口内仅 1 次 → 放行一次
    ] {
        let key = format!("{tool}:dev-1");
        insert_run(
            &pool,
            &format!("run_{tool}"),
            outcome,
            verified,
            false,
            &key,
            "-1 hours",
        )
        .await;
        assert_eq!(
            dispatched_count(&pool, tool).await,
            usize::from(expect_dispatch),
            "{outcome} (verified={verified}) dispatch expectation"
        );
    }
}

// acted+未 verified：窗口内仅放行一次重试，第二次起跳过（窗口内计数）。
#[tokio::test]
async fn acted_unverified_retry_only_once_against_real_db() {
    let pool = test_pool().await;
    let key = "set_hvac:dev-1";

    insert_run(&pool, "r1", "acted", false, false, key, "-1 hours").await;
    assert_eq!(dispatched_count(&pool, "set_hvac").await, 1, "first retry allowed");

    insert_run(&pool, "r2", "acted", false, false, key, "-30 minutes").await;
    assert_eq!(
        dispatched_count(&pool, "set_hvac").await,
        0,
        "two acted+unverified runs in window suppress the second retry"
    );
}

// 超 6h 旧 Run 不抑制：7h 前 acted+verified 的问题复发 → 放行。
#[tokio::test]
async fn recurrence_beyond_6h_dispatches_against_real_db() {
    let pool = test_pool().await;
    insert_run(&pool, "old", "acted", true, false, "set_hvac:dev-1", "-7 hours").await;
    assert_eq!(dispatched_count(&pool, "set_hvac").await, 1);
}

// ack 抑制 7 天（真实 DB）：6h 内 acked → 跳；6h 外 7 天内 acked → 跳；
// 超 7 天 acked → 抑制过期放行。
#[tokio::test]
async fn ack_suppression_windows_against_real_db() {
    let pool = test_pool().await;

    insert_run(&pool, "ack_1h", "acted", true, true, "k1:dev-1", "-1 hours").await;
    assert_eq!(dispatched_count(&pool, "k1").await, 0, "acked within 6h suppressed");

    insert_run(&pool, "ack_3d", "acted", true, true, "k2:dev-1", "-3 days").await;
    assert_eq!(dispatched_count(&pool, "k2").await, 0, "acked within 7d suppressed");

    insert_run(&pool, "ack_8d", "acted", true, true, "k3:dev-1", "-8 days").await;
    assert_eq!(
        dispatched_count(&pool, "k3").await,
        1,
        "ack older than 7d no longer suppresses"
    );
}

// HeartbeatCompleted 无 proposals → 不投递。
#[tokio::test]
async fn no_proposals_dispatches_nothing_against_real_db() {
    let pool = test_pool().await;
    let (bridge, sink) = bridge(&pool);
    bridge.dispatch_proposals(WS, &result_with(vec![])).await;
    assert!(sink.signals.lock().unwrap().is_empty());
}

// 心跳 directive 形态（O5/O24）：Normal、source=Some("heartbeat")、
// problem_key 随指令落库、dedup_key=None 不参与合并。
#[tokio::test]
async fn heartbeat_directive_shape_against_real_db() {
    let pool = test_pool().await;
    let (bridge, sink) = bridge(&pool);
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
