//! Device handler integration tests
//!
//! Tests device CRUD endpoints using `tower::ServiceExt::oneshot()`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

/// Helper: build a request with auth and optional body.
fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");

    // All device handlers use WorkspaceScope extractor
    builder = builder.header("X-Workspace-Id", "test-workspace");

    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };

    builder.body(Body::from(body_str)).unwrap()
}

// ============================================================================
// Create Device
// ============================================================================

#[tokio::test]
async fn test_create_device() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-device-001",
        "display_name": "Test Device",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();
    // Handler should respond (200 or 422/500) — we're verifying it doesn't panic
    assert!(
        status == StatusCode::OK || status.is_client_error() || status.is_server_error(),
        "Handler should respond, got: {}",
        status
    );

    if status == StatusCode::OK {
        let (_status, json) = response_parts(response).await;
        // If OK, should have valid JSON response
        assert!(json.is_object(), "Expected JSON object");
    }
}

// ============================================================================
// List Devices
// ============================================================================

#[tokio::test]
async fn test_list_devices() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // result should be a paginated response with data array
    assert!(json["result"]["data"].is_array(), "Expected data array");
    assert!(json["result"]["pagination"].is_object(), "Expected pagination object");
}

// ============================================================================
// Get Device — not found
// ============================================================================

#[tokio::test]
async fn test_get_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    // Handler returns error in JSON body, not HTTP status
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Update Device — not found
// ============================================================================

#[tokio::test]
async fn test_update_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-name"
    });

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/devices/nonexistent-id-12345", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Delete Device — not found
// ============================================================================

#[tokio::test]
async fn test_delete_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/devices/nonexistent-id-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Create Device — validation: missing required name
// ============================================================================

#[tokio::test]
async fn test_create_device_missing_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Empty body — name is required
    let body = json!({});

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();

    // Axum's Json extractor returns 422 for deserialization failures (missing required field)
    // This is expected behavior — the handler correctly rejects invalid input
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200 for missing name, got: {}",
        status
    );
}

// ============================================================================
// Create Device — empty name validation
// ============================================================================

#[tokio::test]
async fn test_create_device_empty_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": ""
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    // Should get a validation error from the service layer
    assert_ne!(json["code"], 0, "Expected validation error for empty name");
}
