use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use tinyiothub_agent::harness::{EffectReconciler, ReconcileOutcome};
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::runtime::AgentLoop;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_cloud::domains::agent::events::AgentEventBus;
use tinyiothub_cloud::domains::agent::runtime::controls::{CommandControl, CommandRecovery};
use tinyiothub_cloud::domains::agent::runtime::dispatch::{EffectDispatch, HarnessJobResultSink, job_id_for};
use tinyiothub_cloud::domains::agent::runtime::harness_store::HarnessTaskStore;
use tinyiothub_cloud::domains::agent::runtime::{EffectRecovery, RecoveryMode};
use tinyiothub_cloud::domains::agent::thing_agent::active_runs::ActiveRunRegistry;
use tinyiothub_cloud::domains::agent::thing_agent::approval_port::HarnessPendingToolApproval;
use tinyiothub_cloud::domains::agent::thing_agent::manager::AutonomousAgentProvider;
use tinyiothub_cloud::domains::agent::thing_agent::runner::{AgentHandle, RunContextInner, Runner};
use tinyiothub_cloud::domains::agent::thing_agent::task::coordinator::TaskCoordinator;
use tinyiothub_cloud::domains::agent::thing_agent::task::wake::WakeRouter;
use tinyiothub_cloud::shared::runtime_ports::WatchRuleStoreAdapter;
use tinyiothub_core::driver::manifest::{ManifestTool, ToolKindDecl, ToolSafetyDecl, ToolTimeout};
use tinyiothub_core::error::Error;
use tinyiothub_core::harness::{
    CommandKind, CommandState, DecisionState, EffectIntent, EffectState, ExecutionMode, HarnessCommand,
    HarnessEventInput,
};
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
use tinyiothub_runtime::job::service::JobService;
use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobResultSink, JobWorkerLocator};
use tinyiothub_storage::harness_event::EventFilter;
use tinyiothub_storage::harness_snapshot::{ProjectionSnapshot, ProjectionSubject};
use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

const WS: &str = "ws-recovery";

struct ScriptedReconciler {
    outcome: Mutex<ReconcileOutcome>,
    calls: Mutex<Vec<String>>,
}

impl ScriptedReconciler {
    fn new(outcome: ReconcileOutcome) -> Self {
        Self {
            outcome: Mutex::new(outcome),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl EffectReconciler for ScriptedReconciler {
    async fn settle(&self, intent: &EffectIntent) -> ReconcileOutcome {
        self.calls.lock().expect("calls").push(intent.effect_id.clone());
        self.outcome.lock().expect("outcome").clone()
    }
}

fn at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}

async fn fixture() -> WorkspaceFixture {
    fixture_workspace(WS).await
}

async fn seed_effect(fixture: &WorkspaceFixture, effect_id: &str, state: EffectState, task_state: &str) {
    let pool = fixture.workspace_pool().await;
    sqlx::query("INSERT INTO agent_tasks (id, run_id, state, state_version) VALUES (?, ?, ?, 1)")
        .bind(effect_id)
        .bind(format!("run-{effect_id}"))
        .bind(task_state)
        .execute(&pool)
        .await
        .expect("task");
    sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
        .bind(format!("run-{effect_id}"))
        .bind(effect_id)
        .execute(&pool)
        .await
        .expect("run");
    sqlx::query(
        "INSERT INTO effect_intents
            (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
             request_digest, idempotency_key, external_ref, state, state_version, result_json,
             created_at, updated_at)
         VALUES (?, 'session-1', ?, ?, ?, 'set_speed', ?, ?, ?, 'driver-call-1', ?, 1, NULL, ?, ?)",
    )
    .bind(effect_id)
    .bind(effect_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(json!({"device":"robot-1","credentials":{"password":"nested-secret"}}).to_string())
    .bind(format!("digest-{effect_id}"))
    .bind(effect_id)
    .bind(state.as_str())
    .bind(at().to_rfc3339())
    .bind(at().to_rfc3339())
    .execute(&pool)
    .await
    .expect("effect");
    sqlx::query(
        "INSERT INTO device_jobs
            (id, workspace_id, device_id, tool, args_json, state, stop_requested_at, progress_json,
             result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
         VALUES (?, ?, 'robot-1', 'set_speed', '{}', 'running', NULL, NULL, NULL, ?, ?, ?,
                 'driver-call-1', 5000, ?, NULL)",
    )
    .bind(format!("job-{effect_id}"))
    .bind(WS)
    .bind(effect_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(at().to_rfc3339())
    .execute(&pool)
    .await
    .expect("job");
}

async fn seed_decision(
    fixture: &WorkspaceFixture,
    effect_id: &str,
    kind: &str,
    state: DecisionState,
    answer: Option<&str>,
    expires_at: Option<DateTime<Utc>>,
) {
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, consumed_at, consumed_by)
         VALUES (?, ?, ?, 'session-1', ?, ?, ?, '{\"summary\":\"set speed\"}', ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL)",
    )
    .bind(format!("decision-{effect_id}"))
    .bind(kind)
    .bind(state.as_str())
    .bind(format!("run-{effect_id}"))
    .bind(effect_id)
    .bind(effect_id)
    .bind(json!(["applied", "not_applied"]).to_string())
    .bind(at().to_rfc3339())
    .bind(at().to_rfc3339())
    .bind(expires_at.map(|value| value.to_rfc3339()))
    .bind(answer)
    .execute(&fixture.workspace_pool().await)
    .await
    .expect("decision");
}

async fn effect_state(fixture: &WorkspaceFixture, effect_id: &str) -> (String, i64) {
    sqlx::query_as("SELECT state, state_version FROM effect_intents WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("effect state")
}

async fn run_task_states(fixture: &WorkspaceFixture, effect_id: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT r.state, t.state FROM agent_runs r
         JOIN agent_tasks t ON t.id = r.task_id WHERE r.task_id = ?",
    )
    .bind(effect_id)
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("run/task states")
}

async fn decision_state(fixture: &WorkspaceFixture, effect_id: &str) -> (String, Option<String>) {
    sqlx::query_as("SELECT state, answer FROM decisions WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("decision state")
}

async fn event_count(fixture: &WorkspaceFixture, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = ?")
        .bind(event_type)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("event count")
}

async fn wake_count(fixture: &WorkspaceFixture, task_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("wake count")
}

#[tokio::test]
async fn effect_recovery_restart_fences_dispatching_before_or_after_physical_send() {
    for sends_before_crash in [0, 1] {
        let effect_id = format!("effect-send-{sends_before_crash}");
        let fixture = fixture().await;
        seed_effect(&fixture, &effect_id, EffectState::Dispatching, "running").await;
        let reconciler = ScriptedReconciler::new(ReconcileOutcome::Unknown {
            reason: "device unreachable".to_string(),
        });
        let host = EffectRecovery::new(Arc::new(fixture.db.clone()));

        let report = host
            .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
            .await
            .expect("restart recovery");
        assert!(report.dispatch_permits.is_empty());
        assert_eq!(effect_state(&fixture, &effect_id).await.0, "unknown");
        assert_eq!(decision_state(&fixture, &effect_id).await.0, "requested");
        assert_eq!(reconciler.calls.lock().expect("calls").len(), 1);
        let total_physical_sends = sends_before_crash + report.dispatch_permits.len();
        assert_eq!(total_physical_sends, sends_before_crash);

        host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
            .await
            .expect("idempotent restart recovery");
        let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM decisions WHERE effect_id = ?")
            .bind(&effect_id)
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("decision count");
        assert_eq!(decisions, 1);
        fixture.close().await;
    }
}

#[tokio::test]
async fn effect_recovery_terminal_result_wake_and_job_commit_once() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-terminal", EffectState::Dispatching, "running").await;
    seed_decision(
        &fixture,
        "effect-terminal",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"position": 3})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));

    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("settle recovered effect");
    assert_eq!(effect_state(&fixture, "effect-terminal").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-terminal").await.0, "withdrawn");
    assert_eq!(event_count(&fixture, "step.tool_result").await, 1);
    assert_eq!(wake_count(&fixture, "effect-terminal").await, 1);
    let job: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = 'job-effect-terminal'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("job state");
    assert_eq!(job, "done");

    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("terminal recovery is a no-op");
    assert_eq!(event_count(&fixture, "step.tool_result").await, 1);
    assert_eq!(wake_count(&fixture, "effect-terminal").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_human_answer_commit_wins_before_paused_consume() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-human", EffectState::Unknown, "paused").await;
    seed_decision(
        &fixture,
        "effect-human",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.answer_reconciliation(
        WS,
        "effect-human",
        "applied",
        "operator-1",
        Some(json!({"evidence":["evidence-1"]})),
    )
    .await
    .expect("human answer commits while paused");

    let reconciler = ScriptedReconciler::new(ReconcileOutcome::DefinitelyNotApplied);
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("paused recovery preserves answer");
    assert_eq!(effect_state(&fixture, "effect-human").await.0, "unknown");
    assert_eq!(
        decision_state(&fixture, "effect-human").await,
        ("answered".to_string(), Some("applied".to_string()))
    );
    assert_eq!(wake_count(&fixture, "effect-human").await, 1);

    sqlx::query("UPDATE agent_tasks SET state = 'running' WHERE id = 'effect-human'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("resume task");
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("active recovery consumes answer");
    assert_eq!(effect_state(&fixture, "effect-human").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-human").await.0, "consumed");
    assert_eq!(wake_count(&fixture, "effect-human").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_auto_settle_closes_decision_and_late_answer_cannot_override() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-auto", EffectState::Unknown, "running").await;
    seed_decision(
        &fixture,
        "effect-auto",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"confirmed": true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("automatic reconciliation wins");

    host.answer_reconciliation(WS, "effect-auto", "not_applied", "operator-1", None)
        .await
        .expect_err("late human answer must not override terminal state");
    assert_eq!(effect_state(&fixture, "effect-auto").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-auto").await.0, "withdrawn");
    assert_eq!(wake_count(&fixture, "effect-auto").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_cancelling_accepts_confirmed_stop_without_reconciliation() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-stop-confirmed", EffectState::Cancelling, "cancelling").await;
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Cancelled(json!({"stopped": true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("confirmed stop settles without reconciliation");
    assert_eq!(effect_state(&fixture, "effect-stop-confirmed").await.0, "cancelled");
    assert_eq!(
        run_task_states(&fixture, "effect-stop-confirmed").await,
        ("cancelled".to_string(), "cancelled".to_string())
    );
    assert_eq!(wake_count(&fixture, "effect-stop-confirmed").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_closed_approval_settles_without_reopening_decision() {
    for closed_state in [DecisionState::Expired, DecisionState::Withdrawn] {
        let effect_id = format!("effect-{}", closed_state.as_str());
        let fixture = fixture().await;
        seed_effect(&fixture, &effect_id, EffectState::Prepared, "running").await;
        seed_decision(&fixture, &effect_id, "tool_approval", closed_state, None, None).await;
        let reconciler = ScriptedReconciler::new(ReconcileOutcome::Unknown {
            reason: "unused".to_string(),
        });
        let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
        host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
            .await
            .expect("closed approval settles prepared effect");
        assert_eq!(effect_state(&fixture, &effect_id).await.0, "not_applied");
        assert_eq!(decision_state(&fixture, &effect_id).await.0, closed_state.as_str());
        assert_eq!(event_count(&fixture, "decision.consumed").await, 0);
        fixture.close().await;
    }
}

#[tokio::test]
async fn effect_recovery_cancelling_rejects_execution_success_as_stop_proof() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-stop-proof", EffectState::Cancelling, "cancelling").await;
    seed_decision(
        &fixture,
        "effect-stop-proof",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"completed": true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect_err("execution success is not an allowed cancelling outcome");
    assert_eq!(effect_state(&fixture, "effect-stop-proof").await.0, "cancelling");
    assert_eq!(wake_count(&fixture, "effect-stop-proof").await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_cancel_prepared_without_decision_does_not_dispatch() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-no-approval", EffectState::Prepared, "cancelling").await;
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.request_cancellation(WS, "effect-no-approval")
        .await
        .expect("unapproved prepared effect can be cancelled");
    assert_eq!(effect_state(&fixture, "effect-no-approval").await.0, "not_applied");
    assert_eq!(
        run_task_states(&fixture, "effect-no-approval").await,
        ("cancelled".to_string(), "cancelled".to_string())
    );
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Unknown {
        reason: "unused".to_string(),
    });
    let report = host
        .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("terminal cancellation is stable");
    assert!(report.dispatch_permits.is_empty());
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_cancel_before_send_or_stop_unknown_never_claims_stopped() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-cancel-prepared", EffectState::Prepared, "cancelling").await;
    seed_decision(
        &fixture,
        "effect-cancel-prepared",
        "tool_approval",
        DecisionState::Requested,
        None,
        Some(at() + Duration::hours(1)),
    )
    .await;
    seed_effect(&fixture, "effect-cancel-sent", EffectState::Dispatching, "cancelling").await;
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));

    host.request_cancellation(WS, "effect-cancel-prepared")
        .await
        .expect("cancel before send");
    host.request_cancellation(WS, "effect-cancel-sent")
        .await
        .expect("cancel after dispatch permit");
    assert_eq!(effect_state(&fixture, "effect-cancel-prepared").await.0, "not_applied");
    assert_eq!(
        run_task_states(&fixture, "effect-cancel-prepared").await,
        ("cancelled".to_string(), "cancelled".to_string())
    );
    assert_eq!(decision_state(&fixture, "effect-cancel-prepared").await.0, "withdrawn");
    assert_eq!(effect_state(&fixture, "effect-cancel-sent").await.0, "cancelling");
    assert_eq!(
        run_task_states(&fixture, "effect-cancel-sent").await,
        ("cancelling".to_string(), "cancelling".to_string())
    );

    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Unknown {
        reason: "stop request failed; device may still run".to_string(),
    });
    let report = host
        .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("stop uncertainty fails closed");
    assert!(report.dispatch_permits.is_empty());
    assert_eq!(
        effect_state(&fixture, "effect-cancel-sent").await.0,
        "cancelled_unknown"
    );
    let task_state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'effect-cancel-sent'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("task state");
    assert_eq!(task_state, "failed");
    assert_eq!(
        run_task_states(&fixture, "effect-cancel-sent").await,
        ("failed".to_string(), "failed".to_string())
    );
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_restore_fences_prepared_and_approved_without_replay() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-restore", EffectState::Prepared, "running").await;
    seed_decision(
        &fixture,
        "effect-restore",
        "tool_approval",
        DecisionState::Answered,
        Some("approved"),
        Some(at() + Duration::hours(1)),
    )
    .await;
    let backup = fixture.root.join("backup.db");
    sqlx::query("VACUUM INTO ?")
        .bind(backup.to_string_lossy().to_string())
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("self-contained backup");

    sqlx::query("UPDATE effect_intents SET state = 'completed', result_json = '{\"confirmed\":true}' WHERE effect_id = 'effect-restore'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("external action completes after backup");
    fixture
        .db
        .close_workspace_store(WS)
        .await
        .expect("close before restore");
    for suffix in ["", "-wal", "-shm"] {
        let path = format!("{}{suffix}", fixture.database_path().display());
        if tokio::fs::try_exists(&path).await.expect("check live database") {
            tokio::fs::remove_file(&path).await.expect("remove live database");
        }
    }
    tokio::fs::copy(&backup, fixture.database_path())
        .await
        .expect("restore old backup");
    fixture
        .db
        .create_workspace_store(WS)
        .await
        .expect("reopen restored store");

    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"confirmed": true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    let report = host
        .recover_workspace(WS, RecoveryMode::Restore, &reconciler)
        .await
        .expect("restore fencing and reconciliation");
    assert!(report.dispatch_permits.is_empty());
    assert_eq!(effect_state(&fixture, "effect-restore").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-restore").await.0, "withdrawn");
    assert_eq!(event_count(&fixture, "effect.dispatching").await, 0);
    assert_eq!(wake_count(&fixture, "effect-restore").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_invalid_matrix_fails_closed_without_dispatch_consume_or_repair() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-invalid", EffectState::Prepared, "running").await;
    seed_decision(
        &fixture,
        "effect-invalid",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO commands
            (command_id, kind, target_task_id, target_run_id, actor, payload_json, created_at, expires_at, state)
         VALUES ('cmd-invalid-matrix', 'steer', 'effect-invalid', 'run-effect-invalid', 'operator',
                 '{\"guidance\":\"continue\"}', ?, NULL, 'requested')",
    )
    .bind(at().to_rfc3339())
    .execute(&fixture.workspace_pool().await)
    .await
    .expect("seed requested command");

    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"replayed": true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    let error = host
        .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect_err("unlisted persisted combination must fail closed");
    assert!(error.contains("recovery.invalid_state"), "unexpected error: {error}");
    assert!(
        reconciler.calls.lock().expect("calls").is_empty(),
        "an invalid matrix row must be rejected before reconciliation"
    );
    let second = host
        .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect_err("unavailable workspace cannot retry recovery");
    assert!(second.contains("unavailable"), "unexpected error: {second}");

    let raw = sqlx::SqlitePool::connect(&format!("sqlite:{}", fixture.database_path().display()))
        .await
        .expect("raw workspace database");
    let invalid_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'recovery.invalid_state'")
            .fetch_one(&raw)
            .await
            .expect("invalid state event");
    let effect_state: String =
        sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = 'effect-invalid'")
            .fetch_one(&raw)
            .await
            .expect("effect state");
    let decision_state: String = sqlx::query_scalar("SELECT state FROM decisions WHERE effect_id = 'effect-invalid'")
        .fetch_one(&raw)
        .await
        .expect("decision state");
    let command_state: String =
        sqlx::query_scalar("SELECT state FROM commands WHERE command_id = 'cmd-invalid-matrix'")
            .fetch_one(&raw)
            .await
            .expect("command state");
    let dispatching_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'effect.dispatching'")
            .fetch_one(&raw)
            .await
            .expect("dispatch events");
    raw.close().await;
    assert_eq!(invalid_events, 1);
    assert_eq!(effect_state, "prepared");
    assert_eq!(decision_state, "requested");
    assert_eq!(command_state, "requested");
    assert_eq!(dispatching_events, 0);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_restore_journal_never_replays_a_completed_external_action() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-stale-backup", EffectState::Prepared, "running").await;
    let backups = fixture.root.join("data/agents").join(WS).join("harness/backups");
    tokio::fs::create_dir_all(&backups).await.expect("backups directory");
    let backup = backups.join("stale-backup.db");
    sqlx::query("VACUUM INTO ?")
        .bind(backup.to_string_lossy().to_string())
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("self-contained backup");

    // The outside world completes the action after the backup was taken.
    sqlx::query(
        "UPDATE effect_intents SET state = 'completed', result_json = '{\"position\":3}'
         WHERE effect_id = 'effect-stale-backup'",
    )
    .execute(&fixture.workspace_pool().await)
    .await
    .expect("external action completes after backup");
    sqlx::query("UPDATE device_jobs SET state = 'done' WHERE id = 'job-effect-stale-backup'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("external job completion");

    fixture
        .db
        .restore_workspace_store(WS, &backup)
        .await
        .expect("journaled restore");
    // The restored generation intentionally regresses to the pre-action row.
    assert_eq!(effect_state(&fixture, "effect-stale-backup").await.0, "prepared");

    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"position": 3})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    let report = host
        .recover_workspace(WS, RecoveryMode::Restore, &reconciler)
        .await
        .expect("restore fences and reconciles");
    assert!(
        report.dispatch_permits.is_empty(),
        "a restored prepared effect must never regain a dispatch permit"
    );
    assert_eq!(reconciler.calls.lock().expect("calls").len(), 1);
    assert_eq!(effect_state(&fixture, "effect-stale-backup").await.0, "completed");
    assert_eq!(event_count(&fixture, "effect.dispatching").await, 0);
    assert_eq!(wake_count(&fixture, "effect-stale-backup").await, 1);

    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("terminal recovery is a no-op");
    assert_eq!(wake_count(&fixture, "effect-stale-backup").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_startup_journal_selects_one_complete_generation_before_recovery() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-startup-journal", EffectState::Dispatching, "running").await;
    let base = fixture.root.join("data/agents").join(WS);
    let live = base.join("harness");
    let rollback = base.join("harness.rollback");
    let restore = base.join("harness.restore");
    tokio::fs::create_dir_all(&restore)
        .await
        .expect("create restore staging");
    sqlx::query("VACUUM INTO ?")
        .bind(restore.join("harness.db").to_string_lossy().to_string())
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("stage a self-contained restore generation");
    fixture
        .db
        .close_workspace_store(WS)
        .await
        .expect("close store for crash simulation");
    tokio::fs::rename(&live, &rollback)
        .await
        .expect("simulate live->rollback rename");
    sqlx::query(
        "INSERT INTO harness_restore_journal
            (workspace_id, epoch, phase, backup_path, rollback_path, restore_path, updated_at)
         VALUES (?, 1, 'live_renamed', ?, ?, ?, ?)",
    )
    .bind(WS)
    .bind(base.join("harness/backups/snapshot.db").to_string_lossy().to_string())
    .bind(rollback.to_string_lossy().to_string())
    .bind(restore.to_string_lossy().to_string())
    .bind(at().to_rfc3339())
    .execute(fixture.db.pool())
    .await
    .expect("persist crashed restore journal");

    // A crashed process restarts with an empty workspace-store cache; reuse
    // only the platform registry pool.
    let restarted = tinyiothub_storage::Db::new(fixture.db.pool().clone())
        .with_workspace_store_root(fixture.root.join("data/agents"));
    tinyiothub_cloud::bootstrap::reconcile_harness_runs(&restarted).await;

    assert!(live.join("harness.db").is_file(), "startup must finish the swap");
    assert!(
        !rollback.exists(),
        "startup recovery keeps exactly one complete generation"
    );
    let journal: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM harness_restore_journal WHERE workspace_id = ?")
        .bind(WS)
        .fetch_one(fixture.db.pool())
        .await
        .expect("journal cleared");
    assert_eq!(journal, 0);
    let effect_state: String = sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = ?")
        .bind("effect-startup-journal")
        .fetch_one(&restarted.workspace_pool_for_testing(WS).await.expect("recovered store"))
        .await
        .expect("recovered store readable");
    assert_eq!(effect_state, "dispatching");
    restarted
        .close_workspace_store(WS)
        .await
        .expect("close restarted store");
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_tool_approval_expiry_settles_prepared_effect_atomically() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-expired", EffectState::Prepared, "running").await;
    seed_decision(
        &fixture,
        "effect-expired",
        "tool_approval",
        DecisionState::Requested,
        None,
        Some(at() + Duration::hours(1)),
    )
    .await;
    let expired = fixture
        .db
        .expire_harness_decisions(WS, at() + Duration::hours(2))
        .await
        .expect("expire prepared approval");
    assert_eq!(expired, 1);
    assert_eq!(effect_state(&fixture, "effect-expired").await.0, "not_applied");
    assert_eq!(decision_state(&fixture, "effect-expired").await.0, "expired");
    assert_eq!(event_count(&fixture, "decision.expired").await, 1);
    assert_eq!(event_count(&fixture, "step.tool_result").await, 1);
    assert_eq!(wake_count(&fixture, "effect-expired").await, 1);
    let job: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = 'job-effect-expired'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("job state");
    assert_eq!(
        job, "running",
        "a Prepared effect never dispatched, so expiry must not rewrite a Job row"
    );
    let signal: String = sqlx::query_scalar(
        "SELECT signal_json FROM task_wake_outbox WHERE task_id = 'effect-expired' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("expiry wake signal");
    let signal: serde_json::Value = serde_json::from_str(&signal).expect("signal JSON");
    assert_eq!(signal["kind"], "approval_decided");
    assert_eq!(signal["workspace_id"], WS);
    assert_eq!(signal["outcome"], "expired");
    assert_eq!(signal["approval_id"], "decision-effect-expired");
    fixture.close().await;
}

#[derive(Default)]
struct ExpiryAgent {
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentLoop for ExpiryAgent {
    async fn turn_streamed(
        &self,
        user_message: &str,
        _event_tx: tokio::sync::mpsc::Sender<TurnEvent>,
        _cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> anyhow::Result<String> {
        self.prompts.lock().expect("prompts").push(user_message.to_string());
        Ok("continued".into())
    }

    async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
        Ok("continued".into())
    }

    async fn clear_history(&self) {}

    async fn seed_history(&self, _messages: &[tinyiothub_agent::port::provider::ChatMessage]) -> anyhow::Result<()> {
        Ok(())
    }
}

struct ExpiryProvider {
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AutonomousAgentProvider for ExpiryProvider {
    async fn get_or_create(
        &self,
        _workspace_id: &str,
        _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        Ok(Arc::new(tokio::sync::Mutex::new(ExpiryAgent {
            prompts: self.prompts.clone(),
        })))
    }

    fn invalidate(&self, _workspace_id: &str) {}
}

fn task_runtime(
    db: &Arc<tinyiothub_storage::Db>,
    prompts: Arc<Mutex<Vec<String>>>,
) -> (Arc<HarnessTaskStore>, Arc<TaskCoordinator>, WakeRouter) {
    task_runtime_with_jobs(db, prompts, None)
}

fn task_runtime_with_jobs(
    db: &Arc<tinyiothub_storage::Db>,
    prompts: Arc<Mutex<Vec<String>>>,
    jobs: Option<JobService>,
) -> (Arc<HarnessTaskStore>, Arc<TaskCoordinator>, WakeRouter) {
    let store = Arc::new(HarnessTaskStore::new(db.clone()));
    let coordinator = Arc::new(
        TaskCoordinator::new(
            store.clone(),
            jobs,
            Arc::new(WatchRuleStoreAdapter::new(db.as_ref().clone())),
            Arc::new(dashmap::DashMap::new()),
            Arc::new(AgentEventBus::new(64)),
            ActiveRunRegistry::new(4),
            Arc::new(Runner::new()),
            Arc::new(ExpiryProvider {
                prompts: prompts.clone(),
            }),
        )
        .with_pending_tool_approvals(Arc::new(HarnessPendingToolApproval::new(db.clone()))),
    );
    let router = WakeRouter::new(store.clone(), coordinator.clone());
    (store, coordinator, router)
}

#[derive(Default)]
struct E2eWorker {
    submitted: Mutex<Vec<String>>,
}

impl JobDeviceWorker for E2eWorker {
    fn submit(&self, _command: ThingCommand, job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
        self.submitted.lock().expect("submissions").push(job_id.into());
        Ok(())
    }

    fn cancel_job(&self, _job_id: &str) -> bool {
        true
    }

    fn has_recoverable_call(&self, _job_id: &str) -> bool {
        false
    }

    fn complete_job(&self, _job_id: &str) {}
}

struct E2eWorkers {
    worker: Arc<E2eWorker>,
}

impl JobWorkerLocator for E2eWorkers {
    fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
        Some(self.worker.clone())
    }
}

/// Rejects `deviceId == "missing"` so the dispatch path can be driven to
/// `not_applied` through the real validator.
struct E2eValidator;

#[async_trait]
impl AccessCallValidator for E2eValidator {
    async fn validate(
        &self,
        _workspace_id: &str,
        thing_id: &str,
        tool: &str,
        _params: Option<&serde_json::Value>,
    ) -> Result<ValidatedCall, CallError> {
        if thing_id == "missing" {
            return Err(CallError::NoDeclaration(thing_id.to_string()));
        }
        Ok(ValidatedCall {
            tool: ManifestTool {
                name: tool.to_string(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: json!({"type":"object"}),
                timeout: ToolTimeout {
                    default_ms: 5_000,
                    max_ms: 5_000,
                },
                safety: ToolSafetyDecl::Control,
                resources: vec![],
                cancellable: true,
            },
            timeout_ms: 5_000,
            clamped: false,
        })
    }
}

struct E2eResources;

#[async_trait]
impl ResourceResolver for E2eResources {
    async fn resolve_action(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _tool_name: &str,
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![device_claim()])
    }

    async fn resolve_properties(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _properties: &[String],
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![device_claim()])
    }
}

fn e2e_job_service(db: &Arc<tinyiothub_storage::Db>, worker: Arc<E2eWorker>) -> JobService {
    JobService::new(
        Arc::new(tinyiothub_cloud::shared::runtime_ports::WorkspaceDeviceJobStore::new(
            db.clone(),
        )),
        Arc::new(E2eWorkers { worker }),
        Arc::new(E2eValidator),
        Arc::new(E2eResources),
        Arc::new(ArbitrationRegistry::default()),
    )
}

/// Seed a live Harness Task and one Prepared Effect with no approval Decision.
async fn seed_task_effect(fixture: &WorkspaceFixture, task_id: &str, effect_id: &str, request_json: serde_json::Value) {
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};

    let now = at();
    let task = AgentTask {
        id: task_id.into(),
        workspace_id: WS.into(),
        idempotency_key: format!("{task_id}-key"),
        objective: "continue after the effect".into(),
        session_key: Some("session-effect".into()),
        created_by: "user-1".into(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: task_id.into(),
        event_id: format!("create-{task_id}"),
        signal_json: json!({"workspace_id":WS}).to_string(),
        dispatch_state: WakeDispatchState::Dispatched,
        created_at: now,
        dispatched_at: Some(now),
        resolved_at: Some(now),
    };
    fixture
        .db
        .harness_create_task_with_wake(&task, &wake)
        .await
        .expect("harness Task");
    let pool = fixture.workspace_pool().await;
    sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
        .bind(format!("run-{effect_id}"))
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("run");
    sqlx::query(
        "INSERT INTO effect_intents
            (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
             request_digest, idempotency_key, external_ref, state, state_version, result_json,
             created_at, updated_at)
         VALUES (?, 'session-effect', ?, ?, ?, 'set_speed', ?, ?, ?, NULL, 'prepared', 1, NULL, ?, ?)",
    )
    .bind(effect_id)
    .bind(task_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(request_json.to_string())
    .bind(format!("digest-{effect_id}"))
    .bind(effect_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await
    .expect("effect");
}

async fn dead_wake_count(fixture: &WorkspaceFixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE dispatch_state = 'dead'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("dead wakes")
}

/// Seed a full Harness Task plus a Prepared Effect and a due requested
/// Decision. The Task deliberately has no current Run so the expiry
/// continuation must start one.
async fn seed_expiry_task(fixture: &WorkspaceFixture, task_id: &str, effect_id: &str) {
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};

    let now = at();
    let task = AgentTask {
        id: task_id.into(),
        workspace_id: WS.into(),
        idempotency_key: format!("{task_id}-key"),
        objective: "exercise approval expiry".into(),
        session_key: Some("session-expiry".into()),
        created_by: "user-1".into(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: task_id.into(),
        event_id: format!("create-{task_id}"),
        signal_json: json!({"workspace_id":WS}).to_string(),
        dispatch_state: WakeDispatchState::Dispatched,
        created_at: now,
        dispatched_at: Some(now),
        resolved_at: Some(now),
    };
    fixture
        .db
        .harness_create_task_with_wake(&task, &wake)
        .await
        .expect("harness Task");
    let pool = fixture.workspace_pool().await;
    sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
        .bind(format!("run-{effect_id}"))
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("run");
    sqlx::query(
        "INSERT INTO effect_intents
            (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
             request_digest, idempotency_key, external_ref, state, state_version, result_json,
             created_at, updated_at)
         VALUES (?, 'session-expiry', ?, ?, ?, 'set_speed', '{\"deviceId\":\"robot-1\",\"argsJson\":{}}',
                 ?, ?, NULL, 'prepared', 1, NULL, ?, ?)",
    )
    .bind(effect_id)
    .bind(task_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(format!("digest-{effect_id}"))
    .bind(effect_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await
    .expect("effect");
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, answer_evidence_json, consumed_at, consumed_by)
         VALUES (?, 'tool_approval', 'requested', 'session-expiry', ?, ?, ?,
                 '{\"tool\":\"set_speed\",\"reason\":\"confirm\"}', '[\"approved\",\"denied\"]',
                 ?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(format!("decision-{effect_id}"))
    .bind(format!("run-{effect_id}"))
    .bind(task_id)
    .bind(effect_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind((now + Duration::hours(1)).to_rfc3339())
    .execute(&pool)
    .await
    .expect("decision");
}

#[tokio::test]
async fn approval_expiry_wake_is_dispatched_and_continues_with_the_outcome() {
    let fixture = fixture_workspace(WS).await;
    seed_expiry_task(&fixture, "task-expiry", "effect-expiry").await;
    let db = Arc::new(fixture.db.clone());
    let expired = db
        .expire_harness_decisions(WS, at() + Duration::hours(2))
        .await
        .expect("expire");
    assert_eq!(expired, 1);

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, coordinator, router) = task_runtime(&db, prompts.clone());
    assert_eq!(router.run(64).await.expect("route expiry wake"), 1);
    let wake_state: String = sqlx::query_scalar(
        "SELECT dispatch_state FROM task_wake_outbox WHERE event_id = 'approval:decision-effect-expiry:expired'",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("expiry wake state");
    assert_eq!(wake_state, "dispatched", "expiry wake must not dead-letter");

    let mut status = None;
    for _ in 0..300 {
        let task = store
            .get_task(WS, "task-expiry")
            .await
            .expect("task read")
            .expect("task row");
        status = Some(task.status);
        if task.status == tinyiothub_core::agent_task::TaskStatus::Completed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        status,
        Some(tinyiothub_core::agent_task::TaskStatus::Completed),
        "the expiry continuation must finish the Task"
    );
    let prompt = prompts.lock().expect("prompts").join("\n");
    assert!(prompt.contains("decision-effect-expiry"), "{prompt}");
    assert!(prompt.contains("expired"), "{prompt}");
    drop(coordinator);
    fixture.close().await;
}

#[tokio::test]
async fn denied_answer_without_a_handler_wake_survives_restart_and_continues() {
    let fixture = fixture_workspace(WS).await;
    seed_expiry_task(&fixture, "task-deny", "effect-deny").await;
    let db = Arc::new(fixture.db.clone());
    // Commit the answer only (simulating a crash before any HTTP handler
    // wake). The answer transaction itself must own the continuation wake.
    db.harness_answer_tool_approval(WS, "task-deny", "decision-effect-deny", "denied", "user-1", at())
        .await
        .expect("denied answer");
    let signal: String = sqlx::query_scalar(
        "SELECT signal_json FROM task_wake_outbox WHERE event_id = 'approval:decision-effect-deny:denied'",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("atomic denial wake");
    let signal: serde_json::Value = serde_json::from_str(&signal).expect("signal JSON");
    assert_eq!(signal["kind"], "approval_decided");
    assert_eq!(signal["workspace_id"], WS);
    assert_eq!(signal["outcome"], "denied");
    assert_eq!(signal["approval_id"], "decision-effect-deny");

    // Restart/recover: the wake must dispatch, not dead-letter, and the Task
    // must continue to completion.
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, _, router) = task_runtime(&db, prompts.clone());
    assert_eq!(router.run(64).await.expect("route denial wake"), 1);
    let wake_state: String = sqlx::query_scalar(
        "SELECT dispatch_state FROM task_wake_outbox WHERE event_id = 'approval:decision-effect-deny:denied'",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("wake state");
    assert_eq!(wake_state, "dispatched");
    let mut status = None;
    for _ in 0..300 {
        let task = store
            .get_task(WS, "task-deny")
            .await
            .expect("task read")
            .expect("task row");
        status = Some(task.status);
        if task.status == tinyiothub_core::agent_task::TaskStatus::Completed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(status, Some(tinyiothub_core::agent_task::TaskStatus::Completed));
    let prompt = prompts.lock().expect("prompts").join("\n");
    assert!(prompt.contains("decision-effect-deny"), "{prompt}");
    assert!(prompt.contains("denied"), "{prompt}");
    fixture.close().await;
}

#[tokio::test]
async fn deduplicated_denial_reheals_a_lost_continuation_wake() {
    let fixture = fixture_workspace(WS).await;
    seed_expiry_task(&fixture, "task-heal", "effect-heal").await;
    let db = Arc::new(fixture.db.clone());
    db.harness_answer_tool_approval(WS, "task-heal", "decision-effect-heal", "denied", "user-1", at())
        .await
        .expect("first denial");
    let event_id = "approval:decision-effect-heal:denied";
    sqlx::query("DELETE FROM task_wake_outbox WHERE event_id = ?")
        .bind(event_id)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("simulate lost wake");
    let answer = db
        .harness_answer_tool_approval(WS, "task-heal", "decision-effect-heal", "denied", "user-1", at())
        .await
        .expect("deduplicated denial");
    assert!(answer.deduplicated);
    let signal: String = sqlx::query_scalar("SELECT signal_json FROM task_wake_outbox WHERE event_id = ?")
        .bind(event_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("healed wake");
    let signal: serde_json::Value = serde_json::from_str(&signal).expect("signal JSON");
    assert_eq!(signal["kind"], "approval_decided");
    assert_eq!(signal["outcome"], "denied");
    fixture.close().await;
}

async fn wait_for_task_completion(store: &HarnessTaskStore, task_id: &str) -> tinyiothub_core::agent_task::TaskStatus {
    let mut status = tinyiothub_core::agent_task::TaskStatus::Active;
    for _ in 0..300 {
        let task = store.get_task(WS, task_id).await.expect("task read").expect("task row");
        status = task.status;
        if status.is_terminal() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    status
}

#[tokio::test]
async fn job_completion_settles_once_and_leaves_no_dead_wakes() {
    let fixture = fixture_workspace(WS).await;
    seed_task_effect(
        &fixture,
        "task-effect",
        "effect-complete",
        json!({"deviceId":"robot-1","argsJson":{"level":1}}),
    )
    .await;
    let db = Arc::new(fixture.db.clone());
    let worker = Arc::new(E2eWorker::default());
    let jobs = e2e_job_service(&db, worker.clone());
    let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
    let report = dispatch.dispatch_workspace(WS).await.expect("dispatch");
    assert_eq!(report.dispatched, 1);
    assert_eq!(worker.submitted.lock().expect("submissions").len(), 1);
    let job_id = job_id_for("effect-complete");

    let sink = HarnessJobResultSink::new(jobs.clone(), dispatch.clone());
    JobResultSink::command_finished(&sink, &job_id, true, None).await;
    assert_eq!(effect_state(&fixture, "effect-complete").await.0, "completed");
    // The Job terminal wake owns the continuation; no second generic wake.
    let settle_wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id LIKE 'settle:%'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("settle wakes");
    assert_eq!(settle_wakes, 0);

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, _, router) = task_runtime_with_jobs(&db, prompts.clone(), Some(jobs));
    assert_eq!(router.run(64).await.expect("route job wake"), 1);
    assert_eq!(
        dead_wake_count(&fixture).await,
        0,
        "the continuation must not dead-letter"
    );
    let wake_state: String = sqlx::query_scalar("SELECT dispatch_state FROM task_wake_outbox WHERE event_id = ?")
        .bind(format!("device_job:{job_id}:done"))
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("job wake state");
    assert_eq!(wake_state, "dispatched");
    assert_eq!(
        wait_for_task_completion(&store, "task-effect").await,
        tinyiothub_core::agent_task::TaskStatus::Completed
    );
    fixture.close().await;
}

#[tokio::test]
async fn dispatch_failure_wake_is_parseable_and_not_dead() {
    let fixture = fixture_workspace(WS).await;
    seed_task_effect(
        &fixture,
        "task-invalid",
        "effect-invalid",
        json!({"deviceId":"missing","argsJson":{"level":1}}),
    )
    .await;
    let db = Arc::new(fixture.db.clone());
    let worker = Arc::new(E2eWorker::default());
    let jobs = e2e_job_service(&db, worker.clone());
    let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
    let report = dispatch.dispatch_workspace(WS).await.expect("dispatch");
    assert_eq!(report.not_applied, 1);
    assert!(worker.submitted.lock().expect("submissions").is_empty());

    let signal: String = sqlx::query_scalar(
        "SELECT signal_json FROM task_wake_outbox
         WHERE event_id = 'dispatch:effect-invalid:not_applied:wake'",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("effect terminal wake");
    let signal: serde_json::Value = serde_json::from_str(&signal).expect("signal JSON");
    assert_eq!(signal["kind"], "effect_terminal");
    assert_eq!(signal["workspace_id"], WS);
    assert_eq!(signal["task_id"], "task-invalid");

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, _, router) = task_runtime_with_jobs(&db, prompts.clone(), Some(jobs));
    assert_eq!(router.run(64).await.expect("route failure wake"), 1);
    assert_eq!(
        dead_wake_count(&fixture).await,
        0,
        "a dispatch failure must not dead-letter"
    );
    let wake_state: String = sqlx::query_scalar("SELECT dispatch_state FROM task_wake_outbox WHERE event_id = ?")
        .bind("dispatch:effect-invalid:not_applied:wake")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("failure wake state");
    assert_eq!(wake_state, "dispatched");
    assert_eq!(
        wait_for_task_completion(&store, "task-invalid").await,
        tinyiothub_core::agent_task::TaskStatus::Completed
    );
    fixture.close().await;
}

#[tokio::test]
async fn interrupted_job_unknown_effect_continues_after_operator_reconciliation() {
    let fixture = fixture_workspace(WS).await;
    seed_task_effect(
        &fixture,
        "task-interrupted",
        "effect-interrupted",
        json!({"deviceId":"robot-1","argsJson":{"level":1}}),
    )
    .await;
    let db = Arc::new(fixture.db.clone());
    let worker = Arc::new(E2eWorker::default());
    let jobs = e2e_job_service(&db, worker.clone());
    let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
    dispatch.dispatch_workspace(WS).await.expect("dispatch");
    assert_eq!(worker.submitted.lock().expect("submissions").len(), 1);

    // Restart over the same workspace store with a fresh worker: any recovery
    // resend would be visible in `restart_worker`.
    let restart_worker = Arc::new(E2eWorker::default());
    let restart_jobs = e2e_job_service(&db, restart_worker.clone());
    restart_jobs.recover().await.expect("job recovery");
    let restarted = Arc::new(EffectDispatch::new(db.clone(), restart_jobs.clone()));
    restarted.reconcile_startup().await;

    assert_eq!(effect_state(&fixture, "effect-interrupted").await.0, "unknown");
    assert_eq!(decision_state(&fixture, "effect-interrupted").await.0, "requested");
    let task_status: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'task-interrupted'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("task state");
    assert_eq!(task_status, "paused", "the Run stays blocked until reconciliation");
    assert!(
        restart_worker.submitted.lock().expect("submissions").is_empty(),
        "restart recovery must not resend"
    );

    // Operator answer while paused: durable and committed. The recovery pass
    // consumes it (the Task itself stays paused for the explicit resume).
    EffectRecovery::new(db.clone())
        .answer_reconciliation(WS, "effect-interrupted", "applied", "user-1", None)
        .await
        .expect("answer reconciliation");
    assert_eq!(effect_state(&fixture, "effect-interrupted").await.0, "unknown");
    assert_eq!(
        decision_state(&fixture, "effect-interrupted").await,
        ("answered".to_string(), Some("applied".to_string()))
    );
    assert!(restart_worker.submitted.lock().expect("submissions").is_empty());
    restarted.reconcile_startup().await;
    assert_eq!(effect_state(&fixture, "effect-interrupted").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-interrupted").await.0, "consumed");

    // Resume now lifts the reconciliation pause; the Task continues from the
    // pending continuation wake with no resend.
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, _, router) = task_runtime_with_jobs(&db, prompts.clone(), Some(restart_jobs));
    let task = store
        .get_task(WS, "task-interrupted")
        .await
        .expect("task read")
        .expect("task row");
    assert_eq!(task.status, tinyiothub_core::agent_task::TaskStatus::Paused);
    // The production pause/resume API restores the state recorded when the
    // Effect fence blocked the Run.
    let restored = db
        .resume_workflow_task(WS, "task-interrupted")
        .await
        .expect("resume task");
    assert_eq!(restored, "running");
    router.run(64).await.expect("route continuation wakes");
    assert_eq!(
        wait_for_task_completion(&store, "task-interrupted").await,
        tinyiothub_core::agent_task::TaskStatus::Completed
    );
    assert_eq!(dead_wake_count(&fixture).await, 0);
    assert!(
        restart_worker.submitted.lock().expect("submissions").is_empty(),
        "the continuation must never re-send the physical action"
    );
    fixture.close().await;
}

#[tokio::test]
async fn startup_recovery_does_not_start_a_run_before_the_unknown_fence() {
    let fixture = fixture_workspace(WS).await;
    seed_task_effect(
        &fixture,
        "task-fence",
        "effect-fence",
        json!({"deviceId":"robot-1","argsJson":{"level":1}}),
    )
    .await;
    let db = Arc::new(fixture.db.clone());
    let worker = Arc::new(E2eWorker::default());
    let jobs = e2e_job_service(&db, worker.clone());
    let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
    dispatch.dispatch_workspace(WS).await.expect("dispatch");
    assert_eq!(worker.submitted.lock().expect("submissions").len(), 1);

    // Restart over the same workspace store with a fresh worker. Any resend
    // would land in `restart_worker`.
    let restart_worker = Arc::new(E2eWorker::default());
    let restart_jobs = e2e_job_service(&db, restart_worker.clone());
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, coordinator, router) = task_runtime_with_jobs(&db, prompts.clone(), Some(restart_jobs.clone()));

    // Manager production order: Job recovery -> Effect fence -> Task recovery.
    // The fence must settle the indeterminate Job and pause the Task before
    // recovery can start a Run.
    restart_jobs.recover().await.expect("job recovery");
    let restarted = Arc::new(EffectDispatch::new(db.clone(), restart_jobs.clone()));
    restarted.reconcile_startup().await;
    assert_eq!(effect_state(&fixture, "effect-fence").await.0, "unknown");
    assert_eq!(decision_state(&fixture, "effect-fence").await.0, "requested");
    assert!(
        restart_worker.submitted.lock().expect("submissions").is_empty(),
        "startup recovery must not resend the physical action"
    );
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no Run may start before the Effect fence"
    );
    let task = store
        .get_task(WS, "task-fence")
        .await
        .expect("task read")
        .expect("task row");
    assert_eq!(task.status, tinyiothub_core::agent_task::TaskStatus::Paused);
    assert!(task.current_run_id.is_none());

    coordinator.recover().await.expect("task recovery");
    assert!(
        restart_worker.submitted.lock().expect("submissions").is_empty(),
        "Task recovery must not resend the physical action"
    );
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "Task recovery must not start a Run for a fenced Task"
    );

    // Resume before the operator answer must keep the pause and name the
    // reconciliation conflict.
    let blocked = db.resume_workflow_task(WS, "task-fence").await;
    assert!(
        matches!(blocked, Err(tinyiothub_storage::DbError::Validation { .. })),
        "resume before reconciliation must conflict: {blocked:?}"
    );
    let still_paused = store
        .get_task(WS, "task-fence")
        .await
        .expect("task read")
        .expect("task row");
    assert_eq!(still_paused.status, tinyiothub_core::agent_task::TaskStatus::Paused);

    // Operator answer: the recovery pass consumes it even while paused (the
    // Task itself stays paused until the explicit resume below).
    EffectRecovery::new(db.clone())
        .answer_reconciliation(WS, "effect-fence", "applied", "user-1", None)
        .await
        .expect("answer reconciliation");
    restarted.reconcile_startup().await;
    assert_eq!(effect_state(&fixture, "effect-fence").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-fence").await.0, "consumed");

    // The answer wake is redundant with the Job continuation; remove it so the
    // assertion below is about one continuation, not delivery ordering.
    sqlx::query("DELETE FROM task_wake_outbox WHERE task_id = ? AND event_id LIKE 'decision-answer:%'")
        .bind("task-fence")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("drop redundant answer wake");

    let restored = db
        .resume_workflow_task(WS, "task-fence")
        .await
        .expect("resume after reconciliation");
    assert_eq!(restored, "running");

    assert_eq!(router.run(64).await.expect("route continuation wake"), 1);
    assert_eq!(
        wait_for_task_completion(&store, "task-fence").await,
        tinyiothub_core::agent_task::TaskStatus::Completed
    );
    assert_eq!(prompts.lock().expect("prompts").len(), 1, "one continuation Run");
    assert!(
        restart_worker.submitted.lock().expect("submissions").is_empty(),
        "the continuation must never re-send the physical action"
    );
    assert_eq!(dead_wake_count(&fixture).await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn approved_but_undispatched_prepared_effect_blocks_recovery_runs() {
    let fixture = fixture_workspace(WS).await;
    seed_expiry_task(&fixture, "task-undispatched", "effect-undispatched").await;
    let db = Arc::new(fixture.db.clone());

    // The approval answer commits, then the process crashes before the
    // dispatcher consumes it: Prepared Effect + Answered approved Decision.
    db.harness_answer_tool_approval(
        WS,
        "task-undispatched",
        "decision-effect-undispatched",
        "approved",
        "user-1",
        at(),
    )
    .await
    .expect("answer approval");
    assert_eq!(effect_state(&fixture, "effect-undispatched").await.0, "prepared");
    assert_eq!(
        decision_state(&fixture, "effect-undispatched").await,
        ("answered".to_string(), Some("approved".to_string()))
    );

    let worker = Arc::new(E2eWorker::default());
    let jobs = e2e_job_service(&db, worker.clone());
    let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let (store, coordinator, router) = task_runtime_with_jobs(&db, prompts.clone(), Some(jobs.clone()));

    // Production restart order: Job recovery -> Task recovery. The pending
    // dispatch is a wait, so no Run starts; the Task stays Active so the
    // dispatcher can still consume the approval.
    jobs.recover().await.expect("job recovery");
    coordinator.recover().await.expect("task recovery");
    assert!(
        worker.submitted.lock().expect("submissions").is_empty(),
        "recovery must not dispatch or resend before the dispatcher acts"
    );
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no Run may start before the approved dispatch is consumed"
    );
    let task = store
        .get_task(WS, "task-undispatched")
        .await
        .expect("task read")
        .expect("task row");
    assert_eq!(task.status, tinyiothub_core::agent_task::TaskStatus::Active);
    assert!(task.current_run_id.is_none());

    // The dispatcher consumes the answered approval and submits exactly one Job.
    let report = dispatch.dispatch_task(WS, "task-undispatched").await.expect("dispatch");
    assert_eq!(report.dispatched, 1);
    assert_eq!(worker.submitted.lock().expect("submissions").len(), 1);
    let job_id = job_id_for("effect-undispatched");

    // The Job terminal fact settles the Effect and writes the continuation wake.
    let sink = HarnessJobResultSink::new(jobs.clone(), dispatch.clone());
    JobResultSink::command_finished(&sink, &job_id, true, None).await;
    assert_eq!(effect_state(&fixture, "effect-undispatched").await.0, "completed");

    // The approval wake is consumed without a Run while the Job exists; the Job
    // terminal wake starts exactly one continuation.
    router.run(64).await.expect("route wakes");
    assert_eq!(
        wait_for_task_completion(&store, "task-undispatched").await,
        tinyiothub_core::agent_task::TaskStatus::Completed
    );
    assert_eq!(prompts.lock().expect("prompts").len(), 1, "one continuation Run");
    assert_eq!(
        worker.submitted.lock().expect("submissions").len(),
        1,
        "no duplicate physical send"
    );
    assert_eq!(dead_wake_count(&fixture).await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn cancelling_a_task_withdraws_a_requested_approval_and_settles_the_effect() {
    let fixture = fixture_workspace(WS).await;
    seed_expiry_task(&fixture, "task-cancel", "effect-cancel").await;
    let db = Arc::new(fixture.db.clone());
    let (_, coordinator, _) = task_runtime(&db, Arc::new(Mutex::new(Vec::new())));
    let outcome = coordinator.cancel(WS, "task-cancel").await.expect("cancel");
    assert!(outcome.accepted);
    let pool = fixture.workspace_pool().await;
    let decision: String = sqlx::query_scalar("SELECT state FROM decisions WHERE effect_id = 'effect-cancel'")
        .fetch_one(&pool)
        .await
        .expect("decision state");
    assert_eq!(decision, "withdrawn");
    let effect: String = sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = 'effect-cancel'")
        .fetch_one(&pool)
        .await
        .expect("effect state");
    assert_eq!(effect, "not_applied");
    fixture.close().await;
}

#[tokio::test]
async fn approval_expiry_preserves_paused_cancelling_and_terminal_task_state() {
    for (label, task_state) in [
        ("paused", "paused"),
        ("cancelling", "cancelling"),
        ("terminal", "completed"),
    ] {
        let fixture = fixture_workspace(WS).await;
        seed_effect(&fixture, "effect-state", EffectState::Prepared, task_state).await;
        // A finished Run must not be reopened by the sweep.
        sqlx::query("UPDATE agent_runs SET state = 'completed' WHERE task_id = 'effect-state'")
            .execute(&fixture.workspace_pool().await)
            .await
            .expect("run state");
        seed_decision(
            &fixture,
            "effect-state",
            "tool_approval",
            DecisionState::Requested,
            None,
            Some(at() + Duration::hours(1)),
        )
        .await;
        let before = run_task_states(&fixture, "effect-state").await;
        assert_eq!(before.0, "completed");
        assert_eq!(before.1, task_state);
        let expired = fixture
            .db
            .expire_harness_decisions(WS, at() + Duration::hours(2))
            .await
            .expect("expire");
        assert_eq!(expired, 1, "{label}");
        let after = run_task_states(&fixture, "effect-state").await;
        assert_eq!(after, before, "{label}: task and run state must not move");
        assert_eq!(effect_state(&fixture, "effect-state").await.0, "not_applied");
        fixture.close().await;
    }
}

#[tokio::test]
async fn approval_expiry_continues_past_a_missing_effect() {
    let fixture = fixture_workspace(WS).await;
    seed_effect(&fixture, "z-valid", EffectState::Prepared, "running").await;
    seed_decision(
        &fixture,
        "z-valid",
        "tool_approval",
        DecisionState::Requested,
        None,
        Some(at() + Duration::hours(1)),
    )
    .await;
    // A due row whose Effect vanished sorts first; it must not starve the sweep.
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, consumed_at, consumed_by)
         VALUES ('decision-a-missing', 'tool_approval', 'requested', 'session-1', 'run-missing',
                 'a-missing', 'a-missing', '{}', '[\"approved\"]', ?, ?, ?, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(at().to_rfc3339())
    .bind(at().to_rfc3339())
    .bind((at() + Duration::hours(1)).to_rfc3339())
    .execute(&fixture.workspace_pool().await)
    .await
    .expect("missing-effect decision");
    let expired = fixture
        .db
        .expire_harness_decisions(WS, at() + Duration::hours(2))
        .await
        .expect("expire");
    assert_eq!(expired, 2, "the later valid row must still be expired");
    assert_eq!(effect_state(&fixture, "z-valid").await.0, "not_applied");
    let missing: String = sqlx::query_scalar("SELECT state FROM decisions WHERE decision_id = 'decision-a-missing'")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("missing decision");
    assert_eq!(missing, "expired");
    fixture.close().await;
}

struct ObservingReconciler {
    db: Arc<tinyiothub_storage::Db>,
    other_effect_id: String,
    observed_other_states: Mutex<Vec<EffectState>>,
}

#[async_trait]
impl EffectReconciler for ObservingReconciler {
    async fn settle(&self, intent: &EffectIntent) -> ReconcileOutcome {
        if intent.effect_id == "effect-a" {
            let other = self
                .db
                .effect_intent(WS, &self.other_effect_id)
                .await
                .expect("observe other effect")
                .expect("other effect exists");
            self.observed_other_states
                .lock()
                .expect("observed states")
                .push(other.state);
        }
        ReconcileOutcome::Unknown {
            reason: "hold for operator".to_string(),
        }
    }
}

#[tokio::test]
async fn effect_recovery_restore_fences_all_effects_before_any_reconciliation() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-a", EffectState::Prepared, "running").await;
    seed_effect(&fixture, "effect-b", EffectState::Dispatching, "running").await;
    seed_effect(&fixture, "effect-c", EffectState::Unknown, "running").await;
    seed_decision(
        &fixture,
        "effect-c",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let reconciler = ObservingReconciler {
        db: Arc::new(fixture.db.clone()),
        other_effect_id: "effect-b".to_string(),
        observed_other_states: Mutex::new(Vec::new()),
    };
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restore, &reconciler)
        .await
        .expect("two-pass restore");
    assert_eq!(
        reconciler
            .observed_other_states
            .lock()
            .expect("observed states")
            .as_slice(),
        &[EffectState::Unknown]
    );
    assert_eq!(effect_state(&fixture, "effect-a").await.0, "unknown");
    assert_eq!(effect_state(&fixture, "effect-b").await.0, "unknown");
    assert_eq!(
        run_task_states(&fixture, "effect-c").await,
        ("paused".to_string(), "paused".to_string())
    );
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_restore_cancelling_preserves_stop_semantics() {
    for (case, outcome, expected) in [
        (
            "unknown",
            ReconcileOutcome::Unknown {
                reason: "stop unavailable; password=nested-secret".to_string(),
            },
            "cancelled_unknown",
        ),
        (
            "settled",
            ReconcileOutcome::Settled(json!({"completed":true})),
            "invalid",
        ),
        (
            "cancelled",
            ReconcileOutcome::Cancelled(json!({"stopped":true})),
            "cancelled",
        ),
    ] {
        let fixture = fixture().await;
        let effect_id = format!("effect-restore-stop-{case}");
        seed_effect(&fixture, &effect_id, EffectState::Cancelling, "cancelling").await;
        let reconciler = ScriptedReconciler::new(outcome);
        let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
        let result = host.recover_workspace(WS, RecoveryMode::Restore, &reconciler).await;
        if expected == "invalid" {
            result.expect_err("execution success cannot prove a restored stop");
            assert_eq!(effect_state(&fixture, &effect_id).await.0, "unknown");
            assert_eq!(
                run_task_states(&fixture, &effect_id).await,
                ("cancelling".to_string(), "cancelling".to_string())
            );
        } else {
            result.expect("restored stop outcome");
            assert_eq!(effect_state(&fixture, &effect_id).await.0, expected);
            let expected_parent = if expected == "cancelled_unknown" {
                "failed"
            } else {
                "cancelled"
            };
            assert_eq!(
                run_task_states(&fixture, &effect_id).await,
                (expected_parent.to_string(), expected_parent.to_string())
            );
        }
        fixture.close().await;
    }
}

#[tokio::test]
async fn effect_recovery_answered_approval_cancel_and_consume_orders() {
    for (answer, consume_first) in [
        ("approved", false),
        ("approved", true),
        ("denied", false),
        ("denied", true),
    ] {
        let fixture = fixture().await;
        let effect_id = format!("effect-answered-{answer}-{consume_first}");
        seed_effect(&fixture, &effect_id, EffectState::Prepared, "running").await;
        seed_decision(
            &fixture,
            &effect_id,
            "tool_approval",
            DecisionState::Answered,
            Some(answer),
            None,
        )
        .await;
        let reconciler = ScriptedReconciler::new(ReconcileOutcome::Unknown {
            reason: "unused".to_string(),
        });
        let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
        if consume_first {
            let report = host
                .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
                .await
                .expect("consume answer");
            assert_eq!(report.dispatch_permits.len(), usize::from(answer == "approved"));
        } else {
            host.request_cancellation(WS, &effect_id)
                .await
                .expect("cancel answered approval");
        }
        host.request_cancellation(WS, &effect_id)
            .await
            .expect("idempotent cancellation ordering");
        if answer == "approved" && consume_first {
            assert_eq!(effect_state(&fixture, &effect_id).await.0, "cancelling");
            assert_eq!(
                run_task_states(&fixture, &effect_id).await,
                ("cancelling".to_string(), "cancelling".to_string())
            );
            assert_eq!(decision_state(&fixture, &effect_id).await.0, "consumed");
        } else {
            assert_eq!(effect_state(&fixture, &effect_id).await.0, "not_applied");
            let expected_parent = if answer == "denied" && consume_first {
                "running"
            } else {
                "cancelled"
            };
            assert_eq!(
                run_task_states(&fixture, &effect_id).await,
                (expected_parent.to_string(), expected_parent.to_string())
            );
            let expected_decision = if consume_first { "consumed" } else { "withdrawn" };
            assert_eq!(decision_state(&fixture, &effect_id).await.0, expected_decision);
            let report = host
                .recover_workspace(WS, RecoveryMode::Restart, &reconciler)
                .await
                .expect("no later permit");
            assert!(report.dispatch_permits.is_empty());
        }
        fixture.close().await;
    }
}

#[tokio::test]
async fn effect_recovery_operator_evidence_and_debug_are_redacted() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-evidence", EffectState::Unknown, "paused").await;
    seed_decision(
        &fixture,
        "effect-evidence",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.answer_reconciliation(
        WS,
        "effect-evidence",
        "applied",
        "operator-1",
        Some(json!({"evidence":["evidence-1"],"credentials":{"password":"nested-secret"},"token":"raw-token"})),
    )
    .await
    .expect("answer with evidence");
    let decision = fixture
        .db
        .effect_decision(WS, "effect-evidence")
        .await
        .expect("decision query")
        .expect("decision");
    assert!(!format!("{decision:?}").contains("nested-secret"));
    let recovery_items = fixture.db.list_effect_recovery(WS).await.expect("recovery items");
    assert!(!format!("{recovery_items:?}").contains("nested-secret"));
    let stored_evidence: String =
        sqlx::query_scalar("SELECT answer_evidence_json FROM decisions WHERE effect_id = 'effect-evidence'")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("stored evidence");
    assert!(!stored_evidence.contains("nested-secret"));
    assert!(!stored_evidence.contains("raw-token"));
    assert!(stored_evidence.contains("evidence-1"));

    sqlx::query("UPDATE agent_tasks SET state = 'running' WHERE id = 'effect-evidence'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("resume");
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::DefinitelyNotApplied);
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("consume sanitized answer");
    let page = fixture
        .db
        .session_event_page(
            WS,
            &EventFilter {
                session_id: Some("session-1".to_string()),
                run_id: None,
                task_id: None,
                execution_mode: ExecutionMode::Production,
            },
            0,
            256,
        )
        .await
        .expect("SSE projection page");
    let public_json = serde_json::to_string(&page.events).expect("serialize SSE events");
    assert!(!public_json.contains("nested-secret"));
    assert!(!public_json.contains("raw-token"));
    let internal = fixture
        .db
        .effect_intent(WS, "effect-evidence")
        .await
        .expect("intent query")
        .expect("intent");
    assert!(
        !serde_json::to_string(&internal)
            .expect("intent DTO")
            .contains("nested-secret")
    );
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_automatic_settlement_preserves_paused_task() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-paused-auto", EffectState::Unknown, "paused").await;
    seed_decision(
        &fixture,
        "effect-paused-auto",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let reconciler = ScriptedReconciler::new(ReconcileOutcome::Settled(json!({"confirmed":true})));
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("settle without resuming");
    assert_eq!(effect_state(&fixture, "effect-paused-auto").await.0, "completed");
    assert_eq!(decision_state(&fixture, "effect-paused-auto").await.0, "withdrawn");
    assert_eq!(
        run_task_states(&fixture, "effect-paused-auto").await,
        ("paused".to_string(), "paused".to_string())
    );
    assert_eq!(wake_count(&fixture, "effect-paused-auto").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn effect_recovery_reconciliation_never_expires_and_events_redact_nested_secrets() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-secret", EffectState::Unknown, "running").await;
    seed_decision(
        &fixture,
        "effect-secret",
        "effect_reconciliation",
        DecisionState::Requested,
        None,
        None,
    )
    .await;
    let outcome = ReconcileOutcome::Unknown {
        reason: "reconcile failed with password=nested-secret".to_string(),
    };
    assert!(!format!("{outcome:?}").contains("nested-secret"));
    let reconciler = ScriptedReconciler::new(outcome);
    let host = EffectRecovery::new(Arc::new(fixture.db.clone()));
    host.recover_workspace(WS, RecoveryMode::Restart, &reconciler)
        .await
        .expect("record unknown attempt");
    let expired = fixture
        .db
        .expire_harness_decisions(WS, at() + Duration::days(365))
        .await
        .expect("expire only expirable decisions");
    assert_eq!(expired, 0);
    assert_eq!(decision_state(&fixture, "effect-secret").await.0, "requested");

    let payloads: Vec<String> = sqlx::query_scalar("SELECT payload_json FROM session_events")
        .fetch_all(&fixture.workspace_pool().await)
        .await
        .expect("public events");
    assert!(!payloads.is_empty());
    assert!(payloads.iter().all(|payload| !payload.contains("nested-secret")));
    assert!(payloads.iter().all(|payload| !payload.contains("request_json")));
    let internal = fixture
        .db
        .effect_intent(WS, "effect-secret")
        .await
        .expect("load internal intent")
        .expect("intent exists");
    let accidental_dto = serde_json::to_string(&internal).expect("serialize internal intent");
    assert!(!format!("{internal:?}").contains("nested-secret"));
    assert!(!accidental_dto.contains("nested-secret"));
    assert!(!accidental_dto.contains("request_json"));
    fixture.close().await;
}

#[derive(Default)]
struct RecordingCommandControl {
    interrupts: Mutex<Vec<String>>,
    steering: Mutex<Vec<String>>,
    cancellations: Mutex<Vec<String>>,
}

impl CommandControl for RecordingCommandControl {
    fn interrupt_model_call(&self, command: &HarnessCommand) {
        self.interrupts
            .lock()
            .expect("interrupts")
            .push(command.command_id.clone());
    }

    fn steer_before_model_call(&self, command: &HarnessCommand) {
        self.steering.lock().expect("steering").push(command.command_id.clone());
    }

    fn cancel_model_call(&self, command: &HarnessCommand) {
        self.cancellations
            .lock()
            .expect("cancellations")
            .push(command.command_id.clone());
    }
}

fn harness_command(id: &str, kind: CommandKind, task_id: &str) -> HarnessCommand {
    HarnessCommand {
        command_id: id.to_string(),
        kind,
        target_task_id: task_id.to_string(),
        target_run_id: format!("run-{task_id}"),
        actor: "operator-1".to_string(),
        payload: json!({"guidance":"use the next model call","input":"queued"}),
        created_at: at(),
        expires_at: Some(Utc::now() + Duration::hours(1)),
        state: CommandState::Requested,
    }
}

async fn command_state(fixture: &WorkspaceFixture, command_id: &str) -> CommandState {
    fixture
        .db
        .harness_command(WS, command_id)
        .await
        .expect("load command")
        .expect("command exists")
        .state
}

#[tokio::test]
async fn command_recovery_interrupt_aborts_only_current_model_call_and_never_replays() {
    let fixture = fixture().await;
    seed_effect(
        &fixture,
        "effect-command-interrupt",
        EffectState::Dispatching,
        "running",
    )
    .await;
    sqlx::query("UPDATE agent_runs SET state = 'model_calling' WHERE id = 'run-effect-command-interrupt'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("model call in progress");
    fixture
        .db
        .request_harness_command(
            WS,
            &harness_command("interrupt-current", CommandKind::Interrupt, "effect-command-interrupt"),
        )
        .await
        .expect("request interrupt");
    let host = CommandRecovery::new(Arc::new(fixture.db.clone()));
    let control = RecordingCommandControl::default();
    let wakes_before = wake_count(&fixture, "effect-command-interrupt").await;

    let consumed = host
        .consume_next(WS, "effect-command-interrupt", at(), &control)
        .await
        .expect("consume interrupt")
        .expect("current model call");
    assert_eq!(consumed.command_id, "interrupt-current");
    assert_eq!(control.interrupts.lock().expect("interrupts").len(), 1);
    assert!(control.cancellations.lock().expect("cancellations").is_empty());
    assert_eq!(
        effect_state(&fixture, "effect-command-interrupt").await.0,
        "dispatching"
    );
    assert_eq!(
        run_task_states(&fixture, "effect-command-interrupt").await,
        ("interrupted".to_string(), "running".to_string())
    );
    assert_eq!(wake_count(&fixture, "effect-command-interrupt").await, wakes_before);

    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("restart recovery");
    assert_eq!(control.interrupts.lock().expect("interrupts").len(), 1);
    assert_eq!(event_count(&fixture, "run.interrupted").await, 1);
    assert_eq!(wake_count(&fixture, "effect-command-interrupt").await, wakes_before);
    fixture.close().await;
}

#[tokio::test]
async fn command_recovery_restart_consumes_valid_steer_once_and_rejects_stale_interrupt() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-command-steer", EffectState::Dispatching, "running").await;
    fixture
        .db
        .request_harness_command(
            WS,
            &harness_command("steer-after-restart", CommandKind::Steer, "effect-command-steer"),
        )
        .await
        .expect("request steer");
    fixture
        .db
        .request_harness_command(
            WS,
            &harness_command(
                "interrupt-after-restart",
                CommandKind::Interrupt,
                "effect-command-steer",
            ),
        )
        .await
        .expect("request interrupt");
    fixture
        .db
        .close_workspace_store(WS)
        .await
        .expect("close before restart");
    fixture
        .db
        .create_workspace_store(WS)
        .await
        .expect("reopen after restart");
    let host = CommandRecovery::new(Arc::new(fixture.db.clone()));
    let control = RecordingCommandControl::default();

    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("restart command recovery");
    assert_eq!(
        control.steering.lock().expect("steering").as_slice(),
        &["steer-after-restart"]
    );
    assert!(control.interrupts.lock().expect("interrupts").is_empty());
    assert_eq!(
        command_state(&fixture, "steer-after-restart").await,
        CommandState::Consumed
    );
    assert_eq!(
        command_state(&fixture, "interrupt-after-restart").await,
        CommandState::Rejected
    );
    assert_eq!(effect_state(&fixture, "effect-command-steer").await.0, "dispatching");

    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("duplicate restart recovery");
    assert_eq!(control.steering.lock().expect("steering").len(), 1);
    assert_eq!(wake_count(&fixture, "effect-command-steer").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn command_recovery_pause_resume_and_cancel_keep_durable_fifo_semantics() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-command-queue", EffectState::Dispatching, "running").await;
    for command in [
        harness_command("pause-before-queue", CommandKind::Pause, "effect-command-queue"),
        harness_command("queue-after-pause", CommandKind::Queue, "effect-command-queue"),
    ] {
        fixture
            .db
            .request_harness_command(WS, &command)
            .await
            .expect("request command");
    }
    let host = CommandRecovery::new(Arc::new(fixture.db.clone()));
    let control = RecordingCommandControl::default();

    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("consume pause");
    assert_eq!(
        command_state(&fixture, "pause-before-queue").await,
        CommandState::Consumed
    );
    assert_eq!(
        command_state(&fixture, "queue-after-pause").await,
        CommandState::Requested
    );
    assert_eq!(run_task_states(&fixture, "effect-command-queue").await.1, "paused");

    sqlx::query("UPDATE agent_tasks SET state = 'running' WHERE id = 'effect-command-queue'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("resume task");
    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("consume queue after resume");
    assert_eq!(
        command_state(&fixture, "queue-after-pause").await,
        CommandState::Consumed
    );
    assert_eq!(effect_state(&fixture, "effect-command-queue").await.0, "dispatching");

    for command in [
        harness_command("cancel-after-queue", CommandKind::Cancel, "effect-command-queue"),
        harness_command("steer-superseded", CommandKind::Steer, "effect-command-queue"),
    ] {
        fixture
            .db
            .request_harness_command(WS, &command)
            .await
            .expect("request command");
    }
    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("consume cancel");
    assert_eq!(
        command_state(&fixture, "cancel-after-queue").await,
        CommandState::Consumed
    );
    assert_eq!(
        command_state(&fixture, "steer-superseded").await,
        CommandState::Superseded
    );
    assert_eq!(control.cancellations.lock().expect("cancellations").len(), 1);
    assert!(control.steering.lock().expect("steering").is_empty());
    assert_eq!(effect_state(&fixture, "effect-command-queue").await.0, "dispatching");

    host.recover_workspace(WS, RecoveryMode::Restart, &control)
        .await
        .expect("duplicate cancel recovery");
    assert_eq!(control.cancellations.lock().expect("cancellations").len(), 1);
    fixture.close().await;
}

#[tokio::test]
async fn stream_snapshot_tail_uses_matching_version_and_bounded_pages() {
    let fixture = fixture().await;
    let pool = fixture.workspace_pool().await;
    sqlx::query(
        "INSERT INTO projection_snapshots
            (projection_name, subject_id, last_seq, projection_version, state_version, state_json, updated_at)
         VALUES ('task', 'task-1', 0, 7, 1, '{\"steps\":[]}', ?)",
    )
    .bind(at().to_rfc3339())
    .execute(&pool)
    .await
    .expect("snapshot fixture");

    let snapshot = fixture
        .db
        .projection_snapshot(WS, "task", "task-1", 7)
        .await
        .expect("read snapshot")
        .expect("matching snapshot");
    assert_eq!(
        snapshot,
        ProjectionSnapshot {
            projection_name: "task".to_string(),
            subject_id: "task-1".to_string(),
            last_seq: 0,
            projection_version: 7,
            state_version: 1,
            state_json: json!({"steps": []}),
            updated_at: at(),
        }
    );
    assert!(
        fixture
            .db
            .projection_snapshot(WS, "task", "task-1", 8)
            .await
            .expect("version mismatch")
            .is_none(),
        "a stale projection format must trigger an explicit rebuild"
    );

    for number in 0..257 {
        fixture
            .db
            .append_session_event(&HarnessEventInput {
                event_id: format!("stream-event-{number}"),
                workspace_id: WS.to_string(),
                session_id: "session-1".to_string(),
                execution_mode: ExecutionMode::Production,
                run_id: Some("run-1".to_string()),
                task_id: Some("task-1".to_string()),
                event_type: "step.text".to_string(),
                occurred_at: at(),
                cause_id: None,
                schema_v: 1,
                payload_digest: format!("digest-{number}"),
                payload_json: json!({"text": format!("step {number}")}),
            })
            .await
            .expect("append stream fixture");
    }
    fixture
        .db
        .append_session_event(&HarnessEventInput {
            event_id: "stream-filtered-gap".to_string(),
            workspace_id: WS.to_string(),
            session_id: "session-other".to_string(),
            execution_mode: ExecutionMode::Production,
            run_id: Some("run-other".to_string()),
            task_id: Some("task-other".to_string()),
            event_type: "step.tool_result".to_string(),
            occurred_at: at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-gap".to_string(),
            payload_json: json!({"result": {"ok": true}}),
        })
        .await
        .expect("append filtered event");

    let subject = ProjectionSubject {
        workspace_id: WS.to_string(),
        session_id: Some("session-1".to_string()),
        run_id: None,
        task_id: None,
        execution_mode: ExecutionMode::Production,
    };
    let first = fixture
        .db
        .projection_tail(&subject, 0, 1_000)
        .await
        .expect("first tail page");
    assert_eq!(first.events.len(), 256);
    assert_eq!(first.high_watermark, 258);
    let second = fixture
        .db
        .projection_tail(&subject, first.events.last().expect("page boundary").seq, 256)
        .await
        .expect("second tail page");
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].seq, 257);
    assert_eq!(second.high_watermark, 258);
    let empty = fixture
        .db
        .projection_tail(&subject, 257, 256)
        .await
        .expect("filtered gap is an empty page");
    assert!(empty.events.is_empty());
    assert_eq!(empty.high_watermark, 258);

    fixture.close().await;
}

#[tokio::test]
async fn stream_crash_recovery_marks_incomplete_tail_with_run_filtered_seq() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-crash-tail", EffectState::Dispatching, "running").await;
    let run_id = "run-effect-crash-tail";
    let task_id = "effect-crash-tail";
    fixture
        .db
        .append_session_event(&HarnessEventInput {
            event_id: "crash-last-text".to_string(),
            workspace_id: WS.to_string(),
            session_id: format!("task:{task_id}"),
            execution_mode: ExecutionMode::Production,
            run_id: Some(run_id.to_string()),
            task_id: Some(task_id.to_string()),
            event_type: "step.text".to_string(),
            occurred_at: at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-crash-last-text".to_string(),
            payload_json: json!({"text": "persisted before crash"}),
        })
        .await
        .expect("persist last complete text");
    let prior_seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE run_id = ?")
        .bind(run_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("run-filtered prior seq");
    fixture
        .db
        .append_session_event(&HarnessEventInput {
            event_id: "crash-other-run-interleave".to_string(),
            workspace_id: WS.to_string(),
            session_id: "session-other".to_string(),
            execution_mode: ExecutionMode::Production,
            run_id: Some("run-other".to_string()),
            task_id: Some("task-other".to_string()),
            event_type: "step.tool_result".to_string(),
            occurred_at: at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-crash-other".to_string(),
            payload_json: json!({"result": {"ok": true}}),
        })
        .await
        .expect("interleaved other run event");

    let interrupted = fixture
        .db
        .interrupt_harness_run(WS, task_id, run_id, at())
        .await
        .expect("harness crash recovery")
        .expect("running run becomes interrupted");
    assert_eq!(interrupted.payload_json["incomplete_tail"], true);
    assert_eq!(interrupted.payload_json["last_persisted_seq"], prior_seq);
    assert_eq!(interrupted.event_type, "run.interrupted");
    assert!(
        fixture
            .db
            .interrupt_harness_run(WS, task_id, run_id, at())
            .await
            .expect("idempotent crash recovery")
            .is_none(),
        "a terminal run must not emit a second interruption"
    );

    let page = fixture
        .db
        .projection_tail(
            &ProjectionSubject {
                workspace_id: WS.to_string(),
                session_id: None,
                run_id: Some(run_id.to_string()),
                task_id: None,
                execution_mode: ExecutionMode::Production,
            },
            prior_seq,
            256,
        )
        .await
        .expect("crash tail replay");
    let replayed = page
        .events
        .iter()
        .find(|event| event.event_type == "run.interrupted")
        .expect("interruption replays in the run tail");
    assert_eq!(replayed.payload_json["incomplete_tail"], true);
    assert_eq!(replayed.payload_json["last_persisted_seq"], prior_seq);
    fixture.close().await;
}

#[tokio::test]
async fn stream_interrupted_run_marks_unclosed_text_tail_and_last_persisted_seq() {
    let fixture = fixture().await;
    seed_effect(&fixture, "effect-stream-interrupt", EffectState::Dispatching, "running").await;
    fixture
        .db
        .append_session_event(&HarnessEventInput {
            event_id: "stream-before-interrupt".to_string(),
            workspace_id: WS.to_string(),
            session_id: "session-1".to_string(),
            execution_mode: ExecutionMode::Production,
            run_id: Some("run-effect-stream-interrupt".to_string()),
            task_id: Some("effect-stream-interrupt".to_string()),
            event_type: "step.text".to_string(),
            occurred_at: at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-before-interrupt".to_string(),
            payload_json: json!({"text": "last complete segment"}),
        })
        .await
        .expect("persist last complete text");
    fixture
        .db
        .request_harness_command(
            WS,
            &harness_command("interrupt-stream", CommandKind::Interrupt, "effect-stream-interrupt"),
        )
        .await
        .expect("request interrupt");
    sqlx::query("UPDATE agent_runs SET state = 'model_calling' WHERE id = 'run-effect-stream-interrupt'")
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("model call in progress");
    let host = CommandRecovery::new(Arc::new(fixture.db.clone()));
    host.consume_next(WS, "effect-stream-interrupt", at(), &RecordingCommandControl::default())
        .await
        .expect("consume interrupt")
        .expect("interrupt event");

    let (incomplete, last_seq, interrupted_seq): (bool, i64, i64) = sqlx::query_as(
        "SELECT json_extract(payload_json, '$.incomplete_tail'),
                json_extract(payload_json, '$.last_persisted_seq'), seq
         FROM session_events WHERE event_id = 'command:interrupt-stream:interrupted'",
    )
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("interrupted payload");
    assert!(incomplete, "an unclosed crash/interrupt tail must not look complete");
    let prior_run_seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM session_events
         WHERE run_id = 'run-effect-stream-interrupt' AND seq < ?",
    )
    .bind(interrupted_seq)
    .fetch_one(&fixture.workspace_pool().await)
    .await
    .expect("run-filtered prior seq");
    assert_eq!(last_seq, prior_run_seq);
    fixture.close().await;
}

mod workflow_tests {
    use std::sync::{Arc, Mutex};

    use super::{seed_decision, seed_effect};
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use tinyiothub_agent::harness::{ReconcileOutcome, RecoveryFact, TaskOutcome, reduce_recovery};
    use tinyiothub_cloud::domains::agent::runtime::controls::WorkflowControls;
    use tinyiothub_cloud::domains::agent::runtime::{EffectDispatcher, EffectRecovery, EffectSender, RecoveryMode};
    use tinyiothub_core::harness::{
        DecisionState, EffectIntent, EffectState, WorkflowDefinition, WorkflowRevision, WorkflowTrigger,
    };
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    #[derive(Default)]
    struct RecordingEffectSender {
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl EffectSender for RecordingEffectSender {
        async fn send(&self, intent: &EffectIntent) -> Result<(), String> {
            self.calls.lock().expect("send calls").push(intent.effect_id.clone());
            Ok(())
        }
    }

    async fn seed_control_task(fixture: &WorkspaceFixture, task_id: &str, state: &str, parent: Option<&str>) {
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, parent_task_id)
             VALUES (?, ?, ?, 1, ?)",
        )
        .bind(task_id)
        .bind(format!("run-{task_id}"))
        .bind(state)
        .bind(parent)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("control task");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
            .bind(format!("run-{task_id}"))
            .bind(task_id)
            .execute(&fixture.workspace_pool().await)
            .await
            .expect("control run");
    }

    async fn control_state(fixture: &WorkspaceFixture, task_id: &str) -> String {
        sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("control state")
    }

    fn control_outcome() -> TaskOutcome {
        TaskOutcome {
            status: "completed".to_string(),
            summary: "child finished".to_string(),
            observations: vec!["observed".to_string()],
            evidence_refs: vec!["evidence-1".to_string()],
            recommended_actions: vec!["not-an-execution-permit".to_string()],
        }
    }

    #[tokio::test]
    async fn workflow_controls_pause_resume_preserves_wait_without_cancelling_inflight_work() {
        let fixture = fixture_workspace("ws-workflow-controls-pause").await;
        let workspace_id = "ws-workflow-controls-pause";
        let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
        seed_effect(&fixture, "control-waiting-job", EffectState::Dispatching, "waiting_job").await;
        seed_effect(
            &fixture,
            "control-needs-approval",
            EffectState::Prepared,
            "needs_approval",
        )
        .await;
        seed_decision(
            &fixture,
            "control-needs-approval",
            "tool_approval",
            DecisionState::Answered,
            Some("approved"),
            None,
        )
        .await;
        seed_control_task(&fixture, "control-waiting-watch", "waiting_watch", None).await;
        seed_control_task(&fixture, "control-running", "running", None).await;

        for task_id in [
            "control-waiting-job",
            "control-needs-approval",
            "control-waiting-watch",
            "control-running",
        ] {
            controls.pause(task_id).await.expect("pause task");
            assert_eq!(control_state(&fixture, task_id).await, "paused");
        }
        let run_state: String =
            sqlx::query_scalar("SELECT state FROM agent_runs WHERE task_id = 'control-waiting-job'")
                .fetch_one(&fixture.workspace_pool().await)
                .await
                .expect("current run");
        assert_eq!(run_state, "running", "pause must not abort the current model/run token");
        assert_eq!(
            super::effect_state(&fixture, "control-waiting-job").await.0,
            "dispatching",
            "pause must not cancel an in-flight effect"
        );

        let recovery = EffectRecovery::new(Arc::new(fixture.db.clone()));
        let reconciler = super::ScriptedReconciler::new(ReconcileOutcome::Unknown {
            reason: "still running".to_string(),
        });
        let report = recovery
            .recover_workspace(workspace_id, RecoveryMode::Restart, &reconciler)
            .await
            .expect("paused recovery");
        assert!(report.dispatch_permits.is_empty(), "pause must block new dispatch");
        assert_eq!(
            super::effect_state(&fixture, "control-needs-approval").await.0,
            "prepared"
        );
        assert_ne!(
            super::effect_state(&fixture, "control-waiting-job").await.0,
            "cancelled"
        );

        // The restart recovery fences the in-flight `dispatching` Effect to
        // `unknown` with a reconciliation Decision, so resume must keep the
        // pause until the operator resolves it (N1).
        let blocked = controls.resume("control-waiting-job").await;
        assert!(
            blocked.is_err(),
            "resume must not lift a reconciliation pause: {blocked:?}"
        );
        assert_eq!(control_state(&fixture, "control-waiting-job").await, "paused");
        recovery
            .answer_reconciliation(workspace_id, "control-waiting-job", "applied", "user-1", None)
            .await
            .expect("answer reconciliation");
        recovery
            .ensure_reconciliation(workspace_id, "control-waiting-job")
            .await
            .expect("consume reconciliation");
        assert_eq!(
            controls
                .resume("control-waiting-job")
                .await
                .expect("resume after reconciliation"),
            "waiting_job"
        );
        assert_eq!(control_state(&fixture, "control-waiting-job").await, "waiting_job");

        for (task_id, expected) in [
            ("control-needs-approval", "needs_approval"),
            ("control-waiting-watch", "waiting_watch"),
            ("control-running", "running"),
        ] {
            assert_eq!(controls.resume(task_id).await.expect("resume task"), expected);
            assert_eq!(control_state(&fixture, task_id).await, expected);
        }
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_controls_production_dispatcher_rechecks_pause_and_cancel_before_send() {
        for control in ["pause", "cancel"] {
            let workspace_id = format!("ws-workflow-controls-dispatch-{control}");
            let fixture = fixture_workspace(&workspace_id).await;
            seed_effect(&fixture, "dispatch-race-effect", EffectState::Prepared, "running").await;
            seed_decision(
                &fixture,
                "dispatch-race-effect",
                "tool_approval",
                DecisionState::Answered,
                Some("approved"),
                None,
            )
            .await;
            let recovery = EffectRecovery::new(Arc::new(fixture.db.clone()));
            let reconciler = super::ScriptedReconciler::new(ReconcileOutcome::Unknown {
                reason: "unused".to_string(),
            });
            let report = recovery
                .recover_workspace(&workspace_id, RecoveryMode::Restart, &reconciler)
                .await
                .expect("commit dispatch permit");
            assert_eq!(report.dispatch_permits.len(), 1);
            let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), &workspace_id);
            if control == "pause" {
                controls
                    .pause("dispatch-race-effect")
                    .await
                    .expect("pause after commit");
            } else {
                controls
                    .cancel("dispatch-race-effect")
                    .await
                    .expect("cancel after commit");
            }
            let sender = RecordingEffectSender::default();
            let sent = EffectDispatcher::new(Arc::new(fixture.db.clone()))
                .dispatch(&workspace_id, &report.dispatch_permits[0], &sender)
                .await
                .expect("production dispatch decision");
            assert!(!sent, "{control} must suppress the physical send");
            assert!(sender.calls.lock().expect("send calls").is_empty());
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn workflow_controls_nonterminal_effect_commit_cannot_unpause_task() {
        let workspace_id = "ws-workflow-controls-pause-race";
        let fixture = fixture_workspace(workspace_id).await;
        seed_effect(&fixture, "pause-race-effect", EffectState::Prepared, "running").await;
        seed_decision(
            &fixture,
            "pause-race-effect",
            "tool_approval",
            DecisionState::Answered,
            Some("approved"),
            None,
        )
        .await;
        let item = fixture
            .db
            .list_effect_recovery(workspace_id)
            .await
            .expect("recovery item")
            .into_iter()
            .next()
            .expect("approved effect");
        let mut transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            item.job_id.as_deref(),
            "pause-race-transition",
            at(),
            &item.task_state,
            RecoveryFact::ConsumeApproval,
        )
        .expect("nonterminal dispatch projection");
        let projection = transition.run_task.as_mut().expect("Run/Task projection");
        projection.run_state = "waiting_job".to_string();
        projection.task_state = "waiting_job".to_string();
        transition.event.as_mut().expect("effect event").workspace_id = workspace_id.to_string();
        for event in &mut transition.additional_events {
            event.workspace_id = workspace_id.to_string();
        }
        let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
        controls.pause("pause-race-effect").await.expect("pause before commit");
        fixture
            .db
            .apply_workspace_transition(workspace_id, &transition.precondition.clone(), &transition)
            .await
            .expect("commit in-flight nonterminal outcome");

        assert_eq!(control_state(&fixture, "pause-race-effect").await, "paused");
        assert_eq!(
            super::effect_state(&fixture, "pause-race-effect").await.0,
            "dispatching"
        );
        let run_state: String = sqlx::query_scalar("SELECT state FROM agent_runs WHERE task_id = 'pause-race-effect'")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("Run state");
        assert_eq!(run_state, "waiting_job", "the underlying Run fact must still commit");
        assert_eq!(
            controls.resume("pause-race-effect").await.expect("resume latest wait"),
            "waiting_job"
        );
        assert_eq!(control_state(&fixture, "pause-race-effect").await, "waiting_job");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_controls_terminal_effect_commit_completes_paused_task() {
        let workspace_id = "ws-workflow-controls-pause-terminal";
        let fixture = fixture_workspace(workspace_id).await;
        seed_effect(&fixture, "pause-terminal-effect", EffectState::Dispatching, "running").await;
        let item = fixture
            .db
            .list_effect_recovery(workspace_id)
            .await
            .expect("recovery item")
            .into_iter()
            .next()
            .expect("dispatching effect");
        let mut transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            item.job_id.as_deref(),
            "pause-terminal-transition",
            at(),
            &item.task_state,
            RecoveryFact::Reconcile(ReconcileOutcome::Settled(json!({"confirmed":true}))),
        )
        .expect("terminal outcome transition");
        let projection = transition.run_task.as_mut().expect("Run/Task projection");
        projection.run_state = "completed".to_string();
        projection.task_state = "completed".to_string();
        transition.event.as_mut().expect("effect event").workspace_id = workspace_id.to_string();
        let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
        controls
            .pause("pause-terminal-effect")
            .await
            .expect("pause before terminal commit");
        fixture
            .db
            .apply_workspace_transition(workspace_id, &transition.precondition.clone(), &transition)
            .await
            .expect("commit terminal outcome while paused");

        assert_eq!(control_state(&fixture, "pause-terminal-effect").await, "completed");
        assert_eq!(
            super::effect_state(&fixture, "pause-terminal-effect").await.0,
            "completed"
        );
        assert_eq!(
            controls
                .resume("pause-terminal-effect")
                .await
                .expect("terminal resume is a no-op"),
            "completed"
        );
        assert_eq!(control_state(&fixture, "pause-terminal-effect").await, "completed");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_controls_parent_cancel_drains_children_and_fails_on_stop_unknown() {
        let fixture = fixture_workspace("ws-workflow-controls-cancel").await;
        let workspace_id = "ws-workflow-controls-cancel";
        seed_control_task(&fixture, "control-parent", "running", None).await;
        seed_effect(&fixture, "control-prepared", EffectState::Prepared, "needs_approval").await;
        seed_decision(
            &fixture,
            "control-prepared",
            "tool_approval",
            DecisionState::Requested,
            None,
            None,
        )
        .await;
        seed_effect(&fixture, "control-sent", EffectState::Dispatching, "running").await;
        seed_effect(&fixture, "control-unknown", EffectState::Unknown, "waiting_job").await;
        seed_decision(
            &fixture,
            "control-unknown",
            "effect_reconciliation",
            DecisionState::Requested,
            None,
            None,
        )
        .await;
        seed_control_task(&fixture, "control-idle", "running", Some("control-parent")).await;
        for child in ["control-prepared", "control-sent", "control-unknown"] {
            sqlx::query("UPDATE agent_tasks SET parent_task_id = 'control-parent' WHERE id = ?")
                .bind(child)
                .execute(&fixture.workspace_pool().await)
                .await
                .expect("attach child");
        }
        let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
        assert!(
            controls
                .dispatch_permitted("control-sent")
                .await
                .expect("pre-send dispatch check")
        );
        controls.cancel("control-parent").await.expect("cancel parent");
        assert!(
            !controls
                .dispatch_permitted("control-sent")
                .await
                .expect("post-cancel dispatch check")
        );
        assert!(
            !controls
                .dispatch_permitted("control-prepared")
                .await
                .expect("prepared dispatch check")
        );

        assert_eq!(control_state(&fixture, "control-parent").await, "cancelling");
        assert_eq!(control_state(&fixture, "control-prepared").await, "cancelled");
        assert_eq!(control_state(&fixture, "control-idle").await, "cancelled");
        assert_eq!(control_state(&fixture, "control-sent").await, "cancelling");
        assert_eq!(control_state(&fixture, "control-unknown").await, "cancelling");
        assert_eq!(super::effect_state(&fixture, "control-prepared").await.0, "not_applied");
        assert_eq!(super::effect_state(&fixture, "control-sent").await.0, "cancelling");
        assert_eq!(super::decision_state(&fixture, "control-prepared").await.0, "withdrawn");
        assert_eq!(
            super::decision_state(&fixture, "control-unknown").await,
            ("requested".to_string(), None),
            "reconciliation must survive cancellation"
        );
        let external_ref: String =
            sqlx::query_scalar("SELECT external_ref FROM effect_intents WHERE effect_id = 'control-sent'")
                .fetch_one(&fixture.workspace_pool().await)
                .await
                .expect("same-ID stop reference");
        assert_eq!(external_ref, "driver-call-1");

        let recovery = EffectRecovery::new(Arc::new(fixture.db.clone()));
        let reconciler = super::ScriptedReconciler::new(ReconcileOutcome::Unknown {
            reason: "stop request failed; device may still run".to_string(),
        });
        let report = recovery
            .recover_workspace(workspace_id, RecoveryMode::Restart, &reconciler)
            .await
            .expect("stop uncertainty");
        assert!(report.dispatch_permits.is_empty());
        assert_eq!(
            super::effect_state(&fixture, "control-sent").await.0,
            "cancelled_unknown"
        );
        assert_eq!(control_state(&fixture, "control-sent").await, "failed");
        assert_eq!(control_state(&fixture, "control-unknown").await, "cancelling");
        assert_eq!(
            control_state(&fixture, "control-parent").await,
            "cancelling",
            "one unknown child must keep the parent draining"
        );

        recovery
            .answer_reconciliation(workspace_id, "control-unknown", "not_applied", "operator", None)
            .await
            .expect("resolve remaining child");
        recovery
            .recover_workspace(workspace_id, RecoveryMode::Restart, &reconciler)
            .await
            .expect("finish drain");
        assert_eq!(control_state(&fixture, "control-unknown").await, "cancelled");
        assert_eq!(control_state(&fixture, "control-parent").await, "failed");
        let failure_reason: String = sqlx::query_scalar(
            "SELECT json_extract(payload_json, '$.reason') FROM session_events
             WHERE task_id = 'control-parent' AND type = 'task.failed'",
        )
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("explicit cancel_unknown failure");
        assert_eq!(failure_reason, "cancel_unknown");

        let sent_wakes = super::wake_count(&fixture, "control-sent").await;
        let outcome = control_outcome();
        assert!(
            !fixture
                .db
                .complete_subagent_task(
                    workspace_id,
                    "control-sent",
                    &outcome.status,
                    serde_json::to_value(&outcome).expect("outcome json"),
                    at(),
                )
                .await
                .expect("late outcome is ignored")
        );
        recovery
            .recover_workspace(workspace_id, RecoveryMode::Restart, &reconciler)
            .await
            .expect("late recovery is a no-op");
        assert_eq!(control_state(&fixture, "control-parent").await, "failed");
        assert_eq!(super::wake_count(&fixture, "control-parent").await, 0);
        assert_eq!(super::wake_count(&fixture, "control-sent").await, sent_wakes);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_controls_cancel_cascade_recovers_inflight_stop_after_restart() {
        for mode in [RecoveryMode::Restart, RecoveryMode::Restore] {
            let workspace_id = match mode {
                RecoveryMode::Restart => "ws-workflow-controls-cancel-restart",
                RecoveryMode::Restore => "ws-workflow-controls-cancel-restore",
            };
            let fixture = fixture_workspace(workspace_id).await;
            seed_control_task(&fixture, "restart-parent", "running", None).await;
            seed_effect(&fixture, "restart-sent", EffectState::Dispatching, "running").await;
            sqlx::query("UPDATE agent_tasks SET parent_task_id = 'restart-parent' WHERE id = 'restart-sent'")
                .execute(&fixture.workspace_pool().await)
                .await
                .expect("attach child");
            fixture
                .db
                .cancel_workflow_task(workspace_id, "restart-parent")
                .await
                .expect("commit only the task cascade before restart");
            assert_eq!(super::effect_state(&fixture, "restart-sent").await.0, "dispatching");

            let recovery = EffectRecovery::new(Arc::new(fixture.db.clone()));
            let reconciler = super::ScriptedReconciler::new(ReconcileOutcome::Unknown {
                reason: "stop result unknown".to_string(),
            });
            recovery
                .recover_workspace(workspace_id, mode, &reconciler)
                .await
                .expect("recover same-ID stop");
            assert_eq!(
                super::effect_state(&fixture, "restart-sent").await.0,
                "cancelled_unknown"
            );
            assert_eq!(control_state(&fixture, "restart-sent").await, "failed");
            assert_eq!(control_state(&fixture, "restart-parent").await, "failed");
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn workflow_controls_subagent_outcome_and_parent_wake_are_atomic_in_both_orders() {
        for outcome_first in [true, false] {
            let workspace_id = if outcome_first {
                "ws-workflow-controls-result-first"
            } else {
                "ws-workflow-controls-cancel-first"
            };
            let fixture = fixture_workspace(workspace_id).await;
            seed_control_task(&fixture, "outcome-parent", "running", None).await;
            seed_control_task(&fixture, "outcome-child", "running", Some("outcome-parent")).await;
            let outcome = control_outcome();
            let outcome_json = serde_json::to_value(&outcome).expect("outcome json");
            let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
            if outcome_first {
                assert!(
                    fixture
                        .db
                        .complete_subagent_task(
                            workspace_id,
                            "outcome-child",
                            &outcome.status,
                            outcome_json.clone(),
                            at()
                        )
                        .await
                        .expect("commit outcome")
                );
            }
            controls.cancel("outcome-parent").await.expect("cancel parent");
            assert!(
                !fixture
                    .db
                    .complete_subagent_task(workspace_id, "outcome-child", &outcome.status, outcome_json, at())
                    .await
                    .expect("late or duplicate outcome")
            );
            assert_eq!(control_state(&fixture, "outcome-parent").await, "cancelled");
            assert_eq!(
                control_state(&fixture, "outcome-child").await,
                if outcome_first { "completed" } else { "cancelled" }
            );
            assert_eq!(
                super::event_count(&fixture, "task.outcome").await,
                i64::from(outcome_first)
            );
            assert_eq!(
                super::wake_count(&fixture, "outcome-parent").await,
                i64::from(outcome_first)
            );
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn workflow_controls_outcome_event_conflict_rolls_back_child_and_parent_wake() {
        let workspace_id = "ws-workflow-controls-outcome-rollback";
        let fixture = fixture_workspace(workspace_id).await;
        seed_control_task(&fixture, "rollback-parent", "running", None).await;
        seed_control_task(&fixture, "rollback-child", "running", Some("rollback-parent")).await;
        sqlx::query(
            "INSERT INTO session_events
                (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at,
                 cause_id, schema_v, payload_digest, payload_json)
             VALUES ('task:rollback-child:outcome', 'task:rollback-child', 'production',
                     'run-rollback-child', 'rollback-child', 'conflict.marker', ?, NULL, 1,
                     'conflicting-digest', '{}')",
        )
        .bind(at().to_rfc3339())
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("conflicting event");
        let outcome = control_outcome();
        fixture
            .db
            .complete_subagent_task(
                workspace_id,
                "rollback-child",
                &outcome.status,
                serde_json::to_value(&outcome).expect("outcome json"),
                at(),
            )
            .await
            .expect_err("event conflict must roll back the outcome");
        assert_eq!(control_state(&fixture, "rollback-child").await, "running");
        assert_eq!(super::wake_count(&fixture, "rollback-parent").await, 0);
        assert_eq!(super::event_count(&fixture, "task.outcome").await, 0);
        fixture.close().await;
    }

    fn definition(objective: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            objective: objective.to_string(),
            trigger: WorkflowTrigger::Cron {
                trigger_id: "minutely".to_string(),
                expression: "0 * * * * *".to_string(),
            },
            device_selector: json!({}),
            source_selector: json!({}),
            tool_ids: vec![],
            skill_refs: vec![],
            required_capabilities: vec![],
            approval_policy: json!({}),
            evidence_policy: json!({}),
        }
    }

    #[tokio::test]
    async fn subagent_children_pin_revision_and_cannot_create_children() {
        let workspace_id = "ws-controls-version";
        let fixture = fixture_workspace(workspace_id).await;
        fixture
            .db
            .create_workflow(
                workspace_id,
                &WorkflowRevision {
                    workflow_id: "workflow-1".to_string(),
                    version: 1,
                    enabled: false,
                    definition: definition("inspect v1"),
                    created_at: at(),
                },
            )
            .await
            .expect("create workflow");
        sqlx::query(
            "INSERT INTO agent_tasks
                (id, run_id, state, state_version, workflow_id, workflow_version)
             VALUES ('root-task', 'root-run', 'running', 1, 'workflow-1', 1)",
        )
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("root task");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES ('root-run', 'root-task', 'running')")
            .execute(&fixture.workspace_pool().await)
            .await
            .expect("root run");

        fixture
            .db
            .create_subagent_task(workspace_id, "root-task", "child-task", "child-run", at())
            .await
            .expect("create child");
        fixture
            .db
            .append_workflow_revision(
                workspace_id,
                &WorkflowRevision {
                    workflow_id: "workflow-1".to_string(),
                    version: 2,
                    enabled: true,
                    definition: definition("inspect v2"),
                    created_at: at(),
                },
                1,
            )
            .await
            .expect("edit after child creation");
        let pinned: (i64, String) = sqlx::query_as(
            "SELECT t.workflow_version, r.parent_run_id FROM agent_tasks t
             JOIN agent_runs r ON r.task_id = t.id WHERE t.id = ?",
        )
        .bind("child-task")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("child lineage");
        assert_eq!(pinned, (1, "root-run".to_string()));
        fixture
            .db
            .create_subagent_task(workspace_id, "child-task", "grandchild-task", "grandchild-run", at())
            .await
            .expect_err("v1 forbids recursive subagents");
        fixture.close().await;
    }
}

mod supervisor_health_tests {
    use super::*;
    use tinyiothub_cloud::domains::agent::workflows::{
        RetryOutcome, StalledReason, SupervisorHealth, SupervisorStatus,
    };
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

    const WS: &str = "ws-supervisor-health";
    const OTHER_WS: &str = "ws-supervisor-other";

    fn seconds(value: i64) -> Duration {
        Duration::seconds(value)
    }

    fn cron_definition() -> serde_json::Value {
        json!({
            "objective": "supervisor health fixture",
            "trigger": {"type": "cron", "trigger_id": "t1", "expression": "0 0 0 1 1 *"},
            "deviceSelector": {},
            "sourceSelector": {},
            "toolIds": [],
            "skillRefs": [],
            "requiredCapabilities": [],
            "approvalPolicy": {},
            "evidencePolicy": {},
        })
    }

    async fn seed_enabled_cron(
        fixture: &WorkspaceFixture,
        workflow_id: &str,
        enabled_at: DateTime<Utc>,
        next_slot: Option<DateTime<Utc>>,
    ) {
        let pool = fixture.workspace_pool().await;
        sqlx::query(
            "INSERT INTO agent_workflows (workflow_id, enabled, current_version, updated_at)
             VALUES (?, 1, 1, ?)",
        )
        .bind(workflow_id)
        .bind(enabled_at)
        .execute(&pool)
        .await
        .expect("seed enabled workflow");
        sqlx::query(
            "INSERT INTO workflow_revisions (workflow_id, version, definition_json, created_at)
             VALUES (?, 1, ?, ?)",
        )
        .bind(workflow_id)
        .bind(cron_definition().to_string())
        .bind(enabled_at)
        .execute(&pool)
        .await
        .expect("seed workflow revision");
        if let Some(next_slot) = next_slot {
            sqlx::query(
                "INSERT INTO workflow_cron_cursors (workflow_id, trigger_id, next_slot)
                 VALUES (?, 't1', ?)",
            )
            .bind(workflow_id)
            .bind(next_slot)
            .execute(&pool)
            .await
            .expect("seed cron cursor");
        }
    }

    async fn seed_dead_wake(
        fixture: &WorkspaceFixture,
        workspace_id: &str,
        task_id: &str,
        event_id: &str,
        reason: Option<&str>,
        created_at: DateTime<Utc>,
    ) {
        if workspace_id != fixture.workspace_id {
            sqlx::query(
                "INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at)
                 VALUES (?, 'other', 'tenant-default-001', 'now', 'now')",
            )
            .bind(workspace_id)
            .execute(fixture.db.pool())
            .await
            .expect("seed other workspace registry");
            fixture
                .db
                .create_workspace_store(workspace_id)
                .await
                .expect("create other workspace store");
        }
        let pool = fixture
            .db
            .workspace_pool_for_testing(workspace_id)
            .await
            .expect("dead wake pool");
        sqlx::query("INSERT INTO agent_tasks (id, run_id, state, state_version) VALUES (?, ?, 'running', 1)")
            .bind(task_id)
            .bind(format!("run-{task_id}"))
            .execute(&pool)
            .await
            .expect("dead wake target task");
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, failure_reason)
             VALUES (?, ?, ?, 'dead', ?, ?)",
        )
        .bind(task_id)
        .bind(event_id)
        .bind(json!({"workspace_id": workspace_id}).to_string())
        .bind(created_at)
        .bind(reason)
        .execute(&pool)
        .await
        .expect("seed dead wake");
    }

    fn health(fixture: &WorkspaceFixture) -> SupervisorHealth {
        SupervisorHealth::new(fixture.db.clone(), WS)
    }

    #[tokio::test]
    async fn supervisor_health_without_enabled_workflows_is_normal_idle_not_stalled() {
        let fixture = fixture_workspace(WS).await;
        let snapshot = health(&fixture).snapshot_at(at()).await.expect("idle snapshot");

        assert_eq!(snapshot.status, SupervisorStatus::Idle);
        assert_eq!(snapshot.stalled_reason, None);
        assert_eq!(snapshot.tick_seq, 0);
        assert_eq!(snapshot.enabled_workflows, 0);
        assert!(snapshot.no_new_runs, "idle must never claim new Runs");
        assert_eq!(event_count(&fixture, "supervisor.stalled").await, 0);
        assert_eq!(event_count(&fixture, "supervisor.recovered").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_far_next_wake_is_healthy_and_never_stalled() {
        let fixture = fixture_workspace(WS).await;
        let far = at() + seconds(6 * 3600);
        seed_enabled_cron(&fixture, "workflow-far", at(), Some(far)).await;

        let health = health(&fixture);
        let started = health.record_tick_started(at()).await.expect("tick start");
        assert_eq!(started, 1);
        health.record_tick_completed(at()).await.expect("tick complete");

        let snapshot = health.snapshot_at(at()).await.expect("healthy snapshot");
        assert_eq!(snapshot.status, SupervisorStatus::Healthy);
        assert_eq!(snapshot.stalled_reason, None);
        assert_eq!(snapshot.next_wake_at, Some(far));
        assert_eq!(snapshot.enabled_workflows, 1);
        assert_eq!(snapshot.last_completed_at, Some(at()));
        assert_eq!(event_count(&fixture, "supervisor.stalled").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_tick_deadline_stalls_once_and_success_recovers() {
        let fixture = fixture_workspace(WS).await;
        let health = health(&fixture);
        health.record_tick_started(at()).await.expect("tick start");

        let stalled = health.snapshot_at(at() + seconds(31)).await.expect("stalled snapshot");
        assert_eq!(stalled.status, SupervisorStatus::Stalled);
        assert_eq!(stalled.stalled_reason, Some(StalledReason::TickDeadlineExceeded));

        // Only the state transition is permanent; repeated observation is not.
        health.observe_at(at() + seconds(31)).await.expect("observe stall");
        health.observe_at(at() + seconds(32)).await.expect("observe same stall");
        assert_eq!(event_count(&fixture, "supervisor.stalled").await, 1);
        assert_eq!(event_count(&fixture, "supervisor.recovered").await, 0);

        // A successful tick clears the recovering state and writes one recovery.
        health
            .record_tick_completed(at() + seconds(33))
            .await
            .expect("tick complete");
        let recovered = health.observe_at(at() + seconds(33)).await.expect("observe recovery");
        assert_eq!(recovered.status, SupervisorStatus::Idle);
        assert_eq!(recovered.stalled_reason, None);
        health.observe_at(at() + seconds(34)).await.expect("repeat observe");
        assert_eq!(event_count(&fixture, "supervisor.stalled").await, 1);
        assert_eq!(event_count(&fixture, "supervisor.recovered").await, 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_due_not_started_is_stalled_while_far_wake_is_not() {
        let fixture = fixture_workspace(WS).await;
        seed_enabled_cron(&fixture, "workflow-due", at() - seconds(120), Some(at() - seconds(60))).await;

        let stalled = health(&fixture).snapshot_at(at()).await.expect("due snapshot");
        assert_eq!(stalled.status, SupervisorStatus::Stalled);
        assert_eq!(stalled.stalled_reason, Some(StalledReason::DueNotStarted));
        assert_eq!(stalled.next_wake_at, Some(at() - seconds(60)));
        assert_eq!(stalled.scheduling_delay_seconds, 60);

        // A cursorless enabled Workflow is due when the scheduler never even
        // initialized its plan, but not before the grace window.
        let fixture_fresh = fixture_workspace("ws-supervisor-cursorless").await;
        seed_enabled_cron(&fixture_fresh, "workflow-no-cursor", at(), None).await;
        let fresh = SupervisorHealth::new(fixture_fresh.db.clone(), "ws-supervisor-cursorless");
        let not_yet_due = fresh.snapshot_at(at()).await.expect("within grace");
        assert_eq!(not_yet_due.stalled_reason, None);
        let due = fresh.snapshot_at(at() + seconds(31)).await.expect("past grace");
        assert_eq!(due.stalled_reason, Some(StalledReason::DueNotStarted));
        fixture.close().await;
        fixture_fresh.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_persisted_utc_rollback_is_stalled_across_restart() {
        let fixture = fixture_workspace(WS).await;
        let pool = fixture.workspace_pool().await;
        sqlx::query("UPDATE supervisor_state SET tick_seq = 7, last_started_at = ?, last_completed_at = ?")
            .bind(at() + seconds(3600))
            .bind(at() + seconds(3600))
            .execute(&pool)
            .await
            .expect("seed future persisted state");

        let snapshot = health(&fixture).snapshot_at(at()).await.expect("rollback snapshot");
        assert_eq!(snapshot.status, SupervisorStatus::Stalled);
        assert_eq!(snapshot.stalled_reason, Some(StalledReason::ClockRollback));
        assert_eq!(snapshot.last_completed_at, Some(at() + seconds(3600)));
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_ai_heartbeat_never_masks_a_stalled_scheduler() {
        let fixture = fixture_workspace(WS).await;
        seed_enabled_cron(
            &fixture,
            "workflow-masked",
            at() - seconds(120),
            Some(at() - seconds(60)),
        )
        .await;
        fixture
            .db
            .append_session_event(&HarnessEventInput {
                event_id: "fresh-ai-heartbeat".to_string(),
                workspace_id: WS.to_string(),
                session_id: format!("supervisor:{WS}"),
                execution_mode: ExecutionMode::Production,
                run_id: None,
                task_id: None,
                event_type: "heartbeat.completed".to_string(),
                occurred_at: at(),
                cause_id: None,
                schema_v: 1,
                payload_digest: "digest-fresh-ai-heartbeat".to_string(),
                payload_json: json!({"status": "ok"}),
            })
            .await
            .expect("seed AI heartbeat");

        let snapshot = health(&fixture).snapshot_at(at()).await.expect("health snapshot");
        assert_eq!(snapshot.status, SupervisorStatus::Stalled);
        assert_eq!(
            snapshot.stalled_reason,
            Some(StalledReason::DueNotStarted),
            "AI heartbeat is not process liveness"
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_dead_wake_retry_is_identity_preserving_and_workspace_scoped() {
        let fixture = fixture_workspace(WS).await;
        seed_dead_wake(
            &fixture,
            WS,
            "task-original",
            "event-original",
            Some("provider timeout"),
            at(),
        )
        .await;
        seed_dead_wake(
            &fixture,
            WS,
            "task-sibling",
            "event-sibling",
            Some("worker unavailable"),
            at() + seconds(1),
        )
        .await;
        seed_dead_wake(
            &fixture,
            OTHER_WS,
            "task-foreign",
            "event-foreign",
            Some("other workspace failure"),
            at(),
        )
        .await;

        let health = health(&fixture);
        let dead = health.dead_wakes(20).await.expect("dead wakes");
        assert_eq!(dead.len(), 2, "only this workspace's failures are visible");
        assert_eq!(dead[0].event_id, "event-original");
        assert_eq!(dead[0].task_id, "task-original");
        assert_eq!(dead[0].failure_reason.as_deref(), Some("provider timeout"));

        assert_eq!(
            health
                .retry_dead_wake(WS, "task-original", "event-original", "admin-user")
                .await
                .expect("first retry"),
            RetryOutcome::Retried,
            "confirming a dead wake retries the original event"
        );
        let pool = fixture.workspace_pool().await;
        let (state, retried_by): (String, Option<String>) =
            sqlx::query_as("SELECT dispatch_state, retried_by FROM task_wake_outbox WHERE event_id = 'event-original'")
                .fetch_one(&pool)
                .await
                .expect("retried row");
        assert_eq!(state, "pending");
        assert_eq!(retried_by.as_deref(), Some("admin-user"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&pool)
            .await
            .expect("outbox count");
        assert_eq!(count, 2, "retry must not replace or duplicate the original event");

        assert_eq!(
            health
                .retry_dead_wake(WS, "task-original", "event-original", "admin-user")
                .await
                .expect("idempotent retry"),
            RetryOutcome::AlreadyPending,
            "a repeated retry returns the same pending fact"
        );
        let sibling: String =
            sqlx::query_scalar("SELECT dispatch_state FROM task_wake_outbox WHERE event_id = 'event-sibling'")
                .fetch_one(&pool)
                .await
                .expect("sibling untouched");
        assert_eq!(sibling, "dead", "no bulk retry is allowed");
        assert_eq!(
            health
                .retry_dead_wake(WS, "task-foreign", "event-foreign", "admin-user")
                .await
                .expect("foreign retry"),
            RetryOutcome::NotFound,
            "another workspace's failures are never retried"
        );
        let foreign: String =
            sqlx::query_scalar("SELECT dispatch_state FROM task_wake_outbox WHERE event_id = 'event-foreign'")
                .fetch_one(
                    &fixture
                        .db
                        .workspace_pool_for_testing(OTHER_WS)
                        .await
                        .expect("other pool"),
                )
                .await
                .expect("foreign row");
        assert_eq!(foreign, "dead");

        fixture
            .db
            .close_workspace_store(OTHER_WS)
            .await
            .expect("close other workspace");
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_dead_wake_retry_keys_on_task_and_event_and_refuses_terminal_targets() {
        let fixture = fixture_workspace(WS).await;
        // The outbox primary key is (task_id, event_id): one event ID may
        // exist under two Tasks and each row is retried independently.
        seed_dead_wake(&fixture, WS, "task-a", "event-shared", Some("a failed"), at()).await;
        seed_dead_wake(&fixture, WS, "task-b", "event-shared", Some("b failed"), at()).await;
        let health = health(&fixture);
        assert_eq!(
            health
                .retry_dead_wake(WS, "task-a", "event-shared", "admin-user")
                .await
                .expect("retry task-a"),
            RetryOutcome::Retried
        );
        let pool = fixture.workspace_pool().await;
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT task_id, dispatch_state FROM task_wake_outbox WHERE event_id = 'event-shared' ORDER BY task_id",
        )
        .fetch_all(&pool)
        .await
        .expect("shared event rows");
        assert_eq!(
            rows,
            [
                ("task-a".to_string(), "pending".to_string()),
                ("task-b".to_string(), "dead".to_string())
            ],
            "exactly one (task, event) row is retried"
        );
        assert_eq!(
            health
                .retry_dead_wake(WS, "missing-task", "event-shared", "admin-user")
                .await
                .expect("wrong task identity"),
            RetryOutcome::NotFound
        );

        // A dead wake produced by cancellation must not be replayed.
        seed_dead_wake(
            &fixture,
            WS,
            "task-cancelled",
            "event-cancelled",
            Some("cancelled"),
            at(),
        )
        .await;
        sqlx::query("UPDATE agent_tasks SET state = 'cancelled' WHERE id = 'task-cancelled'")
            .execute(&pool)
            .await
            .expect("cancel target");
        assert_eq!(
            health
                .retry_dead_wake(WS, "task-cancelled", "event-cancelled", "admin-user")
                .await
                .expect("terminal target"),
            RetryOutcome::TargetNotEligible
        );
        let cancelled_state: String =
            sqlx::query_scalar("SELECT dispatch_state FROM task_wake_outbox WHERE task_id = 'task-cancelled'")
                .fetch_one(&pool)
                .await
                .expect("cancelled wake");
        assert_eq!(cancelled_state, "dead");

        fixture.close().await;
    }

    #[tokio::test]
    async fn workspace_cancel_marks_pending_wakes_dead_with_a_reason() {
        let fixture = fixture_workspace(WS).await;
        let now = Utc::now();

        // Workspace cancellation writer: Task cancel marks its pending wakes
        // dead with the `cancelled` reason, and retry refuses the terminal Task.
        let pool = fixture.workspace_pool().await;
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
             VALUES ('task-cancel', 'run-cancel', 'running', 1, 'production')",
        )
        .execute(&pool)
        .await
        .expect("workspace task");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES ('run-cancel', 'task-cancel', 'running')")
            .execute(&pool)
            .await
            .expect("workspace run");
        sqlx::query(
            "INSERT INTO task_wake_outbox (task_id, event_id, signal_json, dispatch_state, created_at)
             VALUES ('task-cancel', 'event-cancel', '{}', 'pending', ?)",
        )
        .bind(now)
        .execute(&pool)
        .await
        .expect("workspace pending wake");
        fixture
            .db
            .cancel_workflow_task(WS, "task-cancel")
            .await
            .expect("cancel workflow task");
        let (state, reason): (String, Option<String>) = sqlx::query_as(
            "SELECT dispatch_state, failure_reason FROM task_wake_outbox
             WHERE task_id = 'task-cancel' AND event_id = 'event-cancel'",
        )
        .fetch_one(&pool)
        .await
        .expect("cancelled dead wake");
        assert_eq!(state, "dead");
        assert_eq!(reason.as_deref(), Some("cancelled"));
        assert_eq!(
            health(&fixture)
                .retry_dead_wake(WS, "task-cancel", "event-cancel", "admin-user")
                .await
                .expect("cancelled target retry"),
            RetryOutcome::TargetNotEligible
        );

        fixture.close().await;
    }

    /// N6: a Task with an accepted/running workspace Job must stay `cancelling`
    /// after its other Effects/children settle, so the later
    /// `cancelled_unknown -> failed` decision still has a Task to finalize.
    #[tokio::test]
    async fn cancelling_task_with_a_running_device_job_is_not_drained_to_terminal() {
        let fixture = fixture_workspace("ws-cancel-job-drain").await;
        let pool = fixture.workspace_pool().await;
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
             VALUES ('task-job-cancel', 'run-job-cancel', 'running', 1, 'production')",
        )
        .execute(&pool)
        .await
        .expect("task");
        sqlx::query(
            "INSERT INTO device_jobs
                (id, workspace_id, device_id, tool, args_json, state, task_id, run_id,
                 tool_call_id, timeout_ms, started_at)
             VALUES ('job-cancel-active', 'ws-cancel-job-drain', 'device-1', 'set_speed',
                     '{}', 'running', 'task-job-cancel', 'run-job-cancel', 'call-job-cancel',
                     30000, ?)",
        )
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("running job");

        fixture
            .db
            .cancel_workflow_task("ws-cancel-job-drain", "task-job-cancel")
            .await
            .expect("cancel");
        let state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'task-job-cancel'")
            .fetch_one(&pool)
            .await
            .expect("state");
        assert_eq!(state, "cancelling");

        // Once the Job leaves the active states, the same drain finalizes it.
        sqlx::query("UPDATE device_jobs SET state = 'done', ended_at = ? WHERE id = 'job-cancel-active'")
            .bind(Utc::now())
            .execute(&pool)
            .await
            .expect("finish job");
        fixture
            .db
            .drain_workflow_cancellation("ws-cancel-job-drain")
            .await
            .expect("drain");
        let state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'task-job-cancel'")
            .fetch_one(&pool)
            .await
            .expect("state");
        assert_eq!(state, "cancelled");

        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_healthy_tick_writes_no_events_and_failed_tick_records_error_and_recovers() {
        let fixture = fixture_workspace(WS).await;
        let health = health(&fixture);

        // Healthy tick: zero permanent events overall, not just zero statuses.
        health.record_tick_started(at()).await.expect("healthy start");
        health.record_tick_completed(at()).await.expect("healthy complete");
        assert_eq!(
            event_count(&fixture, "supervisor.stalled").await + event_count(&fixture, "supervisor.recovered").await,
            0
        );
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("event total");
        assert_eq!(total, 0, "a normal tick must not append permanent events");

        // A completed failed tick records last_error but is not a stall.
        health.record_tick_started(at()).await.expect("failed start");
        health
            .record_tick_error(at(), "provider timeout")
            .await
            .expect("failed tick");
        let failed = health.snapshot_at(at()).await.expect("failed snapshot");
        assert_eq!(failed.status, SupervisorStatus::Idle);
        assert_eq!(failed.last_error.as_deref(), Some("provider timeout"));

        // A tick-deadline stall recovers on the next completed tick even when
        // that tick records an error (L4: completed, not necessarily successful).
        health
            .record_tick_started(at() + seconds(1))
            .await
            .expect("stall start");
        assert_eq!(
            health
                .snapshot_at(at() + seconds(32))
                .await
                .expect("stalled snapshot")
                .stalled_reason,
            Some(StalledReason::TickDeadlineExceeded)
        );
        health.observe_at(at() + seconds(32)).await.expect("record stall");
        health
            .record_tick_error(at() + seconds(33), "still failing")
            .await
            .expect("failed completion");
        let recovered = health.observe_at(at() + seconds(33)).await.expect("recover on error");
        assert_eq!(recovered.stalled_reason, None);
        assert_eq!(recovered.last_error.as_deref(), Some("still failing"));
        assert_eq!(event_count(&fixture, "supervisor.stalled").await, 1);
        assert_eq!(event_count(&fixture, "supervisor.recovered").await, 1);

        // A reason change inside the stalled state updates the row but writes
        // no new permanent event.
        sqlx::query("UPDATE supervisor_state SET stalled_reason = 'due_not_started', last_completed_at = ?")
            .bind(at() + seconds(3600))
            .execute(&fixture.workspace_pool().await)
            .await
            .expect("seed stalled reason");
        health.observe_at(at()).await.expect("reason change");
        let persisted: Option<String> = sqlx::query_scalar("SELECT stalled_reason FROM supervisor_state WHERE id = 1")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("persisted reason");
        assert_eq!(persisted.as_deref(), Some("clock_rollback"));
        assert_eq!(
            event_count(&fixture, "supervisor.stalled").await + event_count(&fixture, "supervisor.recovered").await,
            2,
            "stalled→stalled reason change must not append an event"
        );

        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_active_work_without_workflows_is_healthy_not_idle() {
        let fixture = fixture_workspace(WS).await;
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
             VALUES ('main-active', 'run-active', 'running', 1, 'production')",
        )
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("active main");
        let snapshot = health(&fixture).snapshot_at(at()).await.expect("active snapshot");
        assert_eq!(snapshot.status, SupervisorStatus::Healthy);
        assert_eq!(snapshot.active_mains, 1);
        assert!(!snapshot.no_new_runs);
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_bootstrap_wiring_recovers_existing_stores_only() {
        let fixture = fixture_workspace(WS).await;
        let pool = fixture.workspace_pool().await;
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
             VALUES ('task-boot', 'run-boot', 'running', 1, 'production')",
        )
        .execute(&pool)
        .await
        .expect("bootstrap task");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state, execution_mode) VALUES ('run-boot', 'task-boot', 'running', 'production')")
            .execute(&pool)
            .await
            .expect("bootstrap run");
        sqlx::query(
            "INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws-boot-no-store', 'none', 'tenant-default-001', 'now', 'now')",
        )
        .execute(fixture.db.pool())
        .await
        .expect("storeless workspace");

        tinyiothub_cloud::bootstrap::reconcile_harness_runs(&fixture.db).await;

        let run_state: String = sqlx::query_scalar("SELECT state FROM agent_runs WHERE id = 'run-boot'")
            .fetch_one(&pool)
            .await
            .expect("boot run state");
        assert_eq!(run_state, "interrupted");
        let task_state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = 'task-boot'")
            .fetch_one(&pool)
            .await
            .expect("boot task state");
        assert_eq!(task_state, "interrupted");
        assert_eq!(event_count(&fixture, "run.interrupted").await, 1);
        assert!(
            !fixture
                .root
                .join("data/agents/ws-boot-no-store/harness/harness.db")
                .exists(),
            "bootstrap recovery must not create empty stores"
        );

        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_startup_recovery_uses_the_run_mode_and_does_not_create_stores() {
        let fixture = fixture_workspace(WS).await;
        let pool = fixture.workspace_pool().await;
        for (run_id, task_id, mode, state) in [
            ("run-prod", "task-prod", "production", "running"),
            ("run-sim", "task-sim", "simulation", "model_calling"),
            ("run-done", "task-done", "production", "completed"),
        ] {
            sqlx::query(
                "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
                 VALUES (?, ?, 'running', 1, ?)",
            )
            .bind(task_id)
            .bind(run_id)
            .bind(mode)
            .execute(&pool)
            .await
            .expect("seed recovery task");
            sqlx::query("INSERT INTO agent_runs (id, task_id, state, execution_mode) VALUES (?, ?, ?, ?)")
                .bind(run_id)
                .bind(task_id)
                .bind(state)
                .bind(mode)
                .execute(&pool)
                .await
                .expect("seed recovery run");
        }

        let events = fixture
            .db
            .recover_interrupted_harness_runs(WS, at())
            .await
            .expect("startup Harness recovery");
        assert_eq!(events.len(), 2, "only in-flight Runs are interrupted");
        let modes: Vec<ExecutionMode> = events.iter().map(|event| event.execution_mode).collect();
        assert!(modes.contains(&ExecutionMode::Production));
        assert!(modes.contains(&ExecutionMode::Simulation), "simulation stays isolated");
        assert!(
            events
                .iter()
                .all(|event| event.event_type == "run.interrupted" && event.payload_json["incomplete_tail"] == true)
        );
        let (task_state, run_state): (String, String) = sqlx::query_as(
            "SELECT t.state, r.state FROM agent_tasks t JOIN agent_runs r ON r.task_id = t.id
             WHERE t.id = 'task-prod'",
        )
        .fetch_one(&pool)
        .await
        .expect("recovered prod state");
        assert_eq!(task_state, "interrupted");
        assert_eq!(run_state, "interrupted");
        let completed: String = sqlx::query_scalar("SELECT state FROM agent_runs WHERE id = 'run-done'")
            .fetch_one(&pool)
            .await
            .expect("terminal run");
        assert_eq!(completed, "completed");
        assert!(
            fixture
                .db
                .recover_interrupted_harness_runs(WS, at())
                .await
                .expect("idempotent recovery")
                .is_empty()
        );

        // A registered Workspace that never opened a Harness store is skipped.
        sqlx::query(
            "INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws-no-harness-store', 'none', 'tenant-default-001', 'now', 'now')",
        )
        .execute(fixture.db.pool())
        .await
        .expect("seed storeless workspace");
        assert!(
            fixture
                .db
                .recover_interrupted_harness_runs("ws-no-harness-store", at())
                .await
                .expect("storeless recovery")
                .is_empty()
        );
        assert!(
            !fixture
                .root
                .join("data/agents/ws-no-harness-store/harness/harness.db")
                .exists(),
            "startup recovery must not create an empty store"
        );
        fixture.close().await;
    }
}

mod compaction_tests {
    use std::sync::Arc;

    use super::{WS, at, effect_state, fixture, run_task_states, seed_effect};
    use serde_json::json;
    use tinyiothub_cloud::domains::agent::workflows::{
        ProjectionComparison, ProjectionComparisonStatus, ProjectionInspector,
    };
    use tinyiothub_core::harness::{EffectState, ExecutionMode, HarnessEventInput};
    use tinyiothub_storage::DbError;
    use tinyiothub_storage::test_helpers::WorkspaceFixture;

    const PROJECTION: &str = "supervisor";
    const SUBJECT: &str = "supervisor:ws-recovery";

    fn event_input(event_id: &str, text: &str) -> HarnessEventInput {
        HarnessEventInput {
            event_id: event_id.to_string(),
            workspace_id: WS.to_string(),
            session_id: SUBJECT.to_string(),
            execution_mode: ExecutionMode::Production,
            run_id: Some("run-compaction".to_string()),
            task_id: Some("task-compaction".to_string()),
            event_type: "step.text".to_string(),
            occurred_at: at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("digest-{event_id}"),
            payload_json: json!({ "text": text }),
        }
    }

    async fn append_text(fixture: &WorkspaceFixture, event_id: &str, text: &str) -> i64 {
        fixture
            .db
            .append_session_event(&event_input(event_id, text))
            .await
            .expect("append compaction event")
            .seq
    }

    async fn seed_snapshot(fixture: &WorkspaceFixture, last_seq: i64, state_version: i64, state: &serde_json::Value) {
        sqlx::query(
            "INSERT INTO projection_snapshots
                (projection_name, subject_id, last_seq, projection_version, state_version, state_json, updated_at)
             VALUES (?, ?, ?, 1, ?, ?, ?)
             ON CONFLICT(projection_name, subject_id) DO UPDATE SET
                last_seq = excluded.last_seq,
                projection_version = excluded.projection_version,
                state_version = excluded.state_version,
                state_json = excluded.state_json,
                updated_at = excluded.updated_at",
        )
        .bind(PROJECTION)
        .bind(SUBJECT)
        .bind(last_seq)
        .bind(state_version)
        .bind(state.to_string())
        .bind(at().to_rfc3339())
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("seed projection snapshot");
    }

    async fn snapshot_row(fixture: &WorkspaceFixture) -> (String, i64, i64) {
        sqlx::query_as(
            "SELECT state_json, last_seq, state_version FROM projection_snapshots
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(PROJECTION)
        .bind(SUBJECT)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("projection snapshot row")
    }

    async fn total_events(fixture: &WorkspaceFixture) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("event count")
    }

    async fn matched_inspector(
        fixture: &WorkspaceFixture,
        last_seq: i64,
    ) -> (ProjectionInspector, ProjectionComparison) {
        let inspector = ProjectionInspector::new(fixture.db.clone(), WS);
        let missing = inspector
            .compare(WS, PROJECTION, SUBJECT, last_seq, 1)
            .await
            .expect("compare without snapshot");
        assert_eq!(missing.status, ProjectionComparisonStatus::SnapshotMissing);
        assert!(!missing.matched);
        seed_snapshot(fixture, last_seq, 3, &missing.canonical_state).await;
        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last_seq, 1)
            .await
            .expect("compare matching snapshot");
        assert_eq!(comparison.status, ProjectionComparisonStatus::Matched);
        assert!(comparison.matched);
        (inspector, comparison)
    }

    #[tokio::test]
    async fn compaction_fixed_boundary_match_replaces_snapshot_with_zero_dispatch() {
        let fixture = fixture().await;
        seed_effect(&fixture, "effect-compaction-match", EffectState::Prepared, "running").await;
        let first = append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        // Simulation facts from the same session must never enter this
        // production projection comparison.
        fixture
            .db
            .append_session_event(&HarnessEventInput {
                execution_mode: ExecutionMode::Simulation,
                ..event_input("compaction-simulation", "simulated")
            })
            .await
            .expect("append simulation event");

        let events_before = total_events(&fixture).await;
        let effect_before = effect_state(&fixture, "effect-compaction-match").await;
        let task_before = run_task_states(&fixture, "effect-compaction-match").await;
        let transitions_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM harness_transitions")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("transition count");

        let (inspector, comparison) = matched_inspector(&fixture, last).await;
        assert_eq!(comparison.boundary_last_seq, last);
        assert_eq!(comparison.projection_version, 1);
        assert_eq!(comparison.seq_first, Some(first));
        assert_eq!(comparison.seq_last, Some(last));
        assert_eq!(comparison.last_complete_seq, last);
        assert_eq!(comparison.evidence_refs.len(), 2);
        assert!(
            comparison
                .evidence_refs
                .iter()
                .all(|evidence| !evidence.reference.contains("simulation"))
        );
        assert_eq!(
            comparison.canonical_hash,
            comparison.snapshot_canonical_hash.clone().expect("snapshot hash")
        );

        let state_version = comparison.snapshot_state_version.expect("snapshot state version");
        let replaced = inspector
            .replace_snapshot(&comparison, last, state_version)
            .await
            .expect("CAS replace matched boundary");
        assert_eq!(replaced.last_seq, last);
        assert_eq!(replaced.projection_version, 1);
        assert_eq!(replaced.state_version, state_version + 1);
        assert_eq!(replaced.state_json, comparison.canonical_state);

        // Session facts, Task/Run, Decision/Effect and external dispatch stay
        // untouched; only the projection snapshot row is replaced.
        assert_eq!(total_events(&fixture).await, events_before);
        assert_eq!(effect_state(&fixture, "effect-compaction-match").await, effect_before);
        assert_eq!(run_task_states(&fixture, "effect-compaction-match").await, task_before);
        let transitions_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM harness_transitions")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("transition count");
        assert_eq!(transitions_after, transitions_before);

        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_hash_mismatch_keeps_latest_snapshot_and_reports_differences() {
        let fixture = fixture().await;
        let first = append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let (inspector, _) = matched_inspector(&fixture, last).await;

        let tampered = json!({
            "projectionName": PROJECTION,
            "subjectId": SUBJECT,
            "lastSeq": last,
            "events": [{
                "seq": first,
                "eventId": "compaction-1",
                "type": "step.text",
                "occurredAt": at().to_rfc3339(),
                "causeId": null,
                "payloadDigest": "digest-compaction-1",
                "payload": { "text": "tampered" }
            }]
        });
        seed_snapshot(&fixture, last, 3, &tampered).await;
        let before = snapshot_row(&fixture).await;

        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last, 1)
            .await
            .expect("compare mismatch");
        assert_eq!(comparison.status, ProjectionComparisonStatus::HashMismatch);
        assert!(!comparison.matched);
        assert!(!comparison.differences.is_empty());
        assert!(
            comparison
                .differences
                .iter()
                .any(|difference| difference.path.contains("/payload/text"))
        );
        assert_ne!(
            comparison.canonical_hash,
            comparison.snapshot_canonical_hash.clone().unwrap()
        );

        let error = inspector
            .replace_snapshot(&comparison, last, 3)
            .await
            .expect_err("mismatch must never replace the snapshot");
        assert!(matches!(error, DbError::Conflict { .. } | DbError::Validation { .. }));
        assert_eq!(snapshot_row(&fixture).await, before);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_append_during_compare_keeps_latest_snapshot() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let (inspector, _) = matched_inspector(&fixture, last).await;
        let before = snapshot_row(&fixture).await;

        let db = fixture.db.clone();
        let inspector = inspector.with_compare_barrier(Arc::new(move || {
            let db = db.clone();
            Box::pin(async move {
                db.append_session_event(&event_input("compaction-during", "arrived during rebuild"))
                    .await
                    .expect("append during compare");
            })
        }));

        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last, 1)
            .await
            .expect("compare with concurrent append");
        assert!(comparison.appended_during_compare);
        assert_eq!(comparison.status, ProjectionComparisonStatus::StaleAppend);
        assert!(!comparison.matched);
        assert!(comparison.observed_high_watermark > last);

        let error = inspector
            .replace_snapshot(&comparison, last, 3)
            .await
            .expect_err("a stale comparison must never replace");
        assert!(matches!(error, DbError::Conflict { .. } | DbError::Validation { .. }));
        assert_eq!(snapshot_row(&fixture).await, before);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_state_version_change_after_compare_never_overwrites_newer_snapshot() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let (inspector, comparison) = matched_inspector(&fixture, last).await;
        let expected_state_version = comparison.snapshot_state_version.expect("snapshot version");

        // A newer writer bumps state_version after the comparison was built.
        sqlx::query(
            "UPDATE projection_snapshots SET state_version = state_version + 1
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(PROJECTION)
        .bind(SUBJECT)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("bump state version");
        let bumped = snapshot_row(&fixture).await;

        let error = inspector
            .replace_snapshot(&comparison, last, expected_state_version)
            .await
            .expect_err("state_version CAS must fail");
        assert!(matches!(error, DbError::CasConflict { .. }));
        assert_eq!(snapshot_row(&fixture).await, bumped);

        // A newer append advances the snapshot boundary; the old comparison
        // must not overwrite newer state with older state.
        let newer_state = json!({
            "projectionName": PROJECTION,
            "subjectId": SUBJECT,
            "lastSeq": last + 1,
            "events": [],
            "newer": true
        });
        sqlx::query(
            "UPDATE projection_snapshots
             SET last_seq = ?, state_version = state_version + 1, state_json = ?, updated_at = ?
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(last + 1)
        .bind(newer_state.to_string())
        .bind(at().to_rfc3339())
        .bind(PROJECTION)
        .bind(SUBJECT)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("advance snapshot");
        let advanced = snapshot_row(&fixture).await;

        let error = inspector
            .replace_snapshot(&comparison, last, expected_state_version)
            .await
            .expect_err("older boundary must never replace a newer snapshot");
        assert!(matches!(error, DbError::CasConflict { .. }));
        assert_eq!(snapshot_row(&fixture).await, advanced);
        assert_eq!(advanced.1, last + 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_snapshot_advanced_keeps_the_newer_boundary() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let (inspector, _) = matched_inspector(&fixture, last).await;
        let newer_seq = append_text(&fixture, "compaction-newer", "three").await;
        let newer_state = json!({
            "projectionName": PROJECTION,
            "subjectId": SUBJECT,
            "lastSeq": newer_seq,
            "events": [],
            "newer": true
        });
        sqlx::query(
            "UPDATE projection_snapshots
             SET last_seq = ?, state_version = state_version + 1, state_json = ?, updated_at = ?
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(newer_seq)
        .bind(newer_state.to_string())
        .bind(at().to_rfc3339())
        .bind(PROJECTION)
        .bind(SUBJECT)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("advance snapshot");
        let before = snapshot_row(&fixture).await;

        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last, 1)
            .await
            .expect("compare older boundary");
        assert_eq!(comparison.status, ProjectionComparisonStatus::SnapshotAdvanced);
        assert!(!comparison.matched);
        assert_eq!(comparison.snapshot_last_seq, Some(newer_seq));
        let error = inspector
            .replace_snapshot(&comparison, last, 3)
            .await
            .expect_err("an older boundary must not replace a newer snapshot");
        assert!(matches!(error, DbError::Conflict { .. } | DbError::Validation { .. }));
        assert_eq!(snapshot_row(&fixture).await, before);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_boundary_mismatch_keeps_the_snapshot_at_its_own_boundary() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let (inspector, _) = matched_inspector(&fixture, last).await;
        sqlx::query(
            "UPDATE projection_snapshots SET last_seq = ?
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(last - 1)
        .bind(PROJECTION)
        .bind(SUBJECT)
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("older snapshot boundary");
        let before = snapshot_row(&fixture).await;

        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last, 1)
            .await
            .expect("compare boundary mismatch");
        assert_eq!(comparison.status, ProjectionComparisonStatus::BoundaryMismatch);
        assert!(!comparison.matched);
        assert_eq!(comparison.snapshot_last_seq, Some(last - 1));
        assert!(inspector.replace_snapshot(&comparison, last, 3).await.is_err());
        assert_eq!(snapshot_row(&fixture).await, before);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_unavailable_boundary_reports_the_observed_high_watermark() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        let last = append_text(&fixture, "compaction-2", "two").await;
        let inspector = ProjectionInspector::new(fixture.db.clone(), WS);

        let comparison = inspector
            .compare(WS, PROJECTION, SUBJECT, last + 100, 1)
            .await
            .expect("compare beyond high watermark");
        assert_eq!(comparison.status, ProjectionComparisonStatus::BoundaryUnavailable);
        assert!(!comparison.matched);
        assert_eq!(comparison.observed_high_watermark, last);
        assert_eq!(comparison.last_complete_seq, last);
        assert_eq!(comparison.snapshot_last_seq, None);
        assert!(inspector.replace_snapshot(&comparison, last + 100, 1).await.is_err());
        let snapshots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projection_snapshots")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("snapshot count");
        assert_eq!(snapshots, 0, "an unavailable boundary must not create a snapshot");
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_gap_boundary_reports_the_last_production_seq_in_the_slice() {
        let fixture = fixture().await;
        append_text(&fixture, "compaction-1", "one").await;
        // An unrelated session occupies seq 2; the boundary falls inside the
        // subject's production-event gap.
        fixture
            .db
            .append_session_event(&HarnessEventInput {
                session_id: "session-other".to_string(),
                run_id: Some("run-other".to_string()),
                task_id: Some("task-other".to_string()),
                ..event_input("compaction-other", "interleaved")
            })
            .await
            .expect("interleaved event");
        let last_subject = append_text(&fixture, "compaction-3", "three").await;
        let gap_boundary = last_subject - 1;

        let (inspector, comparison) = matched_inspector(&fixture, gap_boundary).await;
        assert_eq!(comparison.boundary_last_seq, gap_boundary);
        assert_eq!(comparison.seq_first, Some(1));
        assert_eq!(comparison.seq_last, Some(1));
        assert_eq!(comparison.last_complete_seq, 1);
        assert!(comparison.last_complete_seq < gap_boundary);
        assert_eq!(comparison.observed_high_watermark, last_subject);
        assert!(comparison.matched);

        let state_version = comparison.snapshot_state_version.expect("snapshot version");
        let replaced = inspector
            .replace_snapshot(&comparison, gap_boundary, state_version)
            .await
            .expect("replace at the gap boundary");
        assert_eq!(replaced.last_seq, gap_boundary);
        fixture.close().await;
    }
}
