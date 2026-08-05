use std::sync::Arc;
use tinyiothub_edge::app_state::AppState;
use tinyiothub_edge::config::{EdgeConfig, GatewayCredentials};

fn test_config() -> EdgeConfig {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("edge.db");
    let config_file = dir.path().join("config.yaml");
    std::mem::forget(dir);
    EdgeConfig {
        db_path,
        config_file,
        ..EdgeConfig::default()
    }
}

fn test_credentials() -> GatewayCredentials {
    GatewayCredentials {
        device_id: "test-dev".into(),
        client_id: "test-client".into(),
        username: "user".into(),
        password: "pass".into(),
        workspace_id: "ws-1".into(),
    }
}

async fn test_state() -> Arc<AppState> {
    Arc::new(AppState::new(test_config(), test_credentials()).await.unwrap())
}

// ── Health ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_health_handler() {
    use tinyiothub_edge::modules::http::handlers::get_health;
    let state = test_state().await;
    let response = get_health(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

// ── Devices ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_devices_handler_empty() {
    use tinyiothub_edge::modules::http::handlers::get_devices;
    let state = test_state().await;
    let response = get_devices(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

#[tokio::test]
async fn test_get_device_handler_not_found() {
    use tinyiothub_edge::modules::http::handlers::get_device;
    let state = test_state().await;
    let response = get_device(
        axum::extract::State(state),
        axum::extract::Path("nonexistent".to_string()),
    )
    .await;
    assert_eq!(response.0.code, -1);
    assert!(response.0.result.is_none());
    assert!(response.0.msg.contains("not found"));
}

#[tokio::test]
async fn test_get_device_properties_handler_not_found() {
    use tinyiothub_edge::modules::http::handlers::get_device_properties;
    let state = test_state().await;
    let response = get_device_properties(
        axum::extract::State(state),
        axum::extract::Path("nonexistent".to_string()),
    )
    .await;
    assert_eq!(response.0.code, -1);
}

#[tokio::test]
async fn test_post_device_properties_handler_not_found() {
    use tinyiothub_edge::modules::http::handlers::post_device_properties;
    let state = test_state().await;
    let response = post_device_properties(
        axum::extract::State(state),
        axum::extract::Path("nonexistent".to_string()),
        axum::Json(serde_json::json!({"key": "value"})),
    )
    .await;
    assert_eq!(response.0.code, -1);
}

#[tokio::test]
async fn test_post_device_command_handler_not_found() {
    use tinyiothub_edge::modules::http::handlers::post_device_command;
    let state = test_state().await;
    let response = post_device_command(
        axum::extract::State(state),
        axum::extract::Path("nonexistent".to_string()),
        axum::Json(serde_json::json!({"action": "reboot"})),
    )
    .await;
    assert_eq!(response.0.code, -1);
}

// ── Drivers ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_drivers_handler() {
    use tinyiothub_edge::modules::http::handlers::get_drivers;
    let state = test_state().await;
    let response = get_drivers(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

#[tokio::test]
async fn test_post_driver_scan_handler() {
    use tinyiothub_edge::modules::http::handlers::post_driver_scan;
    let state = test_state().await;
    let response = post_driver_scan(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
}

// ── Alarms ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_alarms_handler() {
    use tinyiothub_edge::modules::http::handlers::get_alarms;
    let state = test_state().await;
    let response = get_alarms(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

// ── Config ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_config_handler() {
    use tinyiothub_edge::modules::http::handlers::get_config;
    let state = test_state().await;
    let response = get_config(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

#[tokio::test]
async fn test_put_config_handler() {
    use tinyiothub_edge::modules::http::handlers::put_config;
    let state = test_state().await;
    let response = put_config(
        axum::extract::State(state),
        axum::Json(serde_json::json!({"telemetry_interval_secs": 60})),
    )
    .await;
    assert_eq!(response.0.code, 0);
}

// ── Offline buffer ───────────────────────────────────────────────

#[tokio::test]
async fn test_get_offline_buffer_handler() {
    use tinyiothub_edge::modules::http::handlers::get_offline_buffer;
    let state = test_state().await;
    let response = get_offline_buffer(axum::extract::State(state)).await;
    assert_eq!(response.0.code, 0);
    assert!(response.0.result.is_some());
}

// ── Router creation ──────────────────────────────────────────────

#[tokio::test]
async fn test_create_router_smoke() {
    let state = test_state().await;
    let _router = tinyiothub_edge::modules::http::service::create_router(state);
}

// ── Auth helpers & serialization ────────────────────────────────
// std::env::set_var/remove_var are process-global, so auth tests
// that mutate the env must not run concurrently.

use std::sync::Mutex;
static AUTH_TEST_LOCK: Mutex<()> = Mutex::new(());

fn set_auth_key(key: &str) {
    unsafe { std::env::set_var("EDGE_LOCAL_API_KEY", key) };
    tinyiothub_edge::modules::http::auth::reset_api_key_cache();
}

fn clear_auth_key() {
    unsafe { std::env::remove_var("EDGE_LOCAL_API_KEY") };
    tinyiothub_edge::modules::http::auth::reset_api_key_cache();
}

// ── Auth middleware: no key configured ───────────────────────────

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_auth_middleware_no_key_passes() {
    let _guard = AUTH_TEST_LOCK.lock().unwrap();
    clear_auth_key();

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tinyiothub_edge::modules::http::auth::auth_middleware;
    use tower::ServiceExt;

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn(auth_middleware));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ── Auth middleware: with key configured ─────────────────────────

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_auth_with_valid_token_passes() {
    let _guard = AUTH_TEST_LOCK.lock().unwrap();
    set_auth_key("test-key");

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tinyiothub_edge::modules::http::auth::auth_middleware;
    use tower::ServiceExt;

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn(auth_middleware));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    clear_auth_key();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_auth_with_invalid_token_returns_401() {
    let _guard = AUTH_TEST_LOCK.lock().unwrap();
    set_auth_key("test-key");

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tinyiothub_edge::modules::http::auth::auth_middleware;
    use tower::ServiceExt;

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn(auth_middleware));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("Authorization", "Bearer wrong-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    clear_auth_key();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_auth_with_missing_header_returns_401() {
    let _guard = AUTH_TEST_LOCK.lock().unwrap();
    set_auth_key("test-key");

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tinyiothub_edge::modules::http::auth::auth_middleware;
    use tower::ServiceExt;

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn(auth_middleware));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    clear_auth_key();
}
