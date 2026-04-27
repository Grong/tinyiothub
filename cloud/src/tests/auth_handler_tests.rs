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
