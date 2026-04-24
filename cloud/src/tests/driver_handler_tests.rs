//! Driver handler integration tests
//!
//! Tests driver listing, detail, config, and support check endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
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
// List Drivers
// ============================================================================

#[tokio::test]
async fn test_list_drivers() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/drivers?page=1&page_size=20", &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_list_driver_names() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/drivers/names", &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Get Driver Detail
// ============================================================================

#[tokio::test]
async fn test_get_driver_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/drivers/nonexistent-driver", &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent driver");
}

// ============================================================================
// Get Driver Config
// ============================================================================

#[tokio::test]
async fn test_get_driver_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/drivers/nonexistent-driver/config", &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent driver config");
}

// ============================================================================
// Check Driver Support
// ============================================================================

#[tokio::test]
async fn test_check_driver_support_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/drivers/nonexistent-driver/supported", &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}
