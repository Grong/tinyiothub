// Device Handler Tests
// Tests for device MCP tool handlers

use serde_json::json;

/// Test get_thing handler returns error for non-existent thing
#[tokio::test]
async fn test_get_thing_not_found() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_thing").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-device-id"})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(
        err,
        crate::modules::mcp::ToolError::NotFound(_) | crate::modules::mcp::ToolError::Internal(_)
    ));
}

/// Test search_things handler returns valid response (or graceful error)
#[tokio::test]
async fn test_search_things_returns_response() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler.execute(json!({"keyword": "test"})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "search_things should return an object");
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

/// Test search_things accepts keyword and limit parameters
#[tokio::test]
async fn test_search_things_with_params() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler
        .execute(json!({
            "keyword": "sensor",
            "limit": 10
        }))
        .await;
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

    let result = handler
        .execute(json!({
            "keyword": "modbus",
            "tag": "production"
        }))
        .await;
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

    // Empty keyword is now allowed — returns all devices (same behavior as non-empty)
    let result = handler.execute(json!({"keyword": ""})).await;
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

/// Test search_things with tag filter
#[tokio::test]
async fn test_search_things_with_tag() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler
        .execute(json!({
            "keyword": "temp",
            "tag": "critical"
        }))
        .await;

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

/// Test create_thing handler metadata
#[tokio::test]
async fn test_create_thing_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("create_thing").unwrap();

    assert_eq!(handler.name(), "create_thing");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    let required = json_schema["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "name"), "name should be required for create_thing");
}

/// Test delete_thing handler metadata
#[tokio::test]
async fn test_delete_thing_handler_metadata() {
    crate::modules::mcp::register_tools().await;
    let registry = crate::modules::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("delete_thing").unwrap();

    assert_eq!(handler.name(), "delete_thing");
    assert!(!handler.description().is_empty());
}
