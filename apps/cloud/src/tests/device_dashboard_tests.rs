//! Device dashboard handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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

// ============================================================================
// Device Status Distribution
// ============================================================================

#[tokio::test]
async fn test_get_device_distribution() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/distribution", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // DeviceStatusDistribution has fields like online, offline, etc.
    assert!(json["result"].is_object(), "Expected device status distribution object");
}

// ============================================================================
// Quick Devices
// ============================================================================

#[tokio::test]
async fn test_get_quick_devices() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/quick?limit=5", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of quick devices");
}

#[tokio::test]
async fn test_get_quick_devices_default_limit() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/quick", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success without limit param");
}
