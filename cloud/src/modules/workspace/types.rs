use serde::{Deserialize, Serialize};

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
