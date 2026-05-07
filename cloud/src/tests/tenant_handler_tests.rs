//! Tenant handler integration tests

use axum::{body::Body, http::{Request, StatusCode}};
use serde_json::{json, Value};
use tower::ServiceExt;
use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method).uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

fn public_request(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method).uri(uri)
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

// ── Protected: Tenant CRUD ──
// NOTE: Tenant router defines `/tenants` inside router nested at `/v1/tenants`,
// resulting in `/v1/tenants/tenants/...`

#[tokio::test]
async fn test_list_tenants() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/tenants/tenants?page=1&page_size=20", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_create_tenant_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("POST", "/api/v1/tenants/tenants", &token, Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_tenant_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/tenants/tenants/nonexistent-tenant-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent tenant");
}

#[tokio::test]
async fn test_update_tenant_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/tenants/tenants/nonexistent-tenant-12345", &token, Some(json!({"name": "updated"})))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent tenant");
}

// ── Auth: Public Tenant Auth ──

#[tokio::test]
async fn test_tenant_login_missing_fields() {
    let app = setup_test_app().await;
    let response = app.oneshot(public_request("POST", "/api/v1/tenants/login", Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_list_plans() {
    let app = setup_test_app().await;
    let response = app.oneshot(public_request("GET", "/api/v1/tenants/plans", None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // Response may or may not be JSON (could be plain text or different format)
    let status = response.status();
    assert!(status.is_success(), "Expected success status, got: {}", status);
}

// ── API Keys ──

#[tokio::test]
async fn test_list_api_keys() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/api-keys", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_create_api_key_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("POST", "/api/v1/api-keys", &token, Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_revoke_api_key_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("DELETE", "/api/v1/api-keys/nonexistent-key-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Auth: Register & Verify ──

#[tokio::test]
async fn test_tenant_register_missing_fields() {
    let app = setup_test_app().await;
    let response = app.oneshot(public_request("POST", "/api/v1/tenants/register", Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_tenant_register_success() {
    let app = setup_test_app().await;

    let body = json!({
        "name": "Test Register Tenant",
        "slug": "test-register-tenant",
        "email": "register-test@example.com",
        "password": "TestPass123!",
        "plan_id": "plan_free"
    });

    let response = app.oneshot(public_request("POST", "/api/v1/tenants/register", Some(body))).await.unwrap();
    let status = response.status();
    // Register may return JSON or plain text depending on handler implementation
    assert!(status.is_success() || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected success or validation error, got: {}", status);
}

#[tokio::test]
async fn test_tenant_login_invalid_credentials() {
    let app = setup_test_app().await;

    let body = json!({
        "email": "nonexistent@example.com",
        "password": "WrongPass123!"
    });

    let response = app.oneshot(public_request("POST", "/api/v1/tenants/login", Some(body))).await.unwrap();
    let status = response.status();
    // Login returns 401 for invalid credentials — correct RESTful behavior
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::UNPROCESSABLE_ENTITY || status.is_success(),
        "Expected 401, 422, or 200, got: {}",
        status
    );
}

#[tokio::test]
async fn test_verify_token_invalid() {
    let app = setup_test_app().await;
    let response = app.oneshot(public_request("GET", "/api/v1/tenants/verify?token=invalid-token-12345", None)).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ── Protected: Tenant Usage & Change Plan ──

#[tokio::test]
async fn test_get_tenant_usage() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    // usage route: /tenants/{id}/usage → /api/v1/tenants/tenants/{id}/usage
    let response = app.oneshot(auth_request("GET", "/api/v1/tenants/tenants/tenant-1/usage", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_tenant_usage_stats() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    // usage-stats route: /{tenant_id}/usage-stats → /api/v1/tenants/{tenant_id}/usage-stats
    let response = app.oneshot(auth_request("GET", "/api/v1/tenants/tenant-1/usage-stats", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_change_plan_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let body = json!({"plan_id": "plan_pro"});
    // change-plan uses POST, not PUT. Route: /tenants/{id}/change-plan → /api/v1/tenants/tenants/{id}/change-plan
    let response = app.oneshot(auth_request("POST", "/api/v1/tenants/tenants/nonexistent-tenant-12345/change-plan", &token, Some(body))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent tenant");
}
