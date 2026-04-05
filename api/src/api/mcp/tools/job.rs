// Job Tools Module
// MCP tools for scheduled job management

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::handlers::get_mcp_context;
use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::dto::entity::job::{CreateJobRequest, JobQueryParams};

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
    tags: Option<String>,
}

/// Tool input: Delete schedule
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteScheduleInput {
    id: String,
}

/// List schedules tool handler
pub struct ListSchedulesHandler;

#[async_trait]
impl ToolHandler for ListSchedulesHandler {
    fn name(&self) -> &str {
        "list_schedules"
    }

    fn description(&self) -> &str {
        "List all scheduled jobs (cron jobs) for the current tenant."
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
                description: Some("Filter by job type (e.g., device_command, http, script)".to_string()),
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
        let db = state.database();

        let params = JobQueryParams {
            name: None,
            job_type: input.job_type,
            is_enabled: input.is_enabled,
            page: input.page,
            page_size: input.page_size,
        };

        let jobs = crate::dto::entity::job::Job::find_all(db, &params)
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
                description: Some("Job type: device_command, http, script, sql".to_string()),
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
                description: Some("Number of retries on failure (default: 0)".to_string()),
            },
        );
        props.insert(
            "tags".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Tags as JSON array string".to_string()),
            },
        );
        InputSchema::object(vec!["name".to_string(), "jobType".to_string(), "cronExpression".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateScheduleInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let _claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::api::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;
        let db = state.database();

        let config = input.config.unwrap_or_else(|| "{}".to_string());

        let request = CreateJobRequest {
            name: input.name,
            description: input.description,
            job_type: input.job_type,
            cron_expression: input.cron_expression,
            config,
            timeout_seconds: input.timeout_seconds,
            retry_count: input.retry_count,
            retry_delay_seconds: Some(60),
            concurrency: Some(1),
            target_device_id: input.target_device_id,
            target_command_name: input.target_command_name,
            target_command_params: input.target_command_params,
            tags: input.tags,
            alert_config: Some("{}".to_string()),
        };

        let job = crate::dto::entity::job::Job::create(db, &request)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to create schedule: {}", e)))?;

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
        let db = state.database();

        // Verify the job exists first
        let existing = crate::dto::entity::job::Job::find_by_id(db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to get schedule: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("schedule not found".to_string()))?;

        // TODO: Verify tenant ownership via claims.tenant_id when jobs have tenant_id

        crate::dto::entity::job::Job::delete(db, &input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("failed to delete schedule: {}", e)))?;

        // Delete associated executions
        let _ = sqlx::query("DELETE FROM job_executions WHERE job_id = ?")
            .bind(&input.id)
            .execute(db.pool())
            .await;

        Ok(serde_json::json!({
            "success": true,
            "id": input.id,
            "deleted_job_name": existing.name
        }).into())
    }
}
