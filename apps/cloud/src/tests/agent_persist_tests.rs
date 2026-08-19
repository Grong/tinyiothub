//! Task 8 持久化订阅者测试：AgentEvent → DB 投影。
//!
//! 覆盖：RunRecorded 幂等投影、stale 回放 fencing（insert-once 幂等即
//! fencing）、Lagged → dump_state 全量 resync、周期全量对账、
//! HeartbeatResultReady → agent_actions 投影。

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use sqlx::SqlitePool;
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_core::heartbeat::{HeartbeatResult, HeartbeatStatus, TrustConfig};
use tinyiothub_storage::Db;

use crate::domains::agent::host::persist::{ResyncFailures, resync, run_persistence_loop, run_persistence_subscriber};
use crate::domains::agent::host::test_utils::seed_test_workspace;
use tinyiothub_agent::runtime::event::bus::AiEventPublisher;
use tinyiothub_agent::runtime::events::{AgentEventBus, AgentEventKind};
use tinyiothub_agent::runtime::runtime::{AgentRuntime, RuntimeDeps};
use tinyiothub_agent::runtime::snapshot::{RestoreSnapshot, WorkspaceHeartbeatState};

// ── fixtures ──────────────────────────────────────────────

/// 全量迁移的内存库（max_connections=1：:memory: 每连接独立）。
async fn test_db() -> (Arc<Db>, SqlitePool) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run migrations");
    (Arc::new(Db::new(pool.clone())), pool)
}

fn report(run_id: &str, workspace_id: &str, summary: &str) -> RunReport {
    RunReport {
        run_id: run_id.into(),
        workspace_id: workspace_id.into(),
        trigger: "timer:ws1".into(),
        outcome: Outcome::NoActionNeeded,
        summary: summary.into(),
        actions: vec![],
        verified: true,
        duration_ms: 10,
        tool_calls: 0,
        tokens: 0,
    }
}

fn empty_snapshot() -> RestoreSnapshot {
    RestoreSnapshot {
        heartbeat: vec![],
        recent_runs: vec![],
        problem_meta: vec![],
    }
}

fn runtime_with(capacity: usize, snapshot: RestoreSnapshot) -> Arc<AgentRuntime> {
    let mut deps = RuntimeDeps::test_stub();
    deps.agent_events = Arc::new(AgentEventBus::new(capacity));
    Arc::new(AgentRuntime::restore(snapshot, deps))
}

fn test_publisher() -> Arc<AiEventPublisher> {
    Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())))
}

async fn count_runs(pool: &SqlitePool, run_id: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("count runs");
    n
}

async fn wait_until<F, Fut>(mut cond: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..300 {
        if cond().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not met within 3s");
}

/// receiver 在 spawn 前由调用方创建（Task 9 起订阅不再发生于 spawned
/// task 内），但事件投影是异步的——RunRecorded 投影幂等，重发无害，
/// 发射直到落库为止。
async fn emit_run_until_persisted(runtime: &AgentRuntime, pool: &SqlitePool, rep: &RunReport) {
    for _ in 0..60 {
        runtime.bus().emit(AgentEventKind::RunRecorded {
            report: Box::new(rep.clone()),
            problem_key: None,
            dedup_key: None,
        });
        if count_runs(pool, &rep.run_id).await == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("run {} not persisted within 3s", rep.run_id);
}

// ── tests ─────────────────────────────────────────────────

#[tokio::test]
async fn projects_run_recorded_to_agent_runs() {
    let (db, pool) = test_db().await;
    let runtime = runtime_with(16, empty_snapshot());
    let h = tokio::spawn(run_persistence_subscriber(
        runtime.clone(),
        db.clone(),
        runtime.subscribe(),
        CancellationToken::new(),
    ));

    emit_run_until_persisted(&runtime, &pool, &report("run1", "ws1", "巡检正常")).await;

    let (summary,): (String,) = sqlx::query_as("SELECT summary FROM agent_runs WHERE id = 'run1'")
        .fetch_one(&pool)
        .await
        .expect("run1 row");
    assert_eq!(summary, "巡检正常");
    h.abort();
}

#[tokio::test]
async fn stale_event_does_not_overwrite_newer_row() {
    // insert-once 幂等即 fencing：先落新事件（occurred_at=T2），再回放
    // 同 run_id 的 stale 事件（T1<T2），行内容保持 T2 版本。
    let (db, pool) = test_db().await;
    let runtime = runtime_with(16, empty_snapshot());
    let h = tokio::spawn(run_persistence_subscriber(
        runtime.clone(),
        db.clone(),
        runtime.subscribe(),
        CancellationToken::new(),
    ));

    // 顺序保证：先确认新事件落库，再回放 stale（订阅者按序处理）。
    emit_run_until_persisted(&runtime, &pool, &report("run1", "ws1", "newer")).await;
    // stale 回放：同 run_id，内容更旧
    runtime.bus().emit(AgentEventKind::RunRecorded {
        report: Box::new(report("run1", "ws1", "stale")),
        problem_key: None,
        dedup_key: None,
    });
    // marker：run2 落库时 stale 回放已处理完
    emit_run_until_persisted(&runtime, &pool, &report("run2", "ws1", "marker")).await;

    assert_eq!(count_runs(&pool, "run1").await, 1, "stale 回放不得产生重复行");
    let (summary,): (String,) = sqlx::query_as("SELECT summary FROM agent_runs WHERE id = 'run1'")
        .fetch_one(&pool)
        .await
        .expect("run1 row");
    assert_eq!(summary, "newer", "stale 事件不得覆盖更新的行");
    h.abort();
}

#[tokio::test]
async fn lagged_subscriber_resyncs_from_dump_state() {
    let (db, pool) = test_db().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;

    // 预热 runtime 内存真相源：一条 recent_run + ws1 的 trust config
    let mut trust = TrustConfig::default();
    trust.max_auto_actions_per_tick = 7;
    let snapshot = RestoreSnapshot {
        heartbeat: vec![WorkspaceHeartbeatState {
            workspace_id: "ws1".into(),
            tasks: vec![],
            trust_config: trust,
            interval_minutes: 30,
        }],
        recent_runs: vec![report("run_lagged", "ws1", "resync me")],
        problem_meta: vec![],
    };
    // 容量 2 的 bus：先订阅再发 5 事件 → 首 recv 必为 Lagged
    let runtime = runtime_with(2, snapshot);
    let rx = runtime.subscribe();
    for i in 0..5 {
        runtime.bus().emit(AgentEventKind::HeartbeatTasksChanged {
            workspace_id: format!("ws{i}"),
        });
    }

    let dump = {
        let runtime = runtime.clone();
        move || runtime.dump_state()
    };
    // 长对账周期：只有 Lagged resync 能投影，排除周期对账干扰
    let h = tokio::spawn(run_persistence_loop(
        rx,
        dump,
        db.clone(),
        test_publisher(),
        Duration::from_secs(3600),
        CancellationToken::new(),
    ));

    wait_until(|| async { count_runs(&pool, "run_lagged").await == 1 }).await;

    let trust_repo = tinyiothub_storage::heartbeat::HeartbeatTaskRepository::new(pool.clone());
    let loaded = trust_repo
        .load_trust_config("ws1")
        .await
        .expect("load trust config")
        .expect("trust config resynced");
    assert_eq!(loaded.max_auto_actions_per_tick, 7);
    h.abort();
}

#[tokio::test]
async fn periodic_reconcile_projects_dump_state() {
    let (db, pool) = test_db().await;
    let snapshot = RestoreSnapshot {
        heartbeat: vec![],
        recent_runs: vec![report("run_reconcile", "ws1", "reconciled")],
        problem_meta: vec![],
    };
    let runtime = runtime_with(16, snapshot);

    let dump = {
        let runtime = runtime.clone();
        move || runtime.dump_state()
    };
    let h = tokio::spawn(run_persistence_loop(
        runtime.subscribe(),
        dump,
        db.clone(),
        test_publisher(),
        Duration::from_millis(50),
        CancellationToken::new(),
    ));

    // 无任何事件：只有周期对账能把 dump_state 投影落库
    wait_until(|| async { count_runs(&pool, "run_reconcile").await == 1 }).await;
    h.abort();
}

#[tokio::test]
async fn projects_heartbeat_result_to_agent_actions() {
    let (db, pool) = test_db().await;
    let runtime = runtime_with(16, empty_snapshot());
    let h = tokio::spawn(run_persistence_subscriber(
        runtime.clone(),
        db.clone(),
        runtime.subscribe(),
        CancellationToken::new(),
    ));

    let count_actions = || async {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_actions WHERE workspace_id = 'ws1' AND event_type = 'heartbeat'",
        )
        .fetch_one(&pool)
        .await
        .expect("count agent_actions");
        n
    };
    // 订阅竞态（见 emit_run_until_persisted 注释）：心跳结果非幂等，
    // 每次只在确认未落库时重发一次。
    let mut projected = false;
    for _ in 0..60 {
        runtime.bus().emit(AgentEventKind::HeartbeatResultReady {
            result: Box::new(HeartbeatResult {
                workspace_id: "ws1".into(),
                status: HeartbeatStatus::Complete,
                summary: "tick done".into(),
                task_count: 2,
                executed_actions: vec![],
                proposals: vec![],
                error: None,
            }),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        if count_actions().await >= 1 {
            projected = true;
            break;
        }
    }
    assert!(projected, "heartbeat result not persisted within 3s");
    h.abort();
}

/// Task 8 fix round 1：resync 单项连续失败达阈值升级 error! + DLQ，
/// 越阈值不重复刷屏；恢复后计数清零。
#[tokio::test]
async fn resync_escalates_to_dlq_after_consecutive_failures_then_recovers() {
    let (db, pool) = test_db().await;

    // 制造永久失败：drop agent_runs（先存 schema 供恢复）
    let (create_sql,): (String,) = sqlx::query_as("SELECT sql FROM sqlite_master WHERE name = 'agent_runs'")
        .fetch_one(&pool)
        .await
        .expect("agent_runs schema");
    sqlx::query("DROP TABLE agent_runs")
        .execute(&pool)
        .await
        .expect("drop agent_runs");

    let snap = || RestoreSnapshot {
        heartbeat: vec![],
        recent_runs: vec![report("run_bad", "ws1", "fails")],
        problem_meta: vec![],
    };
    let mut failures = ResyncFailures::default();

    let dlq_count = || async {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_dead_letters WHERE workspace_id = 'ws1' AND event_type = 'RunRecorded'",
        )
        .fetch_one(&pool)
        .await
        .expect("dlq count");
        n
    };

    // 3 个对账周期连续失败 → 第 3 次越阈值升级 error! + DLQ
    for _ in 0..3 {
        resync(snap(), db.pool(), &mut failures).await;
    }
    assert_eq!(failures.runs.get("run_bad"), Some(&3));
    assert_eq!(dlq_count().await, 1, "越阈值升级一次 DLQ");

    // 第 4 次仍失败：已升级过，不重复刷 DLQ
    resync(snap(), db.pool(), &mut failures).await;
    assert_eq!(failures.runs.get("run_bad"), Some(&4));
    assert_eq!(dlq_count().await, 1, "升级后不重复刷 DLQ");

    // 恢复（重建表）→ 下一次 resync 成功且计数清零
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("recreate agent_runs");
    resync(snap(), db.pool(), &mut failures).await;
    assert!(!failures.runs.contains_key("run_bad"), "恢复后计数清零");
    assert_eq!(count_runs(&pool, "run_bad").await, 1);
}
