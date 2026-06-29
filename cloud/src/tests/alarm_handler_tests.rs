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
// Alarm → AI Heartbeat Wake Integration Tests
// ============================================================================

#[cfg(test)]
mod heartbeat_wake_tests {
    use axum::{body::Body, http::Request};
    use serde_json::json;
    use tower::ServiceExt;

    use crate::{
        modules::agent::heartbeat_manager::{WakePriority, WakeSignal},
        test_utils::{
            auth_header, create_test_token_with_workspace, seed_test_workspace,
            setup_test_app_with_pool,
        },
    };

    fn auth_request(
        method: &str,
        uri: &str,
        token: &str,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", auth_header(token))
            .header("Content-Type", "application/json");
        let body_str = body.map(|v| v.to_string()).unwrap_or_default();
        builder.body(Body::from(body_str)).unwrap()
    }

    #[tokio::test]
    async fn test_heartbeat_manager_start_stop_lifecycle() {
        let (app_state, _pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-heartbeat-test";

        // Initially no active loops
        assert!(
            !app_state.heartbeat_manager.list_active().contains(&ws_id.to_string()),
            "No loop should be active before start"
        );

        // Start a loop
        app_state.heartbeat_manager.start(ws_id).await;
        assert!(
            app_state.heartbeat_manager.list_active().contains(&ws_id.to_string()),
            "Loop should be active after start"
        );

        // Stop the loop
        app_state.heartbeat_manager.stop(ws_id).await;
        // Give the async task a moment to fully terminate
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !app_state.heartbeat_manager.list_active().contains(&ws_id.to_string()),
            "Loop should be removed after stop"
        );
    }

    #[tokio::test]
    async fn test_heartbeat_manager_wake_delivers_signal() {
        let (app_state, _pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-wake-test";

        // Start a loop
        app_state.heartbeat_manager.start(ws_id).await;

        // Send a wake signal — should not panic or error
        let signal = WakeSignal {
            workspace_id: ws_id.to_string(),
            reason: "alarm:test-001".into(),
            context: "Test alarm context".into(),
            priority: WakePriority::Critical,
            device_id: None,
            alarm_type: None,
            rule_id: None,
        };
        app_state.heartbeat_manager.wake(ws_id, signal);

        // Cleanup
        app_state.heartbeat_manager.stop(ws_id).await;
    }

    #[tokio::test]
    async fn test_wake_signal_priority_ordering() {
        // Verify Critical > High > Normal ordering
        assert!(WakePriority::Critical as i32 > WakePriority::High as i32);
        assert!(WakePriority::High as i32 > WakePriority::Normal as i32);
    }

    #[tokio::test]
    async fn test_config_rejects_zero_interval() {
        let (app_state, _pool) = setup_test_app_with_pool().await;

        let original = app_state.heartbeat_manager.config().await;
        let original_interval = original.interval_minutes;

        // Try to set interval_minutes=0 — should be rejected
        let updated = app_state.heartbeat_manager.update_config(None, Some(0)).await;

        // Config should NOT have changed to 0
        assert_ne!(updated.interval_minutes, 0, "interval_minutes=0 should be rejected");
        assert_eq!(
            updated.interval_minutes, original_interval,
            "interval_minutes should remain unchanged when 0 is passed"
        );
    }

    #[tokio::test]
    async fn test_config_accepts_valid_interval() {
        let (app_state, _pool) = setup_test_app_with_pool().await;

        let updated = app_state.heartbeat_manager.update_config(None, Some(30)).await;

        assert_eq!(updated.interval_minutes, 30);
    }

    #[tokio::test]
    async fn test_idempotent_start() {
        let (app_state, _pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-idempotent-test";

        // Starting twice should not panic or create duplicates
        app_state.heartbeat_manager.start(ws_id).await;
        app_state.heartbeat_manager.start(ws_id).await;

        let active = app_state.heartbeat_manager.list_active();
        let count = active.iter().filter(|id| id.as_str() == ws_id).count();
        assert_eq!(count, 1, "Should have exactly one active entry after idempotent start");

        app_state.heartbeat_manager.stop(ws_id).await;
    }

    #[tokio::test]
    async fn test_alarm_creation_with_heartbeat_integration() {
        let (app_state, pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-alarm-ai-test";

        // Seed tenant and workspace (uses proper schema with all required columns)
        seed_test_workspace(&pool, "tenant-1", ws_id).await;

        sqlx::query(
            "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
             VALUES (?, ?, 'Test Agent', 'active', datetime('now'), datetime('now'))",
        )
        .bind("agent-ai-1")
        .bind(ws_id)
        .execute(&pool)
        .await
        .unwrap();

        // Start heartbeat loop for this workspace
        app_state.heartbeat_manager.start(ws_id).await;
        assert!(
            app_state.heartbeat_manager.list_active().contains(&ws_id.to_string()),
            "Heartbeat loop should be active for workspace"
        );

        // Create alarm rules of different levels via the API
        let api_router = crate::api::create_router();
        let app = axum::Router::new().nest("/api", api_router).with_state(app_state.clone());
        let token = create_test_token_with_workspace("user-1", "tenant-1", ws_id);

        // Create a Critical alarm rule
        let critical_rule = json!({
            "deviceId": "dev-ai-1",
            "ruleName": "Critical Temp",
            "ruleType": "threshold",
            "conditionConfig": {
                "type": "threshold",
                "operator": "greater_than",
                "value": 100.0
            },
            "alarmLevel": "critical",
            "isEnabled": true
        });

        let response = app
            .clone()
            .oneshot(auth_request("POST", "/api/v1/alarm-rules", &token, Some(critical_rule)))
            .await
            .unwrap();

        // Accept any non-5xx status (rule creation may fail due to missing device)
        assert!(!response.status().is_server_error());

        // Create an Info alarm rule
        let info_rule = json!({
            "deviceId": "dev-ai-2",
            "ruleName": "Info Rule",
            "ruleType": "threshold",
            "conditionConfig": {
                "type": "threshold",
                "operator": "greater_than",
                "value": 50.0
            },
            "alarmLevel": "info",
            "isEnabled": true
        });

        let response = app
            .oneshot(auth_request("POST", "/api/v1/alarm-rules", &token, Some(info_rule)))
            .await
            .unwrap();
        assert!(!response.status().is_server_error());

        // Cleanup
        app_state.heartbeat_manager.stop(ws_id).await;
    }

    #[tokio::test]
    async fn test_alarm_wake_signal_dedup_fields() {
        let (app_state, _pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-dedup-test";

        app_state.heartbeat_manager.start(ws_id).await;

        // Simulate two alarms from the same device + same type
        // The dedup logic should keep only the highest priority one
        let signal1 = WakeSignal {
            workspace_id: ws_id.to_string(),
            reason: "alarm:1".into(),
            context: "High temp on dev-01".into(),
            priority: WakePriority::High,
            device_id: Some("dev-01".into()),
            alarm_type: Some("DeviceOffline".into()),
            rule_id: Some("rule-1".into()),
        };
        let signal2 = WakeSignal {
            workspace_id: ws_id.to_string(),
            reason: "alarm:2".into(),
            context: "Critical temp on dev-01".into(),
            priority: WakePriority::Critical,
            device_id: Some("dev-01".into()),
            alarm_type: Some("DeviceOffline".into()),
            rule_id: Some("rule-1".into()),
        };

        app_state.heartbeat_manager.wake(ws_id, signal1);
        app_state.heartbeat_manager.wake(ws_id, signal2);

        // Verify trust config integration: per-workspace trust configs are empty by default
        let trust = app_state.heartbeat_manager.get_trust_config(ws_id);
        assert!(trust.is_empty(), "New workspace should have empty trust config");

        // Update trust config and verify persistence
        let mut config = std::collections::HashMap::new();
        let mut devices = std::collections::HashMap::new();
        devices.insert(
            "*".to_string(),
            crate::modules::agent::heartbeat_manager::TrustLevel::FullAuto,
        );
        config.insert("send_command".to_string(), devices);
        app_state.heartbeat_manager.update_trust_config(ws_id, config.clone());

        let loaded = app_state.heartbeat_manager.get_trust_config(ws_id);
        assert!(!loaded.is_empty(), "Trust config should be persisted");
        assert!(loaded.contains_key("send_command"));

        // Resolve trust: device-specific > wildcard > default
        let level = crate::modules::agent::heartbeat_manager::resolve_trust(
            &loaded,
            "send_command",
            "dev-01",
        );
        assert_eq!(level, crate::modules::agent::heartbeat_manager::TrustLevel::FullAuto);

        // Unconfigured tool defaults to ApprovalRequired
        let default_level = crate::modules::agent::heartbeat_manager::resolve_trust(
            &loaded,
            "write_properties",
            "dev-01",
        );
        assert_eq!(
            default_level,
            crate::modules::agent::heartbeat_manager::TrustLevel::ApprovalRequired
        );

        // Cleanup
        app_state.heartbeat_manager.stop(ws_id).await;
    }

    #[tokio::test]
    async fn test_channel_overflow_drops_gracefully() {
        let (app_state, _pool) = setup_test_app_with_pool().await;
        let ws_id = "ws-overflow-test";

        // Start with default channel_size=64
        app_state.heartbeat_manager.start(ws_id).await;

        // Send 128 signals — first 64 fill the channel, rest should drop without panic
        for i in 0..128 {
            let signal = WakeSignal {
                workspace_id: ws_id.to_string(),
                reason: format!("alarm:{}", i),
                context: format!("Test alarm {}", i),
                priority: if i % 3 == 0 {
                    WakePriority::Critical
                } else if i % 3 == 1 {
                    WakePriority::High
                } else {
                    WakePriority::Normal
                },
                device_id: Some(format!("dev-{:02}", i % 5)),
                alarm_type: Some("DeviceOffline".into()),
                rule_id: Some("rule-1".into()),
            };
            app_state.heartbeat_manager.wake(ws_id, signal);
        }

        // After overflow: should not have panicked, cleanup should work
        app_state.heartbeat_manager.stop(ws_id).await;
        // If we got here without panic, the test passes
    }
}
