//! Task 9 启动接线测试：DB 快照 rehydration + 僵尸 run reconcile。
//!
//! 全部用例走真实启动顺序（D11-①③，与 service_manager 接线逐行对应）：
//! build_agent_snapshot → bus 订阅先于 restore → AgentRuntime::restore →
//! reconcile_zombie_runs。

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_core::heartbeat::TrustConfig;
use tinyiothub_storage::Db;
use tinyiothub_storage::heartbeat::{HeartbeatTaskRepository, WorkspaceHeartbeatConfig};

use crate::bootstrap::{build_agent_snapshot, reconcile_zombie_runs};
use crate::domains::agent::host::test_utils::seed_test_workspace;
use tinyiothub_agent::runtime::runtime::{AgentRuntime, RuntimeDeps};

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

/// 真实启动顺序的测试骨架：snapshot →（订阅先于 restore）→ restore →
/// 僵尸 reconcile。返回 runtime 供断言内存真相源。
async fn bootstrap_test_runtime(db: &Arc<Db>) -> Arc<AgentRuntime> {
    let snapshot = build_agent_snapshot(db).await;
    let deps = RuntimeDeps::test_stub();
    // 订阅先于 restore（bus 经 RuntimeDeps 注入，restore 前创建的 receiver
    // 捕获 restore 期间及之后的一切事件）。
    let _rx = deps.agent_events.subscribe();
    let runtime = Arc::new(AgentRuntime::restore(snapshot, deps));
    reconcile_zombie_runs(db, &runtime).await;
    runtime
}

fn report_json(run_id: &str, workspace_id: &str) -> String {
    serde_json::to_string(&RunReport {
        run_id: run_id.into(),
        workspace_id: workspace_id.into(),
        trigger: "timer:ws1".into(),
        outcome: Outcome::NoActionNeeded,
        summary: format!("summary of {run_id}"),
        actions: vec![],
        verified: true,
        duration_ms: 10,
        tool_calls: 0,
        tokens: 0,
    })
    .expect("serialize report")
}

/// 僵尸行夹具：report 为 '{}' —— 该 run 从未完成（无完成报告可投影），
/// 因此不进入 restore 预热的 recent_runs 窗口，registry 不认领。
async fn insert_run_with_status(pool: &SqlitePool, run_id: &str, workspace_id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, status)
         VALUES (?, ?, 'timer', 'acted', 's', '{}', ?)",
    )
    .bind(run_id)
    .bind(workspace_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert run with status");
}

/// 带有效完成报告的 run 行（会被快照 recent_runs 段预热，registry 认领）。
async fn insert_completed_run(pool: &SqlitePool, run_id: &str, workspace_id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, status)
         VALUES (?, ?, 'timer', 'acted', 's', ?, ?)",
    )
    .bind(run_id)
    .bind(workspace_id)
    .bind(report_json(run_id, workspace_id))
    .bind(status)
    .execute(pool)
    .await
    .expect("insert completed run");
}

async fn run_status(pool: &SqlitePool, run_id: &str) -> String {
    let (status,): (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("run status");
    status
}

// ── tests ─────────────────────────────────────────────────

/// Brief 僵尸 reconcile 用例：DB 中 status='running' 但 registry 无主的
/// run 在启动时被标记 'interrupted'。
#[tokio::test]
async fn startup_marks_orphan_running_runs_interrupted() {
    let (db, pool) = test_db().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    insert_run_with_status(&pool, "ghost", "ws1", "running").await;

    let _runtime = bootstrap_test_runtime(&db).await;

    assert_eq!(run_status(&pool, "ghost").await, "interrupted");
}

/// completed 行不动；registry 预热窗口认领的 run_id 即使 DB 行残留
/// 'running' 也不判僵尸（防御性排除语义：registry 已有完成报告）。
#[tokio::test]
async fn startup_keeps_completed_and_registry_owned_runs_untouched() {
    let (db, pool) = test_db().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    // registry 认领：report JSON 有效 → prewarm 进 recent_runs 窗口。
    insert_completed_run(&pool, "owned", "ws1", "running").await;
    // 正常完成行：status='completed'，不得被误标。
    insert_completed_run(&pool, "done", "ws1", "completed").await;
    // 真僵尸：report 不可解析（不进 registry 窗口）且 status='running'。
    insert_run_with_status(&pool, "zombie", "ws1", "running").await;

    let _runtime = bootstrap_test_runtime(&db).await;

    assert_eq!(
        run_status(&pool, "owned").await,
        "running",
        "registry 认领的 run 不判僵尸"
    );
    assert_eq!(run_status(&pool, "done").await, "completed", "completed 行不动");
    assert_eq!(run_status(&pool, "zombie").await, "interrupted");
}

/// 快照 heartbeat 段：tasks/trust/interval 从 DB 装配并预热进 runner
/// 内存真源；recent_runs 段按 旧→新（契约：registry 无时间戳不能自排序）。
#[tokio::test]
async fn build_snapshot_prewarms_heartbeat_state_and_recent_runs_oldest_first() {
    let (db, pool) = test_db().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;

    let task_repo = HeartbeatTaskRepository::new(pool.clone());
    task_repo
        .insert("ws1", "P1", "巡检设备在线率")
        .await
        .expect("insert task");
    let mut trust = TrustConfig::default();
    trust.max_auto_actions_per_tick = 7;
    task_repo.save_trust_config("ws1", &trust).await.expect("save trust");
    task_repo
        .save_heartbeat_config(
            "ws1",
            &WorkspaceHeartbeatConfig {
                enabled: true,
                interval_minutes: 45,
            },
        )
        .await
        .expect("save heartbeat config");

    // 两条 run：显式 created_at 控制时序（旧→新插入，乱序写库）。
    for (id, age) in [("run_new", "-1 hours"), ("run_old", "-2 hours")] {
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, created_at)
             VALUES (?, 'ws1', 'timer', 'acted', 's', ?, datetime('now', ?))",
        )
        .bind(id)
        .bind(report_json(id, "ws1"))
        .bind(age)
        .execute(&pool)
        .await
        .expect("insert run");
    }

    let snapshot = build_agent_snapshot(&db).await;

    // 全量迁移的库自带种子工作空间（20260407000001 迁移 ws-default-001）——
    // heartbeat 段含 2 条，按 id 定位 ws1。
    let state = snapshot
        .heartbeat
        .iter()
        .find(|s| s.workspace_id == "ws1")
        .expect("ws1 heartbeat state");
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.trust_config.max_auto_actions_per_tick, 7);
    assert_eq!(state.interval_minutes, 45);

    // recent_runs 旧→新（prewarm 输入契约）。
    let ids: Vec<&str> = snapshot.recent_runs.iter().map(|r| r.run_id.as_str()).collect();
    assert_eq!(ids, ["run_old", "run_new"]);

    // restore 后内存真源可读；registry 新→旧读出口确认预热顺序正确。
    let deps = RuntimeDeps::test_stub();
    let runtime = AgentRuntime::restore(snapshot, deps);
    assert_eq!(runtime.heartbeat_tasks("ws1").len(), 1);
    assert_eq!(runtime.active_runs().len(), 2);
    let recent = runtime.run_registry().recent("ws1", 1);
    assert_eq!(recent[0].run_id, "run_new", "窗口队尾必须为最新 run");
}

/// 快照 dedup 元数据段（Task 6 遗留指针）：agent_runs 的
/// problem_key/outcome/verified/acked_at/created_at 直接查询装配，
/// restore 预热进 RunRegistry —— 重启后 O11 dedup 不丢状态（含行级 ack）。
#[tokio::test]
async fn build_snapshot_restores_problem_dedup_metadata() {
    let (db, pool) = test_db().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;

    // 旧 run（未 ack）+ 新 run（已 ack）：行级 ack 语义随预热恢复。
    for (id, age) in [("p_old", "-2 hours"), ("p_new", "-1 hours")] {
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, verified, problem_key, created_at)
             VALUES (?, 'ws1', 'timer', 'acted', 's', ?, 1, 'p1', datetime('now', ?))",
        )
        .bind(id)
        .bind(report_json(id, "ws1"))
        .bind(age)
        .execute(&pool)
        .await
        .expect("insert problem run");
    }
    sqlx::query("UPDATE agent_runs SET acked_at = datetime('now'), acked_by = 'u1' WHERE id = 'p_new'")
        .execute(&pool)
        .await
        .expect("ack newest");
    // 保留窗（7d）外的行不预热。
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report, problem_key, created_at)
         VALUES ('p_ancient', 'ws1', 'timer', 'acted', 's', '{}', 'p1', datetime('now', '-8 days'))",
    )
    .execute(&pool)
    .await
    .expect("insert ancient");

    let snapshot = build_agent_snapshot(&db).await;
    assert_eq!(snapshot.problem_meta.len(), 2, "8 天前的行超出 7d 保留窗");

    let deps = RuntimeDeps::test_stub();
    let runtime = AgentRuntime::restore(snapshot, deps);
    let registry = runtime.run_registry();
    let d7 = Duration::from_secs(7 * 24 * 3600);
    assert_eq!(registry.count_problem_runs("ws1", "p1", d7), 2);
    // 最新一条（p_new）已 ack —— 行级保真恢复。
    let (outcome, verified, acked) = registry
        .last_problem_run("ws1", "p1", Duration::from_secs(6 * 3600))
        .expect("problem run restored");
    assert_eq!(outcome, Outcome::Acted);
    assert!(verified);
    assert!(acked, "最新 run 的行级 ack 标记必须随预热恢复");
}
