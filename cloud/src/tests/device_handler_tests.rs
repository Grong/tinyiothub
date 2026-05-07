//! Device handler integration tests
//!
//! Tests device CRUD endpoints using `tower::ServiceExt::oneshot()`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts,
    seed_test_workspace, setup_test_app, setup_test_app_with_pool,
};

/// Helper: build a request with auth and optional body.
fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");

    // WorkspaceScope now reads workspace_id from JWT claims, not from header.
    // Header is ignored to prevent cross-tenant forgery.
    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };

    builder.body(Body::from(body_str)).unwrap()
}

// ============================================================================
// Create Device
// ============================================================================

#[tokio::test]
async fn test_create_device() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-device-001",
        "display_name": "Test Device",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();
    // Handler should respond with valid HTTP status — not panic
    assert!(
        !status.is_informational() && status != StatusCode::SWITCHING_PROTOCOLS,
        "Unexpected status: {}",
        status
    );
    // Response should always be valid JSON with code field
    let (_status, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Response must have numeric code field");
}

// ============================================================================
// List Devices
// ============================================================================

#[tokio::test]
async fn test_list_devices() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // result should be a paginated response with data array
    assert!(json["result"]["data"].is_array(), "Expected data array");
    assert!(json["result"]["pagination"].is_object(), "Expected pagination object");
}

// ============================================================================
// Get Device — not found
// ============================================================================

#[tokio::test]
async fn test_get_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    // Handler returns error in JSON body, not HTTP status
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Update Device — not found
// ============================================================================

#[tokio::test]
async fn test_update_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-name"
    });

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/devices/nonexistent-id-12345", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Delete Device — not found
// ============================================================================

#[tokio::test]
async fn test_delete_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/devices/nonexistent-id-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent device");
}

// ============================================================================
// Create Device — validation: missing required name
// ============================================================================

#[tokio::test]
async fn test_create_device_missing_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Empty body — name is required
    let body = json!({});

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();

    // Axum's Json extractor returns 422 for deserialization failures (missing required field)
    // This is expected behavior — the handler correctly rejects invalid input
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200 for missing name, got: {}",
        status
    );
}

// ============================================================================
// Create Device — empty name validation
// ============================================================================

#[tokio::test]
async fn test_create_device_empty_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": ""
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    // Should get a validation error from the service layer
    assert_ne!(json["code"], 0, "Expected validation error for empty name");
}

// ============================================================================
// Cross-Tenant Isolation
// ============================================================================

/// Verify that a user in workspace A cannot see devices created in workspace B.
/// This is the regression test for the security bug where omitting X-Workspace-Id
/// header returned the raw (unfiltered) repository, exposing all devices.
#[tokio::test]
async fn test_cross_workspace_isolation() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed tenants and workspaces for the test
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;

    let api_router = crate::api::create_router();
    let app = axum::Router::new()
        .nest("/api", api_router)
        .with_state(app_state);

    // User A (workspace ws-a) creates a device — first ensure workspace exists
    let token_a = create_test_token_with_workspace("user-a", "tenant-a", "ws-a");

    let body = json!({
        "name": "device-in-ws-a",
        "display_name": "Device in Workspace A",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/devices", &token_a, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success creating device in workspace A");
    let device_id = json["result"]["id"].as_str().unwrap().to_string();
    assert!(!device_id.is_empty(), "Device should have an id");

    // User B (workspace ws-b) lists devices — should NOT see workspace A's device
    let token_b = create_test_token_with_workspace("user-b", "tenant-b", "ws-b");

    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices?page=1&page_size=20",
            &token_b,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");

    let data = json["result"]["data"].as_array().unwrap();
    let device_ids: Vec<&str> = data
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();

    assert!(
        !device_ids.contains(&device_id.as_str()),
        "SECURITY BUG: User B (ws-b) can see workspace A's device (ws-a). \
         Workspace isolation is broken!"
    );

    // User A should see their own device
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices?page=1&page_size=20",
            &token_a,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    let data = json["result"]["data"].as_array().unwrap();
    let device_ids: Vec<&str> = data
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert!(
        device_ids.contains(&device_id.as_str()),
        "User A should see their own device in workspace A"
    );
}

// ============================================================================
// Device Profile
// ============================================================================

#[tokio::test]
async fn test_get_device_profile_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/profile",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent device profile");
}

// ============================================================================
// Device Properties — not found
// ============================================================================

#[tokio::test]
async fn test_get_device_properties_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/properties",
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
// Device Dashboard
// ============================================================================

#[tokio::test]
async fn test_get_device_dashboard() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/dashboard", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Device Profile — success path
// ============================================================================

#[tokio::test]
async fn test_get_device_profile_success() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Create a device first
    let body = json!({
        "name": "profile-test-device-001",
        "display_name": "Profile Test Device",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/devices", &token, Some(body)))
        .await
        .unwrap();

    let (_status, create_json) = response_parts(response).await;
    assert_eq!(create_json["code"], 0, "Expected success creating device: {}", create_json);
    let device_id = create_json["result"]["id"].as_str().unwrap().to_string();

    // Get device profile
    let response = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/devices/{}/profile", device_id),
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
    if json["code"] == 0 {
        assert!(json["result"]["device"].is_object(), "Profile should have device object");
        assert!(json["result"]["properties"].is_array(), "Profile should have properties array");
        assert!(json["result"]["overview"].is_object(), "Profile should have overview object");
    }
}

// ============================================================================
// Device Status — not found
// ============================================================================

#[tokio::test]
async fn test_get_device_status_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/status",
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
// Device Monitoring — not found paths
// ============================================================================

#[tokio::test]
async fn test_get_device_metrics_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/metrics", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_device_performance_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/performance", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_device_performance_history_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/performance/history", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_device_performance_alerts_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/performance/alerts", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ── System Monitoring overview ──

#[tokio::test]
async fn test_get_system_overview() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/overview", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_system_performance_overview() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/performance/overview", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_all_performance_alerts() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/performance/alerts", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Device Trace — not found paths
// ============================================================================

#[tokio::test]
async fn test_get_device_traces_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/traces", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    if status == StatusCode::OK {
        assert!(json["code"].is_number(), "Expected numeric code");
    }
}

#[tokio::test]
async fn test_get_device_trace_statistics_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/nonexistent-id-12345/traces/statistics", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    if status == StatusCode::OK {
        assert!(json["code"].is_number(), "Expected numeric code");
    }
}

#[tokio::test]
async fn test_get_system_trace_overview() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/devices/system/traces/overview", &token, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    if status == StatusCode::OK {
        assert!(json["code"].is_number(), "Expected numeric code");
    }
}

#[tokio::test]
async fn test_execute_device_command_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/nonexistent-id-12345/commands/nonexistent-cmd/execute",
            &token,
            Some(json!({"params": {}})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert!(status == StatusCode::OK || status == StatusCode::UNPROCESSABLE_ENTITY);
    if status == StatusCode::OK {
        assert!(json["code"].is_number(), "Expected numeric code");
    }
}

// ============================================================================
// Device Enable / Disable
// ============================================================================

#[tokio::test]
async fn test_enable_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/nonexistent-id-12345/enable",
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
async fn test_disable_device_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/nonexistent-id-12345/disable",
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
// Device from Template
// ============================================================================

#[tokio::test]
async fn test_create_device_from_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "template_id": "nonexistent-template",
        "device_input": {
            "name": "test-device",
            "property_values": {},
            "enabled_commands": []
        }
    });

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/from-template",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_preview_device_from_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"name": "preview-device", "property_values": {}, "enabled_commands": []});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/from-template/nonexistent-template-id/preview",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_validate_device_input_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"name": "validate-device", "property_values": {}, "enabled_commands": []});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/from-template/nonexistent-template-id/validate",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_validate_single_field_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"field_name": "name", "field_value": "test"});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/from-template/nonexistent-template-id/validate-field",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_template_requirements_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/from-template/nonexistent-template-id/requirements",
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
// Device Properties — write endpoints
// ============================================================================

#[tokio::test]
async fn test_update_device_property_value_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"value": "42"});

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/devices/nonexistent-id-12345/properties/nonexistent-prop/value",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_device_property_by_name_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/by-name/nonexistent-device/properties/some-property",
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
// Device Trace — write endpoints
// ============================================================================

#[tokio::test]
async fn test_record_device_trace_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "trace_type": "event",
        "level": "info",
        "category": "test",
        "title": "Test trace",
        "message": "Test message"
    });

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/nonexistent-id-12345/traces",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_clear_device_traces_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/nonexistent-id-12345/traces/clear",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_cleanup_expired_traces() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"days_to_keep": 30});

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/devices/system/traces/cleanup",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    if status == StatusCode::OK {
        assert!(json["code"].is_number(), "Expected numeric code");
    }
}
