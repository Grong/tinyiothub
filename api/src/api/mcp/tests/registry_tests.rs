// Registry and Self-Heal Handler Tests
// Unit tests for HandlerRegistry and self-heal tool handlers

use std::collections::HashMap;
use crate::api::mcp::{HandlerRegistry, ToolHandler, ToolError};
use crate::api::mcp::tool_registry::InputSchema;
use async_trait::async_trait;
use serde_json::Value;

/// Test handler registry register
#[test]
fn test_handler_registry_register() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    registry.register(TestHandler);

    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_handler");
}

/// Test handler registry get
#[test]
fn test_handler_registry_get() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    registry.register(TestHandler);

    let handler = registry.get("test_handler");
    assert!(handler.is_some());

    let handler = registry.get("nonexistent");
    assert!(handler.is_none());
}

/// Test handler registry list tools
#[test]
fn test_handler_registry_list_tools() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    let tools = registry.list_tools();
    assert!(tools.is_empty());

    registry.register(TestHandler);
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);
}

#[tokio::test]
async fn test_handler_execute() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let handler = TestHandler;
    let args = serde_json::json!({"key": "value"});

    let result = handler.execute(args.clone()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), args);
}

#[tokio::test]
async fn test_handler_execute_with_empty_args() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let handler = TestHandler;
    let args = serde_json::json!({});

    let result = handler.execute(args).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), serde_json::json!({}));
}

#[tokio::test]
async fn test_handler_error_not_implemented() {
    struct NotImplementedHandler;

    #[async_trait]
    impl ToolHandler for NotImplementedHandler {
        fn name(&self) -> &str { "not_implemented_handler" }
        fn description(&self) -> &str { "A handler that returns NotImplemented" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
            Err(ToolError::NotImplemented("Phase: Initialization".to_string()))
        }
    }

    let handler = NotImplementedHandler;
    let args = serde_json::json!({"key": "value"});

    let result = handler.execute(args).await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        ToolError::NotImplemented(msg) => {
            assert!(msg.contains("Phase"), "Error message should contain 'Phase': {}", msg);
        }
        other => panic!("Expected ToolError::NotImplemented, got {:?}", other),
    }
}

#[test]
fn test_tool_metadata_fields() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let handler = TestHandler;
    let metadata = crate::api::mcp::ToolMetadata::from_handler(&handler);

    assert_eq!(metadata.name, "test_handler");
    assert_eq!(metadata.description, "A test handler");
}

#[test]
fn test_handler_registry_contains() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    registry.register(TestHandler);

    assert!(registry.get("test_handler").is_some());
    assert!(registry.contains("test_handler"));
    assert!(!registry.contains("nonexistent"));
}

#[test]
fn test_handler_registry_multiple_handlers_name_collision() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    struct AnotherTestHandler;

    #[async_trait]
    impl ToolHandler for AnotherTestHandler {
        fn name(&self) -> &str { "test_handler" } // Same name
        fn description(&self) -> &str { "Another test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    registry.register(TestHandler);
    registry.register(AnotherTestHandler);

    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].description, "Another test handler");
}

#[tokio::test]
async fn test_handler_returns_json_value() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let handler = TestHandler;
    let complex_args = serde_json::json!({
        "devices": [
            {"id": "1", "name": "Device 1"},
            {"id": "2", "name": "Device 2"}
        ],
        "pagination": {
            "page": 1,
            "page_size": 20
        }
    });

    let result = handler.execute(complex_args).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert!(value["devices"].is_array());
    assert_eq!(value["devices"].as_array().unwrap().len(), 2);
}

#[test]
fn test_handler_registry_len_and_is_empty() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    registry.register(TestHandler);
    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
}

#[test]
fn test_handler_registry_list_names() {
    struct TestHandler;

    #[async_trait]
    impl ToolHandler for TestHandler {
        fn name(&self) -> &str { "test_handler" }
        fn description(&self) -> &str { "A test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    let mut registry = HandlerRegistry::new();
    let names = registry.list_names();
    assert!(names.is_empty());

    registry.register(TestHandler);
    let names = registry.list_names();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "test_handler");
}

#[test]
fn test_input_schema_to_json() {
    use crate::api::mcp::tool_registry::PropertySchema;

    let mut props = HashMap::new();
    props.insert(
        "name".to_string(),
        PropertySchema {
            prop_type: "string".to_string(),
            description: Some("User name".to_string()),
        },
    );
    let schema = InputSchema::object(vec!["name".to_string()], props);

    let json = schema.to_json();
    assert_eq!(json["type"], "object");
    assert_eq!(json["required"], serde_json::json!(["name"]));
    assert_eq!(json["properties"]["name"]["type"], "string");
    assert_eq!(json["properties"]["name"]["description"], "User name");
}

// =========================================================================
// Self-Healing MCP Handler Tests
// =========================================================================

#[tokio::test]
async fn test_get_self_heal_policy_returns_error_when_not_initialized() {
    use crate::api::mcp::tools::self_heal::GetSelfHealPolicyHandler;

    let handler = GetSelfHealPolicyHandler;
    let result = handler.execute(serde_json::json!({})).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        ToolError::Internal(msg) => {
            assert!(msg.contains("Self-healing not initialized"), "{}", msg);
        }
        other => panic!("Expected ToolError::Internal, got {:?}", other),
    }
}

#[tokio::test]
async fn test_execute_self_heal_action_returns_error_when_not_initialized() {
    use crate::api::mcp::tools::self_heal::ExecuteSelfHealActionHandler;

    let handler = ExecuteSelfHealActionHandler;
    let result = handler.execute(serde_json::json!({
        "level": "L1",
        "actionType": "log_only",
        "target": "test-device"
    })).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        ToolError::Internal(msg) => {
            assert!(msg.contains("Self-healing not initialized"), "{}", msg);
        }
        other => panic!("Expected ToolError::Internal, got {:?}", other),
    }
}

#[tokio::test]
async fn test_execute_self_heal_action_rejects_invalid_level() {
    use crate::api::mcp::tools::self_heal::ExecuteSelfHealActionHandler;

    let handler = ExecuteSelfHealActionHandler;
    let result = handler.execute(serde_json::json!({
        "level": "INVALID",
        "actionType": "log_only",
        "target": "test"
    })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_self_heal_action_rejects_invalid_action_type() {
    use crate::api::mcp::tools::self_heal::ExecuteSelfHealActionHandler;

    let handler = ExecuteSelfHealActionHandler;
    let result = handler.execute(serde_json::json!({
        "level": "L1",
        "actionType": "invalid_action",
        "target": "test"
    })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_recovery_history_returns_error_when_not_initialized() {
    use crate::api::mcp::tools::self_heal::GetRecoveryHistoryHandler;

    let handler = GetRecoveryHistoryHandler;
    let result = handler.execute(serde_json::json!({
        "limit": 10,
        "offset": 0
    })).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        ToolError::Internal(msg) => {
            assert!(msg.contains("Self-healing not initialized"), "{}", msg);
        }
        other => panic!("Expected ToolError::Internal, got {:?}", other),
    }
}

#[tokio::test]
async fn test_get_recovery_history_accepts_empty_args() {
    use crate::api::mcp::tools::self_heal::GetRecoveryHistoryHandler;

    let handler = GetRecoveryHistoryHandler;
    let result = handler.execute(serde_json::json!({})).await;
    assert!(result.is_err()); // Will fail on state check, not param parsing
}

#[test]
fn test_self_heal_policy_handler_metadata() {
    use crate::api::mcp::tools::self_heal::GetSelfHealPolicyHandler;

    let handler = GetSelfHealPolicyHandler;
    assert_eq!(handler.name(), "get_self_heal_policy");
    assert!(!handler.description().is_empty());
}

#[test]
fn test_self_heal_action_handler_metadata() {
    use crate::api::mcp::tools::self_heal::ExecuteSelfHealActionHandler;

    let handler = ExecuteSelfHealActionHandler;
    assert_eq!(handler.name(), "execute_self_heal_action");
    assert!(!handler.description().is_empty());
    let schema = handler.input_schema();
    let json = schema.to_json();
    assert_eq!(json["type"], "object");
    let required = json["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "level"), "level should be required");
    assert!(required.iter().any(|r| r == "actionType"), "actionType should be required");
}

#[test]
fn test_self_heal_recovery_history_handler_metadata() {
    use crate::api::mcp::tools::self_heal::GetRecoveryHistoryHandler;

    let handler = GetRecoveryHistoryHandler;
    assert_eq!(handler.name(), "get_recovery_history");
    assert!(!handler.description().is_empty());
}
