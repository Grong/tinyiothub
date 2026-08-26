//! Device profile handler integration tests

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

// ============================================================================
// Get Device Profile
// ============================================================================

#[tokio::test]
async fn test_get_device_profile_nonexistent_device() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id/profile", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent device");
}

#[tokio::test]
async fn test_get_device_profile_existing_device() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    // First create a device
    let create_body = json!({
        "name": "profile-test-device",
        "display_name": "Profile Test Device",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot({
            let mut builder = Request::builder()
                .method("POST")
                .uri("/api/v1/devices")
                .header("Authorization", auth_header(&token))
                .header("Content-Type", "application/json");
            builder.body(Body::from(create_body.to_string())).unwrap()
        })
        .await
        .unwrap();

    let (_status, create_json) = response_parts(response).await;
    let device_id = create_json["result"]["id"].as_str().unwrap().to_string();

    // Now get the profile
    let response = app
        .oneshot(auth_request("GET", &format!("/api/v1/devices/{}/profile", device_id), &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // DeviceProfile structure
    assert!(json["result"]["device"].is_object(), "Expected device object");
    assert!(json["result"]["is_online"].is_boolean(), "Expected is_online boolean");
    assert!(json["result"]["properties"].is_array(), "Expected properties array");
    assert!(json["result"]["commands"].is_array(), "Expected commands array");
    assert!(json["result"]["overview"].is_object(), "Expected overview object");
    assert!(json["result"]["generated_at"].is_string(), "Expected generated_at timestamp");
}
