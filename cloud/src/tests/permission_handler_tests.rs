//! Permission handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

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
// List Permissions
// ============================================================================

#[tokio::test]
async fn test_list_permissions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response =
        app.oneshot(auth_request("GET", "/api/v1/users/permissions", &token, None)).await.unwrap();

    let status = response.status();
    assert!(
        !status.is_informational() && status != StatusCode::SWITCHING_PROTOCOLS,
        "Unexpected status: {}",
        status
    );
    let (_, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Response must have numeric code field");
}

// ============================================================================
// Get User Permissions (stub endpoint)
// ============================================================================

#[tokio::test]
async fn test_get_user_permissions_stub() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/permissions/user-1/permissions", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code for stub endpoint");
    // Stub returns empty array
    assert!(json["result"].is_array(), "Expected array result");
}
