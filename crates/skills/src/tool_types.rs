//! Tool types -- ToolDependencyProvider trait and tool metadata.

use std::sync::Arc;

/// Interface for tool adapters to resolve workspace/knowledge dependencies
/// without directly holding WorkspaceService/KnowledgeService references.
#[async_trait::async_trait]
pub trait ToolDependencyProvider: Send + Sync {
    async fn resolve_knowledge(&self, workspace_id: &str) -> Option<Arc<dyn KnowledgeProvider>>;
    async fn resolve_workspace(&self, workspace_id: &str) -> Option<Arc<dyn WorkspaceProvider>>;
}

/// Minimal trait for knowledge queries.
pub trait KnowledgeProvider: Send + Sync {
    fn query(&self, query: &str) -> Vec<String>;
}

/// Minimal trait for workspace metadata.
pub trait WorkspaceProvider: Send + Sync {
    fn name(&self) -> &str;
    fn settings(&self) -> serde_json::Value;
}

/// Tool metadata for catalog generation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: Vec<ToolParameter>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub param_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {name}")]
    NotFound { name: String },
    #[error("Permission denied for tool: {name}")]
    PermissionDenied { name: String },
    #[error("Execution failed: {0}")]
    Execution(String),
}
