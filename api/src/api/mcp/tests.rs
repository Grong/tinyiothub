//! MCP 模块测试

#[cfg(test)]
mod tests {
    use crate::api::mcp::{HandlerRegistry, ToolMetadata, ToolHandler, ToolError};
    use crate::api::mcp::tool_registry::InputSchema;
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
}
