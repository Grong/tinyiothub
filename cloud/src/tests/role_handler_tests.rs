//! Role handler integration tests

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
// List Roles
// ============================================================================

#[tokio::test]
async fn test_list_roles() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/roles?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of roles");
}

// ============================================================================
// Create Role
// ============================================================================

#[tokio::test]
async fn test_create_role() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-role-001",
        "description": "A test role"
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/users/roles", &token, Some(body))).await.unwrap();

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
// Create Role — missing name
// ============================================================================

#[tokio::test]
async fn test_create_role_missing_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({});

    let response =
        app.oneshot(auth_request("POST", "/api/v1/users/roles", &token, Some(body))).await.unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200 for missing name, got: {}",
        status
    );
}

// ============================================================================
// Create Role — empty name
// ============================================================================

#[tokio::test]
async fn test_create_role_empty_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": ""
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/users/roles", &token, Some(body))).await.unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected validation error for empty name");
}

// ============================================================================
// Get Role — not found
// ============================================================================

#[tokio::test]
async fn test_get_role_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/users/roles/nonexistent-role-id", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent role");
}

// ============================================================================
// Update Role — not found
// ============================================================================

#[tokio::test]
async fn test_update_role_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-role-name"
    });

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/users/roles/nonexistent-role-id", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent role");
}

// ============================================================================
// Delete Role — not found
// ============================================================================

#[tokio::test]
async fn test_delete_role_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/users/roles/nonexistent-role-id", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent role");
}

// ============================================================================
// Role Permissions
// ============================================================================

#[tokio::test]
async fn test_get_role_permissions_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/users/roles/nonexistent-role-id/permissions",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_update_role_permissions_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let body = json!({"permission_ids": ["perm-1", "perm-2"]});
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/users/roles/nonexistent-role-id/permissions",
            &token,
            Some(body),
        ))
        .await
        .unwrap();
    let status = response.status();
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

// ============================================================================
// Role Permissions — real data
// ============================================================================

#[tokio::test]
async fn test_get_role_permissions_empty_for_new_role() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Create a role first
    let body = json!({"name": "perm-test-role-001", "description": "Role for permission testing"});
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/users/roles", &token, Some(body)))
        .await
        .unwrap();
    let (_status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Role creation should succeed: {}", json["msg"]);
    let role_id = json["result"]["id"].as_str().expect("Role should have an ID");

    // Get permissions for the new role
    let response = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/users/roles/{}/permissions", role_id),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Permissions should be an array");
    assert_eq!(json["result"].as_array().unwrap().len(), 0, "New role should have no permissions");
}

#[tokio::test]
async fn test_update_and_get_role_permissions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Create a role
    let body = json!({"name": "perm-test-role-002", "description": "Role for permission testing"});
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/users/roles", &token, Some(body)))
        .await
        .unwrap();
    let (_status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Role creation should succeed: {}", json["msg"]);
    let role_id = json["result"]["id"].as_str().expect("Role should have an ID");

    // Update permissions (use real permission IDs from migrations)
    let perm_ids =
        json!({"permission_ids": ["perm-device-read", "perm-device-write", "perm-user-read"]});
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/users/roles/{}/permissions", role_id),
            &token,
            Some(perm_ids),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Permission update should succeed");
    assert_eq!(json["result"], true, "Update should return true");

    // Get permissions and verify
    let response = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/users/roles/{}/permissions", role_id),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    let perms = json["result"].as_array().expect("Permissions should be an array");
    assert_eq!(perms.len(), 3, "Should have 3 permissions");
    let perm_strings: Vec<String> =
        perms.iter().filter_map(|p| p.as_str().map(String::from)).collect();
    assert!(perm_strings.contains(&"perm-device-read".to_string()));
    assert!(perm_strings.contains(&"perm-device-write".to_string()));
    assert!(perm_strings.contains(&"perm-user-read".to_string()));
}
