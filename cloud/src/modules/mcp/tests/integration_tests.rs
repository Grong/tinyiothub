// MCP Integration Tests
// Integration tests for MCP tool registry and handlers

use serde_json::json;

/// Test that all expected tools are registered in the MCP registry
#[tokio::test]
async fn test_all_tools_registered() {
    // Initialize registry
    crate::modules::mcp::register_tools().await;

    let registry = crate::modules::mcp::get_mcp_registry()
        .expect("Registry not initialized");

    let tools = registry.read().await.list_tools();

    // Expected count: 14 device + 7 driver + 3 heartbeat + 5 workspace + 4 job + 2 batch + 4 alarm + 3 device_enhanced + 3 self_heal + 3 knowledge = 48
    // Note: generate_driver returns NotImplemented in Phase 1
    assert_eq!(tools.len(), 48, "Expected 48 tools registered");

    // Verify critical tools exist
    let tool_names: Vec<_> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"list_devices".to_string()), "list_devices should be registered");
    assert!(tool_names.contains(&"create_device".to_string()), "create_device should be registered");
    assert!(tool_names.contains(&"report_heartbeat".to_string()), "report_heartbeat should be registered");
    assert!(tool_names.contains(&"get_self_heal_policy".to_string()), "get_self_heal_policy should be registered");
    assert!(tool_names.contains(&"query_knowledge_base".to_string()), "query_knowledge_base should be registered");
}

/// Test that generate_driver returns NotImplemented error with correct message
#[tokio::test]
async fn test_generate_driver_returns_not_implemented() {
    // Initialize
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("generate_driver").unwrap();

    let result = handler.execute(json!({})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    match err {
        crate::modules::mcp::ToolError::NotImplemented(msg) => {
            assert!(msg.contains("Phase 3"), "Error should mention Phase 3: {}", msg);
        }
        _ => panic!("Expected NotImplemented error, got {:?}", err),
    }
}

/// Test that pagination works for list_devices
#[tokio::test]
async fn test_list_devices_respects_pagination() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("list_devices").unwrap();

    // Test over-limit page_size - should be rejected with InvalidParams (schema says max: 100)
    let result = handler.execute(json!({"pageSize": 1000 })).await;
    assert!(
        matches!(result, Err(crate::modules::mcp::ToolError::InvalidParams(_))),
        "Expected InvalidParams for page_size > 100, got {:?}",
        result
    );
}

/// Test that list_devices returns array result or graceful error
#[tokio::test]
async fn test_list_devices_returns_valid_response() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("list_devices").unwrap();

    let result = handler.execute(json!({"page": 1, "page_size": 10 })).await;

    match result {
        Ok(value) => {
            // Result should be an array of devices
            assert!(value.is_array(), "list_devices should return an array");
        }
        Err(e) => {
            // AppState may not be initialized in unit test context
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test heartbeat tool reporting
#[tokio::test]
async fn test_report_heartbeat_tool_exists() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("report_heartbeat").unwrap();

    assert_eq!(handler.name(), "report_heartbeat");
    assert!(!handler.description().is_empty());
}

/// Test heartbeat status tool
#[tokio::test]
async fn test_get_heartbeat_status_tool_exists() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_heartbeat_status").unwrap();

    assert_eq!(handler.name(), "get_heartbeat_status");
    assert!(!handler.description().is_empty());
}

/// Test configure_heartbeat tool
#[tokio::test]
async fn test_configure_heartbeat_tool_exists() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("configure_heartbeat").unwrap();

    assert_eq!(handler.name(), "configure_heartbeat");
    assert!(!handler.description().is_empty());
}

/// Test knowledge base query tool exists
#[tokio::test]
async fn test_query_knowledge_base_tool_exists() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("query_knowledge_base").unwrap();

    assert_eq!(handler.name(), "query_knowledge_base");
    assert!(!handler.description().is_empty());
}

/// Test all device tools are registered
#[tokio::test]
async fn test_all_device_tools_registered() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let device_tools = [
        "list_devices",
        "search_devices",
        "get_device",
        "get_device_status",
        "read_properties",
        "write_properties",
        "send_command",
        "create_device",
        "update_device",
        "delete_device",
        "get_device_history",
        "get_device_metrics",
        "export_device_report",
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

    let driver_tools = [
        "list_drivers",
        "get_driver_config_schema",
        "match_driver",
        "generate_driver",
        "load_driver",
        "unload_driver",
        "test_driver",
    ];

    for tool_name in driver_tools {
        assert!(
            tool_names.contains(&tool_name.to_string()),
            "Driver tool '{}' should be registered",
            tool_name
        );
    }
}

/// Test all self-heal tools are registered
#[tokio::test]
async fn test_all_self_heal_tools_registered() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let self_heal_tools = [
        "get_self_heal_policy",
        "execute_self_heal_action",
        "get_recovery_history",
    ];

    for tool_name in self_heal_tools {
        assert!(
            tool_names.contains(&tool_name.to_string()),
            "Self-heal tool '{}' should be registered",
            tool_name
        );
    }
}

/// Test all knowledge tools are registered
#[tokio::test]
async fn test_all_knowledge_tools_registered() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let tool_names = registry.read().await.list_names();

    let knowledge_tools = [
        "query_knowledge_base",
        "add_knowledge_entry",
        "get_device_manual",
    ];

    for tool_name in knowledge_tools {
        assert!(
            tool_names.contains(&tool_name.to_string()),
            "Knowledge tool '{}' should be registered",
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
    // Should return an error (either NotFound or Internal if DB not initialized)
    assert!(result.is_err(), "get_device should error for non-existent device");
}

/// Test add_knowledge_entry returns NotImplemented
#[tokio::test]
async fn test_add_knowledge_entry_returns_not_implemented() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("add_knowledge_entry").unwrap();

    let result = handler.execute(json!({
        "title": "Test Entry",
        "content": "Test content",
        "category": "device"
    })).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    match err {
        crate::modules::mcp::ToolError::NotImplemented(msg) => {
            assert!(msg.contains("Phase 2"), "Error should mention Phase 2");
        }
        _ => panic!("Expected NotImplemented error, got {:?}", err),
    }
}

/// Test get_device_manual returns NotImplemented
#[tokio::test]
async fn test_get_device_manual_returns_not_implemented() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_device_manual").unwrap();

    let result = handler.execute(json!({"device_type": "modbus_rtu"})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    match err {
        crate::modules::mcp::ToolError::NotImplemented(msg) => {
            assert!(msg.contains("Phase 2"), "Error should mention Phase 2");
        }
        _ => panic!("Expected NotImplemented error, got {:?}", err),
    }
}
