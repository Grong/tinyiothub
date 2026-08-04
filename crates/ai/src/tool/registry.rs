//! Tool Registry — centralized tool discovery and metadata.
//!
//! AI crate defines the registry trait; cloud wires concrete tools
//! (zeroclaw Tool impls, DB-backed tools, skill-based tools).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::trust::ToolSafety;

/// Schema for structured LLM output enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchema {
    /// JSON Schema describing the expected output shape.
    pub json_schema: serde_json::Value,
    /// Whether the LLM should be forced into JSON mode for this tool.
    pub enforce_json_mode: bool,
}

/// A registered tool's full descriptor.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: String,
    pub safety: ToolSafety,
    pub parameters: Vec<ToolParameter>,
    pub output_schema: Option<OutputSchema>,
    pub requires_approval: bool,
}

/// A tool parameter descriptor (for LLM function-calling schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// Central registry for tool discovery and validation.
pub trait ToolRegistry: Send + Sync {
    /// Look up a tool by name.
    fn get(&self, name: &str) -> Option<Arc<ToolDescriptor>>;

    /// List all registered tools, optionally filtered by category.
    fn list(&self, category: Option<&str>) -> Vec<Arc<ToolDescriptor>>;

    /// Generate a JSON Schema representation of all tools for LLM function calling.
    fn generate_llm_schema(&self, category: Option<&str>) -> serde_json::Value;

    /// Check if a tool name is registered.
    fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}
