// Heartbeat API Handlers for Agents
// HTTP endpoint handlers for agent heartbeat configuration and execution logs
//
// Endpoints:
// - GET  /agents/{id}/heartbeat/config
// - PUT  /agents/{id}/heartbeat/config
// - GET  /agents/{id}/heartbeat/logs
// - GET  /agents/{id}/heartbeat/tasks
// - PUT  /agents/{id}/heartbeat/tasks

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::agent::heartbeat::{
        HeartbeatExecutionRecord, HeartbeatTask, get_heartbeat_state, read_heartbeat_tasks,
        write_heartbeat_tasks,
    },
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

/// Request to update heartbeat config
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHeartbeatConfigRequest {
    pub enabled: Option<bool>,
    pub interval_minutes: Option<u32>,
}

/// Request to update heartbeat tasks
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHeartbeatTasksRequest {
    pub tasks: Vec<HeartbeatTask>,
}

/// Response for heartbeat config
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfigResponse {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub workspace_id: String,
    pub agent_id: String,
    pub tasks: Vec<HeartbeatTask>,
}

/// Response for heartbeat logs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatLogsResponse {
    pub logs: Vec<HeartbeatExecutionRecord>,
}

/// Create the heartbeat router for /agents/{id}/heartbeat
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_heartbeat_config).put(update_heartbeat_config))
        .route("/logs", get(get_heartbeat_logs))
        .route("/tasks", get(get_heartbeat_tasks).put(update_heartbeat_tasks))
}

/// Helper to get heartbeat state or return error early
macro_rules! get_state_or_error {
    ($resp_type:ty) => {
        match get_heartbeat_state() {
            Some(s) => s,
            None => return ApiResponseBuilder::error("Agent heartbeat not initialized"),
        }
    };
}

/// GET /api/v1/agents/{agent_id}/heartbeat/config
pub async fn get_heartbeat_config(
    State(_state): State<AppState>,
    Path(_agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    let state_lock = get_state_or_error!(HeartbeatConfigResponse);
    let state = state_lock.read().await;
    let tasks = match read_heartbeat_tasks(&state.workspace_dir).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to read heartbeat tasks: {}", e);
            Vec::new()
        }
    };
    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled: state.enabled,
        interval_minutes: state.interval_minutes,
        workspace_id: state.workspace_id.clone(),
        agent_id: state.agent_id.clone(),
        tasks,
    })
}

/// PUT /api/v1/agents/{agent_id}/heartbeat/config
pub async fn update_heartbeat_config(
    State(_state): State<AppState>,
    Path(_agent_id): Path<String>,
    _claims: Claims,
    Json(req): Json<UpdateHeartbeatConfigRequest>,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    let state_lock = get_state_or_error!(HeartbeatConfigResponse);
    let mut state = state_lock.write().await;
    if let Some(enabled) = req.enabled {
        state.enabled = enabled;
    }
    if let Some(interval) = req.interval_minutes {
        state.interval_minutes = interval;
    }

    tracing::info!(
        "💓 Heartbeat config updated: enabled={}, interval={}min",
        state.enabled,
        state.interval_minutes
    );

    let tasks = match read_heartbeat_tasks(&state.workspace_dir).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to read heartbeat tasks: {}", e);
            Vec::new()
        }
    };
    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled: state.enabled,
        interval_minutes: state.interval_minutes,
        workspace_id: state.workspace_id.clone(),
        agent_id: state.agent_id.clone(),
        tasks,
    })
}

/// GET /api/v1/agents/{agent_id}/heartbeat/logs
pub async fn get_heartbeat_logs(
    State(_state): State<AppState>,
    Path(_agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<HeartbeatLogsResponse>> {
    let state_lock = get_state_or_error!(HeartbeatConfigResponse);
    let state = state_lock.read().await;
    ApiResponseBuilder::success(HeartbeatLogsResponse { logs: state.execution_history.clone() })
}

/// GET /api/v1/agents/{agent_id}/heartbeat/tasks
pub async fn get_heartbeat_tasks(
    State(_state): State<AppState>,
    Path(_agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    let state_lock = get_state_or_error!(HeartbeatConfigResponse);
    let state = state_lock.read().await;
    match read_heartbeat_tasks(&state.workspace_dir).await {
        Ok(tasks) => ApiResponseBuilder::success(tasks),
        Err(e) => ApiResponseBuilder::error(format!("Failed to read tasks: {}", e)),
    }
}

/// PUT /api/v1/agents/{agent_id}/heartbeat/tasks
pub async fn update_heartbeat_tasks(
    State(_state): State<AppState>,
    Path(_agent_id): Path<String>,
    _claims: Claims,
    Json(req): Json<UpdateHeartbeatTasksRequest>,
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    let state_lock = get_state_or_error!(HeartbeatConfigResponse);
    let state = state_lock.write().await;
    match write_heartbeat_tasks(&state.workspace_dir, &req.tasks).await {
        Ok(()) => {
            tracing::info!("💓 Heartbeat tasks updated: {} tasks", req.tasks.len());
            ApiResponseBuilder::success(req.tasks)
        }
        Err(e) => ApiResponseBuilder::error(format!("Failed to save tasks: {}", e)),
    }
}
