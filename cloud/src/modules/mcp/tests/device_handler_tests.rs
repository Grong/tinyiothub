// Device Handler Tests
// Tests for device MCP tool handlers

use serde_json::json;

/// Test get_device handler returns error for non-existent device
#[tokio::test]
async fn test_get_device_not_found() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-device-id"})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, crate::modules::mcp::ToolError::NotFound(_) |
                          crate::modules::mcp::ToolError::Internal(_)));
}

/// Test search_devices handler returns valid response (or graceful error)
#[tokio::test]
async fn test_search_devices_returns_response() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_devices").unwrap();

    let result = handler.execute(json!({"keyword": "test"})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "search_devices should return an object");
        }
        Err(e) => {
            // If AppState is not initialized, we get Internal error
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test search_devices accepts keyword and limit parameters
#[tokio::test]
async fn test_search_devices_with_params() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_devices").unwrap();

    // Test with keyword and limit
    let result = handler.execute(json!({
        "keyword": "sensor",
        "limit": 10
    })).await;
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }

    // Test with keyword and tag
    let result = handler.execute(json!({
        "keyword": "modbus",
        "tag": "production"
    })).await;
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }

    // Empty keyword should be rejected
    let result = handler.execute(json!({"keyword": ""})).await;
    assert!(
        matches!(result, Err(crate::modules::mcp::ToolError::InvalidParams(_))),
        "Expected InvalidParams for empty keyword, got {:?}",
        result
    );
}

/// Test search_devices with tag filter
#[tokio::test]
async fn test_search_devices_with_tag() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_devices").unwrap();

    let result = handler.execute(json!({
        "keyword": "temp",
        "tag": "critical"
    })).await;

    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test get_device_status handler exists and has correct name
#[tokio::test]
async fn test_get_device_status_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device_status").unwrap();

    assert_eq!(handler.name(), "get_device_status");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test read_properties handler metadata
#[tokio::test]
async fn test_read_properties_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("read_properties").unwrap();

    assert_eq!(handler.name(), "read_properties");
    assert!(!handler.description().is_empty());
}

/// Test write_properties handler metadata
#[tokio::test]
async fn test_write_properties_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("write_properties").unwrap();

    assert_eq!(handler.name(), "write_properties");
    assert!(!handler.description().is_empty());
}

/// Test send_command handler metadata
#[tokio::test]
async fn test_send_command_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("send_command").unwrap();

    assert_eq!(handler.name(), "send_command");
    assert!(!handler.description().is_empty());
}

/// Test create_device handler metadata
#[tokio::test]
async fn test_create_device_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("create_device").unwrap();

    assert_eq!(handler.name(), "create_device");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    // create_device should have required fields
    let required = json_schema["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "name"), "name should be required for create_device");
}

/// Test update_device handler metadata
#[tokio::test]
async fn test_update_device_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("update_device").unwrap();

    assert_eq!(handler.name(), "update_device");
    assert!(!handler.description().is_empty());
}

/// Test delete_device handler metadata
#[tokio::test]
async fn test_delete_device_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("delete_device").unwrap();

    assert_eq!(handler.name(), "delete_device");
    assert!(!handler.description().is_empty());
}

/// Test get_device_history handler metadata
#[tokio::test]
async fn test_get_device_history_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device_history").unwrap();

    assert_eq!(handler.name(), "get_device_history");
    assert!(!handler.description().is_empty());
}

/// Test get_device_metrics handler metadata
#[tokio::test]
async fn test_get_device_metrics_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device_metrics").unwrap();

    assert_eq!(handler.name(), "get_device_metrics");
    assert!(!handler.description().is_empty());
}

/// Test export_device_report handler metadata
#[tokio::test]
async fn test_export_device_report_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("export_device_report").unwrap();

    assert_eq!(handler.name(), "export_device_report");
    assert!(!handler.description().is_empty());
}
