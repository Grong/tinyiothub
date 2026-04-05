// Workspace Tools Module
// MCP tools for workspace management

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::dto::entity::workspace::{
    Workspace, WorkspaceWithDeviceCount,
};
use crate::infrastructure::openclaw_agent::{OpenClawAgentClient, OpenClawAgentConfig};

/// Tool input: List workspaces
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListWorkspacesInput {
    page: Option<u32>,
    page_size: Option<u32>,
}

/// Tool input: Get workspace
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkspaceInput {
    id: String,
}

/// Tool input: Create workspace
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorkspaceInput {
    name: String,
    description: Option<String>,
}

/// Tool input: Update workspace
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateWorkspaceInput {
    id: String,
    name: Option<String>,
    description: Option<String>,
    agent_config: Option<String>,
}

/// Tool input: Delete workspace
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteWorkspaceInput {
    id: String,
}

/// List workspaces tool handler
pub struct ListWorkspacesHandler;

#[async_trait]
impl ToolHandler for ListWorkspacesHandler {
    fn name(&self) -> &str {
        "workspace_list"
    }

    fn description(&self) -> &str {
        "List all workspaces for the current tenant. Returns workspaces with device counts."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "page".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Page number (default: 1)".to_string()),
            },
        );
        props.insert(
            "pageSize".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Page size (default: 20, max: 100)".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ListWorkspacesInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        let workspaces = Workspace::find_by_tenant(
            &db,
            &claims.tenant_id,
            input.page,
            input.page_size,
        )
        .await
        .map_err(|e| ToolError::Internal(format!("failed to list workspaces: {}", e)))?;

        Ok(serde_json::to_value(workspaces).unwrap())
    }
}

/// Get workspace tool handler
pub struct GetWorkspaceHandler;

#[async_trait]
impl ToolHandler for GetWorkspaceHandler {
    fn name(&self) -> &str {
        "workspace_get"
    }

    fn description(&self) -> &str {
        "Get a workspace by ID. Returns workspace details including device count."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetWorkspaceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        let workspace = Workspace::find_by_id(&db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to get workspace: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("workspace not found".to_string()))?;

        // Verify tenant ownership
        if workspace.tenant_id != claims.tenant_id {
            return Err(ToolError::Unauthorized(
                "workspace does not belong to this tenant".to_string(),
            ));
        }

        Ok(serde_json::to_value(workspace).unwrap())
    }
}

/// Create workspace tool handler
pub struct CreateWorkspaceHandler;

#[async_trait]
impl ToolHandler for CreateWorkspaceHandler {
    fn name(&self) -> &str {
        "workspace_create"
    }

    fn description(&self) -> &str {
        "Create a new workspace. Automatically creates an associated OpenClaw AI agent."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace name".to_string()),
            },
        );
        props.insert(
            "description".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Optional workspace description".to_string()),
            },
        );
        InputSchema::object(vec!["name".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateWorkspaceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        // Create workspace in DB
        let workspace = Workspace::create(
            &db,
            &claims.tenant_id,
            &input.name,
            input.description.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|e| ToolError::Internal(format!("failed to create workspace: {}", e)))?;

        // Try to create OpenClaw Agent
        let openclaw_url = crate::infrastructure::config::get()
            .openclaw
            .as_ref()
            .map(|c| c.url.clone())
            .unwrap_or_else(|| "http://localhost:4010".to_string());

        let client = crate::infrastructure::openclaw_agent::RealOpenClawAgentClient::new(openclaw_url);
        let agent_result = client
            .create_agent(&OpenClawAgentConfig {
                workspace_id: workspace.id.clone(),
                name: workspace.name.clone(),
            })
            .await;

        let (final_workspace, warning) = match agent_result {
            Ok(agent_id) => {
                // Update workspace with agent_id
                if let Ok(Some(updated)) =
                    Workspace::update(&db, &workspace.id, None, None, None).await
                {
                    (updated, None)
                } else {
                    let wc = WorkspaceWithDeviceCount {
                        id: workspace.id.clone(),
                        name: workspace.name.clone(),
                        description: workspace.description.clone(),
                        tenant_id: workspace.tenant_id.clone(),
                        agent_id: workspace.agent_id.clone(),
                        created_at: workspace.created_at.clone(),
                        updated_at: workspace.updated_at.clone(),
                        device_count: Some(0),
                        warning: None,
                    };
                    (wc, None)
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create OpenClaw agent for workspace {}: {}",
                    workspace.id,
                    e
                );
                let wc = WorkspaceWithDeviceCount {
                    id: workspace.id.clone(),
                    name: workspace.name.clone(),
                    description: workspace.description.clone(),
                    tenant_id: workspace.tenant_id.clone(),
                    agent_id: workspace.agent_id.clone(),
                    created_at: workspace.created_at.clone(),
                    updated_at: workspace.updated_at.clone(),
                    device_count: Some(0),
                    warning: None,
                };
                (wc, Some(format!("OpenClaw unavailable: {}", e)))
            }
        };

        let mut result = final_workspace;
        if let Some(w) = warning {
            result.warning = Some(w);
        }

        Ok(serde_json::to_value(result).unwrap())
    }
}

/// Update workspace tool handler
pub struct UpdateWorkspaceHandler;

#[async_trait]
impl ToolHandler for UpdateWorkspaceHandler {
    fn name(&self) -> &str {
        "workspace_update"
    }

    fn description(&self) -> &str {
        "Update a workspace's name, description, or agent configuration."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID".to_string()),
            },
        );
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("New workspace name".to_string()),
            },
        );
        props.insert(
            "description".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("New workspace description".to_string()),
            },
        );
        props.insert(
            "agentConfig".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Agent configuration as JSON string".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: UpdateWorkspaceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        // Verify workspace exists and belongs to tenant
        let existing = Workspace::find_by_id(&db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to get workspace: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("workspace not found".to_string()))?;

        if existing.tenant_id != claims.tenant_id {
            return Err(ToolError::Unauthorized(
                "workspace does not belong to this tenant".to_string(),
            ));
        }

        // Update workspace
        let workspace = Workspace::update(
            &db,
            &input.id,
            input.name.as_deref(),
            input.description.as_deref(),
            input.agent_config.as_deref(),
        )
        .await
        .map_err(|e| ToolError::Internal(format!("failed to update workspace: {}", e)))?
        .ok_or_else(|| ToolError::NotFound("workspace not found after update".to_string()))?;

        Ok(serde_json::to_value(workspace).unwrap())
    }
}

/// Delete workspace tool handler
pub struct DeleteWorkspaceHandler;

#[async_trait]
impl ToolHandler for DeleteWorkspaceHandler {
    fn name(&self) -> &str {
        "workspace_delete"
    }

    fn description(&self) -> &str {
        "Delete a workspace. Also deletes the associated OpenClaw AI agent."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID to delete".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: DeleteWorkspaceInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        // Get workspace to find agent_id
        let workspace = Workspace::find_by_id(&db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to get workspace: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("workspace not found".to_string()))?;

        if workspace.tenant_id != claims.tenant_id {
            return Err(ToolError::Unauthorized(
                "workspace does not belong to this tenant".to_string(),
            ));
        }

        // Try to delete OpenClaw Agent
        if let Some(agent_id) = workspace.agent_id {
            let openclaw_url = crate::infrastructure::config::get()
                .openclaw
                .as_ref()
                .map(|c| c.url.clone())
                .unwrap_or_else(|| "http://localhost:4010".to_string());

            let client =
                crate::infrastructure::openclaw_agent::RealOpenClawAgentClient::new(openclaw_url);
            if let Err(e) = client.delete_agent(&agent_id).await {
                tracing::warn!(
                    "Failed to delete OpenClaw agent {}: {}. Proceeding with workspace deletion.",
                    agent_id,
                    e
                );
            }
        }

        // Delete workspace from DB
        Workspace::delete(&db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to delete workspace: {}", e)))?;

        Ok(serde_json::json!({ "success": true, "id": input.id }).into())
    }
}
