// Phase 2 Tools Tests
// Tests for job schedule and alarm MCP tool handlers

use serde_json::json;

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
            assert!(
                matches!(
                    e,
                    crate::modules::mcp::ToolError::Internal(_)
                        | crate::modules::mcp::ToolError::Unauthorized(_)
                ),
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

    let result = handler
        .execute(json!({
            "page": 1,
            "pageSize": 20,
            "taskType": "probe",
            "enabled": true
        }))
        .await;

    match result {
        Ok(_)
        | Err(
            crate::modules::mcp::ToolError::Internal(_)
            | crate::modules::mcp::ToolError::Unauthorized(_),
        ) => {}
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

/// Test alarm_list accepts filters
#[tokio::test]
async fn test_alarm_list_with_filters() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("alarm_list").unwrap();

    let result = handler
        .execute(json!({
            "page": 1,
            "pageSize": 20,
            "level": "error",
            "acknowledged": false
        }))
        .await;

    match result {
        Ok(_)
        | Err(
            crate::modules::mcp::ToolError::Internal(_)
            | crate::modules::mcp::ToolError::Unauthorized(_),
        ) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
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

    let result = handler
        .execute(json!({
            "name": "Test Rule",
            "ruleType": "invalid_type",
            "deviceId": "device-1",
            "property": "temperature",
            "level": "error"
        }))
        .await;

    assert!(result.is_err());
}
