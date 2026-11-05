use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use tinyiothub_agent::harness::{EffectReconciler, ReconcileOutcome};
use tinyiothub_cloud::domains::agent::runtime::controls::{CommandControl, CommandRecovery};
use tinyiothub_cloud::domains::agent::runtime::{EffectRecovery, RecoveryMode};
use tinyiothub_core::harness::{
    CommandKind, CommandState, DecisionState, EffectIntent, EffectState, ExecutionMode, HarnessCommand,
    HarnessEventInput,
};
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
    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;
    use tinyiothub_agent::harness::{
        MainAgent, MainInput, ReconcileOutcome, RecoveryFact, SubagentExecutor, SubagentInput, TaskOutcome,
        reduce_recovery,
    };
    use tinyiothub_cloud::domains::agent::runtime::controls::WorkflowControls;
    use tinyiothub_cloud::domains::agent::runtime::{EffectDispatcher, EffectRecovery, EffectSender, RecoveryMode};
    use tinyiothub_cloud::domains::agent::workflows::{
        MainCoordinator, SubagentRunner, WorkflowScheduler, WorkflowService,
    };
    use tinyiothub_core::harness::{
        DecisionState, DeviceFactsSnapshot, EffectIntent, EffectState, HarnessFact, WorkflowDefinition, WorkflowTrigger,
    };
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};
    use tokio_util::sync::CancellationToken;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn cron_definition(objective: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            objective: objective.to_string(),
            trigger: WorkflowTrigger::Cron {
                trigger_id: "minutely".to_string(),
                expression: "0 * * * * *".to_string(),
            },
            device_selector: json!({"device_ids":["robot-1"]}),
            source_selector: json!({"source":"telemetry"}),
            tool_ids: vec!["read_property".to_string()],
            skill_refs: vec!["inspection@1".to_string()],
            required_capabilities: vec![],
            approval_policy: json!({"default":"required"}),
            evidence_policy: json!({"required":true}),
        }
    }

    fn event_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            trigger: WorkflowTrigger::Event {
                trigger_id: "alarm-events".to_string(),
            },
            ..cron_definition("inspect alarms")
        }
    }

    async fn event_count(fixture: &WorkspaceFixture, event_type: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = ?")
            .bind(event_type)
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("event count")
    }

    async fn occurrence_count(fixture: &WorkspaceFixture) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM workflow_occurrences")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("occurrence count")
    }

    #[tokio::test]
    async fn workflow_revisions_restart_and_source_identity_pin_the_first_occurrence() {
        let fixture = fixture_workspace("ws-workflow-revisions").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-revisions");
        let original = service
            .create(cron_definition("inspect v1"))
            .await
            .expect("blank create");
        assert_eq!(original.version, 1);
        assert!(!original.enabled);

        let enabled = service
            .set_enabled(&original.workflow_id, true)
            .await
            .expect("explicit enable");
        assert!(enabled.enabled);
        let scheduler = WorkflowScheduler::new(db.clone(), "ws-workflow-revisions");
        assert!(scheduler.tick(at()).await.expect("initial scheduler load").is_empty());

        let first = scheduler
            .tick(at() + Duration::minutes(1))
            .await
            .expect("first due slot")
            .into_iter()
            .next()
            .expect("first occurrence");
        assert_eq!(first.workflow_version, 1);
        assert_eq!(
            first.source_key,
            format!("{}:minutely:2026-11-03T00%3A01%3A00Z", original.workflow_id)
        );
        assert_eq!(first.session_id, format!("task:{}/run:{}", first.task_id, first.run_id));

        let edited = service
            .edit(&original.workflow_id, 1, cron_definition("inspect v2"))
            .await
            .expect("ordinary edit");
        assert_eq!(edited.version, 2);
        assert!(edited.enabled);
        let duplicate = scheduler
            .schedule_slot(&original.workflow_id, "minutely", at() + Duration::minutes(1))
            .await
            .expect("duplicate slot")
            .expect("existing occurrence");
        assert_eq!(duplicate.task_id, first.task_id);
        assert_eq!(duplicate.workflow_version, 1);
        assert_eq!(occurrence_count(&fixture).await, 1);

        fixture
            .db
            .close_workspace_store("ws-workflow-revisions")
            .await
            .expect("close for restart");
        fixture
            .db
            .create_workspace_store("ws-workflow-revisions")
            .await
            .expect("reopen");
        let restarted = scheduler
            .schedule_slot(&original.workflow_id, "minutely", at() + Duration::minutes(1))
            .await
            .expect("restart replay")
            .expect("persisted occurrence");
        assert_eq!(restarted.task_id, first.task_id);
        assert_eq!(restarted.workflow_version, 1);

        let rolled_back = service
            .edit(&original.workflow_id, 2, cron_definition("inspect v1"))
            .await
            .expect("rollback appends a revision");
        assert_eq!(rolled_back.version, 3);
        assert!(rolled_back.enabled);
        let rollback_replay = scheduler
            .schedule_slot(&original.workflow_id, "minutely", at() + Duration::minutes(1))
            .await
            .expect("rollback replay")
            .expect("original occurrence");
        assert_eq!(rollback_replay.task_id, first.task_id);

        let second = scheduler.tick(at() + Duration::minutes(2)).await.expect("next slot");
        assert_eq!(second[0].workflow_version, 3);
        assert_ne!(second[0].task_id, first.task_id);
        let old = service.revision(&original.workflow_id, 1).await.expect("old revision");
        assert_eq!(old.definition.objective, "inspect v1");

        service
            .set_enabled(&original.workflow_id, false)
            .await
            .expect("disable future wakes");
        let task_state: String = sqlx::query_scalar("SELECT state FROM agent_tasks WHERE id = ?")
            .bind(&second[0].task_id)
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("in-flight task state");
        assert_eq!(task_state, "pending");
        let disabled_edit = service
            .edit(&original.workflow_id, 3, cron_definition("disabled edit"))
            .await
            .expect("disabled edit");
        assert!(!disabled_edit.enabled);
        assert!(
            scheduler
                .tick(at() + Duration::minutes(3))
                .await
                .expect("disabled tick")
                .is_empty()
        );
        assert_eq!(occurrence_count(&fixture).await, 2);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_cron_tick_with_subsecond_now_schedules_latest_and_misses_only_older_slots() {
        let fixture = fixture_workspace("ws-workflow-subsecond").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-subsecond");
        let workflow = service.create(cron_definition("inspect")).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, "ws-workflow-subsecond");
        scheduler.tick(at()).await.expect("initialize cursor");

        let due = scheduler
            .tick(at() + Duration::milliseconds(120_123))
            .await
            .expect("subsecond tick");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].occurrence_id, "2026-11-03T00:02:00Z");
        let missed: Vec<String> = sqlx::query_scalar(
            "SELECT json_extract(payload_json, '$.slot') FROM session_events
             WHERE type = 'trigger.missed' ORDER BY seq",
        )
        .fetch_all(&fixture.workspace_pool().await)
        .await
        .expect("missed slots");
        assert_eq!(missed, vec!["2026-11-03T00:01:00Z"]);
        assert_eq!(occurrence_count(&fixture).await, 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_slot_replay_rejects_previously_unseen_expired_slots() {
        let fixture = fixture_workspace("ws-workflow-expired-replay").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-expired-replay");
        let workflow = service.create(cron_definition("inspect")).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, "ws-workflow-expired-replay");
        scheduler.tick(at()).await.expect("initialize cursor");

        let replay = scheduler
            .schedule_slot(&workflow.workflow_id, "minutely", at() - Duration::minutes(10))
            .await
            .expect("expired replay query");
        assert!(replay.is_none());
        assert_eq!(occurrence_count(&fixture).await, 0);
        assert_eq!(event_count(&fixture, "trigger.missed").await, 0);
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("wake count");
        assert_eq!(wakes, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_cron_misfires_skip_every_expired_slot_without_creating_work() {
        let fixture = fixture_workspace("ws-workflow-misfire").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-misfire");
        let workflow = service.create(cron_definition("inspect")).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, "ws-workflow-misfire");
        scheduler.tick(at()).await.expect("initialize cursor");

        let due = scheduler
            .tick(at() + Duration::minutes(4))
            .await
            .expect("misfire window");
        assert_eq!(due.len(), 1);
        assert_eq!(
            due[0].source_key,
            format!("{}:minutely:2026-11-03T00%3A04%3A00Z", workflow.workflow_id)
        );
        assert_eq!(event_count(&fixture, "trigger.missed").await, 3);
        assert_eq!(occurrence_count(&fixture).await, 1);
        let missed_sessions: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT session_id FROM session_events WHERE type = 'trigger.missed'")
                .fetch_all(&fixture.workspace_pool().await)
                .await
                .expect("missed event sessions");
        assert_eq!(missed_sessions, vec!["supervisor:ws-workflow-misfire"]);
        let replay = fixture
            .db
            .session_event_page(
                "ws-workflow-misfire",
                &tinyiothub_storage::harness_event::EventFilter {
                    session_id: Some("supervisor:ws-workflow-misfire".to_string()),
                    run_id: None,
                    task_id: None,
                    execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
                },
                0,
                256,
            )
            .await
            .expect("replay persisted trigger facts");
        assert_eq!(replay.events.len(), 3);

        scheduler
            .tick(at() + Duration::minutes(4))
            .await
            .expect("idempotent misfire replay");
        scheduler
            .tick(at() + Duration::minutes(2))
            .await
            .expect("clock rollback");
        assert_eq!(event_count(&fixture, "trigger.missed").await, 3);
        assert_eq!(occurrence_count(&fixture).await, 1);

        let shutdown = CancellationToken::new();
        let runner = WorkflowScheduler::new(Arc::new(fixture.db.clone()), "ws-workflow-misfire");
        let stopped = shutdown.clone();
        let handle = tokio::spawn(async move { runner.run(stopped).await });
        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("scheduler shutdown")
            .expect("scheduler task");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_event_gaps_and_unreadable_sources_alert_without_synthesizing_effects() {
        let fixture = fixture_workspace("ws-workflow-events").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-events");
        let workflow = service.create(event_definition()).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, "ws-workflow-events");

        let first = scheduler
            .on_event("alarm-events", 1, "source-event-1", at())
            .await
            .expect("first source event");
        assert_eq!(first.len(), 1);
        let replay = scheduler
            .on_event("alarm-events", 1, "source-event-1", at())
            .await
            .expect("source replay");
        assert_eq!(replay[0].task_id, first[0].task_id);
        assert_eq!(occurrence_count(&fixture).await, 1);

        let gap = scheduler
            .on_event("alarm-events", 3, "source-event-3", at())
            .await
            .expect("gap is an alert, not work");
        assert!(gap.is_empty());
        scheduler
            .on_source_unavailable("alarm-events", "source cannot be read", at())
            .await
            .expect("unreadable source alert");
        assert_eq!(event_count(&fixture, "trigger.unavailable").await, 2);
        assert_eq!(occurrence_count(&fixture).await, 1);
        let effects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM effect_intents")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("effect count");
        assert_eq!(effects, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_source_identity_escapes_delimiters_across_revisions() {
        let fixture = fixture_workspace("ws-workflow-delimiters").await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), "ws-workflow-delimiters");
        let mut first_definition = event_definition();
        first_definition.trigger = WorkflowTrigger::Event {
            trigger_id: "a:b".to_string(),
        };
        let workflow = service.create(first_definition).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, "ws-workflow-delimiters");

        let first = scheduler.on_event("a:b", 1, "c", at()).await.expect("first identity");
        let mut second_definition = event_definition();
        second_definition.trigger = WorkflowTrigger::Event {
            trigger_id: "a".to_string(),
        };
        let edited = service
            .edit(&workflow.workflow_id, 1, second_definition)
            .await
            .expect("edit trigger identity");
        assert!(edited.enabled);
        let second = scheduler
            .on_event("a", 1, "b:c", at())
            .await
            .expect("distinct identity");

        assert_ne!(first[0].task_id, second[0].task_id);
        assert_eq!(first[0].workflow_version, 1);
        assert_eq!(second[0].workflow_version, 2);
        assert_eq!(first[0].source_key, format!("{}:a%3Ab:c", workflow.workflow_id));
        assert_eq!(second[0].source_key, format!("{}:a:b%3Ac", workflow.workflow_id));
        assert_eq!(occurrence_count(&fixture).await, 2);
        fixture.close().await;
    }

    #[derive(Default)]
    struct RecordingMainAgent {
        inputs: Mutex<Vec<MainInput>>,
    }

    #[async_trait]
    impl MainAgent for RecordingMainAgent {
        async fn coordinate(&self, input: MainInput) -> anyhow::Result<()> {
            self.inputs.lock().expect("main inputs").push(input);
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSubagent {
        inputs: Mutex<Vec<SubagentInput>>,
    }

    #[async_trait]
    impl SubagentExecutor for RecordingSubagent {
        async fn execute(&self, input: SubagentInput) -> anyhow::Result<TaskOutcome> {
            self.inputs.lock().expect("subagent inputs").push(input);
            Ok(TaskOutcome {
                status: "completed".to_string(),
                summary: "inspection complete".to_string(),
                observations: vec!["temperature nominal".to_string()],
                evidence_refs: vec!["evidence-1".to_string()],
                recommended_actions: vec!["set_speed".to_string()],
            })
        }
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

    fn subagent_input(task_id: &str, run_id: &str) -> SubagentInput {
        SubagentInput {
            session_id: "task:shared/run:shared".to_string(),
            task_id: task_id.to_string(),
            run_id: run_id.to_string(),
            workflow: tinyiothub_core::harness::WorkflowRevision {
                workflow_id: "workflow-1".to_string(),
                version: 7,
                enabled: true,
                definition: event_definition(),
                created_at: at(),
            },
            facts: DeviceFactsSnapshot {
                observed_at: at(),
                declaration_hash: "declaration-1".to_string(),
                driver_version: "driver-1".to_string(),
                facts: json!({"temperature":20}),
            },
            source: json!({"event_id":"source-event-1"}),
            transcript: vec![tinyiothub_agent::port::provider::ChatMessage::user(
                "shared transcript must not leak",
            )],
        }
    }

    #[tokio::test]
    async fn workflow_main_calls_are_bounded_and_subagents_have_fresh_explicit_context() {
        let fixture = fixture_workspace("ws-workflow-context").await;
        let main_agent = Arc::new(RecordingMainAgent::default());
        let main = MainCoordinator::new(Arc::new(fixture.db.clone()), "ws-workflow-context", main_agent.clone());
        let fact = HarnessFact {
            transition_id: "transition-1".to_string(),
            effect_id: "effect-1".to_string(),
            next: EffectState::Completed,
            occurred_at: at(),
            external_ref: None,
            result: Some(json!({"accepted":true})),
            job: None,
            wake: None,
        };
        main.on_fact(fact.clone()).await.expect("first bounded coordination");
        main.on_fact(fact).await.expect("second bounded coordination");
        {
            let main_inputs = main_agent.inputs.lock().expect("main inputs");
            assert_eq!(main_inputs.len(), 2);
            assert!(
                main_inputs
                    .iter()
                    .all(|input| input.session_id == "supervisor:ws-workflow-context")
            );
            assert!(main_inputs.iter().all(|input| input.transcript.is_empty()));
            assert_eq!(main_inputs[0].projection, main_inputs[1].projection);
        }

        let subagent = Arc::new(RecordingSubagent::default());
        let runner = SubagentRunner::new(subagent.clone());
        let first = runner
            .run(subagent_input("task-1", "run-1"))
            .await
            .expect("first outcome");
        let second = runner
            .run(subagent_input("task-2", "run-2"))
            .await
            .expect("second outcome");
        assert_eq!(first.status, "completed");
        assert_eq!(first.summary, "inspection complete");
        assert_eq!(first.observations, vec!["temperature nominal"]);
        assert_eq!(first.evidence_refs, vec!["evidence-1"]);
        assert_eq!(second.recommended_actions, vec!["set_speed"]);
        {
            let inputs = subagent.inputs.lock().expect("subagent inputs");
            assert_eq!(inputs.len(), 2);
            assert_eq!(inputs[0].session_id, "task:task-1/run:run-1");
            assert_eq!(inputs[1].session_id, "task:task-2/run:run-2");
            assert_ne!(inputs[0].task_id, inputs[1].task_id);
            assert!(inputs.iter().all(|input| input.transcript.is_empty()));
        }
        let effects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM effect_intents")
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("effect count");
        assert_eq!(effects, 0, "recommended actions must not execute effects");
        fixture.close().await;
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

        for (task_id, expected) in [
            ("control-waiting-job", "waiting_job"),
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
        let runner = SubagentRunner::new(Arc::new(RecordingSubagent::default()));
        assert!(
            !runner
                .commit_outcome(&fixture.db, workspace_id, "control-sent", &control_outcome(), at(),)
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
            let runner = SubagentRunner::new(Arc::new(RecordingSubagent::default()));
            let controls = WorkflowControls::new(Arc::new(fixture.db.clone()), workspace_id);
            if outcome_first {
                assert!(
                    runner
                        .commit_outcome(&fixture.db, workspace_id, "outcome-child", &control_outcome(), at())
                        .await
                        .expect("commit outcome")
                );
            }
            controls.cancel("outcome-parent").await.expect("cancel parent");
            assert!(
                !runner
                    .commit_outcome(&fixture.db, workspace_id, "outcome-child", &control_outcome(), at())
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
        let runner = SubagentRunner::new(Arc::new(RecordingSubagent::default()));
        runner
            .commit_outcome(&fixture.db, workspace_id, "rollback-child", &control_outcome(), at())
            .await
            .expect_err("event conflict must roll back the outcome");
        assert_eq!(control_state(&fixture, "rollback-child").await, "running");
        assert_eq!(super::wake_count(&fixture, "rollback-parent").await, 0);
        assert_eq!(super::event_count(&fixture, "task.outcome").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_controls_children_pin_revision_and_cannot_create_children() {
        let workspace_id = "ws-workflow-controls-version";
        let fixture = fixture_workspace(workspace_id).await;
        let db = Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), workspace_id);
        let workflow = service.create(cron_definition("inspect v1")).await.expect("create");
        service.set_enabled(&workflow.workflow_id, true).await.expect("enable");
        let scheduler = WorkflowScheduler::new(db, workspace_id);
        scheduler.tick(at()).await.expect("initialize");
        let root = scheduler
            .tick(at() + Duration::minutes(1))
            .await
            .expect("schedule")
            .into_iter()
            .next()
            .expect("root occurrence");
        let (child_id, _) = service.create_subagent(&root.task_id).await.expect("create child");
        service
            .edit(&workflow.workflow_id, 1, cron_definition("inspect v2"))
            .await
            .expect("edit after child creation");
        let pinned: (i64, String) = sqlx::query_as(
            "SELECT t.workflow_version, r.parent_run_id FROM agent_tasks t
             JOIN agent_runs r ON r.task_id = t.id WHERE t.id = ?",
        )
        .bind(&child_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("child lineage");
        assert_eq!(pinned, (1, root.run_id));
        service
            .create_subagent(&child_id)
            .await
            .expect_err("v1 forbids recursive subagents");
        fixture.close().await;
    }
}

mod workflow_template_tests {
    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;
    use tinyiothub_cloud::domains::agent::workflows::{
        TemplateInput, WorkflowScheduler, WorkflowService, WorkflowTemplate, builtin_templates, next_wake_at,
        validate_template,
    };
    use tinyiothub_core::harness::{WorkflowDefinition, WorkflowRevision, WorkflowTrigger};
    use tinyiothub_skills::registry::{ApprovalPolicy, Capability};
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    /// Seed a workspace Thing with a driver manifest that declares the given
    /// `(source_id, data_kind)` pairs. This is the reference surface template
    /// instantiation validates against.
    async fn seed_thing(fixture: &WorkspaceFixture, id: &str, sources: &[(&str, &str)]) {
        seed_thing_named(fixture, id, id, sources).await;
    }

    async fn seed_thing_named(fixture: &WorkspaceFixture, id: &str, name: &str, sources: &[(&str, &str)]) {
        let workspace_id = fixture.workspace_id.as_str();
        let pool = fixture.db.pool().clone();
        sqlx::query("INSERT INTO things (id, name, thing_type, workspace_id) VALUES (?, ?, 'device', ?)")
            .bind(id)
            .bind(name)
            .bind(workspace_id)
            .execute(&pool)
            .await
            .expect("seed thing");
        let driver = format!("driver-{id}");
        let manifest = json!({
            "sources": sources
                .iter()
                .map(|(source_id, data_kind)| json!({"id": source_id, "kind": "poll", "dataKind": data_kind}))
                .collect::<Vec<_>>(),
        });
        sqlx::query(
            "INSERT INTO driver_declarations
                (workspace_id, driver_name, driver_version, manifest_version, manifest_json,
                 manifest_hash, status, installed_at)
             VALUES (?, ?, '1.0.0', '1.0', ?, 'manifest-hash', 'active', 'now')",
        )
        .bind(workspace_id)
        .bind(&driver)
        .bind(manifest.to_string())
        .execute(&pool)
        .await
        .expect("seed driver declaration");
        sqlx::query(
            "INSERT INTO device_declarations
                (workspace_id, thing_id, driver_name, driver_version, declaration_json,
                 declaration_hash, status, updated_at)
             VALUES (?, ?, ?, '1.0.0', ?, 'declaration-hash', 'active', 'now')",
        )
        .bind(workspace_id)
        .bind(id)
        .bind(&driver)
        .bind(
            json!({"manifestVersion":"1.0","driver":driver,"driverVersion":"1.0.0","stateFields":[],"tools":[]})
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("seed device declaration");
    }

    async fn side_effects(fixture: &WorkspaceFixture) -> (usize, i64, i64) {
        let workflows = fixture
            .db
            .list_workflows(&fixture.workspace_id)
            .await
            .expect("list workflows")
            .len();
        let pool = fixture.workspace_pool().await;
        let occurrences: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflow_occurrences")
            .fetch_one(&pool)
            .await
            .expect("occurrence count");
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&pool)
            .await
            .expect("wake count");
        (workflows, occurrences, wakes)
    }

    fn input(device: &str, source: &str) -> TemplateInput {
        TemplateInput {
            workflow_id: String::new(),
            device_selector: json!({"device_ids": [device]}),
            source_selector: json!({"sources": [source]}),
            approval_policy: json!({}),
        }
    }

    #[test]
    fn workflow_templates_are_versioned_and_cover_inspection_video_energy() {
        let templates = builtin_templates();
        let ids: Vec<&str> = templates.iter().map(|template| template.id.as_str()).collect();
        assert_eq!(ids, vec!["inspection", "video", "energy"]);
        for template in &templates {
            assert!(template.version >= 1, "{} must be versioned", template.id);
            assert!(!template.title.trim().is_empty());
            validate_template(template).unwrap_or_else(|error| panic!("{} invalid: {error}", template.id));
        }
        let video = templates
            .iter()
            .find(|template| template.id == "video")
            .expect("video template");
        assert!(video.definition.required_capabilities.iter().any(|cap| cap == "vision"));
        assert!(video.definition.required_capabilities.iter().any(|cap| cap == "video"));
        assert!(!Capability::Video.is_satisfied_by(&[Capability::Video]));
        assert!(Capability::Video.is_satisfied_by(&[Capability::Vision, Capability::Video]));
    }

    #[tokio::test]
    async fn workflow_templates_instantiate_enabled_with_version_and_next_wake() {
        let workspace_id = "ws-template-direct-enabled";
        let fixture = fixture_workspace(workspace_id).await;
        seed_thing(&fixture, "pump-1", &[("telemetry", "values")]).await;
        seed_thing(&fixture, "camera-1", &[("front", "video"), ("telemetry", "values")]).await;

        let service = WorkflowService::new(std::sync::Arc::new(fixture.db.clone()), workspace_id);

        let cases = [
            ("inspection", "pump-1", "telemetry"),
            ("video", "camera-1", "front"),
            ("energy", "pump-1", "telemetry"),
        ];
        for (template_id, device, source) in cases {
            let revision = service
                .instantiate_template(template_id, input(device, source))
                .await
                .unwrap_or_else(|error| panic!("{template_id} instantiate failed: {error}"));
            assert!(revision.enabled, "{template_id} must be created enabled (DR4=B)");
            assert_eq!(revision.version, 1);
            let next = next_wake_at(&revision.definition, at()).unwrap_or_else(|| {
                panic!("{template_id} must expose a planned wake after creation");
            });
            assert!(next > at(), "{template_id} next wake must be in the future");
            assert_eq!(
                revision.definition.device_selector,
                json!({"device_ids": [device]}),
                "{template_id} must use the supplied device selector"
            );
            assert_eq!(
                revision.definition.source_selector,
                json!({"sources": [source]}),
                "{template_id} must use the supplied source selector"
            );
        }

        let (workflows, occurrences, wakes) = side_effects(&fixture).await;
        assert_eq!(workflows, 3);
        // Instantiation is a configuration action; it must not run or queue a wake.
        assert_eq!(occurrences, 0);
        assert_eq!(wakes, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_templates_invalid_references_create_zero_workflows_and_zero_scheduling() {
        let workspace_id = "ws-template-invalid";
        let fixture = fixture_workspace(workspace_id).await;
        seed_thing(&fixture, "pump-1", &[("telemetry", "values")]).await;
        seed_thing(&fixture, "camera-1", &[("front", "video"), ("telemetry", "values")]).await;
        let service = WorkflowService::new(std::sync::Arc::new(fixture.db.clone()), workspace_id);

        let cases: Vec<(&str, TemplateInput)> = vec![
            ("inspection", input("missing-device", "telemetry")),
            ("inspection", input("pump-1", "missing-source")),
            ("video", input("camera-1", "telemetry")),
            ("instantiation", input("pump-1", "telemetry")),
            (
                "inspection",
                TemplateInput {
                    workflow_id: String::new(),
                    device_selector: json!({"device_ids": []}),
                    source_selector: json!({"sources": ["telemetry"]}),
                    approval_policy: json!({}),
                },
            ),
        ];
        for (template_id, template_input) in cases {
            service
                .instantiate_template(template_id, template_input)
                .await
                .expect_err("invalid reference must be rejected");
            let (workflows, occurrences, wakes) = side_effects(&fixture).await;
            assert_eq!(workflows, 0, "rejection must not create a workflow");
            assert_eq!(occurrences, 0, "rejection must not schedule an occurrence");
            assert_eq!(wakes, 0, "rejection must not queue a wake");
        }
        fixture.close().await;
    }

    #[test]
    fn workflow_template_validation_rejects_unknown_references_and_untrusted_policy() {
        let base = builtin_templates().into_iter().next().expect("inspection");

        let unknown_tool = WorkflowTemplate {
            definition: WorkflowDefinition {
                tool_ids: vec!["not_a_real_tool".to_string()],
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(validate_template(&unknown_tool).is_err());

        let unknown_skill = WorkflowTemplate {
            definition: WorkflowDefinition {
                skill_refs: vec!["no-such-skill-reference".to_string()],
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(validate_template(&unknown_skill).is_err());

        let unknown_capability = WorkflowTemplate {
            definition: WorkflowDefinition {
                required_capabilities: vec!["telepathy".to_string()],
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(validate_template(&unknown_capability).is_err());

        let video_without_vision = WorkflowTemplate {
            id: "video".to_string(),
            definition: WorkflowDefinition {
                required_capabilities: vec!["video".to_string()],
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(validate_template(&video_without_vision).is_err());

        let bad_evidence = WorkflowTemplate {
            definition: WorkflowDefinition {
                evidence_policy: json!({"required": true, "kinds": ["invented_evidence"]}),
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(validate_template(&bad_evidence).is_err());

        let disabled_mandatory = WorkflowTemplate {
            id: "inspection".to_string(),
            definition: WorkflowDefinition {
                tool_ids: vec!["delete_thing".to_string()],
                approval_policy: json!({"mandatory": false}),
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(
            validate_template(&disabled_mandatory).is_err(),
            "a template must not disable mandatory approval"
        );

        let malformed_mandatory = WorkflowTemplate {
            definition: WorkflowDefinition {
                approval_policy: json!({"mandatory": "yes"}),
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(
            validate_template(&malformed_mandatory).is_err(),
            "a non-boolean mandatory value must not parse as no approval"
        );

        let malformed_timeout = WorkflowTemplate {
            definition: WorkflowDefinition {
                approval_policy: json!({"timeoutSecs": "120"}),
                ..base.definition.clone()
            },
            ..base.clone()
        };
        assert!(
            validate_template(&malformed_timeout).is_err(),
            "a non-integer timeout must be rejected"
        );
    }

    #[tokio::test]
    async fn workflow_templates_can_only_tighten_approval_policy() {
        let workspace_id = "ws-template-policy";
        let fixture = fixture_workspace(workspace_id).await;
        seed_thing(&fixture, "pump-1", &[("telemetry", "values")]).await;
        let service = WorkflowService::new(std::sync::Arc::new(fixture.db.clone()), workspace_id);

        service
            .instantiate_template(
                "energy",
                TemplateInput {
                    workflow_id: String::new(),
                    device_selector: json!({"device_ids": ["pump-1"]}),
                    source_selector: json!({"sources": ["telemetry"]}),
                    approval_policy: json!({"mandatory": "yes"}),
                },
            )
            .await
            .expect_err("malformed input approval must be rejected, not defaulted to none()");
        assert_eq!(side_effects(&fixture).await, (0, 0, 0));

        let revision = service
            .instantiate_template(
                "energy",
                TemplateInput {
                    workflow_id: String::new(),
                    device_selector: json!({"device_ids": ["pump-1"]}),
                    source_selector: json!({"sources": ["telemetry"]}),
                    approval_policy: json!({"requiresApproval": false, "timeoutSecs": 1}),
                },
            )
            .await
            .expect("tightening-only merge accepts the input");
        let persisted = ApprovalPolicy::from_workflow_value(&revision.definition.approval_policy)
            .expect("effective approval policy");
        assert!(persisted.mandatory, "input must not loosen mandatory approval");
        assert!(persisted.timeout_secs >= 60, "input must not shorten the window");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_templates_skip_expired_slots_without_replay() {
        let workspace_id = "ws-template-misfire";
        let fixture = fixture_workspace(workspace_id).await;
        seed_thing(&fixture, "pump-1", &[("telemetry", "values")]).await;
        let db = std::sync::Arc::new(fixture.db.clone());
        let service = WorkflowService::new(db.clone(), workspace_id);
        let revision = service
            .instantiate_template("inspection", input("pump-1", "telemetry"))
            .await
            .expect("instantiate enabled inspection");
        let scheduler = WorkflowScheduler::new(db, workspace_id);
        scheduler.tick(at()).await.expect("initialize cursor");

        let due = scheduler.tick(at() + Duration::hours(4)).await.expect("misfire window");
        assert_eq!(due.len(), 1, "only the latest due slot is scheduled");
        assert_eq!(due[0].workflow_id, revision.workflow_id);
        assert_eq!(due[0].occurrence_id, "2026-11-03T04:00:00Z");

        let pool = fixture.workspace_pool().await;
        let missed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = 'trigger.missed'")
            .fetch_one(&pool)
            .await
            .expect("missed events");
        assert_eq!(missed, 3);
        let occurrences: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflow_occurrences")
            .fetch_one(&pool)
            .await
            .expect("occurrence count");
        assert_eq!(occurrences, 1, "expired slots are recorded, never replayed");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_templates_resolve_device_names_to_canonical_ids() {
        let workspace_id = "ws-template-device-name";
        let fixture = fixture_workspace(workspace_id).await;
        // The selector uses the display name; declarations are keyed by the
        // canonical things.id. A name-only resolution must still find sources.
        seed_thing_named(&fixture, "thing-42", "pump-alpha", &[("telemetry", "values")]).await;
        let service = WorkflowService::new(std::sync::Arc::new(fixture.db.clone()), workspace_id);

        let revision = service
            .instantiate_template("inspection", input("pump-alpha", "telemetry"))
            .await
            .expect("display-name selector resolves to the canonical Thing ID");
        assert!(revision.enabled);
        assert_eq!(
            revision.definition.device_selector,
            json!({"device_ids": ["pump-alpha"]})
        );

        service
            .instantiate_template("inspection", input("pump-alpha", "missing-source"))
            .await
            .expect_err("a resolved Thing without the source is still rejected");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_templates_duplicate_ids_conflict_and_enabled_is_forced() {
        let workspace_id = "ws-template-id-conflict";
        let fixture = fixture_workspace(workspace_id).await;
        seed_thing(&fixture, "pump-1", &[("telemetry", "values")]).await;
        let service = WorkflowService::new(std::sync::Arc::new(fixture.db.clone()), workspace_id);

        let revision = service
            .instantiate_template(
                "inspection",
                TemplateInput {
                    workflow_id: "workflow-fixed".to_string(),
                    device_selector: json!({"device_ids": ["pump-1"]}),
                    source_selector: json!({"sources": ["telemetry"]}),
                    approval_policy: json!({}),
                },
            )
            .await
            .expect("first explicit ID wins");
        assert_eq!(revision.workflow_id, "workflow-fixed");

        let duplicate = fixture
            .db
            .create_workflow(workspace_id, &revision)
            .await
            .expect_err("a duplicate workflow_id must map to a conflict");
        assert!(matches!(duplicate, tinyiothub_storage::DbError::Conflict { .. }));

        let forced = fixture
            .db
            .create_enabled_workflow(
                workspace_id,
                &WorkflowRevision {
                    workflow_id: "workflow-forced-enabled".to_string(),
                    version: 1,
                    enabled: false,
                    definition: revision.definition.clone(),
                    created_at: at(),
                },
            )
            .await
            .expect("create_enabled_workflow");
        assert!(forced.enabled, "DR4=B entry point must force enabled=true");
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_templates_serialize_the_http_contract() {
        // The service contract is the authority; the handler test lives with
        // the existing harness HTTP tests. Here we assert the catalog payload
        // shape used by the UI (version + definition) is serializable.
        for template in builtin_templates() {
            let value = serde_json::to_value(&template).expect("template serializes");
            assert_eq!(value["id"], template.id);
            assert!(value["version"].as_i64().unwrap_or_default() >= 1);
            assert!(value["definition"]["trigger"]["type"].is_string());
            let trigger: WorkflowTrigger =
                serde_json::from_value(value["definition"]["trigger"].clone()).expect("trigger round-trips");
            assert!(!trigger.trigger_id().is_empty());
        }
    }
}
