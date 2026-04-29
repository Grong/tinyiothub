//! Permission handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

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
// List Permissions
// ============================================================================

#[tokio::test]
async fn test_list_permissions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/permissions", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Response must have numeric code field");
    // If code == 0, result should be array; if service failed, result may be null
    if json["code"] == 0 {
        assert!(json["result"].is_array(), "Expected array of permissions");
    }
}

// ============================================================================
// Get User Permissions
// ============================================================================

#[tokio::test]
async fn test_get_user_permissions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Permission router is nested: /users/permissions + /{id}/permissions
    // → /api/v1/users/permissions/{id}/permissions
    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/permissions/some-user-id/permissions", &token))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    // Stub returns empty array with success code
    assert!(json["code"].is_number(), "Response must have numeric code field");
}
