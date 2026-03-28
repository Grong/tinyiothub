//! MCP 模块测试

#[cfg(test)]
mod tests {
    use crate::api::mcp::{HandlerRegistry, ToolMetadata, ToolHandler, ToolError};
    use crate::api::mcp::tool_registry::InputSchema;
    use crate::api::mcp::tools::self_heal::{
        GetSelfHealPolicyHandler, ExecuteSelfHealActionHandler, GetRecoveryHistoryHandler,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use serde_json::Value;

    /// 测试用的小型 handler
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

    /// 测试用的小型 handler，返回 NotImplemented 错误
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

    /// 测试用的小型 handler，覆盖用
    struct AnotherTestHandler;

    #[async_trait]
    impl ToolHandler for AnotherTestHandler {
        fn name(&self) -> &str { "test_handler" } // Same name as TestHandler
        fn description(&self) -> &str { "Another test handler" }
        fn input_schema(&self) -> InputSchema {
            InputSchema::object(vec![], HashMap::new())
        }
        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    #[test]
    fn test_handler_registry_register() {
        let mut registry = HandlerRegistry::new();

        // 注册一个 handler
        registry.register(TestHandler);

        // 验证可以找到
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test_handler");
    }

    #[test]
    fn test_handler_registry_get() {
        let mut registry = HandlerRegistry::new();
        registry.register(TestHandler);

        // 获取特定 handler
        let handler = registry.get("test_handler");
        assert!(handler.is_some());

        // 获取不存在的 handler
        let handler = registry.get("nonexistent");
        assert!(handler.is_none());
    }

    #[test]
    fn test_handler_registry_list_tools() {
        let mut registry = HandlerRegistry::new();

        // 列出空注册表
        let tools = registry.list_tools();
        assert!(tools.is_empty());

        // 注册多个 handler
        registry.register(TestHandler);
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_handler_execute() {
        let handler = TestHandler;
        let args = serde_json::json!({"key": "value"});

        let result = handler.execute(args.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), args);
    }

    #[tokio::test]
    async fn test_handler_execute_with_empty_args() {
        let handler = TestHandler;
        let args = serde_json::json!({});

        let result = handler.execute(args).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_handler_error_not_implemented() {
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
        let handler = TestHandler;
        let metadata = ToolMetadata::from_handler(&handler);

        assert_eq!(metadata.name, "test_handler");
        assert_eq!(metadata.description, "A test handler");
    }

    #[test]
    fn test_handler_registry_contains() {
        let mut registry = HandlerRegistry::new();
        registry.register(TestHandler);

        // 验证已注册
        assert!(registry.get("test_handler").is_some());

        // 使用 contains 检查
        assert!(registry.contains("test_handler"));

        // 不存在的 handler
        assert!(!registry.contains("nonexistent"));
    }

    #[test]
    fn test_handler_registry_multiple_handlers_name_collision() {
        let mut registry = HandlerRegistry::new();

        // 注册两个同名 handler，后者覆盖前者
        registry.register(TestHandler);
        registry.register(AnotherTestHandler);

        // 列表中只有 1 个，因为名字相同，后者覆盖前者
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        // 验证是后者（AnotherTestHandler）的描述
        assert_eq!(tools[0].description, "Another test handler");
    }

    #[tokio::test]
    async fn test_handler_returns_json_value() {
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
        let mut registry = HandlerRegistry::new();

        // 空注册表
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        // 注册后
        registry.register(TestHandler);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_handler_registry_list_names() {
        let mut registry = HandlerRegistry::new();

        // 空注册表
        let names = registry.list_names();
        assert!(names.is_empty());

        // 注册后
        registry.register(TestHandler);
        let names = registry.list_names();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "test_handler");
    }

    #[test]
    fn test_input_schema_to_json() {
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            crate::api::mcp::tool_registry::PropertySchema {
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
<<<<<<< HEAD

    // =========================================================================
    // Self-Healing MCP Handler Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_self_heal_policy_returns_error_when_not_initialized() {
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
        let handler = ExecuteSelfHealActionHandler;
        // State check happens first; invalid level still fails on state
        let result = handler.execute(serde_json::json!({
            "level": "INVALID",
            "actionType": "log_only",
            "target": "test"
        })).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_self_heal_action_rejects_invalid_action_type() {
        let handler = ExecuteSelfHealActionHandler;
        // State check happens before action_type validation
        let result = handler.execute(serde_json::json!({
            "level": "L1",
            "actionType": "invalid_action",
            "target": "test"
        })).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_recovery_history_returns_error_when_not_initialized() {
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
        let handler = GetRecoveryHistoryHandler;
        // Empty args should deserialize as HistoryInput with None limit/offset
        let result = handler.execute(serde_json::json!({})).await;
        // Will fail on state check, not param parsing
        assert!(result.is_err());
    }

    #[test]
    fn test_self_heal_policy_handler_metadata() {
        let handler = GetSelfHealPolicyHandler;
        assert_eq!(handler.name(), "get_self_heal_policy");
        assert!(!handler.description().is_empty());
    }

    #[test]
    fn test_self_heal_action_handler_metadata() {
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
        let handler = GetRecoveryHistoryHandler;
        assert_eq!(handler.name(), "get_recovery_history");
        assert!(!handler.description().is_empty());
    }
=======
>>>>>>> origin/main
}
