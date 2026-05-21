// Workspaces API handlers

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post, put},
};
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::{
    AssignDeviceRequest, CreateWorkspaceRequest, UpdateWorkspaceRequest, WorkspaceQueryParams,
    WorkspaceWithDeviceCount,
};
use crate::shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims};

/// Create workspaces router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_workspaces))
        .route("/", post(create_workspace))
        .route("/{id}", get(get_workspace))
        .route("/{id}", put(update_workspace))
        .route("/{id}", delete(delete_workspace))
        .route("/{id}/devices", post(assign_device))
}

/// List workspaces for current tenant
async fn list_workspaces(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<WorkspaceQueryParams>,
) -> Json<ApiResponse<Vec<WorkspaceWithDeviceCount>>> {
    match state
        .workspace_service
        .find_by_tenant(&claims.tenant_id, params.page, params.page_size)
        .await
    {
        Ok(workspaces) => ApiResponseBuilder::success(workspaces),
        Err(e) => {
            tracing::error!("Failed to list workspaces: {}", e);
            ApiResponseBuilder::error("获取工作空间列表失败")
        }
    }
}

/// Get workspace by ID
async fn get_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Json<ApiResponse<WorkspaceWithDeviceCount>> {
    match state.workspace_service.find_by_id(&id).await {
        Ok(Some(workspace)) => {
            if workspace.tenant_id != claims.tenant_id {
                return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
            }
            ApiResponseBuilder::success(workspace)
        }
        Ok(None) => ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            ApiResponseBuilder::error("获取工作空间失败")
        }
    }
}

/// Create workspace (synchronously creates Agent)
async fn create_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateWorkspaceRequest>,
) -> Json<ApiResponse<WorkspaceWithDeviceCount>> {
    let workspace = match state
        .workspace_service
        .create(&claims.tenant_id, &payload.name, payload.description.as_deref(), None, None)
        .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("Failed to create workspace: {}", e);
            return ApiResponseBuilder::error("创建工作空间失败");
        }
    };

    let agent_result = state
        .agent_pool
        .create_agent(&crate::shared::agent::AgentConfig {
            workspace_id: workspace.id.clone(),
            name: workspace.name.clone(),
            ..Default::default()
        })
        .await;

    let (final_workspace, warning) = match agent_result {
        Ok(_agent_id) => {
            if let Ok(Some(updated)) = state
                .workspace_service
                .update(&workspace.id, None, None, Some(&_agent_id), None)
                .await
            {
                (updated, None)
            } else {
                let wc = WorkspaceWithDeviceCount {
                    id: workspace.id,
                    name: workspace.name,
                    description: workspace.description,
                    tenant_id: workspace.tenant_id,
                    agent_id: workspace.agent_id,
                    created_at: workspace.created_at,
                    updated_at: workspace.updated_at,
                    device_count: Some(0),
                    warning: None,
                };
                (wc, None)
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create agent for workspace {}: {}. Workspace created with NULL agent_id.",
                workspace.id,
                e
            );
            let wc = WorkspaceWithDeviceCount {
                id: workspace.id,
                name: workspace.name,
                description: workspace.description,
                tenant_id: workspace.tenant_id,
                agent_id: workspace.agent_id,
                created_at: workspace.created_at,
                updated_at: workspace.updated_at,
                device_count: Some(0),
                warning: None,
            };
            (wc, Some(format!("Agent unavailable: {}. Agent pending.", e)))
        }
    };

    let result = WorkspaceWithDeviceCount {
        id: final_workspace.id,
        name: final_workspace.name,
        description: final_workspace.description,
        tenant_id: final_workspace.tenant_id,
        agent_id: final_workspace.agent_id,
        created_at: final_workspace.created_at,
        updated_at: final_workspace.updated_at,
        device_count: Some(0),
        warning,
    };

    ApiResponseBuilder::success(result)
}

/// Update workspace
async fn update_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateWorkspaceRequest>,
) -> Json<ApiResponse<WorkspaceWithDeviceCount>> {
    match state.workspace_service.find_by_id(&id).await {
        Ok(Some(workspace)) => {
            if workspace.tenant_id != claims.tenant_id {
                return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return ApiResponseBuilder::error("获取工作空间失败");
        }
    }

    match state
        .workspace_service
        .update(
            &id,
            payload.name.as_deref(),
            payload.description.as_deref(),
            None,
            payload.agent_config.as_deref(),
        )
        .await
    {
        Ok(Some(workspace)) => ApiResponseBuilder::success(workspace),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
        Err(e) => {
            tracing::error!("Failed to update workspace: {}", e);
            ApiResponseBuilder::error("更新工作空间失败")
        }
    }
}

/// Delete workspace (synchronously deletes Agent)
async fn delete_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let workspace = match state.workspace_service.find_by_id(&id).await {
        Ok(Some(ws)) => {
            if ws.tenant_id != claims.tenant_id {
                return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
            }
            ws
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return ApiResponseBuilder::error("获取工作空间失败");
        }
    };

    if let Some(agent_id) = workspace.agent_id
        && let Err(e) = state.agent_pool.delete_agent(&agent_id).await
    {
        tracing::warn!(
            "Failed to delete agent {}: {}. Proceeding with workspace deletion.",
            agent_id,
            e
        );
    }

    match state.workspace_service.delete(&id).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!("Failed to delete workspace: {}", e);
            ApiResponseBuilder::error("删除工作空间失败")
        }
    }
}

/// Assign device to workspace
async fn assign_device(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<AssignDeviceRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.workspace_service.find_by_id(&workspace_id).await {
        Ok(Some(workspace)) => {
            if workspace.tenant_id != claims.tenant_id {
                return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return ApiResponseBuilder::error("获取工作空间失败");
        }
    };

    match state.workspace_service.assign_device(&payload.device_id, &workspace_id).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => ApiResponseBuilder::error_with_code(409, e.to_string()),
    }
}
