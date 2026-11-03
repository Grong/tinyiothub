use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use tinyiothub_agent::harness::{EffectReconciler, ReconcileOutcome};
use tinyiothub_cloud::domains::agent::runtime::controls::{CommandControl, CommandRecovery};
use tinyiothub_cloud::domains::agent::runtime::{EffectRecovery, RecoveryMode};
use tinyiothub_core::harness::{
    CommandKind, CommandState, DecisionState, EffectIntent, EffectState, ExecutionMode, HarnessCommand,
};
use tinyiothub_storage::harness_event::EventFilter;
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
    assert_eq!(job, "rejected");
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
