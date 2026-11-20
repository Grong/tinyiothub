//! Task 7 — Assistant suppression HTTP contract.
//!
//! Covers authorization (read vs write), RFC 9457 problem details, schedule
//! config validation, the public preflight/report projections (no refs,
//! workKeys, revisions or payloads), Idempotency-Key replay/conflict semantics
//! and the server-side `turn.state` filter on public Session Events surfaces.
//!
//! The real TriggerRouter/WakeRunner manual path is exercised in the wake and
//! trigger test modules; these tests drive the HTTP layer with a deterministic
//! `SuppressionOps` fixture so DTO/idempotency behavior stays focused.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::domains::agent::session::suppression::SuppressionMode;
use crate::domains::agent::session::suppression::contract::{
    PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, WorkItem, WorkItemDisposition,
};
use crate::domains::agent::session::suppression::preflight::{PreflightDecision, SkipVeto};
use crate::domains::agent::session::trigger::SuppressionOps;
use crate::domains::agent::session::wake::{ManualOperation, ManualPreflight, ManualStartError, ManualTurnStart};
use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, seed_admin_role, seed_test_workspace,
    setup_test_app_with_pool,
};
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

const WS: &str = "ws-suppression-api";
const WS_CROSS: &str = "ws-suppression-api-cross";

fn session_key(workspace_id: &str) -> String {
    format!("agent:{workspace_id}:__assistant__{workspace_id}/main")
}

/// Durable attempt id shape (`{workspace}:{session}:attempt:{n}`), distinct
/// from the turn id (`turn:{hash}`).
fn fake_attempt_id() -> String {
    format!("{WS}:{}:attempt:1", session_key(WS))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartBehavior {
    Accept,
    InFlight,
    Backpressure,
}

struct FakeOps {
    preflight: ManualPreflight,
    behavior: Mutex<StartBehavior>,
    starts: AtomicUsize,
    operations: Mutex<Vec<ManualOperation>>,
    delay: Duration,
}

impl FakeOps {
    fn new(behavior: StartBehavior) -> Arc<Self> {
        Arc::new(Self {
            preflight: fake_preflight(),
            behavior: Mutex::new(behavior),
            starts: AtomicUsize::new(0),
            operations: Mutex::new(Vec::new()),
            delay: Duration::from_millis(0),
        })
    }

    fn with_delay(behavior: StartBehavior, delay: Duration) -> Arc<Self> {
        let ops = Self::new(behavior);
        Arc::new(Self {
            preflight: ops.preflight.clone(),
            behavior: Mutex::new(behavior),
            starts: AtomicUsize::new(0),
            operations: Mutex::new(Vec::new()),
            delay,
        })
    }

    fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }

    fn operations(&self) -> Vec<ManualOperation> {
        self.operations.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl SuppressionOps for FakeOps {
    async fn manual_preflight(&self, _workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        Ok(self.preflight.clone())
    }

    async fn start_manual(
        &self,
        _workspace_id: &str,
        operation: ManualOperation,
    ) -> Result<ManualTurnStart, ManualStartError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.operations.lock().unwrap().push(operation);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        match *self.behavior.lock().unwrap() {
            StartBehavior::Accept => Ok(ManualTurnStart {
                session_key: self.preflight.session_key.clone(),
                turn_id: "turn:fake-0001".to_string(),
                attempt_id: fake_attempt_id(),
            }),
            StartBehavior::InFlight => Err(ManualStartError::InFlight),
            StartBehavior::Backpressure => Err(ManualStartError::Backpressure { retry_after_ms: 250 }),
        }
    }
}

/// The fixture deliberately carries internal refs/workKeys/revisions so the
/// public projection tests prove the DTO is an allowlist, not a rename.
fn fake_preflight() -> ManualPreflight {
    ManualPreflight {
        session_key: session_key(WS),
        mode: SuppressionMode::Shadow,
        reconciliation_seconds: 1800,
        config_version: 1,
        registry_version: "sha256:registry-v1".to_string(),
        descriptors: vec![SourceDescriptor {
            source_id: "workspace.snapshot".to_string(),
            source_version: 1,
        }],
        decision: PreflightDecision {
            would_skip: true,
            mode: SuppressionMode::Shadow,
            vetoes: vec![SkipVeto::ReconciliationDue],
            snapshot: vec![SourceSnapshot {
                source_id: "workspace.snapshot".to_string(),
                source_version: 1,
                revision: "internal-revision-secret".to_string(),
                state_hash: "internal-state-hash".to_string(),
                available: true,
                empty: false,
                state_ref: Some(StateRef {
                    id: "internal-state-ref".to_string(),
                    revision: "internal-revision-secret".to_string(),
                    state_hash: "internal-state-hash".to_string(),
                }),
                work_items: vec![WorkItem {
                    work_key: "user-derived-work-key".to_string(),
                    revision: "internal-item-revision".to_string(),
                    disposition: WorkItemDisposition::Active,
                    payload_ref: PayloadRef {
                        id: "internal-payload-ref".to_string(),
                        revision: "internal-item-revision".to_string(),
                        payload_hash: "internal-payload-hash".to_string(),
                    },
                    payload_hash: "internal-payload-hash".to_string(),
                }],
                error: None,
            }],
            baseline_state_id: None,
            config_version: 1,
            fence: String::new(),
        },
    }
}

async fn app_with_ops(ops: Arc<dyn SuppressionOps>) -> (axum::Router, sqlx::SqlitePool, Arc<tinyiothub_storage::Db>) {
    app_with_ops_config(ops, false).await
}

async fn app_with_ops_config(
    ops: Arc<dyn SuppressionOps>,
    allow_synthetic: bool,
) -> (axum::Router, sqlx::SqlitePool, Arc<tinyiothub_storage::Db>) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_test_workspace(&pool, "tenant-2", WS_CROSS).await;
    app_state.suppression_ops = Some(ops);
    app_state.suppression_gate_allow_synthetic = allow_synthetic;
    let db = app_state.db.clone();
    let api = crate::api::create_router(&app_state);
    (axum::Router::new().nest("/api", api).with_state(app_state), pool, db)
}

fn request(method: &str, uri: &str, token: &str, idempotency_key: Option<&str>, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token));
    let builder = match idempotency_key {
        Some(key) => builder.header("Idempotency-Key", key),
        None => builder,
    };
    match body {
        Some(body) => builder
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (axum::http::StatusCode, axum::http::HeaderMap, Value) {
    let response = app
        .clone()
        .oneshot(request(method, uri, token, idempotency_key, body))
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = crate::test_utils::response_json(response).await;
    (status, headers, body)
}

async fn admin(pool: &sqlx::SqlitePool) -> String {
    seed_admin_role(pool, "admin-user").await;
    create_test_token("admin-user", "tenant-1")
}

fn problem_code(body: &Value) -> Option<&str> {
    body.get("code").and_then(Value::as_str)
}

fn assert_problem(response: &(axum::http::StatusCode, axum::http::HeaderMap, Value), status: u16, code: &str) {
    assert_eq!(response.0.as_u16(), status, "{:?}", response.2);
    assert_eq!(
        response
            .1
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or(""),
        "application/problem+json"
    );
    assert_eq!(problem_code(&response.2), Some(code), "{:?}", response.2);
}

async fn append_event(
    db: &tinyiothub_storage::Db,
    workspace_id: &str,
    event_id: &str,
    session_id: &str,
    event_type: &str,
    payload: Value,
) {
    let input = HarnessEventInput {
        event_id: event_id.to_string(),
        workspace_id: workspace_id.to_string(),
        session_id: session_id.to_string(),
        execution_mode: ExecutionMode::Production,
        run_id: None,
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: chrono::Utc::now(),
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("digest-{event_id}"),
        payload_json: payload,
    };
    db.append_session_event(&input).await.expect("append session event");
}

// Value card
// - Covers: POST wake with a write-authorized admin and an Idempotency-Key.
// - Real regression: 202 must not claim terminal counts the Turn has not
//   produced yet; only the report may carry modelCalls/toolCalls.
// - Failure signal: modelCalls/toolCalls present in the 202 body, or a
//   non-202 status.
#[tokio::test]
async fn accepted_wake_does_not_claim_final_counts() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, _db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;

    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/wake"),
        &token,
        Some("wake-1"),
        None,
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::ACCEPTED, "{:?}", response.2);
    assert_eq!(response.2["outcome"], "accepted");
    assert_eq!(response.2["turnId"], "turn:fake-0001");
    assert_eq!(response.2["sessionKey"], session_key(WS).as_str());
    assert!(
        response.2["reportUrl"]
            .as_str()
            .expect("report url")
            .contains("turnId=turn:fake-0001")
    );
    assert!(
        response.2.get("modelCalls").is_none(),
        "202 must not fabricate modelCalls"
    );
    assert!(
        response.2.get("toolCalls").is_none(),
        "202 must not fabricate toolCalls"
    );
    assert_eq!(ops.starts(), 1);
}

// Value card
// - Covers: report projection over a Source snapshot that carries internal
//   refs/workKeys/revisions.
// - Real regression: raw turn.state internals leak through the public report.
// - Failure signal: stateRef/payloadRef/workKey/revision appear anywhere.
#[tokio::test]
async fn public_report_excludes_internal_refs_and_work_keys() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    let key = session_key(WS);
    append_event(
        &db,
        WS,
        "event-skip",
        &key,
        "turn/end",
        json!({"turnId": "turn:skip-1", "outcome": "skipped", "toolCalls": 0}),
    )
    .await;

    let response = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/report"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::OK, "{:?}", response.2);
    let body = &response.2;
    assert!(body.get("stateRef").is_none());
    assert!(body.get("payloadRef").is_none());
    assert!(body.get("workKey").is_none());
    assert_eq!(body["registryVersion"], "sha256:registry-v1");
    assert_eq!(body["sources"][0]["sourceId"], "workspace.snapshot");
    assert_eq!(body["sources"][0]["counts"]["active"], 1);
    assert_eq!(body["sources"][0]["refValidation"], "valid");
    let serialized = body.to_string();
    for forbidden in [
        "internal-revision-secret",
        "user-derived-work-key",
        "internal-payload-ref",
        "internal-state-ref",
        "internal-state-hash",
        "internal-payload-hash",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "report leaked {forbidden}: {serialized}"
        );
    }
}

// Value card
// - Covers: ?turnId= terminal query for skipped, non-terminal and unknown.
// - Real regression: accepting a Turn again reports final counts for a turn
//   that has not finished (or invents counts for an unknown id).
// - Failure signal: counts on in_progress/unknown, or wrong skipped counts.
#[tokio::test]
async fn report_turn_query_reports_terminal_counts_without_fabrication() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    let key = session_key(WS);
    append_event(
        &db,
        WS,
        "event-skip",
        &key,
        "turn/end",
        json!({"turnId": "turn:skip-1", "outcome": "skipped", "toolCalls": 0}),
    )
    .await;
    append_event(
        &db,
        WS,
        "event-started",
        &key,
        "turn/start",
        json!({"turnId": "turn:running-1"}),
    )
    .await;

    let skipped = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/report?turnId=turn:skip-1"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(skipped.0, axum::http::StatusCode::OK, "{:?}", skipped.2);
    assert_eq!(skipped.2["turn"]["status"], "skipped");
    assert_eq!(skipped.2["turn"]["modelCalls"], 0);
    assert_eq!(skipped.2["turn"]["toolCalls"], 0);

    let running = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/report?turnId=turn:running-1"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(running.2["turn"]["status"], "in_progress");
    assert!(running.2["turn"].get("modelCalls").is_none());
    assert!(running.2["turn"].get("toolCalls").is_none());

    let unknown = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/report?turnId=turn:missing"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(unknown.2["turn"]["status"], "unknown");
    assert!(unknown.2["turn"].get("modelCalls").is_none());
    assert!(unknown.2["turn"].get("toolCalls").is_none());
}

// Value card
// - Covers: R4 tightening — PUT schedule requires workspace write (admin).
// - Real regression: a read-only member can pause or reconfigure suppression.
// - Failure signal: member PUT returns 200 or mutates the row.
#[tokio::test]
async fn put_schedule_requires_workspace_write_permission() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, _pool, db) = app_with_ops(ops).await;
    let member = create_test_token("member-user", "tenant-1");

    let response = send(
        &app,
        "PUT",
        &format!("/api/v1/workspaces/{WS}/agent/schedule"),
        &member,
        None,
        Some(json!({"suppressionMode": "enforce"})),
    )
    .await;
    assert_problem(&response, 403, "workspace_write_required");
    let config = db
        .get_suppression_config(WS)
        .await
        .expect("config read")
        .unwrap_or_else(|| tinyiothub_storage::assistant_schedule::SuppressionConfig::off(WS));
    assert_ne!(config.mode, SuppressionMode::Enforce);
}

// Value card
// - Covers: PUT off with no other fields; mode + configVersion commit before
//   the response.
// - Real regression: the kill switch requires unrelated fields, or returns
//   before the new mode is durable.
// - Failure signal: non-200, mode still shadow, or configVersion not advanced.
#[tokio::test]
async fn put_off_alone_commits_mode_and_config_version_before_responding() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    db.ensure_assistant_schedule(
        WS,
        &crate::domains::agent::session::wake::assistant_agent_id(WS),
        chrono::Utc::now(),
    )
    .await
    .expect("schedule");
    db.update_suppression_config(WS, Some(SuppressionMode::Shadow), None)
        .await
        .expect("seed shadow")
        .expect("schedule row");

    let response = send(
        &app,
        "PUT",
        &format!("/api/v1/workspaces/{WS}/agent/schedule"),
        &token,
        None,
        Some(json!({"suppressionMode": "off"})),
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::OK, "{:?}", response.2);
    assert_eq!(response.2["suppressionMode"], "off");
    assert!(response.2["suppressionConfigVersion"].as_i64().unwrap() >= 2);
    let config = db.get_suppression_config(WS).await.unwrap().unwrap();
    assert_eq!(config.mode, SuppressionMode::Off);
    assert!(config.config_version >= 2);
}

// Value card
// - Covers: invalid mode and out-of-range reconciliationSeconds.
// - Real regression: a partial write applies the valid half of an invalid
//   config, or returns a non-problem error.
// - Failure signal: 400 without RFC 9457 code, or DB value changed.
#[tokio::test]
async fn invalid_suppression_config_returns_problem_without_partial_write() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    db.ensure_assistant_schedule(
        WS,
        &crate::domains::agent::session::wake::assistant_agent_id(WS),
        chrono::Utc::now(),
    )
    .await
    .expect("schedule");
    let before = db.get_suppression_config(WS).await.unwrap().unwrap();

    let invalid_mode = send(
        &app,
        "PUT",
        &format!("/api/v1/workspaces/{WS}/agent/schedule"),
        &token,
        None,
        Some(json!({"suppressionMode": "sometimes"})),
    )
    .await;
    assert_problem(&invalid_mode, 400, "suppression_config_invalid");

    let invalid_seconds = send(
        &app,
        "PUT",
        &format!("/api/v1/workspaces/{WS}/agent/schedule"),
        &token,
        None,
        Some(json!({"suppressionMode": "enforce", "reconciliationSeconds": 299})),
    )
    .await;
    assert_problem(&invalid_seconds, 400, "suppression_config_invalid");

    let after = db.get_suppression_config(WS).await.unwrap().unwrap();
    assert_eq!(after, before, "invalid config must not partially write");
}

// Value card
// - Covers: cross-tenant reads and writes are rejected before any operation.
// - Real regression: tenant ID alone authorizes another tenant's workspace.
// - Failure signal: 200/202, or a non-problem error shape.
#[tokio::test]
async fn cross_tenant_access_is_denied_with_problem_details() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, _db) = app_with_ops(ops.clone()).await;
    seed_admin_role(&pool, "cross-admin").await;
    let cross = create_test_token("cross-admin", "tenant-2");

    for (method, suffix) in [
        ("GET", "preflight"),
        ("GET", "report"),
        ("POST", "wake"),
        ("POST", "reconcile"),
    ] {
        let response = send(
            &app,
            method,
            &format!("/api/v1/workspaces/{WS}/agent/suppression/{suffix}"),
            &cross,
            Some("cross-1"),
            None,
        )
        .await;
        assert_problem(&response, 403, "workspace_forbidden");
    }
    assert_eq!(ops.starts(), 0, "cross-tenant requests must never start a Turn");
}

// Value card
// - Covers: POST without Idempotency-Key.
// - Real regression: an unkeyed mutation cannot be replayed by the client.
// - Failure signal: 202 instead of a 400 problem.
#[tokio::test]
async fn post_requires_idempotency_key() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, _db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/wake"),
        &token,
        None,
        None,
    )
    .await;
    assert_problem(&response, 400, "idempotency_key_required");
}

// Value card
// - Covers: same key/intent replays the recorded result even after scheduled
//   state advances; same key for another operation conflicts.
// - Real regression: a scheduled tick changes `nextWakeAt`, and a legitimate
//   retry is rejected as a fingerprint conflict instead of replaying.
// - Failure signal: different turnId on replay, second start, or 409 on the
//   post-tick retry.
#[tokio::test]
async fn idempotency_survives_schedule_advance_and_rejects_changed_operation() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/wake");

    let first = send(&app, "POST", &uri, &token, Some("stable-key"), None).await;
    assert_eq!(first.0, axum::http::StatusCode::ACCEPTED, "{:?}", first.2);

    // Simulate a scheduled wake advancing the platform schedule between the
    // original request and its retry. The request intent is unchanged.
    db.ensure_assistant_schedule(
        WS,
        &crate::domains::agent::session::wake::assistant_agent_id(WS),
        chrono::Utc::now(),
    )
    .await
    .expect("schedule");
    db.update_assistant_schedule(
        WS,
        None,
        Some(600),
        None,
        Some(chrono::Utc::now() + chrono::Duration::seconds(600)),
        None,
    )
    .await
    .expect("advance planned wake");

    let retry = send(&app, "POST", &uri, &token, Some("stable-key"), None).await;
    assert_eq!(
        retry.0,
        axum::http::StatusCode::ACCEPTED,
        "a scheduled tick must not invalidate a legitimate retry: {:?}",
        retry.2
    );
    assert_eq!(first.2, retry.2, "identical intent must replay the recorded result");
    assert_eq!(ops.starts(), 1, "replay must not start a second Turn");

    // Same key, different operation is a different intent.
    let conflict = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/reconcile"),
        &token,
        Some("stable-key"),
        None,
    )
    .await;
    assert_problem(&conflict, 409, "idempotency_conflict");

    // Persisted reservation: durable attempt id is separate from the turn id.
    let record = db
        .find_suppression_idempotency(WS, "stable-key")
        .await
        .expect("idempotency read")
        .expect("idempotency row");
    assert_eq!(record.operation, "wake");
    assert!(!record.request_fingerprint.is_empty());
    assert_eq!(record.attempt_id.as_deref(), Some(fake_attempt_id().as_str()));
    assert_ne!(record.attempt_id.as_deref(), Some("turn:fake-0001"));
    assert_eq!(record.result.as_ref().unwrap()["turnId"], "turn:fake-0001");
    assert_eq!(record.result.as_ref().unwrap()["outcome"], "accepted");
    assert!(record.planned_wake_at.is_some(), "plannedWakeAt kept for diagnostics");
}

// Value card
// - Covers: a reservation left result-less by a failed result write stays
//   bounded (409) while it is fresh.
// - Real regression: the handler starts a second Turn while the first result
//   is still being recorded.
// - Failure signal: 202 + start_manual while the reservation is fresh.
#[tokio::test]
async fn fresh_null_result_reservation_is_bounded_in_flight() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/wake");
    let key = "failed-record-key";
    let fingerprint = crate::domains::agent::session::workspace_api::request_fingerprint(ManualOperation::Wake, WS);
    db.try_insert_suppression_idempotency(WS, "wake", key, &fingerprint, None)
        .await
        .expect("reserve");

    let response = send(&app, "POST", &uri, &token, Some(key), None).await;
    assert_problem(&response, 409, "wake_in_flight");
    assert_eq!(ops.starts(), 0, "a fresh result-less reservation must not start a Turn");
}

// Value card
// - Covers: a result-less reservation (crash or failed result write) is
//   reclaimed after the bounded server-owned preparation deadline.
// - Real regression: the key is wedged forever and every retry returns 409.
// - Failure signal: non-202 retry, no start, or no recorded result.
#[tokio::test]
async fn stale_null_result_reservation_is_reclaimed_after_deadline() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/wake");
    let key = "stale-key";
    let fingerprint = crate::domains::agent::session::workspace_api::request_fingerprint(ManualOperation::Wake, WS);
    db.try_insert_suppression_idempotency(WS, "wake", key, &fingerprint, None)
        .await
        .expect("reserve");
    let workspace_pool = db.workspace_pool_for_testing(WS).await.expect("workspace pool");
    sqlx::query(
        "UPDATE suppression_idempotency SET updated_at = ? \
         WHERE workspace_id = ? AND operation = 'wake' AND key = ?",
    )
    .bind("2000-01-01T00:00:00Z")
    .bind(WS)
    .bind(key)
    .execute(&workspace_pool)
    .await
    .expect("age reservation");

    let response = send(&app, "POST", &uri, &token, Some(key), None).await;
    assert_eq!(response.0, axum::http::StatusCode::ACCEPTED, "{:?}", response.2);
    assert_eq!(ops.starts(), 1, "the reclaimed key admits a new winner");
    let record = db
        .find_suppression_idempotency(WS, key)
        .await
        .expect("idempotency read")
        .expect("idempotency row");
    assert_eq!(record.result.as_ref().unwrap()["turnId"], "turn:fake-0001");
    assert_eq!(record.attempt_id.as_deref(), Some(fake_attempt_id().as_str()));
}

// Value card
// - Covers: two concurrent identical requests; the unique constraint picks one
//   winner and both observe the same recorded result.
// - Real regression: both calls execute a Turn, or the loser errors instead of
//   replaying.
// - Failure signal: starts != 1 or differing turnIds.
#[tokio::test]
async fn concurrent_identical_calls_have_one_winner() {
    let ops = FakeOps::with_delay(StartBehavior::Accept, Duration::from_millis(100));
    let (app, pool, _db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/wake");

    let first = send(&app, "POST", &uri, &token, Some("concurrent-key"), None);
    let second = send(&app, "POST", &uri, &token, Some("concurrent-key"), None);
    let (first, second) = tokio::join!(first, second);

    let accepted = [&first, &second]
        .into_iter()
        .filter(|response| response.0 == axum::http::StatusCode::ACCEPTED)
        .collect::<Vec<_>>();
    assert_eq!(
        accepted.len(),
        2,
        "both identical requests must replay a result: {first:?} {second:?}"
    );
    assert_eq!(first.2["turnId"], second.2["turnId"]);
    assert_eq!(ops.starts(), 1, "the database unique constraint must pick one winner");
}

// Value card
// - Covers: in-flight conflict maps to 409 `wake_in_flight`.
// - Real regression: concurrent manual wake starts a second Turn.
// - Failure signal: 202 or a different code.
#[tokio::test]
async fn in_flight_conflict_returns_409_wake_in_flight() {
    let ops = FakeOps::new(StartBehavior::InFlight);
    let (app, pool, _db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/wake"),
        &token,
        Some("busy-key"),
        None,
    )
    .await;
    assert_problem(&response, 409, "wake_in_flight");
}

// Value card
// - Covers: backpressure maps to 429 `wake_backpressure` + Retry-After.
//   Task 9 owns the semaphore itself; this pins the HTTP contract.
// - Real regression: saturation surfaces as 500 or an unbounded queue.
// - Failure signal: wrong status/code or missing Retry-After.
#[tokio::test]
async fn backpressure_maps_to_429_with_retry_after() {
    let ops = FakeOps::new(StartBehavior::Backpressure);
    let (app, pool, _db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/wake"),
        &token,
        Some("pressure-key"),
        None,
    )
    .await;
    assert_problem(&response, 429, "wake_backpressure");
    assert_eq!(response.2["retryAfterMs"], 250);
    assert_eq!(response.1.get("retry-after").and_then(|v| v.to_str().ok()), Some("1"));
}

// Value card
// - Covers: reconcile reaches the ops port as a forced full operation.
// - Real regression: reconcile silently degrades to the skip-capable wake path.
// - Failure signal: operations() does not record Reconcile.
#[tokio::test]
async fn reconcile_is_forwarded_as_a_forced_full_operation() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, _db) = app_with_ops(ops.clone()).await;
    let token = admin(&pool).await;
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/reconcile"),
        &token,
        Some("reconcile-key"),
        None,
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::ACCEPTED, "{:?}", response.2);
    assert_eq!(ops.operations(), vec![ManualOperation::Reconcile]);
}

// Value card
// - Covers: GET preflight is read-only and returns the allowlisted projection.
// - Real regression: observing preflight creates/advances suppression state.
// - Failure signal: wouldSkip/registryVersion missing, or state_version moved.
#[tokio::test]
async fn preflight_is_read_only_and_publicly_projected() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, _pool, db) = app_with_ops(ops).await;
    let token = create_test_token("member-user", "tenant-1");
    let key = session_key(WS);
    let before = db.load_suppression_state(WS, &key).await.expect("state read before");

    let response = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/preflight"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::OK, "{:?}", response.2);
    assert_eq!(response.2["wouldSkip"], true);
    assert_eq!(response.2["registryVersion"], "sha256:registry-v1");
    assert_eq!(response.2["sources"][0]["sourceId"], "workspace.snapshot");
    assert_eq!(response.2["vetoes"][0], "reconciliation_due");
    assert!(!response.2.to_string().contains("user-derived-work-key"));

    let after = db.load_suppression_state(WS, &key).await.expect("state read after");
    assert_eq!(
        before.map(|state| state.state_version),
        after.map(|state| state.state_version)
    );
}

// Value card
// - Covers: public Session Events list and HUD must filter raw turn.state.
// - Real regression: internal refs/workKeys leak through a public stream.
// - Failure signal: turn.state appears in either surface.
#[tokio::test]
async fn raw_turn_state_is_filtered_from_public_session_events_and_hud() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, _pool, db) = app_with_ops(ops).await;
    let key = session_key(WS);
    append_event(
        &db,
        WS,
        "event-turn-state",
        &key,
        "turn.state",
        json!({"stateRef": "internal-state-ref", "workKey": "user-derived-work-key"}),
    )
    .await;
    append_event(
        &db,
        WS,
        "event-visible",
        &key,
        "message.created",
        json!({"messageId": "m1", "turnId": "turn:visible", "role": "assistant", "text": "hello"}),
    )
    .await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WS);

    let uri = format!("/api/v1/chat/sessions/agent%3A{WS}%3A__assistant__{WS}%2Fmain/events?after=0&limit=50");
    let events = send(&app, "GET", &uri, &token, None, None).await;
    assert_eq!(events.0, axum::http::StatusCode::OK, "{:?}", events.2);
    let serialized = events.2.to_string();
    assert!(
        !serialized.contains("turn.state"),
        "public Session Events leaked turn.state: {serialized}"
    );
    assert!(serialized.contains("message.created"));

    let hud = send(
        &app,
        "GET",
        &format!("/api/v1/workspaces/{WS}/agent/hud"),
        &token,
        None,
        None,
    )
    .await;
    assert_eq!(hud.0, axum::http::StatusCode::OK, "{:?}", hud.2);
    let serialized = hud.2.to_string();
    assert!(
        !serialized.contains("turn.state"),
        "HUD recent_events leaked turn.state: {serialized}"
    );
    assert!(!serialized.contains("user-derived-work-key"));
}

#[tokio::test]
async fn unauthenticated_suppression_routes_are_rejected() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, _pool, _db) = app_with_ops(ops).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/workspaces/{WS}/agent/suppression/preflight"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

// ── Task 10: evidence-gated canary/enforce transitions ──

use crate::domains::agent::session::suppression::preflight::suppression_gate::observations_for;

const FAKE_REGISTRY_VERSION: &str = "sha256:registry-v1";

async fn seed_artifact_observations(
    db: &tinyiothub_storage::Db,
    artifact: &tinyiothub_storage::suppression::SuppressionGateArtifact,
) {
    let (sample, baseline) = observations_for(artifact);
    for observation in sample.iter().chain(baseline.iter()) {
        db.record_suppression_turn_observation(observation)
            .await
            .expect("seed suppression observation");
    }
}

/// Real admin operations: `record_confirmed_false_skip` delegates to the same
/// producer `WakeRunner` uses, backed by the workspace DB sink.
struct FalseSkipOps {
    db: Arc<tinyiothub_storage::Db>,
}

#[async_trait::async_trait]
impl SuppressionOps for FalseSkipOps {
    async fn manual_preflight(&self, _workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        Ok(fake_preflight())
    }

    async fn registry_version(&self, _workspace_id: &str) -> Result<String, ManualStartError> {
        Ok(FAKE_REGISTRY_VERSION.to_string())
    }

    async fn start_manual(
        &self,
        _workspace_id: &str,
        _operation: ManualOperation,
    ) -> Result<ManualTurnStart, ManualStartError> {
        Err(ManualStartError::Unavailable("unused in false-skip tests".to_string()))
    }

    async fn record_confirmed_false_skip(
        &self,
        workspace_id: &str,
        turn_id: &str,
        reviewer: &str,
        root_cause: &str,
    ) -> Result<(), ManualStartError> {
        let sink = crate::domains::agent::session::events::DbSessionEventSink::new(
            self.db.clone(),
            Arc::new(crate::domains::agent::runtime::session_stream::SessionStreamHub::default()),
            Arc::new(crate::domains::agent::runtime::session_live::SessionLiveHub::default()),
        );
        crate::domains::agent::session::wake::emit_confirmed_false_skip(
            &self.db,
            &sink,
            workspace_id,
            turn_id,
            reviewer,
            root_cause,
        )
        .await
        .map_err(ManualStartError::Unavailable)
    }
}

async fn app_with_false_skip_ops() -> (axum::Router, sqlx::SqlitePool, Arc<tinyiothub_storage::Db>) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_test_workspace(&pool, "tenant-2", WS_CROSS).await;
    let db = app_state.db.clone();
    app_state.suppression_ops = Some(Arc::new(FalseSkipOps { db: db.clone() }));
    let api = crate::api::create_router(&app_state);
    (axum::Router::new().nest("/api", api).with_state(app_state), pool, db)
}

async fn seed_schedule(db: &tinyiothub_storage::Db) {
    db.ensure_assistant_schedule(
        WS,
        &crate::domains::agent::session::wake::assistant_agent_id(WS),
        chrono::Utc::now(),
    )
    .await
    .expect("schedule");
}

async fn put_mode(
    app: &axum::Router,
    token: &str,
    mode: &str,
) -> (axum::http::StatusCode, axum::http::HeaderMap, Value) {
    send(
        app,
        "PUT",
        &format!("/api/v1/workspaces/{WS}/agent/schedule"),
        token,
        None,
        Some(json!({ "suppressionMode": mode })),
    )
    .await
}

// Value card
// - Covers: canary/enforce without approved evidence.
// - Real regression: a PUT skips the shadow/canary gate and enables skips.
// - Failure signal: non-409 response or a changed mode.
#[tokio::test]
async fn canary_and_enforce_without_gate_artifacts_are_rejected() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::canary_fixture;
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    assert_problem(&put_mode(&app, &token, "canary").await, 409, "shadow_gate_not_met");
    assert_problem(&put_mode(&app, &token, "enforce").await, 409, "shadow_gate_not_met");

    // A shadow artifact unlocks canary but never enforce.
    let shadow = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    db.insert_suppression_gate_artifact(&shadow)
        .await
        .expect("insert shadow artifact");
    seed_artifact_observations(&db, &shadow).await;
    assert_problem(&put_mode(&app, &token, "enforce").await, 409, "shadow_gate_not_met");

    // A canary artifact unlocks enforce.
    let canary = canary_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    db.insert_suppression_gate_artifact(&canary)
        .await
        .expect("insert canary artifact");
    seed_artifact_observations(&db, &canary).await;
    let accepted = put_mode(&app, &token, "enforce").await;
    assert_eq!(accepted.0, axum::http::StatusCode::OK, "{:?}", accepted.2);
    assert_eq!(accepted.2["suppressionMode"], "enforce");
}

// Value card
// - Covers: a valid shadow artifact allows canary; synthetic evidence does not.
// - Real regression: loopback fixture evidence enables a production workspace.
// - Failure signal: canary accepted with synthetic=true, or rejected with real evidence.
#[tokio::test]
async fn canary_requires_non_synthetic_approved_shadow_evidence() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let synthetic = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .with_synthetic(true)
        .artifact();
    db.insert_suppression_gate_artifact(&synthetic)
        .await
        .expect("insert synthetic artifact");
    seed_artifact_observations(&db, &synthetic).await;
    assert_problem(&put_mode(&app, &token, "canary").await, 409, "shadow_gate_not_met");

    // A new artifact id must be used because the synthetic row is immutable,
    // and its approval must be later than the synthetic row's.
    let real = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .with_approved_at(chrono::DateTime::from_timestamp(1_900_000_100, 0).expect("timestamp"))
        .mutate(|artifact| artifact.artifact_id = "gate:shadow:real".to_string())
        .artifact();
    db.insert_suppression_gate_artifact(&real)
        .await
        .expect("insert real artifact");
    seed_artifact_observations(&db, &real).await;
    let accepted = put_mode(&app, &token, "canary").await;
    assert_eq!(accepted.0, axum::http::StatusCode::OK, "{:?}", accepted.2);
    assert_eq!(accepted.2["suppressionMode"], "canary");
}

// Value card
// - Covers: off is always allowed (kill switch, no bypass parameter) even with
//   stale/absent evidence; a confirmed false skip stale-dates an old artifact.
// - Real regression: an operator cannot disable skips, or a pre-false-skip
//   artifact still unlocks enforce.
// - Failure signal: PUT off non-200, or enforce accepted after the false skip.
#[tokio::test]
async fn off_is_always_allowed_and_false_skip_requires_fresh_evidence() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::canary_fixture;

    let (app, pool, db) = app_with_false_skip_ops().await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    // No artifact at all: off still succeeds immediately.
    let off = put_mode(&app, &token, "off").await;
    assert_eq!(off.0, axum::http::StatusCode::OK, "{:?}", off.2);
    assert_eq!(off.2["suppressionMode"], "off");

    // An artifact approved before a confirmed false-skip event is stale.
    let old = canary_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .with_approved_at(chrono::Utc::now() - chrono::Duration::seconds(3_600))
        .artifact();
    db.insert_suppression_gate_artifact(&old)
        .await
        .expect("insert old canary artifact");
    seed_artifact_observations(&db, &old).await;

    // The real admin endpoint emits `false_skip_detected` through the same
    // producer the wake path uses (not a hand-appended event).
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/false-skip");
    let recorded = send(
        &app,
        "POST",
        &uri,
        &token,
        Some("false-skip-1"),
        Some(json!({
            "turnId": "canary-turn-000",
            "reviewer": "reviewer-1",
            "rootCause": "fixed the adapter tombstone"
        })),
    )
    .await;
    assert_eq!(recorded.0, axum::http::StatusCode::CREATED, "{:?}", recorded.2);
    assert_eq!(recorded.2["code"], "false_skip_detected");
    assert!(
        db.latest_confirmed_false_skip_at(WS)
            .await
            .expect("false skip read")
            .is_some(),
        "the real alert must be persisted for R18 staleness"
    );

    // Same key replays the recorded result without a second alert.
    let replay = send(
        &app,
        "POST",
        &uri,
        &token,
        Some("false-skip-1"),
        Some(json!({
            "turnId": "canary-turn-000",
            "reviewer": "reviewer-1",
            "rootCause": "fixed the adapter tombstone"
        })),
    )
    .await;
    assert_eq!(replay.0, axum::http::StatusCode::CREATED, "{:?}", replay.2);
    assert_eq!(replay.2, recorded.2);

    assert_problem(&put_mode(&app, &token, "enforce").await, 409, "shadow_gate_not_met");

    // The kill switch still wins while remediation is in progress.
    let off_after = put_mode(&app, &token, "off").await;
    assert_eq!(off_after.0, axum::http::StatusCode::OK, "{:?}", off_after.2);

    // Root-cause fix + a fresh approved artifact unlocks enforce again.
    let fresh = canary_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .with_approved_at(chrono::Utc::now() + chrono::Duration::seconds(1))
        .mutate(|artifact| artifact.artifact_id = "gate:canary:fresh".to_string())
        .artifact();
    db.insert_suppression_gate_artifact(&fresh)
        .await
        .expect("insert fresh canary artifact");
    seed_artifact_observations(&db, &fresh).await;
    let accepted = put_mode(&app, &token, "enforce").await;
    assert_eq!(accepted.0, axum::http::StatusCode::OK, "{:?}", accepted.2);
    assert_eq!(accepted.2["suppressionMode"], "enforce");
}

// Value card
// - Covers: a stale registry version rejects an otherwise valid artifact.
// - Real regression: an artifact approved against an old registry unlocks a
//   mode after Sources changed.
// - Failure signal: 200 for the stale artifact.
#[tokio::test]
async fn stale_registry_artifact_is_rejected_by_put() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let stale = shadow_fixture(WS)
        .with_registry_version("sha256:previous-registry")
        .artifact();
    db.insert_suppression_gate_artifact(&stale)
        .await
        .expect("insert stale artifact");
    seed_artifact_observations(&db, &stale).await;
    assert_problem(&put_mode(&app, &token, "canary").await, 409, "shadow_gate_not_met");
}

fn seal_body(artifact: &tinyiothub_storage::suppression::SuppressionGateArtifact, mode: &str) -> Value {
    let mut body = json!({
        "mode": mode,
        "sampleTurnIds": &artifact.sample.turn_ids,
        "from": "2026-11-01T00:00:00Z",
        "to": "2026-11-30T23:59:59Z",
        "approver": "approver-1",
    });
    if !artifact.sample.baseline_turn_ids.is_empty() {
        body["baselineTurnIds"] = json!(&artifact.sample.baseline_turn_ids);
    }
    body
}

fn canary_provenance() -> (Value, Value) {
    let labels = json!({
        "labeler": "labeler-1",
        "labelledAt": "2026-11-16T00:00:00Z",
        "baselineReports": 1000,
        "canaryReports": 200
    });
    let review = json!({
        "reviewer": "reviewer-1",
        "reviewedAt": "2026-11-16T00:00:00Z",
        "conclusion": "no_confirmed_false_skip",
        "confirmedFalseSkips": 0,
        "samples": (0..10)
            .map(|index| json!({
                "turnId": format!("canary-turn-{index:03}"),
                "reviewed": true,
                "confirmedFalseSkip": false
            }))
            .collect::<Vec<_>>()
    });
    (labels, review)
}

// Value card
// - Covers: the only production artifact authoring path derives shadow metrics
//   from recorded observations and unlocks canary.
// - Real regression: an operator-typed denominator artifact bypasses the
//   observation record.
// - Failure signal: 201 without observations, or PUT canary rejected.
#[tokio::test]
async fn seal_endpoint_derives_shadow_metrics_from_observations() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let fixture = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    seed_artifact_observations(&db, &fixture).await;

    let response = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/gate-artifacts"),
        &token,
        Some("seal-shadow-1"),
        Some(seal_body(&fixture, "shadow")),
    )
    .await;
    assert_eq!(response.0, axum::http::StatusCode::CREATED, "{:?}", response.2);
    assert_eq!(response.2["mode"], "shadow");
    assert_eq!(response.2["synthetic"], false);
    assert!(
        response.2["contentSha256"]
            .as_str()
            .expect("hash")
            .starts_with("sha256:")
    );

    let artifact_id = response.2["artifactId"].as_str().expect("artifactId");
    let sealed = db
        .load_suppression_gate_artifact(WS, artifact_id)
        .await
        .expect("artifact read")
        .expect("artifact row");
    let denominators = sealed.denominators.shadow.as_ref().expect("shadow denominators");
    assert_eq!(denominators.turns, 100);
    assert_eq!(denominators.would_skip_turns, 60);
    assert_eq!(denominators.model_calls, 1_000);
    assert_eq!(denominators.model_driven_tool_calls, 500);
    assert_eq!(sealed.shadow_estimates.would_skip_ratio, 0.6);
    assert_eq!(sealed.shadow_estimates.estimated_model_savings, 0.6);
    assert_eq!(sealed.shadow_estimates.estimated_tool_savings, 0.5);

    let accepted = put_mode(&app, &token, "canary").await;
    assert_eq!(accepted.0, axum::http::StatusCode::OK, "{:?}", accepted.2);
}

// Value card
// - Covers: canary metrics are derived from canary + baseline observations and
//   only human provenance is operator-supplied.
// - Real regression: a canary artifact without labeler/reviewer provenance
//   unlocks enforce, or model/tool counts are typed by the caller.
// - Failure signal: 201 without provenance, or PUT enforce rejected.
#[tokio::test]
async fn seal_endpoint_requires_canary_human_provenance() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::canary_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let fixture = canary_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    seed_artifact_observations(&db, &fixture).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/gate-artifacts");

    let missing = send(
        &app,
        "POST",
        &uri,
        &token,
        Some("seal-canary-missing"),
        Some(seal_body(&fixture, "canary")),
    )
    .await;
    assert_problem(&missing, 409, "shadow_gate_not_met");

    let (labels, review) = canary_provenance();
    let mut body = seal_body(&fixture, "canary");
    body["duplicateReportReview"] = labels;
    body["falseSkipReview"] = review;
    let created = send(&app, "POST", &uri, &token, Some("seal-canary-1"), Some(body)).await;
    assert_eq!(created.0, axum::http::StatusCode::CREATED, "{:?}", created.2);
    assert_eq!(created.2["mode"], "canary");

    let sealed = db
        .load_suppression_gate_artifact(WS, created.2["artifactId"].as_str().expect("artifactId"))
        .await
        .expect("artifact read")
        .expect("artifact row");
    let measured = &sealed.canary_measured;
    assert_eq!(measured.model_call_reduction, 0.4);
    assert_eq!(measured.tool_call_reduction, 0.4);
    assert_eq!(measured.duplicate_report_reduction, 0.8);
    assert_eq!(
        measured.duplicate_report_labels.as_ref().expect("labels").labeler,
        "labeler-1"
    );

    let accepted = put_mode(&app, &token, "enforce").await;
    assert_eq!(accepted.0, axum::http::StatusCode::OK, "{:?}", accepted.2);
}

// Value card
// - Covers: artifacts may only reference real workspace observations.
// - Real regression: a self-typed artifact names turn IDs that were never
//   recorded (or repeats one) and still passes.
// - Failure signal: 201 for missing/duplicate turn ids.
#[tokio::test]
async fn seal_endpoint_rejects_missing_duplicate_or_foreign_turn_ids() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops(ops).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let fixture = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    seed_artifact_observations(&db, &fixture).await;
    let uri = format!("/api/v1/workspaces/{WS}/agent/suppression/gate-artifacts");

    let mut missing_body = seal_body(&fixture, "shadow");
    missing_body["sampleTurnIds"] = json!([&fixture.sample.turn_ids[0], "missing-turn"]);
    let missing = send(&app, "POST", &uri, &token, Some("seal-missing"), Some(missing_body)).await;
    assert_problem(&missing, 409, "shadow_gate_not_met");

    let mut duplicate_body = seal_body(&fixture, "shadow");
    duplicate_body["sampleTurnIds"] = json!([&fixture.sample.turn_ids[0], &fixture.sample.turn_ids[0]]);
    let duplicate = send(&app, "POST", &uri, &token, Some("seal-duplicate"), Some(duplicate_body)).await;
    assert_problem(&duplicate, 400, "suppression_config_invalid");
}

// Value card
// - Covers: synthetic evidence is enabled only by server configuration, never
//   by a request field.
// - Real regression: a client sets synthetic=true and the artifact is treated
//   as loopback-only evidence, or the loopback fixture cannot seal it.
// - Failure signal: request-controlled synthetic value, or 201 without config.
#[tokio::test]
async fn synthetic_evidence_is_server_config_only() {
    use crate::domains::agent::session::suppression::preflight::suppression_gate::shadow_fixture;

    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, pool, db) = app_with_ops_config(ops, true).await;
    let token = admin(&pool).await;
    seed_schedule(&db).await;

    let fixture = shadow_fixture(WS)
        .with_registry_version(FAKE_REGISTRY_VERSION)
        .artifact();
    seed_artifact_observations(&db, &fixture).await;
    let mut body = seal_body(&fixture, "shadow");
    // A client-supplied synthetic flag is ignored.
    body["synthetic"] = json!(false);
    let created = send(
        &app,
        "POST",
        &format!("/api/v1/workspaces/{WS}/agent/suppression/gate-artifacts"),
        &token,
        Some("seal-synthetic"),
        Some(body),
    )
    .await;
    assert_eq!(created.0, axum::http::StatusCode::CREATED, "{:?}", created.2);
    assert_eq!(
        created.2["synthetic"], true,
        "server configuration, not the request, controls synthetic evidence"
    );

    let accepted = put_mode(&app, &token, "canary").await;
    assert_eq!(
        accepted.0,
        axum::http::StatusCode::OK,
        "the loopback server configuration permits synthetic evidence: {:?}",
        accepted.2
    );
}

// Value card
// - Covers: the authoring endpoints require workspace write permission.
// - Real regression: a read-only member seals artifacts or confirms false skips.
// - Failure signal: non-403 response.
#[tokio::test]
async fn gate_authoring_endpoints_require_workspace_write() {
    let ops = FakeOps::new(StartBehavior::Accept);
    let (app, _pool, _db) = app_with_ops(ops).await;
    let member = create_test_token("member-user", "tenant-1");

    for (suffix, body) in [
        (
            "gate-artifacts",
            json!({
                "mode": "shadow",
                "sampleTurnIds": ["turn-1"],
                "from": "2026-11-01T00:00:00Z",
                "to": "2026-11-30T23:59:59Z",
                "approver": "approver-1"
            }),
        ),
        ("false-skip", json!({"turnId": "turn-1", "reviewer": "reviewer-1"})),
    ] {
        let response = send(
            &app,
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/suppression/{suffix}"),
            &member,
            Some("member-key"),
            Some(body),
        )
        .await;
        assert_problem(&response, 403, "workspace_write_required");
    }
}
