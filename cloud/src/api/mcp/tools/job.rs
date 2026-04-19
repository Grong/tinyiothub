// Job Tools Module — Compatibility layer over new cron system
// MCP tools for scheduled job management

use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use tinyiothub_core::models::cron_job::{
    CreateCronJobRequest, CronJobQuery, UpdateCronJobRequest,
};

/// Tool input: List schedules
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSchedulesInput {
    page: Option<u32>,
    page_size: Option<u32>,
    job_type: Option<String>,
    is_enabled: Option<bool>,
}

/// Tool input: Create schedule
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateScheduleInput {
    name: String,
    description: Option<String>,
    job_type: String,
    cron_expression: String,
    target_device_id: Option<String>,
    target_command_name: Option<String>,
    target_command_params: Option<String>,
    config: Option<String>,
    timeout_seconds: Option<i32>,
    retry_count: Option<i32>,
    is_enabled: Option<bool>,
}

/// Tool input: Delete schedule
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteScheduleInput {
    id: String,
}

/// Tool input: Update schedule
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateScheduleInput {
    id: String,
    name: Option<String>,
    description: Option<String>,
    job_type: Option<String>,
    cron_expression: Option<String>,
    target_device_id: Option<String>,
    target_command_name: Option<String>,
    target_command_params: Option<String>,
    config: Option<String>,
    timeout_seconds: Option<i32>,
    retry_count: Option<i32>,
    is_enabled: Option<bool>,
}

// ─── Mapping helpers ───────────────────────────────────────────────────────

fn build_config_from_input(input: &CreateScheduleInput) -> String {
    if input.job_type == "device_command" {
        let mut cfg = serde_json::Map::new();
        if let Some(ref did) = input.target_device_id {
            cfg.insert("device_id".to_string(), serde_json::Value::String(did.clone()));
        }
        if let Some(ref cn) = input.target_command_name {
            cfg.insert("command_name".to_string(), serde_json::Value::String(cn.clone()));
        }
        if let Some(ref params) = input.target_command_params {
            cfg.insert("params".to_string(), serde_json::Value::String(params.clone()));
        }
        serde_json::Value::Object(cfg).to_string()
    } else {
        input.config.clone().unwrap_or_else(|| "{}".to_string())
    }
}

fn map_create_input(input: &CreateScheduleInput) -> CreateCronJobRequest {
    let job_type = if input.job_type == "script" {
        "shell".to_string()
    } else {
        input.job_type.clone()
    };

    CreateCronJobRequest {
        name: input.name.clone(),
        description: input.description.clone(),
        job_type,
        cron_expression: input.cron_expression.clone(),
        config: build_config_from_input(input),
        timeout_seconds: input.timeout_seconds,
        max_retries: input.retry_count,
    }
}

fn map_update_input(input: &UpdateScheduleInput) -> UpdateCronJobRequest {
    let job_type = input.job_type.as_ref().map(|t| {
        if t == "script" { "shell".to_string() } else { t.clone() }
    });

    let config = input.config.clone().or_else(|| {
        if job_type.as_deref() == Some("device_command") {
            let mut cfg = serde_json::Map::new();
            if let Some(ref did) = input.target_device_id {
                cfg.insert("device_id".to_string(), serde_json::Value::String(did.clone()));
            }
            if let Some(ref cn) = input.target_command_name {
                cfg.insert("command_name".to_string(), serde_json::Value::String(cn.clone()));
            }
            if let Some(ref params) = input.target_command_params {
                cfg.insert("params".to_string(), serde_json::Value::String(params.clone()));
            }
            if !cfg.is_empty() {
                Some(serde_json::Value::Object(cfg).to_string())
            } else {
                None
            }
        } else {
            None
        }
    });

    UpdateCronJobRequest {
        name: input.name.clone(),
        description: input.description.clone(),
        job_type,
        cron_expression: input.cron_expression.clone(),
        config,
        timeout_seconds: input.timeout_seconds,
        max_retries: input.retry_count,
        is_enabled: input.is_enabled,
    }
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// List schedules tool handler
pub struct ListSchedulesHandler;

#[async_trait]
impl ToolHandler for ListSchedulesHandler {
    fn name(&self) -> &str {
        "list_schedules"
    }

    fn description(&self) -> &str {
        "List all scheduled jobs (cron jobs) for the current workspace."
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
        props.insert(
            "jobType".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Filter by job type (e.g., device_command, shell, agent)".to_string()),
            },
        );
        props.insert(
            "isEnabled".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Filter by enabled status".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ListSchedulesInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let query = CronJobQuery {
            workspace_id: Some(claims.workspace_id.clone()),
            name: None,
            job_type: input.job_type,
            is_enabled: input.is_enabled,
            page: input.page,
            page_size: input.page_size,
        };

        let jobs = state
            .cron_job_repo
            .find_all(&query)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to list schedules: {}", e)))?;

        Ok(serde_json::to_value(jobs).unwrap())
    }
}

/// Create schedule tool handler
pub struct CreateScheduleHandler;

#[async_trait]
impl ToolHandler for CreateScheduleHandler {
    fn name(&self) -> &str {
        "create_schedule"
    }

    fn description(&self) -> &str {
        "Create a new scheduled job (one-time or cron)."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Job name".to_string()),
            },
        );
        props.insert(
            "description".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Optional job description".to_string()),
            },
        );
        props.insert(
            "jobType".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Job type: device_command, shell, agent".to_string()),
            },
        );
        props.insert(
            "cronExpression".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Cron expression (e.g., */5 * * * * for every 5 minutes)".to_string()),
            },
        );
        props.insert(
            "targetDeviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Target device ID for device_command jobs".to_string()),
            },
        );
        props.insert(
            "targetCommandName".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command name to execute".to_string()),
            },
        );
        props.insert(
            "targetCommandParams".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Command parameters as JSON string".to_string()),
            },
        );
        props.insert(
            "config".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Additional config as JSON string".to_string()),
            },
        );
        props.insert(
            "timeoutSeconds".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Timeout in seconds (default: 300)".to_string()),
            },
        );
        props.insert(
            "retryCount".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Number of retries on failure (default: 3)".to_string()),
            },
        );
        InputSchema::object(vec!["name".to_string(), "jobType".to_string(), "cronExpression".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateScheduleInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // SECURITY: Verify target_device_id belongs to authenticated workspace if provided
        if let Some(ref device_id) = input.target_device_id {
            let db = state.database();
            let device = tinyiothub_core::models::device::find_device_by_id(db, device_id)
                .await
                .map_err(|e| ToolError::Internal(format!("failed to verify device: {}", e)))?
                .ok_or_else(|| ToolError::NotFound(format!("device {} not found", device_id)))?;

            if device.workspace_id.as_ref() != Some(&claims.workspace_id) {
                tracing::warn!("MCP create_schedule: access denied to device {} for workspace {}", device_id, claims.workspace_id);
                return Err(ToolError::Forbidden(
                    "Access denied: target device does not belong to authenticated workspace".to_string()
                ));
            }
        }

        // Validate cron expression
        if let Err(e) = cron::Schedule::from_str(&input.cron_expression) {
            return Err(ToolError::InvalidParams(format!("Invalid cron expression: {}", e)));
        }

        let req = map_create_input(&input);

        let job = state
            .cron_job_repo
            .create(&req, &claims.workspace_id, Some(&claims.api_key_name))
            .await
            .map_err(|e| ToolError::Internal(format!("failed to create schedule: {}", e)))?;

        Ok(serde_json::to_value(job).unwrap())
    }
}

/// Update schedule tool handler
pub struct UpdateScheduleHandler;

#[async_trait]
impl ToolHandler for UpdateScheduleHandler {
    fn name(&self) -> &str {
        "update_schedule"
    }

    fn description(&self) -> &str {
        "Update an existing scheduled job."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Schedule ID to update".to_string()),
            },
        );
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Job name".to_string()),
            },
        );
        props.insert(
            "cronExpression".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Cron expression".to_string()),
            },
        );
        props.insert(
            "isEnabled".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Enable or disable the job".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: UpdateScheduleInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        if let Some(ref cron) = input.cron_expression {
            if let Err(e) = cron::Schedule::from_str(cron) {
                return Err(ToolError::InvalidParams(format!("Invalid cron expression: {}", e)));
            }
        }

        let req = map_update_input(&input);

        let job = state
            .cron_job_repo
            .update(&input.id, &claims.workspace_id, &req)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to update schedule: {}", e)))?;

        Ok(serde_json::to_value(job).unwrap())
    }
}

/// Delete schedule tool handler
pub struct DeleteScheduleHandler;

#[async_trait]
impl ToolHandler for DeleteScheduleHandler {
    fn name(&self) -> &str {
        "delete_schedule"
    }

    fn description(&self) -> &str {
        "Delete a scheduled job by ID."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Schedule ID to delete".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: DeleteScheduleInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Verify the job exists and belongs to the workspace
        let existing = state
            .cron_job_repo
            .find_by_id(&input.id, &claims.workspace_id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to get schedule: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("schedule not found".to_string()))?;

        state
            .cron_job_repo
            .delete(&input.id, &claims.workspace_id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to delete schedule: {}", e)))?;

        let _ = state
            .cron_run_repo
            .delete_by_job_id(&input.id, &claims.workspace_id)
            .await;

        Ok(serde_json::json!({
            "success": true,
            "id": input.id,
            "deleted_job_name": existing.name
        }).into())
    }
}
