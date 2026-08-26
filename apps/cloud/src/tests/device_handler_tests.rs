//! Device handler integration tests
//!
//! Tests device CRUD endpoints using `tower::ServiceExt::oneshot()`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts, seed_test_workspace,
    setup_test_app, setup_test_app_with_pool,
};

/// Helper: build a request with auth and optional body.
fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
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
        "category": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body)))
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

/// Helper: create a test app with a seeded workspace (required for /api/v1/things).
async fn setup_with_workspace(tenant_id: &str, workspace_id: &str) -> axum::Router {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, tenant_id, workspace_id).await;
    let api_router = crate::api::create_router(&app_state);
    axum::Router::new().nest("/api", api_router).with_state(app_state)
}

// ============================================================================
// List Things (device management endpoints were removed; /api/v1/things is
// the replacement — see modules/thing/handler)
// ============================================================================

#[tokio::test]
async fn test_list_things() {
    let app = setup_with_workspace("tenant-1", "ws-list").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-list");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things?limit=20&offset=0", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    // result should be a paginated response with items array
    assert!(json["result"]["items"].is_array(), "Expected items array");
    assert!(json["result"]["total"].is_number(), "Expected total count");
}

// ============================================================================
// Get Thing — not found
// ============================================================================

#[tokio::test]
async fn test_get_thing_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/things/nonexistent-id-12345", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent thing");
}

// ============================================================================
// Update Thing — not found
// ============================================================================

#[tokio::test]
async fn test_update_thing_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-name"
    });

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/things/nonexistent-id-12345",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent thing");
}

// ============================================================================
// Delete Thing — not found
// ============================================================================

#[tokio::test]
async fn test_delete_thing_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/things/nonexistent-id-12345",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent thing");
}

// ============================================================================
// Create Thing — validation: missing required name
// ============================================================================

#[tokio::test]
async fn test_create_thing_missing_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Empty body — name is required
    let body = json!({});

    let response = app
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();

    // Axum's Json extractor returns 422 for deserialization failures (missing required field)
    // This is expected behavior — the handler correctly rejects invalid input
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 422 for missing name"
    );
}

// NOTE: empty-name rejection is covered by
// thing_handler_tests.rs::test_create_thing_empty_name_rejected

// ============================================================================
// Cross-Tenant Isolation
// ============================================================================

/// Verify that a user in workspace A cannot see, read, update, or delete
/// things created in workspace B.
/// This is the regression test for the security bug where omitting X-Workspace-Id
/// header returned the raw (unfiltered) repository, exposing all things.
#[tokio::test]
async fn test_cross_workspace_isolation() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed tenants and workspaces for the test
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;

    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // User A (workspace ws-a) creates a thing
    let token_a = create_test_token_with_workspace("user-a", "tenant-a", "ws-a");

    let body = json!({
        "name": "thing-in-ws-a",
        "thingType": "device",
        "deviceType": "sensor",
        "protocolType": "modbus",
        "workspaceId": "ws-a"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token_a, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "Expected 201, got {}: {:?}", status, json);
    assert_eq!(json["code"], 0, "Expected success creating thing in workspace A");
    let thing_id = json["result"]["id"].as_str().unwrap().to_string();
    assert!(!thing_id.is_empty(), "Thing should have an id");

    // User B (workspace ws-b) lists things — should NOT see workspace A's thing
    let token_b = create_test_token_with_workspace("user-b", "tenant-b", "ws-b");

    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/things?limit=20&offset=0", &token_b, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");

    let items = json["result"]["items"].as_array().unwrap();
    let thing_ids: Vec<&str> = items.iter().filter_map(|d| d["id"].as_str()).collect();

    assert!(
        !thing_ids.contains(&thing_id.as_str()),
        "SECURITY BUG: User B (ws-b) can see workspace A's thing (ws-a). \
         Workspace isolation is broken!"
    );

    // User B reads workspace A's thing by id — must be denied (404)
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{}", thing_id),
            &token_b,
            None,
        ))
        .await
        .unwrap();
    let (status, _json) = response_parts(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "SECURITY BUG: User B (ws-b) can read workspace A's thing (ws-a)"
    );

    // User B updates workspace A's thing — must be denied (404)
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{}", thing_id),
            &token_b,
            Some(json!({"name": "hijacked-name"})),
        ))
        .await
        .unwrap();
    let (status, _json) = response_parts(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "SECURITY BUG: User B (ws-b) can update workspace A's thing (ws-a)"
    );

    // User B deletes workspace A's thing — must be denied (404)
    let response = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/things/{}", thing_id),
            &token_b,
            None,
        ))
        .await
        .unwrap();
    let (status, _json) = response_parts(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "SECURITY BUG: User B (ws-b) can delete workspace A's thing (ws-a)"
    );

    // User A should see their own thing
    let response = app
        .oneshot(auth_request("GET", "/api/v1/things?limit=20&offset=0", &token_a, None))
        .await
        .unwrap();

    let (_status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    let items = json["result"]["items"].as_array().unwrap();
    let thing_ids: Vec<&str> = items.iter().filter_map(|d| d["id"].as_str()).collect();
    assert!(
        thing_ids.contains(&thing_id.as_str()),
        "User A should see their own thing in workspace A"
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
// Thing Profile — success path
// ============================================================================

#[tokio::test]
async fn test_get_thing_profile_success() {
    let app = setup_with_workspace("tenant-1", "ws-profile").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-profile");

    // Create a thing first
    let body = json!({
        "name": "profile-test-thing-001",
        "thingType": "device",
        "deviceType": "sensor",
        "protocolType": "modbus",
        "workspaceId": "ws-profile"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body)))
        .await
        .unwrap();

    let (_status, create_json) = response_parts(response).await;
    assert_eq!(
        create_json["code"], 0,
        "Expected success creating thing: {}",
        create_json
    );
    let thing_id = create_json["result"]["id"].as_str().unwrap().to_string();

    // Get thing profile
    let response = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{}/profile", thing_id),
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code: {}", json);
    // ThingResponse is flattened into the profile
    assert_eq!(json["result"]["id"], thing_id, "Profile should contain the thing");
    assert!(
        json["result"]["properties"].is_array(),
        "Profile should have properties array"
    );
    assert!(
        json["result"]["actions"].is_array(),
        "Profile should have actions array"
    );
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
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/metrics",
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
async fn test_get_device_performance_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/performance",
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
async fn test_get_device_performance_history_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/performance/history",
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
async fn test_get_device_performance_alerts_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/performance/alerts",
            &token,
            None,
        ))
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
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/performance/overview",
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
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/traces",
            &token,
            None,
        ))
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
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/nonexistent-id-12345/traces/statistics",
            &token,
            None,
        ))
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
        .oneshot(auth_request(
            "GET",
            "/api/v1/devices/system/traces/overview",
            &token,
            None,
        ))
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
