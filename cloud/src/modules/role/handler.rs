use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::{CreateRoleRequest, Role, UpdateRoleRequest};
use crate::shared::{
    api_response::ApiResponse, app_state::AppState, pagination::PaginationQuery,
    security::jwt::Claims,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleQuery {
    pub search: Option<String>,
    pub is_administrator: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
pub struct UpdateRolePermissionsRequest {
    pub permission_ids: Vec<String>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_roles).post(create_role))
        .route("/{id}", get(get_role).put(update_role).delete(delete_role))
        .route("/{id}/permissions", get(get_role_permissions).put(update_role_permissions))
}

/// 获取角色列表
async fn list_roles(
    State(state): State<AppState>,
    Query(query): Query<RoleQuery>,
    claims: Claims,
) -> Json<ApiResponse<Vec<Role>>> {
    let workspace_id =
        if claims.workspace_id.is_empty() { None } else { Some(claims.workspace_id.as_str()) };

    match state
        .role_service
        .find_with_filters(
            None,
            query.search.as_deref(),
            workspace_id,
            query.pagination.page.unwrap_or(1),
            query.pagination.page_size.unwrap_or(20),
        )
        .await
    {
        Ok(roles) => {
            tracing::debug!("Retrieved {} roles", roles.len());
            ApiResponseBuilder::success(roles)
        }
        Err(e) => {
            tracing::error!("Failed to list roles: {}", e);
            ApiResponseBuilder::error("获取角色列表失败".to_string())
        }
    }
}

/// 创建角色
async fn create_role(
    State(state): State<AppState>,
    claims: Claims,
    Json(mut request): Json<CreateRoleRequest>,
) -> Json<ApiResponse<Role>> {
    // 验证输入
    if request.name.trim().is_empty() {
        return ApiResponseBuilder::error("角色名称不能为空".to_string());
    }

    // 设置 workspace_id
    if !claims.workspace_id.is_empty() {
        request.workspace_id = Some(claims.workspace_id.clone());
    }

    let workspace_id = request.workspace_id.as_deref();

    // 检查角色名称是否已存在
    match state.role_service.exists_by_name(&request.name, workspace_id).await {
        Ok(true) => {
            return ApiResponseBuilder::error("角色名称已存在".to_string());
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check role name existence: {}", e);
            return ApiResponseBuilder::error("创建角色失败".to_string());
        }
    }

    // 创建角色
    match state.role_service.create(&request).await {
        Ok(role) => {
            tracing::info!("Role created: {}", role.name);
            ApiResponseBuilder::success(role)
        }
        Err(e) => {
            tracing::error!("Failed to create role: {}", e);
            ApiResponseBuilder::error("创建角色失败".to_string())
        }
    }
}

/// 获取角色详情
async fn get_role(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<Role>> {
    match state.role_service.find_by_id(&id).await {
        Ok(Some(role)) => {
            // Verify workspace isolation
            if let Some(ref role_ws) = role.workspace_id
                && role_ws != &claims.workspace_id
            {
                return ApiResponseBuilder::error("角色不存在".to_string());
            }
            tracing::debug!("Retrieved role: {}", role.name);
            ApiResponseBuilder::success(role)
        }
        Ok(None) => ApiResponseBuilder::error("角色不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to get role {}: {}", id, e);
            ApiResponseBuilder::error("获取角色信息失败".to_string())
        }
    }
}

/// 更新角色
async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(request): Json<UpdateRoleRequest>,
) -> Json<ApiResponse<Role>> {
    // Verify workspace isolation: get current role first
    match state.role_service.find_by_id(&id).await {
        Ok(Some(ref role)) => {
            if let Some(ref role_ws) = role.workspace_id
                && role_ws != &claims.workspace_id
            {
                return ApiResponseBuilder::error("角色不存在".to_string());
            }
        }
        Ok(None) => return ApiResponseBuilder::error("角色不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to verify role {}: {}", id, e);
            return ApiResponseBuilder::error("更新角色失败".to_string());
        }
    }

    // 验证输入
    if let Some(name) = &request.name {
        if name.trim().is_empty() {
            return ApiResponseBuilder::error("角色名称不能为空".to_string());
        }

        let workspace_id =
            if claims.workspace_id.is_empty() { None } else { Some(claims.workspace_id.as_str()) };

        // 检查角色名称是否已被其他角色使用
        match state.role_service.exists_by_name_exclude_id(name, &id, workspace_id).await {
            Ok(true) => {
                return ApiResponseBuilder::error("角色名称已存在".to_string());
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Failed to check role name uniqueness: {}", e);
                return ApiResponseBuilder::error("更新角色失败".to_string());
            }
        }
    }

    match state.role_service.update(&id, &request).await {
        Ok(role) => {
            tracing::info!("Role updated: {}", role.name);
            ApiResponseBuilder::success(role)
        }
        Err(tinyiothub_core::error::Error::NotFound) => {
            ApiResponseBuilder::error("角色不存在".to_string())
        }
        Err(e) => {
            tracing::error!("Failed to update role {}: {}", id, e);
            ApiResponseBuilder::error("更新角色失败".to_string())
        }
    }
}

/// 删除角色
async fn delete_role(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    // Verify workspace isolation: get current role first
    match state.role_service.find_by_id(&id).await {
        Ok(Some(ref role)) => {
            if let Some(ref role_ws) = role.workspace_id
                && role_ws != &claims.workspace_id
            {
                return ApiResponseBuilder::error("角色不存在".to_string());
            }
        }
        Ok(None) => return ApiResponseBuilder::error("角色不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to verify role {}: {}", id, e);
            return ApiResponseBuilder::error("删除角色失败".to_string());
        }
    }

    match state.role_service.delete(&id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                tracing::info!("Role deleted: {}", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("角色不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete role {}: {}", id, e);
            ApiResponseBuilder::error("删除角色失败".to_string())
        }
    }
}

/// 获取角色权限
async fn get_role_permissions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<Vec<String>>> {
    // Verify workspace isolation: get current role first
    match state.role_service.find_by_id(&id).await {
        Ok(Some(ref role)) => {
            if let Some(ref role_ws) = role.workspace_id
                && role_ws != &claims.workspace_id
            {
                return ApiResponseBuilder::error("角色不存在".to_string());
            }
        }
        Ok(None) => return ApiResponseBuilder::error("角色不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to verify role {}: {}", id, e);
            return ApiResponseBuilder::error("获取角色权限失败".to_string());
        }
    }

    match state.role_service.get_permissions(&id).await {
        Ok(permissions) => ApiResponseBuilder::success(permissions),
        Err(e) => {
            tracing::error!("Failed to get permissions for role {}: {}", id, e);
            ApiResponseBuilder::error("获取角色权限失败".to_string())
        }
    }
}

/// 更新角色权限
async fn update_role_permissions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(request): Json<UpdateRolePermissionsRequest>,
) -> Json<ApiResponse<bool>> {
    // Verify workspace isolation: get current role first
    match state.role_service.find_by_id(&id).await {
        Ok(Some(ref role)) => {
            if let Some(ref role_ws) = role.workspace_id
                && role_ws != &claims.workspace_id
            {
                return ApiResponseBuilder::error("角色不存在".to_string());
            }
        }
        Ok(None) => return ApiResponseBuilder::error("角色不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to verify role {}: {}", id, e);
            return ApiResponseBuilder::error("更新角色权限失败".to_string());
        }
    }

    match state.role_service.update_permissions(&id, &request.permission_ids).await {
        Ok(()) => ApiResponseBuilder::success(true),
        Err(e) => {
            tracing::error!("Failed to update permissions for role {}: {}", id, e);
            ApiResponseBuilder::error("更新角色权限失败".to_string())
        }
    }
}
