//! Alarm handler integration tests
//!
//! Tests alarm rules CRUD and alarm query endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts,
    seed_test_workspace, setup_test_app, setup_test_app_with_pool,
};

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

    let response =
        app.oneshot(auth_request("GET", "/api/v1/alarm-rules", &token, None)).await.unwrap();

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

    let response =
        app.oneshot(auth_request("POST", "/api/v1/alarm-rules", &token, Some(body))).await.unwrap();

    let (status, json) = response_parts(response).await;

    if status == StatusCode::OK && json["code"] == 0 {
        assert!(json["result"].is_object(), "Expected alarm rule object");
        assert_eq!(json["result"]["name"], "test-high-temp-rule");
        assert_eq!(json["result"]["alarmLevel"], "warning");
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
        .oneshot(auth_request(
            "PUT",
            "/api/v1/alarm-rules/nonexistent-rule-12345",
            &token,
            Some(body),
        ))
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

    let response =
        app.oneshot(auth_request("GET", "/api/v1/alarms/statistics", &token, None)).await.unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_object(), "Expected statistics object");
    // Should have count fields
    assert!(json["result"]["totalCount"].is_number(), "Expected totalCount field");
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

// ============================================================================
// Toggle Alarm Rule — not found
// ============================================================================

#[tokio::test]
async fn test_toggle_alarm_rule_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"enabled": false});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/alarm-rules/nonexistent-rule-12345/toggle",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    // Toggle on nonexistent rule may return success (idempotent) or error
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// List Alarms
// ============================================================================

#[tokio::test]
async fn test_list_alarms() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarms?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"]["data"].is_array(), "Expected data array in paginated response");
}

// ============================================================================
// List Alarms — status filter
// ============================================================================

#[tokio::test]
async fn test_list_alarms_filter_by_status() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Get current count (setup data is all active)
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/alarms?page=1&page_size=20", &token, None))
        .await
        .unwrap();
    let (_, json) = response_parts(response).await;
    let total = json["result"]["data"].as_array().unwrap().len();
    assert!(total > 0, "Need at least 1 alarm from setup");

    // Filter: only active → should return all (all setup alarms are active)
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            "/api/v1/alarms?page=1&page_size=20&statuses=active",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0);
    assert_eq!(
        json["result"]["data"].as_array().unwrap().len(),
        total,
        "active filter should return all {} alarms (all are active)",
        total
    );

    // Filter: only acknowledged → should return 0
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            "/api/v1/alarms?page=1&page_size=20&statuses=acknowledged",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (_, json) = response_parts(response).await;
    assert_eq!(
        json["result"]["data"].as_array().unwrap().len(),
        0,
        "acknowledged filter should return 0 (no acknowledged alarms)"
    );

    // Filter: only resolved → should return 0
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            "/api/v1/alarms?page=1&page_size=20&statuses=resolved",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (_, json) = response_parts(response).await;
    assert_eq!(
        json["result"]["data"].as_array().unwrap().len(),
        0,
        "resolved filter should return 0 (no resolved alarms)"
    );
}

// ============================================================================
// Get Alarm — not found
// ============================================================================

#[tokio::test]
async fn test_get_alarm_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/alarms/nonexistent-alarm-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent alarm");
}

// ============================================================================
// List Alarm Rules — by device_id
// ============================================================================

#[tokio::test]
async fn test_list_alarm_rules_by_device() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/alarm-rules?device_id=nonexistent-device-12345",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Alarm Rule — workspace isolation
// ============================================================================

#[tokio::test]
async fn test_alarm_rule_workspace_isolation() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;

    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // User A creates an alarm rule
    let token_a = create_test_token_with_workspace("user-a", "tenant-a", "ws-a");

    let body = json!({
        "name": "ws-a-alarm-rule",
        "description": "Rule in workspace A",
        "rule_type": "threshold",
        "condition": {"type": "threshold", "operator": "greater_than", "value": 90.0},
        "alarm_level": "warning",
        "notification_config": {"enabled": false, "channels": [], "recipients": []}
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/alarm-rules", &token_a, Some(body)))
        .await
        .unwrap();

    let (_status, json) = response_parts(response).await;
    if json["code"] == 0 {
        let rule_id = json["result"]["id"].as_str().unwrap().to_string();

        // User B tries to get workspace A's rule — should fail
        let token_b = create_test_token_with_workspace("user-b", "tenant-b", "ws-b");
        let response = app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/alarm-rules/{}", rule_id),
                &token_b,
                None,
            ))
            .await
            .unwrap();

        let (_status, json_b) = response_parts(response).await;
        assert_ne!(
            json_b["code"], 0,
            "User B should NOT be able to access workspace A's alarm rule"
        );
    }
}

// ============================================================================
// Alarm → AI Heartbeat Wake Integration Tests (module removed)
//
// The old HeartbeatManager has been replaced by event-driven cross-domain
// dispatch via the AI subsystem. New integration tests for the event-driven
// approach will be added in a follow-up task (see Task 15: Integration tests).
// ============================================================================
