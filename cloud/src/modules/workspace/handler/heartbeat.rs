// Heartbeat handlers — per-workspace AI autonomous inspection endpoints
//
// Routes (registered under /workspaces/{id}/heartbeat):
//   GET  /config — read heartbeat config + tasks
//   PUT  /config — update enabled/intervalMinutes
//   GET  /logs  — query heartbeat execution history
//   GET  /tasks — read HEARTBEAT.md tasks
//   PUT  /tasks — write HEARTBEAT.md tasks

use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::agent::heartbeat::{
        HeartbeatTask, get_default_tasks, read_heartbeat_tasks, write_heartbeat_tasks,
    },
    shared::{api_response::ApiResponse, app_state::AppState, paths, security::jwt::Claims},
    verify_workspace_access,
};

// ── Response types ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfigResponse {
    enabled: bool,
    interval_minutes: u32,
    workspace_id: String,
    agent_id: String,
    tasks: Vec<HeartbeatTask>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHeartbeatConfigRequest {
    enabled: Option<bool>,
    interval_minutes: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatLogEntry {
    timestamp: String,
    task_count: u32,
    status: String,
    error_message: Option<String>,
    result: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatLogsResponse {
    logs: Vec<HeartbeatLogEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHeartbeatTasksRequest {
    tasks: Vec<HeartbeatTask>,
}

// ── GET /{id}/heartbeat/config ──

pub async fn get_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);
    let tasks = read_heartbeat_tasks(&workspace_dir).await.unwrap_or_else(|e| {
        tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
        get_default_tasks()
    });

    let config = state.heartbeat_manager.config().await;
    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled: config.enabled,
        interval_minutes: config.interval_minutes,
        workspace_id: workspace_id.clone(),
        agent_id: "default".to_string(),
        tasks,
    })
}

// ── PUT /{id}/heartbeat/config ──

pub async fn update_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatConfigRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    let config = state.heartbeat_manager.update_config(req.enabled, req.interval_minutes).await;

    // Restart the heartbeat loop to apply config changes
    state.heartbeat_manager.restart(&workspace_id).await;

    ApiResponseBuilder::success(serde_json::json!({
        "enabled": config.enabled,
        "intervalMinutes": config.interval_minutes,
    }))
}

// ── GET /{id}/heartbeat/logs ──

pub async fn get_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatLogsResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND event_type = 'heartbeat' \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&workspace_id)
    .fetch_all(state.database.pool())
    .await;

    let logs = match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|(action_type, content, created_at)| {
                let status = if action_type == "error" { "error" } else { "success" };
                let (task_count, message) = parse_action_content(&content);
                HeartbeatLogEntry {
                    timestamp: created_at,
                    task_count,
                    status: status.to_string(),
                    error_message: if status == "error" { message.clone() } else { None },
                    result: if status == "success" { message } else { None },
                }
            })
            .collect(),
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to query heartbeat logs: {}", e);
            vec![]
        }
    };

    ApiResponseBuilder::success(HeartbeatLogsResponse { logs })
}

// ── GET /{id}/heartbeat/tasks ──

pub async fn get_tasks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);
    let tasks = read_heartbeat_tasks(&workspace_dir).await.unwrap_or_else(|e| {
        tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
        get_default_tasks()
    });

    ApiResponseBuilder::success(tasks)
}

// ── PUT /{id}/heartbeat/tasks ──

pub async fn update_tasks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatTasksRequest>,
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);

    // Ensure workspace dir exists
    if !workspace_dir.exists() {
        if let Err(e) = tokio::fs::create_dir_all(&workspace_dir).await {
            tracing::error!(%workspace_id, "Failed to create workspace dir: {}", e);
            return ApiResponseBuilder::error("创建工作空间目录失败");
        }
    }

    if let Err(e) = write_heartbeat_tasks(&workspace_dir, &req.tasks).await {
        tracing::error!(%workspace_id, "Failed to write HEARTBEAT.md: {}", e);
        return ApiResponseBuilder::error("保存心跳任务失败");
    }

    ApiResponseBuilder::success(req.tasks)
}

// ── Helpers ──

fn parse_action_content(content: &str) -> (u32, Option<String>) {
    // New format: {"taskCount": N, "result": "..."} or {"taskCount": N, "error": "..."}
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
        let task_count =
            parsed.get("taskCount").and_then(|v| v.as_u64()).map(|n| n as u32).unwrap_or(0);
        let message = parsed
            .get("result")
            .or_else(|| parsed.get("error"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return (task_count, message);
    }
    // Legacy format: plain text content
    (0, Some(content.to_string()))
}
