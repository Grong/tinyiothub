use std::fmt;

use serde::{Deserialize, Serialize};

pub mod knowledge;
pub use knowledge::*;

/// Workspace entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub agent_config: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Workspace with device count (for list responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceWithDeviceCount {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Create workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Update workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agent_config: Option<String>,
}

/// Assign device request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssignDeviceRequest {
    pub device_id: String,
}

/// Resource type: File (uploaded binaries) or Document (markdown knowledge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Document,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Document => "document",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::File => "文件",
            Self::Document => "文档",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "file" => Some(Self::File),
            "document" => Some(Self::Document),
            _ => None,
        }
    }

    pub fn all() -> [Self; 2] {
        [Self::File, Self::Document]
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unified workspace resource (replaces workspace_resources + knowledge_documents)
/// - type="document": content + parse_status fields are used
/// - type="file": file_path is used (uploaded binaries)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceResource {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: ResourceType,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub parse_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceSearchResult {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: ResourceType,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub parse_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub relevance: i64,
}

/// Create resource request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateResourceRequest {
    pub name: String,
    pub description: Option<String>,
    pub resource_type: ResourceType,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub file_path: Option<String>,
}

/// Update resource request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateResourceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<String>,
    pub parse_status: Option<String>,
}

/// Suggest tags request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SuggestTagsRequest {
    pub name: String,
    pub resource_type: ResourceType,
    pub description: Option<String>,
}

/// Resource query params
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ResourceQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub resource_type: Option<ResourceType>,
}

/// Workspace query params
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Workspace {
    pub fn new(id: String, name: String, description: Option<String>, tenant_id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            name,
            description,
            tenant_id,
            agent_id: None,
            agent_config: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn with_agent(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    pub fn with_config(mut self, config: String) -> Self {
        self.agent_config = Some(config);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new(
            "ws-1".to_string(),
            "Test Workspace".to_string(),
            Some("A test workspace".to_string()),
            "tenant-1".to_string(),
        );
        assert_eq!(ws.id, "ws-1");
        assert_eq!(ws.name, "Test Workspace");
        assert_eq!(ws.description, Some("A test workspace".to_string()));
        assert_eq!(ws.tenant_id, "tenant-1");
        assert!(ws.agent_id.is_none());
        assert!(ws.agent_config.is_none());
    }

    #[test]
    fn test_workspace_with_agent() {
        let ws =
            Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
                .with_agent("agent-1".to_string());
        assert_eq!(ws.agent_id, Some("agent-1".to_string()));
    }

    #[test]
    fn test_workspace_with_config() {
        let ws =
            Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
                .with_config(r#"{"model": "gpt-4"}"#.to_string());
        assert_eq!(ws.agent_config, Some(r#"{"model": "gpt-4"}"#.to_string()));
    }

    #[test]
    fn test_workspace_with_agent_and_config() {
        let ws =
            Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
                .with_agent("agent-1".to_string())
                .with_config("config".to_string());
        assert_eq!(ws.agent_id, Some("agent-1".to_string()));
        assert_eq!(ws.agent_config, Some("config".to_string()));
    }
}
