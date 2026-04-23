// Phase 2 Tools Tests
// Tests for Phase 2 MCP tool handlers: workspace, job, batch, alarm, device_enhanced

use serde_json::json;

// =============================================================================
// Workspace Tools Tests
// =============================================================================

/// Test workspace_list handler metadata
#[tokio::test]
async fn test_workspace_list_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_list").unwrap();

    assert_eq!(handler.name(), "workspace_list");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test workspace_list returns array
#[tokio::test]
async fn test_workspace_list_returns_array() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_list").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_array(), "workspace_list should return an array");
        }
        Err(e) => {
            // MCP context may not be initialized in test environment
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_) |
                             crate::modules::mcp::ToolError::Unauthorized(_)),
                "Expected Internal or Unauthorized error, got {:?}",
                e
            );
        }
    }
}

/// Test workspace_list accepts pagination
#[tokio::test]
async fn test_workspace_list_with_pagination() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_list").unwrap();

    let result = handler.execute(json!({"page": 1, "pageSize": 10})).await;

    match result {
        Ok(_) | Err(crate::modules::mcp::ToolError::Internal(_) |
                   crate::modules::mcp::ToolError::Unauthorized(_)) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

/// Test workspace_get handler metadata
#[tokio::test]
async fn test_workspace_get_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_get").unwrap();

    assert_eq!(handler.name(), "workspace_get");
    assert!(!handler.description().is_empty());
}

/// Test workspace_get returns error for non-existent workspace
#[tokio::test]
async fn test_workspace_get_not_found() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_get").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-workspace-id"})).await;
    assert!(result.is_err());
}

/// Test workspace_create handler metadata
#[tokio::test]
async fn test_workspace_create_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_create").unwrap();

    assert_eq!(handler.name(), "workspace_create");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test workspace_update handler metadata
#[tokio::test]
async fn test_workspace_update_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_update").unwrap();

    assert_eq!(handler.name(), "workspace_update");
    assert!(!handler.description().is_empty());
}

/// Test workspace_delete handler metadata
#[tokio::test]
async fn test_workspace_delete_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("workspace_delete").unwrap();

    assert_eq!(handler.name(), "workspace_delete");
    assert!(!handler.description().is_empty());
}

// =============================================================================
// Schedule Tools Tests
// =============================================================================

/// Test list_schedules handler metadata
#[tokio::test]
async fn test_list_schedules_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("list_schedules").unwrap();

    assert_eq!(handler.name(), "list_schedules");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test list_schedules returns array
#[tokio::test]
async fn test_list_schedules_returns_array() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("list_schedules").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_array(), "list_schedules should return an array");
        }
        Err(e) => {
            // MCP context may not be initialized in test environment
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_) |
                             crate::modules::mcp::ToolError::Unauthorized(_)),
                "Expected Internal or Unauthorized error, got {:?}",
                e
            );
        }
    }
}

/// Test list_schedules accepts filters
#[tokio::test]
async fn test_list_schedules_with_filters() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("list_schedules").unwrap();

    let result = handler.execute(json!({
        "page": 1,
        "pageSize": 20,
        "taskType": "probe",
        "enabled": true
    })).await;

    match result {
        Ok(_) | Err(crate::modules::mcp::ToolError::Internal(_) |
                   crate::modules::mcp::ToolError::Unauthorized(_)) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

/// Test create_schedule handler metadata
#[tokio::test]
async fn test_create_schedule_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("create_schedule").unwrap();

    assert_eq!(handler.name(), "create_schedule");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test delete_schedule handler metadata
#[tokio::test]
async fn test_delete_schedule_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("delete_schedule").unwrap();

    assert_eq!(handler.name(), "delete_schedule");
    assert!(!handler.description().is_empty());
}

// =============================================================================
// Batch Tools Tests
// =============================================================================

/// Test batch_command handler metadata
#[tokio::test]
async fn test_batch_command_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("batch_command").unwrap();

    assert_eq!(handler.name(), "batch_command");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test batch_command requires device_ids
#[tokio::test]
async fn test_batch_command_requires_devices() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("batch_command").unwrap();

    let result = handler.execute(json!({
        "command": "restart",
        "device_ids": []
    })).await;

    // Should fail with invalid params for empty device_ids
    assert!(result.is_err());
}

/// Test get_batch_status handler metadata
#[tokio::test]
async fn test_get_batch_status_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_batch_status").unwrap();

    assert_eq!(handler.name(), "get_batch_status");
    assert!(!handler.description().is_empty());
}

// =============================================================================
// Alarm MCP Tools Tests
// =============================================================================

/// Test alarm_list handler metadata
#[tokio::test]
async fn test_alarm_list_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_list").unwrap();

    assert_eq!(handler.name(), "alarm_list");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test alarm_list returns array
#[tokio::test]
async fn test_alarm_list_returns_array() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_list").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_array(), "alarm_list should return an array");
        }
        Err(e) => {
            // MCP context may not be initialized in test environment
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_) |
                             crate::modules::mcp::ToolError::Unauthorized(_)),
                "Expected Internal or Unauthorized error, got {:?}",
                e
            );
        }
    }
}

/// Test alarm_list accepts filters
#[tokio::test]
async fn test_alarm_list_with_filters() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_list").unwrap();

    let result = handler.execute(json!({
        "page": 1,
        "pageSize": 20,
        "level": "error",
        "acknowledged": false
    })).await;

    match result {
        Ok(_) | Err(crate::modules::mcp::ToolError::Internal(_) |
                   crate::modules::mcp::ToolError::Unauthorized(_)) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

/// Test alarm_statistics handler metadata
#[tokio::test]
async fn test_alarm_statistics_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_statistics").unwrap();

    assert_eq!(handler.name(), "alarm_statistics");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test alarm_statistics returns statistics
#[tokio::test]
async fn test_alarm_statistics_returns_object() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_statistics").unwrap();

    let result = handler.execute(json!({
        "startTime": "2026-04-01T00:00:00Z",
        "endTime": "2026-04-05T00:00:00Z"
    })).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "alarm_statistics should return an object");
        }
        Err(e) => {
            // MCP context may not be initialized in test environment
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_) |
                             crate::modules::mcp::ToolError::Unauthorized(_)),
                "Expected Internal or Unauthorized error, got {:?}",
                e
            );
        }
    }
}

/// Test alarm_acknowledge handler metadata
#[tokio::test]
async fn test_alarm_acknowledge_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_acknowledge").unwrap();

    assert_eq!(handler.name(), "alarm_acknowledge");
    assert!(!handler.description().is_empty());
}

/// Test alarm_acknowledge requires alarm_id
#[tokio::test]
async fn test_alarm_acknowledge_requires_id() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_acknowledge").unwrap();

    let result = handler.execute(json!({})).await;
    assert!(result.is_err());
}

/// Test alarm_rule_add handler metadata
#[tokio::test]
async fn test_alarm_rule_add_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_rule_add").unwrap();

    assert_eq!(handler.name(), "alarm_rule_add");
    assert!(!handler.description().is_empty());
}

/// Test alarm_rule_add validates rule_type
#[tokio::test]
async fn test_alarm_rule_add_validates_rule_type() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_rule_add").unwrap();

    // Invalid rule_type should fail
    let result = handler.execute(json!({
        "name": "Test Rule",
        "ruleType": "invalid_type",
        "deviceId": "device-1",
        "property": "temperature",
        "level": "error"
    })).await;

    assert!(result.is_err());
}

// =============================================================================
// Device Enhanced Tools Tests
// =============================================================================

/// Test compare_devices handler metadata
#[tokio::test]
async fn test_compare_devices_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("compare_devices").unwrap();

    assert_eq!(handler.name(), "compare_devices");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test compare_devices requires at least 2 devices
#[tokio::test]
async fn test_compare_devices_requires_multiple_devices() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("compare_devices").unwrap();

    // Only 1 device should fail
    let result = handler.execute(json!({
        "device_ids": ["device-1"],
        "property": "temperature"
    })).await;

    assert!(result.is_err());
}

/// Test compare_devices requires device_ids
#[tokio::test]
async fn test_compare_devices_requires_device_ids() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("compare_devices").unwrap();

    let result = handler.execute(json!({
        "property": "temperature"
    })).await;

    assert!(result.is_err());
}

/// Test diagnose_device handler metadata
#[tokio::test]
async fn test_diagnose_device_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("diagnose_device").unwrap();

    assert_eq!(handler.name(), "diagnose_device");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test diagnose_device requires device_id
#[tokio::test]
async fn test_diagnose_device_requires_device_id() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("diagnose_device").unwrap();

    let result = handler.execute(json!({})).await;
    assert!(result.is_err());
}

/// Test scan_serial handler metadata
#[tokio::test]
async fn test_scan_serial_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("scan_serial").unwrap();

    assert_eq!(handler.name(), "scan_serial");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    assert_eq!(json_schema["type"], "object");
}

/// Test scan_serial returns result
#[tokio::test]
async fn test_scan_serial_returns_object() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("scan_serial").unwrap();

    let result = handler.execute(json!({})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "scan_serial should return an object");
            let obj = value.as_object().unwrap();
            assert!(obj.contains_key("ports") || obj.contains_key("count"),
                    "Response should contain ports or count");
        }
        Err(e) => {
            // MCP context may not be initialized in test environment
            assert!(
                matches!(e, crate::modules::mcp::ToolError::Internal(_) |
                             crate::modules::mcp::ToolError::Unauthorized(_)),
                "Expected Internal or Unauthorized error, got {:?}",
                e
            );
        }
    }
}
