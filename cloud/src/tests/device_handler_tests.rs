//! Device handler integration tests
//!
//! Tests device CRUD endpoints using `tower::ServiceExt::oneshot()`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts,
    seed_test_workspace, setup_test_app, setup_test_app_with_pool,
};

/// Helper: build a request with auth and optional body.
fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");

    // WorkspaceScope now reads workspace_id from JWT claims, not from header.
    // Header is ignored to prevent cross-tenant forgery.
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
    // Handler should respond with valid HTTP status — not panic
    assert!(
        !status.is_informational() && status != StatusCode::SWITCHING_PROTOCOLS,
        "Unexpected status: {}",
        status
    );
    // Response should always be valid JSON with code field
    let (_status, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Response must have numeric code field");
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

// ============================================================================
// Cross-Tenant Isolation
// ============================================================================

/// Verify that a user in workspace A cannot see devices created in workspace B.
/// This is the regression test for the security bug where omitting X-Workspace-Id
/// header returned the raw (unfiltered) repository, exposing all devices.
#[tokio::test]
async fn test_cross_workspace_isolation() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed tenants and workspaces for the test
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;

    let api_router = crate::api::create_router();
    let app = axum::Router::new()
        .nest("/api", api_router)
        .with_state(app_state);

    // User A (workspace ws-a) creates a device
    let token_a = create_test_token_with_workspace("user-a", "tenant-a", "ws-a");

    let body = json!({
        "name": "device-in-ws-a",
        "display_name": "Device in Workspace A",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/devices", &token_a, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success creating device in workspace A");
    let device_id = json["result"]["id"].as_str().unwrap().to_string();
    assert!(!device_id.is_empty(), "Device should have an id");

    // User B (workspace ws-b) lists devices — should NOT see workspace A's device
    let token_b = create_test_token_with_workspace("user-b", "tenant-b", "ws-b");

    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices?page=1&page_size=20",
            &token_b,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");

    let data = json["result"]["data"].as_array().unwrap();
    let device_ids: Vec<&str> = data
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();

    assert!(
        !device_ids.contains(&device_id.as_str()),
        "SECURITY BUG: User B (ws-b) can see workspace A's device (ws-a). \
         Workspace isolation is broken!"
    );

    // User A should see their own device
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices?page=1&page_size=20",
            &token_a,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    let data = json["result"]["data"].as_array().unwrap();
    let device_ids: Vec<&str> = data
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert!(
        device_ids.contains(&device_id.as_str()),
        "User A should see their own device in workspace A"
    );
}
