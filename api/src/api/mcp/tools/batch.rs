// Batch Tools Module
// MCP tools for batch command management

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::infrastructure::batch_command::{
    BatchCommandExecutor, BatchCommandRepository, CreateBatchCommandRequest,
};

/// Tool input: Batch command
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCommandInput {
    workspace_id: String,
    idempotency_key: String,
    command_name: String,
    device_ids: Vec<String>,
    parameters: Option<String>,
    command_type: Option<String>,
    auto_execute: Option<bool>,
}

/// Tool input: Get batch status
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBatchStatusInput {
    batch_id: String,
}

/// Batch command tool handler
pub struct BatchCommandHandler;

#[async_trait]
impl ToolHandler for BatchCommandHandler {
    fn name(&self) -> &str {
        "batch_command"
    }

    fn description(&self) -> &str {
        "Send a command to multiple devices in a workspace. Returns per-device results with idempotency support."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "workspaceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID".to_string()),
            },
        );
        props.insert(
            "idempotencyKey".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Unique key for idempotency (prevents duplicate execution)".to_string()),
            },
        );
        props.insert(
            "commandName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command name to send to devices".to_string()),
            },
        );
        props.insert(
            "deviceIds".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Array of device IDs to send the command to".to_string()),
            },
        );
        props.insert(
            "parameters".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command parameters as JSON string".to_string()),
            },
        );
        props.insert(
            "commandType".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command type (default: custom)".to_string()),
            },
        );
        props.insert(
            "autoExecute".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Auto-execute the batch after creation (default: true)".to_string()),
            },
        );
        InputSchema::object(
            vec![
                "workspaceId".to_string(),
                "idempotencyKey".to_string(),
                "commandName".to_string(),
                "deviceIds".to_string(),
            ],
            props,
        )
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: BatchCommandInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        // SECURITY: Verify workspace_id matches the authenticated context
        // Prevents IDOR attacks where a user tries to access another workspace's resources
        if input.workspace_id != claims.workspace_id {
            return Err(ToolError::Forbidden(
                "Access denied: workspace_id does not match authenticated workspace".to_string()
            ));
        }

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database.clone();

        // Check idempotency - return existing if found
        if let Some(existing) =
            BatchCommandRepository::find_by_idempotency_key(&db, &input.workspace_id, &input.idempotency_key)
                .await
                .map_err(|e| ToolError::Internal(format!("DB error: {}", e)))?
        {
            // Return existing batch
            let batch_with_items = BatchCommandRepository::get_batch_with_items(&db, &existing.id)
                .await
                .map_err(|e| ToolError::Internal(format!("DB error: {}", e)))?
                .ok_or_else(|| ToolError::Internal("Batch not found".to_string()))?;

            return Ok(serde_json::json!({
                "id": existing.id,
                "status": existing.status,
                "idempotency_matched": true,
                "items": batch_with_items.items,
                "message": "Returned existing batch for idempotency key"
            }).into());
        }

        // Create new batch
        let request = CreateBatchCommandRequest {
            workspace_id: input.workspace_id.clone(),
            idempotency_key: input.idempotency_key.clone(),
            command_name: input.command_name.clone(),
            command_type: input.command_type.clone(),
            parameters: input.parameters.clone(),
            device_ids: input.device_ids.clone(),
            submitted_by: Some(claims.actor_identifier().to_string()),
        };

        let batch_with_items = BatchCommandRepository::create(&db, &request)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to create batch: {}", e)))?;

        // Auto-execute if requested (default true)
        let auto_execute = input.auto_execute.unwrap_or(true);
        if auto_execute {
            let device_service = state.device_service.clone();
            match BatchCommandExecutor::execute(&db, device_service, &batch_with_items.batch.id).await {
                Ok(executed) => {
                    return Ok(serde_json::json!({
                        "id": executed.batch.id,
                        "status": executed.batch.status,
                        "idempotency_matched": false,
                        "items": executed.items,
                        "total_devices": executed.batch.total_devices,
                        "message": "Batch created and executed successfully"
                    }).into());
                }
                Err(e) => {
                    tracing::error!("Auto-execute failed for batch {}: {}", batch_with_items.batch.id, e);
                    // Return batch in pending state
                    return Ok(serde_json::json!({
                        "id": batch_with_items.batch.id,
                        "status": "pending",
                        "idempotency_matched": false,
                        "items": batch_with_items.items,
                        "total_devices": batch_with_items.batch.total_devices,
                        "message": format!("Batch created but auto-execute failed: {}", e)
                    }).into());
                }
            }
        }

        Ok(serde_json::json!({
            "id": batch_with_items.batch.id,
            "status": batch_with_items.batch.status,
            "idempotency_matched": false,
            "items": batch_with_items.items,
            "total_devices": batch_with_items.batch.total_devices,
            "message": "Batch created successfully"
        }).into())
    }
}

/// Get batch status tool handler
pub struct GetBatchStatusHandler;

#[async_trait]
impl ToolHandler for GetBatchStatusHandler {
    fn name(&self) -> &str {
        "get_batch_status"
    }

    fn description(&self) -> &str {
        "Get the status of a batch command by ID."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "batchId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Batch command ID".to_string()),
            },
        );
        InputSchema::object(vec!["batchId".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: GetBatchStatusInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database.clone();

        let batch_with_items = BatchCommandRepository::get_batch_with_items(&db, &input.batch_id)
            .await
            .map_err(|e| ToolError::Internal(format!("DB error: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("Batch not found".to_string()))?;

        // SECURITY: Verify batch belongs to the authenticated workspace
        // Prevents IDOR attacks where a user tries to access another workspace's batch data
        if batch_with_items.batch.workspace_id != claims.workspace_id {
            return Err(ToolError::Forbidden(
                "Access denied: batch does not belong to authenticated workspace".to_string()
            ));
        }

        Ok(serde_json::to_value(batch_with_items).unwrap())
    }
}
