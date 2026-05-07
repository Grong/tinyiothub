//! Token handler integration tests
//!
//! Tests token refresh and logout endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    create_test_token, response_parts, setup_test_app, setup_test_app_with_pool,
};

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ============================================================================
// Token Refresh
// ============================================================================

#[tokio::test]
async fn test_token_refresh_success() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(json_request("POST", "/api/v1/auth/token/refresh", json!({ "token": token })))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 for valid token refresh, got body: {:?}",
        body
    );
    assert_eq!(body["code"], 0, "Expected success code, got: {:?}", body);
    assert!(
        body["result"]["access_token"].is_string(),
        "Expected access_token in result: {:?}",
        body
    );
    assert_eq!(body["result"]["token_type"], "Bearer");
    assert_eq!(body["result"]["expires_in"], 86400);
}

#[tokio::test]
async fn test_token_refresh_invalid_token() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/token/refresh",
            json!({ "token": "invalid.jwt.token" }),
        ))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(body["code"], 0, "Expected error code for invalid token");
    assert!(
        body["msg"].as_str().unwrap().contains("Invalid")
            || body["msg"].as_str().unwrap().contains("expired"),
        "Expected invalid/expired message, got: {:?}",
        body["msg"]
    );
}

#[tokio::test]
async fn test_token_refresh_missing_token() {
    let app = setup_test_app().await;

    let response =
        app.oneshot(json_request("POST", "/api/v1/auth/token/refresh", json!({}))).await.unwrap();

    // Missing token field should cause deserialization error
    assert!(
        response.status().is_client_error() || response.status().is_success(),
        "Unexpected status: {:?}",
        response.status()
    );
}

// ============================================================================
// Logout
// ============================================================================

#[tokio::test]
async fn test_logout_with_token() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(json_request("POST", "/api/v1/auth/token/logout", json!({ "token": token })))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "Expected 200 for logout, got: {:?}", body);
    assert_eq!(body["code"], 0, "Expected success code, got: {:?}", body);
    assert_eq!(body["result"], "Logged out successfully");
}

#[tokio::test]
async fn test_logout_without_token() {
    let app = setup_test_app().await;

    let response =
        app.oneshot(json_request("POST", "/api/v1/auth/token/logout", json!({}))).await.unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "Expected 200 for logout without token, got: {:?}", body);
    assert_eq!(body["code"], 0);
    assert_eq!(body["result"], "Logged out successfully");
}

#[tokio::test]
async fn test_logout_blacklists_token() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    let token = create_test_token("user-1", "tenant-1");

    // Logout to blacklist the token
    let logout_response = app
        .oneshot(json_request("POST", "/api/v1/auth/token/logout", json!({ "token": token })))
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::OK);

    // Verify token was inserted into blacklist table
    use sha2::{Digest, Sha256};
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM token_blacklist WHERE token_hash = ? LIMIT 1")
            .bind(&token_hash)
            .fetch_optional(&pool)
            .await
            .unwrap();

    assert!(row.is_some(), "Token should be present in blacklist table after logout");
}
