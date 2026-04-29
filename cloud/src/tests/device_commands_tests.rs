//! Device commands handler integration tests

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

// ============================================================================
// Execute Device Command
// ============================================================================

#[tokio::test]
async fn test_execute_command_nonexistent_command() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    let body = json!({ "parameters": {} });

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/some-device/commands/nonexistent-cmd/execute",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent command");
}

#[tokio::test]
async fn test_execute_command_without_parameters() {
    let app = setup_test_app().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default-001");

    // No parameters — should still validate command existence
    let body = json!({});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/some-device/commands/nonexistent-cmd/execute",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent command");
}
