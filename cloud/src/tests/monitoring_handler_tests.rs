//! Monitoring handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts,
    seed_test_workspace, setup_test_app, setup_test_app_with_pool,
};

fn auth_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

// ── Metrics ──

#[tokio::test]
async fn test_get_system_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/metrics/system", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_device_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/metrics/devices", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_gateway_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/metrics/gateway", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Health ──

#[tokio::test]
async fn test_get_health() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/health", &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_detailed_health() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/health/detailed", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Logs ──

#[tokio::test]
async fn test_get_logs() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/logs", &token)).await.unwrap();
    // May return 400 if required query params missing
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_get_log_levels() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/logs/levels", &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Real data verification ──

#[tokio::test]
async fn test_get_health_returns_healthy_with_db_up() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/health", &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success code");
    assert_eq!(json["result"]["status"], "healthy", "DB should be up in test environment");
    assert!(json["result"]["uptime_seconds"].is_number(), "Uptime should be present as a number");
}

#[tokio::test]
async fn test_get_system_metrics_forbidden_for_non_admin() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/metrics/system", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 403, "Non-admin should get 403");
}

#[tokio::test]
async fn test_get_detailed_health_has_real_device_counts() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-test-001").await;

    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-test-001");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/monitoring/health/detailed", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success code");
    assert_eq!(json["result"]["database_status"], "connected", "DB should be connected");
    assert!(
        json["result"]["memory_usage_mb"].as_u64().unwrap_or(0) > 0,
        "Memory usage should be > 0"
    );
    assert!(
        json["result"]["cpu_usage_percent"].as_f64().unwrap_or(-1.0) >= 0.0,
        "CPU usage should be >= 0"
    );
}

// ── Dashboard ──

#[tokio::test]
async fn test_get_dashboard_stats() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/stats", &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_dashboard_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/monitoring/metrics", &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}
