//! API-level cutover proof for Task 19B.
//!
//! The production Task HTTP surface must read and write workspace Harness
//! facts only. These tests drive the real router and assert that the legacy
//! `runtime.v1` ledger and platform `task_approvals` table stay untouched.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use futures::StreamExt;
use serde_json::{Value, json};
use tinyiothub_storage::Db;
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_admin_role, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-api-cutover";

struct ApiFixture {
    app: axum::Router,
    db: Arc<Db>,
    token: String,
    pool: sqlx::SqlitePool,
}

async fn fixture() -> ApiFixture {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_admin_role(&pool, "user-1").await;
    let db = app_state.db.clone();
    db.create_workspace_store(WS).await.expect("workspace store");
    let app = axum::Router::new()
        .nest("/api", crate::api::create_router(&app_state))
        .with_state(app_state);
    ApiFixture {
        app,
        db,
        token: create_test_token("user-1", "tenant-1"),
        pool,
    }
}

async fn workspace_pool(fixture: &ApiFixture) -> sqlx::SqlitePool {
    fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool")
}

async fn post(fixture: &ApiFixture, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", auth_header(&fixture.token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await
}

async fn post_task(fixture: &ApiFixture, text: &str, key: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workspaces/{WS}/agent/tasks"))
        .header("Authorization", auth_header(&fixture.token))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", key)
        .body(Body::from(json!({"text":text}).to_string()))
        .expect("request");
    response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await
}

async fn get(fixture: &ApiFixture, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(&fixture.token))
        .body(Body::empty())
        .expect("request");
    response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await
}

async fn stream(fixture: &ApiFixture, task_id: &str, after: i64) -> axum::response::Response {
    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/workspaces/{WS}/agent/tasks/{task_id}/stream?after={after}"
        ))
        .header("Authorization", auth_header(&fixture.token))
        .body(Body::empty())
        .expect("request");
    let response = fixture.app.clone().oneshot(request).await.expect("stream response");
    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn sse_frames(response: axum::response::Response, wanted: usize) -> Vec<Value> {
    let mut body = response.into_body().into_data_stream();
    let mut frames = Vec::new();
    while frames.len() < wanted {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
            .await
            .expect("sse frame timeout")
            .expect("sse stream ended")
            .expect("sse chunk");
        let text = String::from_utf8(chunk.to_vec()).expect("utf8");
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                frames.push(serde_json::from_str(data).expect("sse data"));
            }
        }
    }
    frames
}

#[allow(clippy::too_many_arguments)]
async fn seed_effect_decision(
    fixture: &ApiFixture,
    task_id: &str,
    effect_id: &str,
    tool_call_id: &str,
    effect_state: &str,
    decision_id: &str,
    decision_kind: &str,
    decision_state: &str,
) {
    let now = Utc::now();
    let pool = workspace_pool(fixture).await;
    sqlx::query("INSERT INTO agent_runs (id, task_id, state, execution_mode) VALUES (?, ?, 'running', 'production')")
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
         VALUES (?, 'session-1', ?, ?, ?, 'set_level', '{\"deviceId\":\"device-1\"}',
                 ?, ?, 'driver-call-1', ?, 1, NULL, ?, ?)",
    )
    .bind(effect_id)
    .bind(task_id)
    .bind(format!("run-{effect_id}"))
    .bind(tool_call_id)
    .bind(format!("digest-{effect_id}"))
    .bind(effect_id)
    .bind(effect_state)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("effect");
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, answer_evidence_json, consumed_at, consumed_by)
         VALUES (?, ?, ?, 'session-1', ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(decision_id)
    .bind(decision_kind)
    .bind(decision_state)
    .bind(format!("run-{effect_id}"))
    .bind(task_id)
    .bind(effect_id)
    .bind(json!({"deviceId":"device-1","tool":"set_level","argsJson":{"level":7},"reason":"confirm"}).to_string())
    .bind(json!(["approved", "denied"]).to_string())
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("decision");
}

#[tokio::test]
async fn runtime_api_uses_harness_facts_and_never_writes_legacy_tables() {
    let fixture = fixture().await;
    let (status, created) = post_task(&fixture, "inspect pump", "cutover-key").await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let task_id = created["result"]["taskId"].as_str().expect("task id");

    seed_effect_decision(
        &fixture,
        task_id,
        "effect-approval",
        "call-approval",
        "prepared",
        "approval-1",
        "tool_approval",
        "requested",
    )
    .await;
    let (status, events) = get(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/events?limit=50"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{events}");
    let created_seq = events["result"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .find(|event| event["eventId"] == format!("task:{task_id}:created"))
        .expect("created event")["cursor"]
        .as_i64()
        .expect("cursor");

    // Unknown cursors resync instead of sending partial deltas.
    let frames = sse_frames(stream(&fixture, task_id, 9_999).await, 1).await;
    assert_eq!(frames[0]["type"], "resync");
    assert_eq!(frames[0]["reason"], "unknown_cursor");

    let (status, approved) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/approvals/approval-1/approve"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["result"]["approval"]["state"], "approved");
    assert_eq!(approved["result"]["deduplicated"], false);
    let (status, repeated) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/approvals/approval-1/approve"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repeated}");
    assert_eq!(repeated["result"]["deduplicated"], true);
    let (status, conflict) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/approvals/approval-1/reject"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");

    seed_effect_decision(
        &fixture,
        task_id,
        "effect-reconcile",
        "call-reconcile",
        "unknown",
        "reconcile-1",
        "effect_reconciliation",
        "requested",
    )
    .await;
    let (status, reconciled) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/effects/effect-reconcile/reconcile"),
        json!({"answer":"applied","evidence":{"operator":"user-1"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reconciled}");

    // A known cursor replays exactly the committed facts after it.
    let frames = sse_frames(stream(&fixture, task_id, created_seq).await, 3).await;
    assert_eq!(frames[0]["type"], "snapshot");
    let replayed: Vec<String> = frames[1..]
        .iter()
        .filter(|frame| frame["type"] == "event")
        .map(|frame| frame["event"]["eventId"].as_str().expect("event id").to_string())
        .collect();
    let expected: Vec<String> = sqlx::query_scalar(
        "SELECT event_id FROM session_events
         WHERE task_id = ? AND execution_mode = 'production' AND seq > ? ORDER BY seq",
    )
    .bind(task_id)
    .bind(created_seq)
    .fetch_all(&workspace_pool(&fixture).await)
    .await
    .expect("expected events");
    assert_eq!(&replayed[..expected.len()], expected.as_slice());

    // The same facts observed live carry the same identity as replayed rows.
    let live_after = fixture
        .db
        .harness_event_high_watermark(WS)
        .await
        .expect("high watermark");
    let response = stream(&fixture, task_id, live_after).await;
    fixture
        .db
        .append_session_event(&tinyiothub_core::harness::HarnessEventInput {
            event_id: "live-cutover".to_string(),
            workspace_id: WS.to_string(),
            session_id: format!("task:{task_id}"),
            execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
            run_id: None,
            task_id: Some(task_id.to_string()),
            event_type: "task.updated".to_string(),
            occurred_at: Utc::now(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-live-cutover".to_string(),
            payload_json: json!({"stateVersion": 0}),
        })
        .await
        .expect("live event");
    let frames = sse_frames(response, 2).await;
    assert_eq!(frames[0]["type"], "snapshot");
    assert_eq!(frames[1]["type"], "event");
    assert_eq!(frames[1]["event"]["eventId"], "live-cutover");
    let live_cursor = frames[1]["event"]["cursor"].as_i64().expect("live cursor");
    let persisted_cursor: i64 = sqlx::query_scalar("SELECT seq FROM session_events WHERE event_id = 'live-cutover'")
        .fetch_one(&workspace_pool(&fixture).await)
        .await
        .expect("persisted live cursor");
    assert_eq!(live_cursor, persisted_cursor);

    for action in ["pause", "resume"] {
        let (status, body) = post(
            &fixture,
            &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/{action}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{action}: {body}");
    }

    let now = Utc::now();
    let pool = workspace_pool(&fixture).await;
    sqlx::query(
        "INSERT INTO effect_intents
            (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
             request_digest, idempotency_key, external_ref, state, state_version, result_json,
             created_at, updated_at)
         VALUES ('effect-stop', 'session-1', ?, 'run-effect-approval', 'call-stop', 'set_level', '{}',
                 'digest-stop', 'effect-stop', 'driver-call-1', 'dispatching', 1, NULL, ?, ?)",
    )
    .bind(task_id)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("stop effect");
    sqlx::query(
        "INSERT INTO device_jobs
            (id, workspace_id, device_id, tool, args_json, state, timeout_ms, started_at,
             task_id, run_id, tool_call_id)
         VALUES ('job-stop', ?, 'device-1', 'set_level', '{}', 'running', 5000, ?, ?, 'run-effect-approval', 'call-stop')",
    )
    .bind(WS)
    .bind(now)
    .bind(task_id)
    .execute(&pool)
    .await
    .expect("job");
    let (status, stopped) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/jobs/job-stop/stop"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stopped}");
    let stop_state: String = sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = 'effect-stop'")
        .fetch_one(&pool)
        .await
        .expect("stop effect state");
    assert_eq!(stop_state, "cancelling");

    let (status, cancelled) = post(
        &fixture,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/cancel"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert!(
        matches!(
            cancelled["result"]["task"]["status"].as_str(),
            Some("cancelling") | Some("cancelled")
        ),
        "{cancelled}"
    );

    let runtime_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM hub_ledger WHERE kind = 'runtime.v1' AND workspace_id = ?")
            .bind(WS)
            .fetch_one(&fixture.pool)
            .await
            .expect("legacy runtime rows");
    assert_eq!(runtime_rows, 0, "no runtime.v1 rows may be produced");
    let platform_approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
        .fetch_one(&fixture.pool)
        .await
        .expect("platform approvals");
    assert_eq!(platform_approvals, 0, "no platform approval writes may be produced");
}
