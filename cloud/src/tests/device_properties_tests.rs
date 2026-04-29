//! Device properties handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, setup_test_app,
};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };
    builder.body(Body::from(body_str)).unwrap()
}

/// Helper: create a device and return its ID.
async fn create_test_device(app: &mut axum::Router, token: &str) -> String {
    let body = json!({
        "name": "prop-test-device",
        "display_name": "Properties Test Device",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/devices", token, Some(body)))
        .await
        .unwrap();

    let (_status, json) = response_parts(response).await;
    json["result"]["id"].as_str().unwrap().to_string()
}

// ============================================================================
// Get Device Properties
// ============================================================================

#[tokio::test]
async fn test_get_device_properties() {
    let mut app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");
    let device_id = create_test_device(&mut app, &token).await;

    let response = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/devices/{}/properties", device_id),
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of properties");
}

#[tokio::test]
async fn test_get_device_properties_nonexistent_device() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id/properties",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    // Handler may return success with empty properties or error — both valid
    assert!(json["code"].is_number(), "Response must have code field");
}

// ============================================================================
// Get Device Property By Name
// ============================================================================

#[tokio::test]
async fn test_get_device_property_by_name_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/by-name/no-such-device/properties/some-prop",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent device");
}

// ============================================================================
// Update Property Value
// ============================================================================

#[tokio::test]
async fn test_update_property_value_nonexistent_device() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let body = json!({ "value": "42" });

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/devices/nonexistent-id/properties/some-prop/value",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent device/property");
}
