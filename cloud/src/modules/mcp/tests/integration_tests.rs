// MCP Integration Tests
// Integration tests for MCP tool registry and handlers

use serde_json::json;

/// Test that all expected tools are registered in the MCP registry
#[tokio::test]
async fn test_all_tools_registered() {
    crate::modules::mcp::register_tools().await;

    let registry = crate::modules::mcp::get_mcp_registry().expect("Registry not initialized");

    let tools = registry.read().await.list_tools();

    // 7 device + 2 driver + 4 job + 3 alarm = 16
    assert_eq!(tools.len(), 16, "Expected 16 tools registered");

    let tool_names: Vec<_> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(
        tool_names.contains(&"search_devices".to_string()),
        "search_devices should be registered"
    );
    assert!(
        tool_names.contains(&"create_device".to_string()),
        "create_device should be registered"
    );
    assert!(tool_names.contains(&"alarm_list".to_string()), "alarm_list should be registered");
    assert!(
        tool_names.contains(&"list_schedules".to_string()),
        "list_schedules should be registered"
    );
}

/// Test that search_devices rejects empty keyword
#[tokio::test]
async fn test_search_devices_rejects_empty_keyword() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_devices").unwrap();

    let result = handler.execute(json!({"keyword": ""})).await;
    assert!(
        matches!(result, Err(crate::modules::mcp::ToolError::InvalidParams(_))),
        "Expected InvalidParams for empty keyword, got {:?}",
        result
    );
}

/// Test that search_devices returns response object or graceful error
#[tokio::test]
async fn test_search_devices_returns_valid_response() {
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
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test all device tools are registered
#[tokio::test]
async fn test_all_device_tools_registered() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let device_tools = [
        "search_devices",
        "get_device",
        "read_properties",
        "write_properties",
        "send_command",
        "create_device",
        "delete_device",
    ];

    for tool_name in device_tools {
        assert!(
            tool_names.contains(&tool_name.to_string()),
            "Device tool '{}' should be registered",
            tool_name
        );
    }
}

/// Test all driver tools are registered
#[tokio::test]
async fn test_all_driver_tools_registered() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let driver_tools = ["list_drivers", "test_driver"];

    for tool_name in driver_tools {
        assert!(
            tool_names.contains(&tool_name.to_string()),
            "Driver tool '{}' should be registered",
            tool_name
        );
    }
}

/// Test that tool metadata is properly formatted
#[tokio::test]
async fn test_tool_metadata_format() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tools = registry.read().await.list_tools();

    for tool in tools {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(!tool.description.is_empty(), "Tool description should not be empty");
        assert!(tool.input_schema.is_object(), "Input schema should be a JSON object");
    }
}

/// Test that get_device returns error for non-existent device
#[tokio::test]
async fn test_get_device_not_found() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-id"})).await;
    assert!(result.is_err(), "get_device should error for non-existent device");
}
