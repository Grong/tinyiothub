// Workspace domain service

use std::sync::Arc;

use crate::domain::workspace::entity::{AssignDeviceInput, CreateWorkspaceInput, UpdateWorkspaceInput, Workspace};
use crate::infrastructure::openclaw_agent::{OpenClawAgentClient, OpenClawAgentConfig};
use crate::shared::error::Error;

/// Workspace service — coordinates workspace operations with OpenClaw Agent lifecycle
pub struct WorkspaceService {
    agent_client: Arc<dyn OpenClawAgentClient>,
}

impl WorkspaceService {
    pub fn new(agent_client: Arc<dyn OpenClawAgentClient>) -> Self {
        Self { agent_client }
    }

    /// Create a workspace with synchronized OpenClaw Agent creation
    /// Returns (workspace, warning) — warning is Some if OpenClaw was unavailable
    pub async fn create_workspace(
        &self,
        tenant_id: String,
        input: CreateWorkspaceInput,
    ) -> Result<(Workspace, Option<String>), Error> {
        let workspace_id = format!("ws-{}", uuid::Uuid::new_v4());

        // Create OpenClaw Agent
        let agent_config = OpenClawAgentConfig {
            workspace_id: workspace_id.clone(),
            name: input.name.clone(),
        };

        let agent_result = self.agent_client.create_agent(&agent_config).await;

        let (agent_id, warning) = match agent_result {
            Ok(agent_id) => (Some(agent_id), None),
            Err(e) => {
                tracing::warn!("Failed to create OpenClaw agent: {}. Workspace will be created with NULL agent_id.", e);
                (None, Some(format!("OpenClaw unavailable: {}. Agent pending.", e)))
            }
        };

        let mut workspace = Workspace::new(
            workspace_id,
            input.name,
            input.description,
            tenant_id,
        );

        if let Some(config) = agent_config.to_json() {
            workspace = workspace.with_config(config);
        }

        if let Some(aid) = agent_id {
            workspace = workspace.with_agent(aid);
        }

        Ok((workspace, warning))
    }

    /// Update workspace metadata and/or agent config
    pub async fn update_workspace(
        &self,
        workspace: &mut Workspace,
        input: UpdateWorkspaceInput,
    ) -> Result<Option<String>, Error> {
        let mut warning = None;

        if let Some(name) = input.name {
            workspace.name = name;
        }

        if let Some(desc) = input.description {
            workspace.description = Some(desc);
        }

        if let Some(config) = input.agent_config {
            workspace.agent_config = Some(config.clone());

            // If workspace has an agent, update it
            if let Some(agent_id) = &workspace.agent_id {
                if let Err(e) = self.agent_client.update_agent(agent_id, &config).await {
                    warning = Some(format!("Agent update failed: {}. Changes saved locally.", e));
                }
            }
        }

        Ok(warning)
    }

    /// Delete workspace with synchronized OpenClaw Agent deletion
    pub async fn delete_workspace(&self, workspace: &Workspace) -> Result<(), Error> {
        if let Some(agent_id) = &workspace.agent_id {
            if let Err(e) = self.agent_client.delete_agent(agent_id).await {
                tracing::warn!("Failed to delete OpenClaw agent {}: {}. Proceeding with workspace deletion.", agent_id, e);
            }
        }
        Ok(())
    }

    /// Assign a device to this workspace
    /// Returns error if device is already assigned to another workspace
    pub fn assign_device(
        &self,
        workspace: &Workspace,
        device_workspace_id: Option<String>,
    ) -> Result<(), Error> {
        if let Some(existing_ws) = device_workspace_id {
            if existing_ws != workspace.id {
                return Err(Error::InvalidArgument(format!(
                    "device already assigned to workspace {}",
                    existing_ws
                )));
            }
            // Already assigned to this workspace — no-op
            return Ok(());
        }
        // Free device — allowed
        Ok(())
    }
}

impl Workspace {
    pub fn update_timestamp(&mut self) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
