//! Durable assistant suppression lifecycle tests.
//!
//! These tests pin the crash/fence contract: only a fully installed baseline
//! is eligible for a skip, every unsuccessful terminal leaves `full_failed`,
//! and a skip commit re-validates every version/cursor/fence it was decided
//! against. Suppression state and skipped events live in the workspace
//! Harness database; only the schedule configuration is platform state.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
use tinyiothub_storage::assistant_schedule::SuppressionConfig;
use tinyiothub_storage::harness_event::append_session_event_tx;
use tinyiothub_storage::suppression::{
    FullAttemptCompletion, SkipAttempt, SuppressionMode, SuppressionState, SyncPending, TURN_STATE_VERSION,
};
use tinyiothub_storage::{Db, DbError};

const WS: &str = "ws-suppression";
const SESSION: &str = "session-1";
const REGISTRY_VERSION: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_REGISTRY_VERSION: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct SuppressionFixture {
    root: PathBuf,
    db: Db,
    /// Direct connection to the workspace Harness DB, used only for raw
    /// fixtures (version/fence mutation) and transaction-level event tests.
    workspace: SqlitePool,
}

impl SuppressionFixture {
    async fn create() -> Self {
        let root = std::env::temp_dir().join(format!("tinyiothub-suppression-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("fixture root");
        let platform = tinyiothub_storage::test_helpers::test_pool().await;
        let db = Db::new(platform).with_workspace_store_root(root.join("data/agents"));
        tinyiothub_storage::seed::seed_system(&db).await.expect("seed system");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .bind(WS)
        .execute(db.pool())
        .await
        .expect("insert workspace");
        db.create_workspace_store(WS).await.expect("workspace store");
        let workspace = open_workspace_db(&workspace_path(&root, WS)).await;

        let fixture = Self { root, db, workspace };
        fixture
            .db
            .ensure_assistant_schedule(WS, "__assistant__ws-suppression", ts())
            .await
            .expect("assistant schedule");
        fixture
            .db
            .update_suppression_config(WS, Some(SuppressionMode::Enforce), None)
            .await
            .expect("enable enforce")
            .expect("schedule row");
        fixture
    }

    async fn current(&self) -> SuppressionState {
        self.db
            .load_suppression_state(WS, SESSION)
            .await
            .expect("load suppression state")
            .expect("suppression row")
    }

    async fn config_version(&self) -> i64 {
        self.db
            .get_suppression_config(WS)
            .await
            .expect("suppression config")
            .expect("schedule row")
            .config_version
    }

    /// Open a fresh workspace store connection from the same file, proving
    /// durable state rather than an in-process cache.
    async fn reopen(&self) -> SuppressionState {
        let reopened = Db::new(self.db.pool().clone()).with_workspace_store_root(self.root.join("data/agents"));
        reopened
            .load_suppression_state(WS, SESSION)
            .await
            .expect("load after reopen")
            .expect("suppression row")
    }

    async fn event_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
            .fetch_one(&self.workspace)
            .await
            .expect("event count")
    }

    async fn close(self) {
        let _ = self.db.close_workspace_store(WS).await;
        self.db.pool().close().await;
        self.workspace.close().await;
        let _ = tokio::fs::remove_dir_all(&self.root).await;
    }
}

fn workspace_path(root: &Path, workspace_id: &str) -> PathBuf {
    root.join("data/agents").join(workspace_id).join("harness/harness.db")
}

async fn open_workspace_db(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("workspace database")
}

fn ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}

fn turn_state(state_id: &str) -> Value {
    json!({
        "stateId": state_id,
        "stateVersion": TURN_STATE_VERSION,
        "turnId": "turn-1",
        "sources": [],
    })
}

fn completion(state_id: &str, config_version: i64) -> FullAttemptCompletion {
    FullAttemptCompletion {
        state_id: state_id.to_string(),
        state_version: TURN_STATE_VERSION,
        registry_version: REGISTRY_VERSION.to_string(),
        config_version,
        control_cursor: 0,
        turn_state: turn_state(state_id),
        observed_at: ts(),
    }
}

async fn install_complete_baseline(fixture: &SuppressionFixture) -> SuppressionState {
    let current = fixture
        .db
        .load_suppression_state(WS, SESSION)
        .await
        .expect("load state")
        .unwrap_or_else(|| SuppressionState::initial(WS, SESSION));
    let attempt = fixture
        .db
        .begin_full_attempt(SESSION, &current)
        .await
        .expect("begin full attempt");
    let completed = fixture
        .db
        .complete_full_attempt(
            WS,
            &attempt.attempt_id,
            &completion("state-1", fixture.config_version().await),
        )
        .await
        .expect("complete full attempt");
    assert!(completed.baseline_is_eligible());
    completed
}

async fn begin_skip(fixture: &SuppressionFixture) -> (SuppressionState, SkipAttempt) {
    let state = fixture.current().await;
    let attempt = fixture
        .db
        .begin_skip_attempt(SESSION, &state)
        .await
        .expect("begin skip attempt");
    (state, attempt)
}

fn event(event_id: &str, event_type: &str, payload: Value) -> HarnessEventInput {
    HarnessEventInput {
        event_id: event_id.to_string(),
        workspace_id: WS.to_string(),
        session_id: SESSION.to_string(),
        execution_mode: ExecutionMode::Production,
        run_id: None,
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: ts(),
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", Sha256::digest(serde_json::to_vec(&payload).expect("payload"))),
        payload_json: payload,
    }
}

fn skip_events() -> Vec<HarnessEventInput> {
    vec![
        event("skip:turn.started", "turn.started", json!({"turnKind": "scheduled"})),
        event(
            "skip:turn.completed",
            "turn.completed",
            json!({"outcome": "skipped", "reason": "no_material_change"}),
        ),
        event("skip:session.ended", "session.ended", json!({"outcome": "skipped"})),
    ]
}

#[tokio::test]
async fn failed_full_attempt_makes_old_baseline_ineligible_across_restart() {
    let fixture = SuppressionFixture::create().await;
    let baseline = install_complete_baseline(&fixture).await;
    let attempt = fixture
        .db
        .begin_full_attempt(SESSION, &baseline)
        .await
        .expect("begin full attempt");
    fixture
        .db
        .fail_full_attempt(WS, attempt.attempt_id, "provider_failed")
        .await
        .expect("fail full attempt");
    let reloaded = fixture.reopen().await;
    assert_eq!(reloaded.latest_outcome(), "full_failed");
    assert!(!reloaded.baseline_is_eligible());
    assert!(reloaded.full_failed);
    assert!(!reloaded.full_attempt_in_progress);
    fixture.close().await;
}

#[tokio::test]
async fn cancelled_partial_degraded_and_timeout_full_attempts_all_write_full_failed() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    for reason in ["cancelled", "partial", "degraded", "timeout"] {
        let baseline = fixture.current().await;
        let attempt = fixture
            .db
            .begin_full_attempt(SESSION, &baseline)
            .await
            .expect("begin full attempt");
        let failed = fixture
            .db
            .fail_full_attempt(WS, &attempt.attempt_id, reason)
            .await
            .expect("fail full attempt");
        assert_eq!(failed.latest_outcome(), "full_failed", "reason={reason}");
        assert_eq!(failed.last_error.as_deref(), Some(reason));
        assert!(!failed.baseline_is_eligible(), "reason={reason}");

        // A full Turn is always allowed; restore an eligible baseline for the
        // next terminal outcome so each case starts from the same shape.
        let attempt = fixture
            .db
            .begin_full_attempt(SESSION, &failed)
            .await
            .expect("begin recovery full attempt");
        fixture
            .db
            .complete_full_attempt(
                WS,
                &attempt.attempt_id,
                &completion("state-recovered", fixture.config_version().await),
            )
            .await
            .expect("recovery complete");
    }
    fixture.close().await;
}

#[tokio::test]
async fn two_workers_racing_begin_full_attempt_have_one_cas_winner() {
    let fixture = SuppressionFixture::create().await;
    let baseline = install_complete_baseline(&fixture).await;
    let (left, right) = tokio::join!(
        fixture.db.begin_full_attempt(SESSION, &baseline),
        fixture.db.begin_full_attempt(SESSION, &baseline)
    );
    let results = [left, right];
    let winners = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(winners, 1, "exactly one worker must win the full-attempt CAS");
    let loser = results.into_iter().find(|result| result.is_err()).expect("one loser");
    assert!(matches!(loser, Err(DbError::CasConflict { .. })));
    fixture.close().await;
}

#[tokio::test]
async fn crashed_full_attempt_leaves_full_attempt_in_progress_fence() {
    let fixture = SuppressionFixture::create().await;
    let baseline = install_complete_baseline(&fixture).await;
    fixture
        .db
        .begin_full_attempt(SESSION, &baseline)
        .await
        .expect("begin full attempt");
    let crashed = fixture.reopen().await;
    assert_eq!(crashed.latest_outcome(), "full_in_progress");
    assert!(crashed.full_attempt_in_progress);
    assert!(!crashed.baseline_is_eligible());
    let error = fixture
        .db
        .begin_skip_attempt(SESSION, &crashed)
        .await
        .expect_err("crash fence must forbid a skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    fixture.close().await;
}

#[tokio::test]
async fn failed_skip_audit_keeps_audit_in_progress_fence() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;

    let mut stale = attempt.clone();
    stale.expected_state_version += 1;
    let error = fixture
        .db
        .commit_skip(&stale, &skip_events())
        .await
        .expect_err("stale audit state must not commit");
    assert!(matches!(error, DbError::CasConflict { .. }));

    let fenced = fixture.current().await;
    assert_eq!(fenced.latest_outcome(), "skip_in_progress");
    assert!(fenced.audit_in_progress);
    assert!(!fenced.baseline_is_eligible());
    let error = fixture
        .db
        .begin_skip_attempt(SESSION, &fenced)
        .await
        .expect_err("audit fence must forbid another skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert_eq!(
        fixture.event_count().await,
        0,
        "rolled-back audit leaves no skipped terminal"
    );
    fixture.close().await;
}

#[tokio::test]
async fn event_write_failure_rolls_back_skipped_outcome_and_keeps_audit_fence() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;

    let mut events = skip_events();
    // The first event is written before this invalid one is rejected, proving
    // the whole workspace transaction rolls back.
    events.push(event("skip:invalid", "step.text", json!({"text": "   "})));
    let error = fixture
        .db
        .commit_skip(&attempt, &events)
        .await
        .expect_err("an event write failure must roll back the skip");
    assert!(matches!(error, DbError::Validation { .. }));

    let fenced = fixture.current().await;
    assert_eq!(fenced.latest_outcome(), "skip_in_progress");
    assert!(fenced.audit_in_progress);
    assert!(!fenced.schedule_sync_pending);
    assert!(!fenced.projection_sync_pending);
    assert!(!fenced.notification_pending);
    assert!(!fenced.baseline_is_eligible());
    assert_eq!(
        fixture.event_count().await,
        0,
        "no partial skipped lifecycle is visible"
    );

    let error = fixture
        .db
        .begin_skip_attempt(SESSION, &fenced)
        .await
        .expect_err("the durable audit fence still forbids another skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    fixture.close().await;
}

#[tokio::test]
async fn schedule_projection_notification_pending_flags_clear_independently() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    let committed = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect("commit skip");

    assert_eq!(committed.state.latest_outcome(), "skipped");
    assert!(committed.state.schedule_sync_pending);
    assert!(committed.state.projection_sync_pending);
    assert!(committed.state.notification_pending);
    assert!(!committed.state.baseline_is_eligible());

    let state = fixture
        .db
        .clear_sync_pending(WS, SESSION, SyncPending::Schedule)
        .await
        .expect("clear schedule pending");
    assert!(!state.schedule_sync_pending);
    assert!(state.projection_sync_pending);
    assert!(!state.baseline_is_eligible());

    let state = fixture
        .db
        .clear_sync_pending(WS, SESSION, SyncPending::Projection)
        .await
        .expect("clear projection pending");
    assert!(!state.projection_sync_pending);
    assert!(state.notification_pending);
    assert!(!state.baseline_is_eligible());

    let state = fixture
        .db
        .clear_sync_pending(WS, SESSION, SyncPending::Notification)
        .await
        .expect("clear notification pending");
    assert!(!state.notification_pending);
    assert_eq!(state.latest_outcome(), "skipped");
    assert_eq!(state.baseline_state_id.as_deref(), Some("state-1"));
    assert!(
        state.baseline_is_eligible(),
        "an unchanged baseline is eligible again after sync"
    );
    fixture.close().await;
}

#[tokio::test]
async fn unknown_outcome_full_attempt_reconciliation_is_idempotent() {
    let fixture = SuppressionFixture::create().await;
    let baseline = install_complete_baseline(&fixture).await;
    let attempt = fixture
        .db
        .begin_full_attempt(SESSION, &baseline)
        .await
        .expect("begin full attempt");
    let first = fixture
        .db
        .fail_full_attempt(WS, &attempt.attempt_id, "commit_unknown")
        .await
        .expect("first reconciliation");
    let second = fixture
        .db
        .fail_full_attempt(WS, &attempt.attempt_id, "commit_unknown")
        .await
        .expect("retried reconciliation");
    assert_eq!(
        first.state_version, second.state_version,
        "retry must not advance state"
    );
    assert_eq!(second.latest_outcome(), "full_failed");
    assert!(!second.baseline_is_eligible());
    fixture.close().await;
}

#[tokio::test]
async fn append_session_event_tx_preserves_event_id_and_digest_conflict_semantics() {
    let fixture = SuppressionFixture::create().await;
    let original = event("event-1", "step.tool_result", json!({"result": {"ok": true}}));

    let mut tx = fixture.workspace.begin().await.expect("event tx");
    let first = append_session_event_tx(&mut tx, &original).await.expect("append event");
    assert_eq!(first.seq, 1);
    assert_eq!(first.event_id, "event-1");
    let deduped = append_session_event_tx(&mut tx, &original)
        .await
        .expect("same identity dedupes");
    assert_eq!(deduped, first);
    tx.commit().await.expect("commit event");

    let mut conflicting = original.clone();
    conflicting.payload_digest = "another-digest".to_string();
    let mut tx = fixture.workspace.begin().await.expect("event tx");
    let error = append_session_event_tx(&mut tx, &conflicting)
        .await
        .expect_err("same id with another digest conflicts");
    assert!(matches!(error, DbError::Conflict { .. }));
    drop(tx);
    assert_eq!(fixture.event_count().await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_commits_skipped_events_and_pending_flags() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    let committed = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect("commit skip");

    assert_eq!(committed.events.len(), 3);
    assert_eq!(committed.events[0].seq, 1);
    assert_eq!(fixture.event_count().await, 3);
    assert_eq!(committed.state.latest_outcome(), "skipped");
    assert!(!committed.state.audit_in_progress);
    assert_eq!(
        committed.state.latest_attempt_id.as_deref(),
        Some(attempt.attempt_id.as_str())
    );
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_rejects_changed_state_version() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    sqlx::query(
        "UPDATE suppression_state SET state_version = state_version + 1 WHERE workspace_id = ? AND session_id = ?",
    )
    .bind(WS)
    .bind(SESSION)
    .execute(&fixture.workspace)
    .await
    .expect("advance state version");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("changed stateVersion must reject the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert!(!fixture.current().await.baseline_is_eligible());
    assert_eq!(fixture.event_count().await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_rejects_changed_config_version() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    fixture
        .db
        .update_suppression_config(WS, None, Some(600))
        .await
        .expect("change reconciliation config")
        .expect("schedule row");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("changed configVersion must reject the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert_eq!(fixture.event_count().await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_rejects_changed_registry_version() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    sqlx::query("UPDATE suppression_state SET registry_version = ? WHERE workspace_id = ? AND session_id = ?")
        .bind(OTHER_REGISTRY_VERSION)
        .bind(WS)
        .bind(SESSION)
        .execute(&fixture.workspace)
        .await
        .expect("change registry version");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("changed registryVersion must reject the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert_eq!(fixture.event_count().await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_rejects_changed_cursor() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    let mut tx = fixture.workspace.begin().await.expect("event tx");
    append_session_event_tx(
        &mut tx,
        &event(
            "user:message",
            "message.created",
            json!({"messageId": "m-1", "role": "user", "text": "new input"}),
        ),
    )
    .await
    .expect("append new user input");
    tx.commit().await.expect("commit user input");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("a new user message must reject the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert_eq!(fixture.event_count().await, 1, "only the user message remains");
    fixture.close().await;
}

#[tokio::test]
async fn commit_skip_rejects_changed_fence() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    sqlx::query("UPDATE suppression_state SET full_failed = 1 WHERE workspace_id = ? AND session_id = ?")
        .bind(WS)
        .bind(SESSION)
        .execute(&fixture.workspace)
        .await
        .expect("set another fence");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("another durable fence must reject the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));
    assert_eq!(fixture.event_count().await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn concurrent_suppression_config_updates_do_not_lose_fields() {
    let fixture = SuppressionFixture::create().await;
    let before = fixture
        .db
        .get_suppression_config(WS)
        .await
        .expect("config")
        .expect("schedule row");
    let (mode_update, reconciliation_update) = tokio::join!(
        fixture
            .db
            .update_suppression_config(WS, Some(SuppressionMode::Canary), None),
        fixture.db.update_suppression_config(WS, None, Some(600)),
    );
    mode_update.expect("mode update").expect("schedule row");
    reconciliation_update
        .expect("reconciliation update")
        .expect("schedule row");

    let after = fixture
        .db
        .get_suppression_config(WS)
        .await
        .expect("config")
        .expect("schedule row");
    assert_eq!(
        after.mode,
        SuppressionMode::Canary,
        "a concurrent field write must not revert mode"
    );
    assert_eq!(
        after.reconciliation_seconds, 600,
        "a concurrent field write must not revert reconciliation"
    );
    assert_eq!(
        after.config_version,
        before.config_version + 2,
        "each effective change advances the config generation once"
    );
    fixture.close().await;
}

#[tokio::test]
async fn cursor_rejected_skip_can_be_settled_and_followed_by_a_successful_full_attempt() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;
    let mut tx = fixture.workspace.begin().await.expect("event tx");
    append_session_event_tx(
        &mut tx,
        &event(
            "user:message",
            "message.created",
            json!({"messageId": "m-1", "role": "user", "text": "new input"}),
        ),
    )
    .await
    .expect("append new user input");
    tx.commit().await.expect("commit user input");

    let error = fixture
        .db
        .commit_skip(&attempt, &skip_events())
        .await
        .expect_err("new user input rejects the skip");
    assert!(matches!(error, DbError::CasConflict { .. }));

    let settled = fixture
        .db
        .fail_skip_attempt(WS, SESSION, &attempt.attempt_id, "cursor_changed")
        .await
        .expect("settle the rejected audit");
    assert_eq!(settled.latest_outcome(), "audit_failed");
    assert_eq!(settled.last_error.as_deref(), Some("cursor_changed"));
    assert!(!settled.audit_in_progress);
    assert!(!settled.baseline_is_eligible());

    let full = fixture
        .db
        .begin_full_attempt(SESSION, &settled)
        .await
        .expect("full execution must remain reachable after a settled audit");
    let mut completion = completion("state-2", fixture.config_version().await);
    completion.control_cursor = 1;
    let completed = fixture
        .db
        .complete_full_attempt(WS, &full.attempt_id, &completion)
        .await
        .expect("successful full attempt");
    assert_eq!(completed.latest_outcome(), "full_completed");
    assert_eq!(completed.control_cursor, 1);
    assert!(!completed.audit_in_progress);
    assert!(completed.baseline_is_eligible());
    fixture.close().await;
}

#[tokio::test]
async fn successful_full_attempt_clears_an_existing_audit_fence() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, _attempt) = begin_skip(&fixture).await;
    let fenced = fixture.current().await;
    assert!(fenced.audit_in_progress);

    let full = fixture
        .db
        .begin_full_attempt(SESSION, &fenced)
        .await
        .expect("an unresolved audit must not block a full attempt");
    let completed = fixture
        .db
        .complete_full_attempt(
            WS,
            &full.attempt_id,
            &completion("state-2", fixture.config_version().await),
        )
        .await
        .expect("successful full attempt");
    assert!(
        !completed.audit_in_progress,
        "a full success supersedes the audit fence"
    );
    assert_eq!(completed.latest_outcome(), "full_completed");
    assert!(completed.baseline_is_eligible());
    fixture.close().await;
}

#[tokio::test]
async fn failed_full_attempt_leaves_the_audit_fence_blocking_skips() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, _attempt) = begin_skip(&fixture).await;
    let fenced = fixture.current().await;
    assert!(fenced.audit_in_progress);

    let full = fixture
        .db
        .begin_full_attempt(SESSION, &fenced)
        .await
        .expect("full attempt during audit");
    let failed = fixture
        .db
        .fail_full_attempt(WS, &full.attempt_id, "provider_failed")
        .await
        .expect("failed full attempt");
    assert!(
        failed.audit_in_progress,
        "a failed full attempt must not clear the audit fence"
    );
    assert!(failed.full_failed);
    assert!(!failed.baseline_is_eligible());

    let error = fixture
        .db
        .begin_skip_attempt(SESSION, &failed)
        .await
        .expect_err("the audit fence still blocks skips");
    assert!(matches!(error, DbError::CasConflict { .. }));
    fixture.close().await;
}

#[tokio::test]
async fn fail_skip_attempt_is_idempotent_for_unknown_outcome_reconciliation() {
    let fixture = SuppressionFixture::create().await;
    install_complete_baseline(&fixture).await;
    let (_state, attempt) = begin_skip(&fixture).await;

    let first = fixture
        .db
        .fail_skip_attempt(WS, SESSION, &attempt.attempt_id, "audit_write_failed")
        .await
        .expect("settle audit");
    let second = fixture
        .db
        .fail_skip_attempt(WS, SESSION, &attempt.attempt_id, "audit_write_failed")
        .await
        .expect("retried settlement");
    assert_eq!(
        first.state_version, second.state_version,
        "retry must not advance state"
    );
    assert_eq!(second.latest_outcome(), "audit_failed");
    assert!(!second.audit_in_progress);
    fixture.close().await;
}

#[tokio::test]
async fn suppression_schema_splits_workspace_tables_from_platform_config() {
    let fixture = SuppressionFixture::create().await;
    let workspace_tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND name IN ('suppression_state', 'suppression_idempotency', 'suppression_gate_evaluations') \
         ORDER BY name",
    )
    .fetch_all(&fixture.workspace)
    .await
    .expect("workspace suppression tables");
    assert_eq!(
        workspace_tables,
        [
            "suppression_gate_evaluations",
            "suppression_idempotency",
            "suppression_state"
        ]
    );

    let platform_suppression_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name IN ('suppression_state', 'suppression_idempotency', 'suppression_gate_evaluations')",
    )
    .fetch_one(fixture.db.pool())
    .await
    .expect("platform tables");
    assert_eq!(
        platform_suppression_tables, 0,
        "suppression state must live only in the workspace DB"
    );

    let schedule_columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('assistant_schedules')")
        .fetch_all(fixture.db.pool())
        .await
        .expect("schedule columns");
    for column in [
        "suppression_mode",
        "reconciliation_seconds",
        "suppression_config_version",
    ] {
        assert!(schedule_columns.iter().any(|name| name == column), "missing {column}");
    }

    sqlx::query(
        "INSERT INTO suppression_idempotency \
         (workspace_id, operation, key, request_fingerprint, created_at, updated_at) \
         VALUES (?, 'wake', 'key-1', 'fingerprint-1', 'now', 'now')",
    )
    .bind(WS)
    .execute(&fixture.workspace)
    .await
    .expect("idempotency row");
    let duplicate = sqlx::query(
        "INSERT INTO suppression_idempotency \
         (workspace_id, operation, key, request_fingerprint, created_at, updated_at) \
         VALUES (?, 'wake', 'key-1', 'fingerprint-2', 'now', 'now')",
    )
    .bind(WS)
    .execute(&fixture.workspace)
    .await;
    assert!(duplicate.is_err(), "UNIQUE(workspace_id, operation, key) must hold");
    sqlx::query(
        "INSERT INTO suppression_idempotency \
         (workspace_id, operation, key, request_fingerprint, created_at, updated_at) \
         VALUES (?, 'reconcile', 'key-1', 'fingerprint-2', 'now', 'now')",
    )
    .bind(WS)
    .execute(&fixture.workspace)
    .await
    .expect("another operation may reuse the key");

    fixture.close().await;
}

#[tokio::test]
async fn assistant_schedule_suppression_config_defaults_and_versioning() {
    let fixture = SuppressionFixture::create().await;
    let defaults = fixture
        .db
        .ensure_assistant_schedule("ws-defaults", "__assistant__ws-defaults", ts())
        .await
        .expect("ensure schedule");
    assert_eq!(defaults.interval_seconds, 300);

    let config: SuppressionConfig = fixture
        .db
        .get_suppression_config("ws-defaults")
        .await
        .expect("config")
        .expect("row");
    assert_eq!(config.mode, SuppressionMode::Off);
    assert_eq!(config.reconciliation_seconds, 1800);
    assert_eq!(config.config_version, 1);

    let shadow = fixture
        .db
        .update_suppression_config("ws-defaults", Some(SuppressionMode::Shadow), Some(600))
        .await
        .expect("update config")
        .expect("row");
    assert_eq!(shadow.mode, SuppressionMode::Shadow);
    assert_eq!(shadow.reconciliation_seconds, 600);
    assert_eq!(shadow.config_version, 2);

    let unchanged = fixture
        .db
        .update_suppression_config("ws-defaults", Some(SuppressionMode::Shadow), None)
        .await
        .expect("no-op config")
        .expect("row");
    assert_eq!(unchanged.config_version, 2, "no-op writes must not bump configVersion");

    let error = fixture
        .db
        .update_suppression_config("ws-defaults", None, Some(299))
        .await
        .expect_err("out-of-range reconciliation is rejected");
    assert!(matches!(error, DbError::Validation { .. }));
    let after_invalid = fixture
        .db
        .get_suppression_config("ws-defaults")
        .await
        .expect("config")
        .expect("row");
    assert_eq!(after_invalid, unchanged, "invalid config must not write partial values");
    fixture.close().await;
}
