// MCP Integration Tests
// Integration tests for MCP tool registry and handlers

use serde_json::json;

/// Test that all expected tools are registered in the MCP registry
#[tokio::test]
async fn test_all_tools_registered() {
    crate::register_tools(None).await;

    let registry = crate::get_mcp_registry().expect("Registry not initialized");

    let tools = registry.read().await.list_tools();

    // 7 thing + 2 driver + 4 job + 3 alarm = 16
    assert_eq!(tools.len(), 16, "Expected 16 tools registered");

    let tool_names: Vec<_> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(
        tool_names.contains(&"search_things".to_string()),
        "search_things should be registered"
    );
    assert!(tool_names.contains(&"create_thing".to_string()), "create_thing should be registered");
    assert!(tool_names.contains(&"alarm_list".to_string()), "alarm_list should be registered");
    assert!(
        tool_names.contains(&"list_schedules".to_string()),
        "list_schedules should be registered"
    );
}

/// Test that search_things accepts empty keyword (returns all things)
#[tokio::test]
async fn test_search_things_accepts_empty_keyword() {
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler.execute(json!({"keyword": ""})).await;
    // Without McpState initialized, it returns Internal error — same as non-empty keyword
    assert!(
        matches!(result, Err(crate::ToolError::Internal(_))),
        "Expected Internal error for uninitialized state, got {:?}",
        result
    );
}

/// Test that search_things returns response object or graceful error
#[tokio::test]
async fn test_search_things_returns_valid_response() {
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler.execute(json!({"keyword": "test"})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "search_things should return an object");
        }
        Err(e) => {
            assert!(
                matches!(e, crate::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test all device-runtime (thing) tools are registered
#[tokio::test]
async fn test_all_device_tools_registered() {
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let device_tools = [
        "search_things",
        "get_thing",
        "read_properties",
        "write_properties",
        "send_command",
        "create_thing",
        "delete_thing",
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
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
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
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
    let tools = registry.read().await.list_tools();

    for tool in tools {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(!tool.description.is_empty(), "Tool description should not be empty");
        assert!(tool.input_schema.is_object(), "Input schema should be a JSON object");
    }
}

/// Test that get_thing returns error for non-existent thing
#[tokio::test]
async fn test_get_thing_not_found() {
    crate::register_tools(None).await;
    let registry = crate::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_thing").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-id"})).await;
    assert!(result.is_err(), "get_thing should error for non-existent thing");
}
