//! Invoke action flow integration tests (design doc section 八, suite 4)
//!
//! HTTP-level coverage of the require_action_confirm gate:
//! invoke → confirmation_required + token → confirm → executed/simulated;
//! token replay → 404; mismatched confirm → 400; gate OFF → immediate
//! dispatch; non-device thing → 400; unregistered action → 404.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace,
    setup_test_app_with_pool,
};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

/// App with one seeded workspace; returns (app, pool, token).
async fn setup(workspace_id: &str) -> (axum::Router, sqlx::SqlitePool, String) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", workspace_id).await;
    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    let token = create_test_token_with_workspace("user-1", "tenant-1", workspace_id);
    (app, pool, token)
}

async fn create_thing(
    app: &axum::Router,
    token: &str,
    workspace_id: &str,
    name: &str,
    thing_type: &str,
) -> String {
    let body = json!({"name": name, "thingType": thing_type, "workspaceId": workspace_id});
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "thing create failed: {json:?}");
    json["result"]["id"].as_str().unwrap().to_string()
}

async fn register_action(pool: &sqlx::SqlitePool, thing_id: &str, action_name: &str) {
    sqlx::query(
        "INSERT INTO thing_actions (id, device_id, name, display_name) VALUES (?, ?, ?, ?)",
    )
    .bind(format!("act-{action_name}"))
    .bind(thing_id)
    .bind(action_name)
    .bind(action_name)
    .execute(pool)
    .await
    .expect("register action");
}

async fn invoke(
    app: &axum::Router,
    token: &str,
    thing_id: &str,
    action: &str,
) -> (StatusCode, Value) {
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/{thing_id}/actions/{action}/invoke"),
            token,
            Some(json!({"params": {"delay": 0}})),
        ))
        .await
        .unwrap();
    response_parts(r).await
}

async fn confirm(
    app: &axum::Router,
    token: &str,
    thing_id: &str,
    action: &str,
    confirm_token: &str,
) -> (StatusCode, Value) {
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/{thing_id}/actions/{action}/confirm"),
            token,
            Some(json!({"token": confirm_token})),
        ))
        .await
        .unwrap();
    response_parts(r).await
}

// ──────────────────────────────────────────────
// Gate ON: invoke → confirm → replay
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_invoke_requires_confirmation_then_confirm_executes() {
    let (app, pool, token) = setup("ws-inv").await;
    let thing = create_thing(&app, &token, "ws-inv", "dev-confirm", "device").await;
    register_action(&pool, &thing, "reboot").await;

    // 1. Invoke → confirmation_required + token
    let (status, body) = invoke(&app, &token, &thing, "reboot").await;
    assert_eq!(status, StatusCode::OK, "invoke failed: {body:?}");
    assert_eq!(body["result"]["status"], "confirmation_required");
    let confirm_token = body["result"]["token"].as_str().expect("token must be issued").to_string();

    // 2. Confirm with the token → executed/simulated
    let (status, body) = confirm(&app, &token, &thing, "reboot", &confirm_token).await;
    assert_eq!(status, StatusCode::OK, "confirm failed: {body:?}");
    let final_status = body["result"]["status"].as_str().unwrap_or_default();
    assert!(
        final_status == "executed" || final_status == "simulated",
        "confirm must execute, got: {final_status}"
    );

    // 3. Replay the same token → 404 (tokens are single-use)
    let (status, body) = confirm(&app, &token, &thing, "reboot", &confirm_token).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "replayed token must be 404, got {status}: {body:?}");
}

#[tokio::test]
async fn test_confirm_with_mismatched_action_400() {
    let (app, pool, token) = setup("ws-mis").await;
    let thing = create_thing(&app, &token, "ws-mis", "dev-mismatch", "device").await;
    register_action(&pool, &thing, "reboot").await;
    register_action(&pool, &thing, "shutdown").await;

    let (_, body) = invoke(&app, &token, &thing, "reboot").await;
    let confirm_token = body["result"]["token"].as_str().unwrap().to_string();

    // Confirm against a DIFFERENT action → 400
    let (status, body) = confirm(&app, &token, &thing, "shutdown", &confirm_token).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "mismatched action confirm must be 400, got {status}: {body:?}"
    );
}

// ──────────────────────────────────────────────
// Gate OFF: immediate dispatch, no token
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_invoke_without_confirm_gate_executes_immediately() {
    let (app, pool, token) = setup("ws-open").await;
    sqlx::query("UPDATE workspaces SET require_action_confirm = 0 WHERE id = 'ws-open'")
        .execute(&pool)
        .await
        .unwrap();

    let thing = create_thing(&app, &token, "ws-open", "dev-open", "device").await;
    register_action(&pool, &thing, "reboot").await;

    let (status, body) = invoke(&app, &token, &thing, "reboot").await;
    assert_eq!(status, StatusCode::OK, "invoke failed: {body:?}");
    let final_status = body["result"]["status"].as_str().unwrap_or_default();
    assert!(
        final_status == "executed" || final_status == "simulated",
        "gate-off invoke must dispatch immediately, got: {final_status}"
    );
    assert!(body["result"]["token"].is_null(), "gate-off invoke must not issue a token: {body:?}");
}

// ──────────────────────────────────────────────
// Non-device thing → 400
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_invoke_on_space_thing_400() {
    let (app, _pool, token) = setup("ws-space").await;
    let thing = create_thing(&app, &token, "ws-space", "space-thing", "space").await;

    let (status, body) = invoke(&app, &token, &thing, "reboot").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "invoke on a space thing must be 400, got {status}: {body:?}"
    );
}

// ──────────────────────────────────────────────
// Unregistered action → 404
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_invoke_unregistered_action_404() {
    let (app, _pool, token) = setup("ws-unreg").await;
    let thing = create_thing(&app, &token, "ws-unreg", "dev-unreg", "device").await;

    let (status, body) = invoke(&app, &token, &thing, "nonexistent_action").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "unregistered action must be 404, got {status}: {body:?}"
    );
}
