// MCP Tool Registry
// ToolHandler trait + HandlerRegistry for managing MCP tools

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

/// MCP tool execution errors
#[derive(Debug, Clone)]
pub enum ToolError {
    /// Tool not implemented
    NotImplemented(String),
    /// Invalid parameters
    InvalidParams(String),
    /// Unauthorized access (authentication failed)
    Unauthorized(String),
    /// Forbidden access (authenticated but not authorized for this resource)
    Forbidden(String),
    /// Resource not found
    NotFound(String),
    /// Rate limited
    RateLimited(String),
    /// API error with status code
    ApiError(i32, String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
            ToolError::InvalidParams(msg) => write!(f, "Invalid params: {}", msg),
            ToolError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            ToolError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            ToolError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ToolError::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
            ToolError::ApiError(code, msg) => write!(f, "API error {}: {}", code, msg),
            ToolError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for ToolError {}

/// Tool input schema for JSON Schema format
#[derive(Debug, Clone)]
pub struct InputSchema {
    /// JSON Schema type (e.g., "object")
    pub schema_type: String,
    /// Required properties
    pub required: Vec<String>,
    /// Properties schema
    pub properties: HashMap<String, PropertySchema>,
}

/// Property schema definition
#[derive(Debug, Clone)]
pub struct PropertySchema {
    /// Property type
    pub prop_type: String,
    /// Property description
    pub description: Option<String>,
}

impl InputSchema {
    /// Create a simple object schema with required properties
    pub fn object(required: Vec<String>, properties: HashMap<String, PropertySchema>) -> Self {
        Self { schema_type: "object".to_string(), required, properties }
    }

    /// Convert to JSON value for MCP protocol
    pub fn to_json(&self) -> Value {
        let mut props = serde_json::Map::new();
        for (name, prop) in &self.properties {
            let mut prop_map = serde_json::Map::new();
            prop_map.insert("type".to_string(), serde_json::json!(prop.prop_type));
            if let Some(desc) = &prop.description {
                prop_map.insert("description".to_string(), serde_json::json!(desc));
            }
            props.insert(name.clone(), serde_json::Value::Object(prop_map));
        }

        let mut obj = serde_json::Map::new();
        obj.insert("type".to_string(), serde_json::json!(self.schema_type));
        obj.insert("required".to_string(), serde_json::json!(self.required));
        obj.insert("properties".to_string(), serde_json::Value::Object(props));

        serde_json::Value::Object(obj)
    }
}

/// Tool handler trait for MCP tools
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// Get tool name
    fn name(&self) -> &str;
    /// Get tool description
    fn description(&self) -> &str;
    /// Get input schema
    fn input_schema(&self) -> InputSchema;
    /// Execute tool with arguments
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
}

/// Tool metadata for listing
#[derive(Debug, Clone, Serialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolMetadata {
    /// Create metadata from a tool handler reference
    pub fn from_handler(handler: &dyn ToolHandler) -> Self {
        Self {
            name: handler.name().to_string(),
            description: handler.description().to_string(),
            input_schema: handler.input_schema().to_json(),
        }
    }
}

/// Handler registry for managing MCP tools
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: HashMap<String, std::sync::Arc<dyn ToolHandler>>,
}

impl HandlerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
    }

    /// Register a tool handler
    pub fn register<H: ToolHandler + 'static>(&mut self, handler: H) -> &mut Self {
        self.handlers.insert(handler.name().to_string(), std::sync::Arc::new(handler));
        self
    }

    /// Get a tool handler by name
    pub fn get(&self, name: &str) -> Option<&dyn ToolHandler> {
        self.handlers.get(name).map(|h| h.as_ref())
    }

    /// Get a cloned tool handler by name
    pub fn get_owned(&self, name: &str) -> Option<std::sync::Arc<dyn ToolHandler>> {
        self.handlers.get(name).cloned()
    }

    /// List all registered tool names
    pub fn list_names(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    /// List all tool metadata
    pub fn list_tools(&self) -> Vec<ToolMetadata> {
        self.handlers.values().map(|h| ToolMetadata::from_handler(h.as_ref())).collect()
    }

    /// Check if a tool is registered
    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get the number of registered tools
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    struct DummyHandler;

    #[async_trait]
    impl ToolHandler for DummyHandler {
        fn name(&self) -> &str {
            "dummy_tool"
        }

        fn description(&self) -> &str {
            "A dummy tool for testing"
        }

        fn input_schema(&self) -> InputSchema {
            let mut props = HashMap::new();
            props.insert(
                "param1".to_string(),
                PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Test param".to_string()),
                },
            );
            InputSchema::object(vec!["param1".to_string()], props)
        }

        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = HandlerRegistry::new();
        registry.register(DummyHandler);

        assert!(registry.contains("dummy_tool"));
        assert!(!registry.contains("nonexistent"));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_registry_list_tools() {
        let mut registry = HandlerRegistry::new();
        registry.register(DummyHandler);

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "dummy_tool");
    }

    #[test]
    fn test_input_schema_to_json() {
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
}
