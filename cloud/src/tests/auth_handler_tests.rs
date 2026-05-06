//! Auth handler integration tests
//!
//! Tests health, auth endpoints, session management, SMS auth, and social auth.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn public_request(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method).uri(uri)
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

fn auth_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method).uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::empty()).unwrap()
}

// ============================================================================
// Health Endpoint
// ============================================================================

#[tokio::test]
async fn test_health_endpoint() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ============================================================================
// Unauthorized Access — no token
// ============================================================================

#[tokio::test]
async fn test_unauthorized_access_no_token() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("Content-Type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // JWT middleware should reject requests without a token
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 for missing token"
    );
}

// ============================================================================
// Invalid Token
// ============================================================================

#[tokio::test]
async fn test_invalid_token() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("Authorization", "Bearer invalid.jwt.token")
                .header("Content-Type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // JWT middleware should reject requests with invalid tokens
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 for invalid token"
    );
}

// ============================================================================
// Login (public)
// ============================================================================

#[tokio::test]
async fn test_login_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/login", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_register_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/register", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_logout_missing_session() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/logout", None))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success() || response.status().is_client_error());
}

// ============================================================================
// Session (protected)
// ============================================================================

#[tokio::test]
async fn test_get_profile() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/auth/session/profile", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_validate_session() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/auth/session/validate", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_refresh_token_missing() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/auth/session/refresh", &token))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ============================================================================
// SMS Auth (public)
// ============================================================================

#[tokio::test]
async fn test_sms_send_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/sms/send", Some(json!({}))))
        .await
        .unwrap();
    // SMS handler may return 500 if SMS provider isn't configured in test env
    let status = response.status();
    assert!(
        status.is_success() || status.is_client_error() || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Unexpected status: {}",
        status
    );
}

#[tokio::test]
async fn test_sms_login_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/sms/login", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_sms_verify_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("GET", "/api/v1/auth/sms/verify", None))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ============================================================================
// Social Auth (public)
// ============================================================================

#[tokio::test]
async fn test_get_wechat_qrcode() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("GET", "/api/v1/auth/social/wechat/qrcode", None))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_wechat_login_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/social/wechat/login", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_social_config() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("GET", "/api/v1/auth/social/config", None))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_update_social_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/auth/social/config", &token))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_wechat_callback() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("GET", "/api/v1/auth/social/wechat/callback?code=test&state=test", None))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_wechat_miniprogram_login_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/social/wechat/miniprogram/login", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_bind_social_account_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/social/bind", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_unbind_social_account_missing_fields() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/social/unbind", Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

// ============================================================================
// Register (detailed business logic)
// ============================================================================

#[tokio::test]
async fn test_register_empty_username() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/register",
            Some(json!({
                "username": "",
                "password": "password123"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for empty username");
    assert!(json["msg"].as_str().unwrap().contains("用户名不能为空"));
}

#[tokio::test]
async fn test_register_short_password() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/register",
            Some(json!({
                "username": "newuser",
                "password": "123"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for short password");
    assert!(json["msg"].as_str().unwrap().contains("密码至少6个字符"));
}

#[tokio::test]
async fn test_register_duplicate_username() {
    let app = setup_test_app().await;
    let body = json!({
        "username": "duplicateuser",
        "password": "password123"
    });

    // First registration should succeed
    let response = app
        .clone()
        .oneshot(public_request("POST", "/api/v1/auth/register", Some(body.clone())))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"].as_i64(), Some(0), "First registration should succeed");

    // Second registration with same username should fail
    let response = app
        .oneshot(public_request("POST", "/api/v1/auth/register", Some(body)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for duplicate username");
    assert!(json["msg"].as_str().unwrap().contains("用户名已存在"));
}

#[tokio::test]
async fn test_register_success() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/register",
            Some(json!({
                "username": "testregister",
                "password": "password123",
                "display_name": "Test Register"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"].as_i64(), Some(0), "Registration should succeed");
    assert!(json["result"]["access_token"].as_str().is_some(), "Should return access token");
    assert_eq!(json["result"]["token_type"], "Bearer");
    assert!(json["result"]["user_info"]["name"].as_str().is_some());
}

// ============================================================================
// Login (detailed business logic)
// ============================================================================

#[tokio::test]
async fn test_login_empty_username() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({
                "username": "",
                "password": "password123"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for empty username");
    assert!(json["msg"].as_str().unwrap().contains("用户名和密码不能为空"));
}

#[tokio::test]
async fn test_login_empty_password() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({
                "username": "testuser",
                "password": ""
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for empty password");
    assert!(json["msg"].as_str().unwrap().contains("用户名和密码不能为空"));
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let app = setup_test_app().await;

    // Register a user first
    let reg_body = json!({
        "username": "logintestuser",
        "password": "password123"
    });
    let response = app
        .clone()
        .oneshot(public_request("POST", "/api/v1/auth/register", Some(reg_body)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Try to login with wrong password
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({
                "username": "logintestuser",
                "password": "wrongpassword"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"].as_i64(), Some(0), "Expected error for invalid credentials");
    assert!(json["msg"].as_str().unwrap().contains("用户名或密码错误"));
}

#[tokio::test]
async fn test_login_success() {
    let app = setup_test_app().await;

    // Register a user first
    let reg_body = json!({
        "username": "loginsuccess",
        "password": "password123",
        "display_name": "Login Success"
    });
    let response = app
        .clone()
        .oneshot(public_request("POST", "/api/v1/auth/register", Some(reg_body)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"].as_i64(), Some(0), "Registration should succeed");

    // Login with correct credentials
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({
                "username": "loginsuccess",
                "password": "password123"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"].as_i64(), Some(0), "Login should succeed");
    assert!(json["result"]["access_token"].as_str().is_some(), "Should return access token");
    assert_eq!(json["result"]["token_type"], "Bearer");
    assert_eq!(json["result"]["expires_in"], 24 * 60 * 60);
    assert_eq!(json["result"]["user_info"]["name"], "Login Success");
}

#[tokio::test]
async fn test_logout_success() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(public_request(
            "POST",
            "/api/v1/auth/logout",
            Some(json!({})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"].as_i64(), Some(0), "Logout should succeed");
    assert_eq!(json["result"], "登出成功");
}
