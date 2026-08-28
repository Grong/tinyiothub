//! Thing monitoring handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, setup_test_app,
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

fn auth_request_with_body(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ============================================================================
// Thing Online Status
// ============================================================================

#[tokio::test]
async fn test_get_device_online_status() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/some-device/status", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"]["is_online"].is_boolean(), "Expected is_online boolean");
    assert_eq!(json["result"]["thing_id"], "some-device");
}

// ============================================================================
// Thing Metrics
// ============================================================================

#[tokio::test]
async fn test_get_thing_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/some-device/metrics", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // Metrics may be null if device not found
}

// ============================================================================
// System Overview
// ============================================================================

#[tokio::test]
async fn test_get_system_overview() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/overview", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_object(), "Expected system overview object");
}

// ============================================================================
// Thing Performance Metrics
// ============================================================================

#[tokio::test]
async fn test_get_device_performance_metrics() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/some-device/performance", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
}

// ============================================================================
// Thing Performance History
// ============================================================================

#[tokio::test]
async fn test_get_device_performance_history() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/some-device/performance/history?hours=24", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    // May return error if device not found, or success with empty history
    assert!(json["code"].is_number(), "Response must have code field");
}

// ============================================================================
// System Performance Overview
// ============================================================================

#[tokio::test]
async fn test_get_system_performance_overview() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/performance/overview", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_object(), "Expected performance overview object");
}

// ============================================================================
// Thing Performance Alerts
// ============================================================================

#[tokio::test]
async fn test_get_device_performance_alerts() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/some-device/performance/alerts", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of alerts");
}

// ============================================================================
// All Performance Alerts
// ============================================================================

#[tokio::test]
async fn test_get_all_performance_alerts() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/admin/performance/alerts", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of alerts");
}
