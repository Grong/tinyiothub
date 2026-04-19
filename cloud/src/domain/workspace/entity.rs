// Workspace entity

use serde::{Deserialize, Serialize};

/// Workspace entity — represents a physical/logical environment managed by AI
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Workspace {
    pub fn new(
        id: String,
        name: String,
        description: Option<String>,
        tenant_id: String,
    ) -> Self {
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

/// Input for creating a new workspace
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceInput {
    pub name: String,
    pub description: Option<String>,
}

/// Input for updating a workspace
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkspaceInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agent_config: Option<String>,
}

/// Input for assigning a device to a workspace
#[derive(Debug, Clone, Deserialize)]
pub struct AssignDeviceInput {
    pub device_id: String,
    pub workspace_id: String,
}
