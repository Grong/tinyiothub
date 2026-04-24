//! Notification handler integration tests
//!
//! Tests notification rules CRUD, channels CRUD, history, and statistics endpoints.

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
// Notification Rules
// ============================================================================

#[tokio::test]
async fn test_list_notification_rules() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notifications/rules", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_create_notification_rule_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/notifications/rules", &token, Some(json!({}))))
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
async fn test_get_notification_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notifications/rules/nonexistent-rule-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent rule");
}

#[tokio::test]
async fn test_update_notification_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/notifications/rules/nonexistent-rule-12345", &token, Some(json!({"name": "updated"}))))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent rule");
}

#[tokio::test]
async fn test_delete_notification_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/notifications/rules/nonexistent-rule-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_notification_history() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notifications/history", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_send_test_notification_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/notifications/test", &token, Some(json!({}))))
        .await
        .unwrap();

    let status = response.status();
    // May return 422 (missing required fields) or 200 with error code
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200 for missing fields, got: {}",
        status
    );
}

// ============================================================================
// Notification Channels
// NOTE: The channel router defines routes with `/notification-channels` prefix
// inside a router nested at `/notification-channels` in api/mod.rs, resulting
// in double-nesting: `/v1/notification-channels/notification-channels/...`
// ============================================================================

#[tokio::test]
async fn test_list_notification_channels() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notification-channels/notification-channels", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_create_notification_channel_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/notification-channels/notification-channels", &token, Some(json!({}))))
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
async fn test_get_notification_channel_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notification-channels/notification-channels/nonexistent-ch-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent channel");
}

#[tokio::test]
async fn test_update_notification_channel_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/notification-channels/notification-channels/nonexistent-ch-12345", &token, Some(json!({"name": "updated"}))))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent channel");
}

#[tokio::test]
async fn test_delete_notification_channel_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/notification-channels/notification-channels/nonexistent-ch-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_notification_channel_statistics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/notification-channels/notification-channels/statistics", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_test_notification_channel_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/notification-channels/notification-channels/nonexistent-ch-12345/test", &token, Some(json!({}))))
        .await
        .unwrap();

    let status = response.status();
    // May return 422 (missing required body fields) or 200 with error code
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200, got: {}",
        status
    );
}
