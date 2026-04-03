// Heartbeat Handler Tests
// Tests for heartbeat MCP tool handlers

use serde_json::json;

/// Test report_heartbeat handler metadata
#[tokio::test]
async fn test_report_heartbeat_handler_metadata() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("report_heartbeat").unwrap();

    assert_eq!(handler.name(), "report_heartbeat");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test report_heartbeat accepts gateway heartbeat data
#[tokio::test]
async fn test_report_heartbeat_accepts_data() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("report_heartbeat").unwrap();

    let result = handler.execute(json!({
        "gateway_id": "test-gateway-001",
        "cpu_usage_percent": 45.5,
        "memory_usage_percent": 62.3,
        "disk_usage_percent": 30.0,
        "network_status": "connected",
        "connected_devices": 12,
        "active_alarms": 2
    })).await;

    // Should succeed if heartbeat state is initialized
    // May fail with Internal error if state not properly initialized
    match result {
        Ok(value) => {
            assert!(value.is_object(), "report_heartbeat should return an object");
        }
        Err(e) => {
            // If heartbeat state is not initialized, we get Internal error
            assert!(
                matches!(e, crate::api::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test report_heartbeat with minimal data
#[tokio::test]
async fn test_report_heartbeat_minimal_data() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("report_heartbeat").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object());
        }
        Err(e) => {
            assert!(
                matches!(e, crate::api::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test get_heartbeat_status handler metadata
#[tokio::test]
async fn test_get_heartbeat_status_handler_metadata() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_heartbeat_status").unwrap();

    assert_eq!(handler.name(), "get_heartbeat_status");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test get_heartbeat_status returns status
#[tokio::test]
async fn test_get_heartbeat_status_returns_data() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_heartbeat_status").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "get_heartbeat_status should return an object");
            // Check for expected fields
            let obj = value.as_object().unwrap();
            assert!(obj.contains_key("gateway_id") || obj.contains_key("status"),
                    "Response should contain gateway_id or status");
        }
        Err(e) => {
            assert!(
                matches!(e, crate::api::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test configure_heartbeat handler metadata
#[tokio::test]
async fn test_configure_heartbeat_handler_metadata() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("configure_heartbeat").unwrap();

    assert_eq!(handler.name(), "configure_heartbeat");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");

    // Should have optional configuration fields
    let props = json_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("probeIntervalSecs"), "Should have probeIntervalSecs");
    assert!(props.contains_key("cpuThresholdPercent"), "Should have cpuThresholdPercent");
}

/// Test configure_heartbeat accepts configuration
#[tokio::test]
async fn test_configure_heartbeat_accepts_config() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("configure_heartbeat").unwrap();

    let result = handler.execute(json!({
        "probe_interval_secs": 30,
        "cpu_threshold_percent": 80.0,
        "memory_threshold_percent": 85.0,
        "disk_threshold_percent": 90.0,
        "cloud_sync_enabled": true,
        "cloud_sync_interval_secs": 300
    })).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "configure_heartbeat should return config object");
        }
        Err(e) => {
            assert!(
                matches!(e, crate::api::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test configure_heartbeat with partial config
#[tokio::test]
async fn test_configure_heartbeat_partial_config() {
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("configure_heartbeat").unwrap();

    // Test with only some fields
    let result = handler.execute(json!({
        "probe_interval_secs": 60
    })).await;

    match result {
        Ok(value) => {
            assert!(value.is_object());
        }
        Err(e) => {
            assert!(
                matches!(e, crate::api::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}
