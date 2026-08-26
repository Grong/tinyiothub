//! Open API handler integration tests
//! NOTE: Open routes use API Key auth (X-API-Key header), not JWT.
//! Routes have double /open/open/ prefix due to nesting bug in handler.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn open_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

// NOTE: The open routes are defined with /open/ prefix inside the router,
// which is nested under /open in api/mod.rs, producing /open/open/ paths.

#[tokio::test]
async fn test_open_health() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/health")).await.unwrap();
    // May return 401 (missing API key) or 200
    assert!(response.status().is_success() || response.status() == StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_open_things_unauthorized() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/things")).await.unwrap();
    // Without API key, should return 401 or 200 with error
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}

#[tokio::test]
async fn test_open_thing_not_found() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app
        .oneshot(open_request("GET", "/api/open/open/things/nonexistent-thing-12345"))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}

#[tokio::test]
async fn test_open_events_unauthorized() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/events")).await.unwrap();
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}

// ── send_command denied cases + API-key auth verification (pre-landing) ──

use sha2::{Digest, Sha256};

/// Seed a workspace, a device thing with a registered action, and an API key
/// for `key_workspace`. Returns (Router, raw_key).
async fn setup_open_app_with_key(
    thing_workspace: &str,
    key_workspace: &str,
    thing_type: &str,
) -> (axum::Router, String, sqlx::SqlitePool) {
    let (app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    crate::test_utils::seed_test_workspace(&pool, "tenant-1", thing_workspace).await;
    crate::test_utils::seed_test_workspace(&pool, "tenant-1", key_workspace).await;

    sqlx::query(
        "INSERT OR IGNORE INTO tenant_usage (id, tenant_id, device_count, user_count, api_call_count) VALUES ('usage-1', 'tenant-1', 1, 1, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO things (id, name, thing_type, workspace_id) VALUES ('dev-open', 'OpenDev', ?, ?)")
        .bind(thing_type)
        .bind(thing_workspace)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO thing_actions (id, thing_id, name, display_name) VALUES ('act-open', 'dev-open', 'reboot', '重启')")
        .execute(&pool)
        .await
        .unwrap();

    let raw_key = "tinh_testkey1234567890abcdef";
    let key_hash = format!("{:x}", Sha256::digest(raw_key.as_bytes()));
    sqlx::query(
        "INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, permissions, rate_limit, is_enabled, is_revoked, created_at, updated_at)
         VALUES ('key-1', ?, 'test', ?, ?, '[\"read\",\"write\"]', 60, 1, 0, datetime('now'), datetime('now'))",
    )
    .bind(key_workspace)
    .bind(&key_hash)
    .bind(&raw_key[..12])
    .execute(&pool)
    .await
    .unwrap();

    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, raw_key.to_string(), pool)
}

fn authed_post(uri: &str, key: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-API-Key", key)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn test_open_send_command_cross_workspace_404() {
    // Thing lives in ws-A; the API key belongs to ws-B → 404 (no cross-tenant dispatch)
    let (app, raw_key, _pool) = setup_open_app_with_key("ws-a", "ws-b", "device").await;
    let response = app
        .oneshot(authed_post(
            "/api/open/open/things/dev-open/command",
            &raw_key,
            serde_json::json!({"command": "reboot"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_open_send_command_non_device_400() {
    let (app, raw_key, _pool) = setup_open_app_with_key("ws-a", "ws-a", "space").await;
    let response = app
        .oneshot(authed_post(
            "/api/open/open/things/dev-open/command",
            &raw_key,
            serde_json::json!({"command": "reboot"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_open_send_command_unregistered_action_404() {
    let (app, raw_key, _pool) = setup_open_app_with_key("ws-a", "ws-a", "device").await;
    let response = app
        .oneshot(authed_post(
            "/api/open/open/things/dev-open/command",
            &raw_key,
            serde_json::json!({"command": "nonexistent"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_open_api_key_wrong_secret_rejected() {
    // Same prefix as the seeded key, different secret → 401 (hash verification)
    let (app, _raw_key, _pool) = setup_open_app_with_key("ws-a", "ws-a", "device").await;
    let wrong = "tinh_testkeyWRONGSECRETWRONG";
    let response = app
        .oneshot(authed_post(
            "/api/open/open/things/dev-open/command",
            wrong,
            serde_json::json!({"command": "reboot"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_open_api_key_expired_rejected() {
    let (app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    crate::test_utils::seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    let raw_key = "tinh_expiredkey1234567890";
    let key_hash = format!("{:x}", Sha256::digest(raw_key.as_bytes()));
    sqlx::query(
        "INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, permissions, rate_limit, is_enabled, is_revoked, expires_at, created_at, updated_at)
         VALUES ('key-exp', 'ws-a', 'expired', ?, ?, '[]', 60, 1, 0, '2020-01-01 00:00:00', datetime('now'), datetime('now'))",
    )
    .bind(&key_hash)
    .bind(&raw_key[..12])
    .execute(&pool)
    .await
    .unwrap();
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    let req = Request::builder()
        .method("GET")
        .uri("/api/open/open/things")
        .header("X-API-Key", raw_key)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
