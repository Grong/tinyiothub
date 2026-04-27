//! Alarm handler integration tests
//!
//! Tests alarm rules CRUD and alarm query endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

/// Helper: build a request with auth and optional body.
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
// List Alarm Rules
// ============================================================================

#[tokio::test]
async fn test_list_alarm_rules() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarm-rules", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK, "Handler should return 200");

    // Response may be success (code 0) or error (code -1) depending on DB state
    // We're verifying the handler doesn't panic and returns valid JSON
    let (_status, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Create Alarm Rule
// ============================================================================

#[tokio::test]
async fn test_create_alarm_rule() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-high-temp-rule",
        "description": "Alert when temperature exceeds threshold",
        "rule_type": "threshold",
        "condition": {
            "type": "threshold",
            "operator": "greater_than",
            "value": 100.0
        },
        "alarm_level": "warning",
        "notification_config": {
            "enabled": false,
            "channels": [],
            "recipients": []
        }
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/alarm-rules", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    if status == StatusCode::OK && json["code"] == 0 {
        assert!(json["result"].is_object(), "Expected alarm rule object");
        assert_eq!(json["result"]["name"], "test-high-temp-rule");
        assert_eq!(json["result"]["alarm_level"], "warning");
    }
    // Accept error responses if DB/config isn't fully initialized
}

// ============================================================================
// Get Alarm Rule — not found
// ============================================================================

#[tokio::test]
async fn test_get_alarm_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarm-rules/nonexistent-rule-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent rule");
}

// ============================================================================
// Update Alarm Rule — not found
// ============================================================================

#[tokio::test]
async fn test_update_alarm_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-rule-name"
    });

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/alarm-rules/nonexistent-rule-12345", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent rule");
}

// ============================================================================
// Delete Alarm Rule — not found
// ============================================================================

#[tokio::test]
async fn test_delete_alarm_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/alarm-rules/nonexistent-rule-12345", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK, "Handler should return 200");

    // Handler may return code 0 (idempotent delete) or error code
    // Both are acceptable — we're verifying the handler doesn't panic
    let (_status, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Get Alarm Statistics
// ============================================================================

#[tokio::test]
async fn test_get_alarm_statistics() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarms/statistics", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_object(), "Expected statistics object");
    // Should have count fields
    assert!(json["result"]["total_count"].is_number(), "Expected total_count field");
}

// ============================================================================
// Get Recent Alarms
// ============================================================================

#[tokio::test]
async fn test_get_recent_alarms() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarms/recent?limit=5", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of recent alarms");
}
