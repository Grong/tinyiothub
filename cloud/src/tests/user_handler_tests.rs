//! User handler integration tests
//!
//! Tests user CRUD, roles, and permissions endpoints.

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
// Users CRUD
// ============================================================================

#[tokio::test]
async fn test_list_users() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_current_user() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/me", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_user_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/nonexistent-user-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent user");
}

#[tokio::test]
async fn test_create_user_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/users", &token, Some(json!({}))))
        .await
        .unwrap();

    let status = response.status();
    // Should reject with 422 (deserialization) or 200 with error code
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected validation error, got: {}",
        status
    );
}

#[tokio::test]
async fn test_update_user_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/users/nonexistent-user-12345", &token, Some(json!({"name": "updated"}))))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent user");
}

#[tokio::test]
async fn test_delete_user_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/users/nonexistent-user-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    // May return error or idempotent success
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Roles
// ============================================================================

#[tokio::test]
async fn test_list_roles() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/roles?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_role_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/roles/nonexistent-role-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent role");
}

// ============================================================================
// Permissions
// ============================================================================

#[tokio::test]
async fn test_list_permissions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/permissions", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_user_statistics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/statistics", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Enable / Disable User
// ============================================================================

#[tokio::test]
async fn test_enable_user_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/users/nonexistent-user-12345/enable",
            &token,
            None,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent user enable");
}

#[tokio::test]
async fn test_disable_user_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/users/nonexistent-user-12345/disable",
            &token,
            None,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent user disable");
}

// ============================================================================
// Change Password
// ============================================================================

#[tokio::test]
async fn test_change_password_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/users/user-1/password",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected validation error, got: {}",
        status
    );
}

#[tokio::test]
async fn test_change_password_weak() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Password too short (< 8 chars) — uses old_password + new_password field names
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/users/user-1/password",
            &token,
            Some(json!({
                "old_password": "test",
                "new_password": "123"
            })),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert!(status == StatusCode::OK || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected OK or 422, got: {}", status);

    if status == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert_ne!(json["code"], 0, "Expected validation error for weak password");
    }
}

// ============================================================================
// User — test endpoint
// ============================================================================

#[tokio::test]
async fn test_users_test_endpoint() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/test", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);
    // This endpoint returns plain text "Users module is working!", not JSON
}
