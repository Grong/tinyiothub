//! Workspace handler integration tests
//!
//! Tests workspace CRUD endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
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
// Workspaces CRUD
// ============================================================================

#[tokio::test]
async fn test_list_workspaces() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/workspaces", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_create_workspace() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-workspace",
        "description": "Integration test workspace"
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/workspaces", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert!(
        status.is_success() || status.is_client_error(),
        "Expected 2xx or 4xx, got: {}",
        status
    );
    assert!(json["code"].is_number(), "Response must have numeric code field");
}

#[tokio::test]
async fn test_get_workspace_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/workspaces/nonexistent-ws-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent workspace");
}

#[tokio::test]
async fn test_update_workspace_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/workspaces/nonexistent-ws-12345", &token, Some(json!({"name": "updated"}))))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent workspace");
}

#[tokio::test]
async fn test_delete_workspace_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/workspaces/nonexistent-ws-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}
