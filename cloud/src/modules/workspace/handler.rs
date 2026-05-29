// Workspaces API handlers

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post, put},
};
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::{
    AssignDeviceRequest, CreateResourceRequest, CreateWorkspaceRequest, ResourceQueryParams,
    ResourceSearchResult, SuggestTagsRequest, UpdateResourceRequest, UpdateWorkspaceRequest,
    WorkspaceQueryParams, WorkspaceResource, WorkspaceWithDeviceCount,
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
        .route("/{id}/resources", get(list_resources))
        .route("/{id}/resources", post(create_resource))
        .route("/{id}/resources/suggest-tags", post(suggest_tags))
        .route("/{id}/resources/search", get(search_resources))
        .route("/{id}/resources/{rid}", get(get_resource))
        .route("/{id}/resources/{rid}", put(update_resource))
        .route("/{id}/resources/{rid}", delete(delete_resource))
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

/// List resources in workspace
async fn list_resources(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<ResourceQueryParams>,
) -> Json<ApiResponse<Vec<WorkspaceResource>>> {
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
        .list_resources(&id, params.resource_type.as_deref(), params.page, params.page_size)
        .await
    {
        Ok(resources) => ApiResponseBuilder::success(resources),
        Err(e) => {
            tracing::error!("Failed to list resources: {}", e);
            ApiResponseBuilder::error("获取资源列表失败")
        }
    }
}

/// Search resources in workspace
async fn search_resources(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<Vec<ResourceSearchResult>>> {
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

    let query = match params.get("q") {
        Some(q) if !q.is_empty() => q.as_str(),
        _ => return ApiResponseBuilder::error_with_code(400, "搜索关键词不能为空"),
    };

    let resource_type = params.get("type").map(|s| s.as_str());

    let limit: i64 = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(10).clamp(1, 50);

    match state.workspace_service.search_resources(&id, query, resource_type, limit).await {
        Ok(results) => ApiResponseBuilder::success(results),
        Err(e) => {
            tracing::error!("Failed to search resources: {}", e);
            ApiResponseBuilder::error("搜索资源失败")
        }
    }
}

/// Get resource by ID
async fn get_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, resource_id)): Path<(String, String)>,
) -> Json<ApiResponse<WorkspaceResource>> {
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
    }

    match state.workspace_service.find_resource_by_id(&workspace_id, &resource_id).await {
        Ok(Some(resource)) => ApiResponseBuilder::success(resource),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "资源不存在"),
        Err(e) => {
            tracing::error!("Failed to get resource: {}", e);
            ApiResponseBuilder::error("获取资源失败")
        }
    }
}

/// Create resource in workspace
async fn create_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<CreateResourceRequest>,
) -> Json<ApiResponse<WorkspaceResource>> {
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
    }

    let valid_types = ["scene", "device_model", "image", "document"];
    if !valid_types.contains(&payload.resource_type.as_str()) {
        return ApiResponseBuilder::error_with_code(400, "无效的资源类型");
    }

    let sanitized_name =
        payload.name.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");

    let file_path = payload
        .file_path
        .unwrap_or_else(|| format!("{}/{}.bin", payload.resource_type, sanitized_name));

    match state
        .workspace_service
        .create_resource(
            &workspace_id,
            &payload.resource_type,
            &payload.name,
            payload.description.as_deref(),
            &file_path,
            &payload.tags,
            payload.metadata.as_deref(),
        )
        .await
    {
        Ok(resource) => ApiResponseBuilder::success(resource),
        Err(e) => {
            tracing::error!("Failed to create resource: {}", e);
            ApiResponseBuilder::error("创建资源失败")
        }
    }
}

/// Update resource in workspace
async fn update_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, resource_id)): Path<(String, String)>,
    Json(payload): Json<UpdateResourceRequest>,
) -> Json<ApiResponse<WorkspaceResource>> {
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
    }

    match state
        .workspace_service
        .update_resource(
            &workspace_id,
            &resource_id,
            payload.name.as_deref(),
            payload.description.as_deref(),
            payload.tags.as_deref(),
            payload.metadata.as_deref(),
        )
        .await
    {
        Ok(Some(resource)) => ApiResponseBuilder::success(resource),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "资源不存在"),
        Err(e) => {
            tracing::error!("Failed to update resource: {}", e);
            ApiResponseBuilder::error("更新资源失败")
        }
    }
}

/// Delete resource from workspace
async fn delete_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, resource_id)): Path<(String, String)>,
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
    }

    match state.workspace_service.delete_resource(&workspace_id, &resource_id).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!("Failed to delete resource: {}", e);
            ApiResponseBuilder::error("删除资源失败")
        }
    }
}

/// Suggest tags for a resource using AI
async fn suggest_tags(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<SuggestTagsRequest>,
) -> Json<ApiResponse<Vec<String>>> {
    // Verify workspace access
    match _state.workspace_service.find_by_id(&workspace_id).await {
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

    let auth_token = match crate::shared::config::get()
        .minimax
        .as_ref()
        .map(|m| m.auth_token.clone())
    {
        Some(t) => t,
        None => return ApiResponseBuilder::error("AI 服务未配置"),
    };

    let model = crate::shared::config::get()
        .minimax
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "minimax-m2".into());

    let type_label = match payload.resource_type.as_str() {
        "scene" => "3D 场景",
        "device_model" => "设备模型",
        "image" => "图片",
        "document" => "文档",
        other => other,
    };

    let prompt = format!(
        "你是一个资源标签生成助手。根据用户提供的资源信息，生成 3-5 个简洁的中文标签。\n\
         严格只返回逗号分隔的标签，不要任何解释或额外文字。\n\n\
         示例输出：3D模型, 工厂, 设备, 车间\n\n\
         资源信息：\n- 文件名：{}\n- 资源类型：{}{}",
        payload.name,
        type_label,
        payload
            .description
            .as_deref()
            .map_or(String::new(), |d| format!("\n- 描述：{}", d)),
    );

    let provider = match zeroclaw::providers::create_provider("minimaxi", Some(&auth_token)) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create AI provider: {}", e);
            return ApiResponseBuilder::error("AI 服务初始化失败");
        }
    };

    match provider.chat_with_system(None, &prompt, &model, Some(0.3)).await {
        Ok(response) => {
            let tags: Vec<String> = response
                .split([',', '，', '、', '\n'])
                .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|t| !t.is_empty() && t.len() < 20)
                .collect();

            if tags.is_empty() {
                ApiResponseBuilder::error("AI 未生成有效标签")
            } else {
                ApiResponseBuilder::success(tags)
            }
        }
        Err(e) => {
            tracing::error!("AI tag generation failed: {}", e);
            ApiResponseBuilder::error("AI 生成标签失败，请稍后重试")
        }
    }
}
