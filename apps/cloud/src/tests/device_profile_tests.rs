//! Thing profile handler integration tests

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
// Get Thing Profile
// ============================================================================

#[tokio::test]
async fn test_get_device_profile_nonexistent_thing() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/nonexistent-id/profile", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(json["code"], 0, "Expected error for nonexistent device");
}

#[tokio::test]
async fn test_get_device_profile_existing_thing() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    // First create a device
    let create_body = json!({
        "name": "profile-test-device",
        "display_name": "Profile Test Thing",
        "category": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot({
            let mut builder = Request::builder()
                .method("POST")
                .uri("/api/v1/things")
                .header("Authorization", auth_header(&token))
                .header("Content-Type", "application/json");
            builder.body(Body::from(create_body.to_string())).unwrap()
        })
        .await
        .unwrap();

    let (_status, create_json) = response_parts(response).await;
    let thing_id = create_json["result"]["id"].as_str().unwrap().to_string();

    // Now get the profile (main thing router; the admin duplicate was removed)
    let response = app
        .oneshot(auth_request("GET", &format!("/api/v1/things/{}/profile", thing_id), &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // ThingProfileResponse structure (thing fields flattened at top level)
    assert_eq!(json["result"]["id"].as_str(), Some(thing_id.as_str()));
    assert!(json["result"]["name"].is_string(), "Expected thing name");
    assert!(json["result"]["properties"].is_array(), "Expected properties array");
    assert!(json["result"]["actions"].is_array(), "Expected actions array");
    assert!(json["result"]["recent_events"].is_array(), "Expected recent_events array");
}
