use tinyiothub_web::response::ApiResponseBuilder;
use crate::dto::entity::role::{CreateRoleRequest, Role, UpdateRoleRequest};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        request::pagination::PaginationQuery,
        response::ApiResponse
    },
};
use crate::shared::security::jwt::Claims;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleQuery {
    pub search: Option<String>,
    pub is_administrator: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
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
    _claims: Claims,
) -> Json<ApiResponse<Vec<Role>>> {
    match state.role_service.find_with_filters(
        None, // enabled parameter not used for roles
        query.search.as_deref(),
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
    _claims: Claims,
    Json(request): Json<CreateRoleRequest>,
) -> Json<ApiResponse<Role>> {
    // 验证输入
    if request.name.trim().is_empty() {
        return ApiResponseBuilder::error("角色名称不能为空".to_string());
    }

    // 检查角色名称是否已存在
    match state.role_service.exists_by_name(&request.name).await {
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
    _claims: Claims,
) -> Json<ApiResponse<Role>> {
    match state.role_service.find_by_id(&id).await {
        Ok(Some(role)) => {
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
    _claims: Claims,
    Json(request): Json<UpdateRoleRequest>,
) -> Json<ApiResponse<Role>> {
    // 验证输入
    if let Some(name) = &request.name {
        if name.trim().is_empty() {
            return ApiResponseBuilder::error("角色名称不能为空".to_string());
        }

        // 检查角色名称是否已被其他角色使用
        match state.role_service.exists_by_name_exclude_id(name, &id).await {
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
        Err(crate::shared::error::Error::NotFound) => ApiResponseBuilder::error("角色不存在".to_string()),
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
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
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
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<String>>> {
    // TODO: 实现获取角色权限逻辑
    tracing::info!("Getting permissions for role: {}", id);

    let permissions = vec![];
    ApiResponseBuilder::success(permissions)
}

/// 更新角色权限
async fn update_role_permissions(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(_permissions): Json<Vec<String>>,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现更新角色权限逻辑
    tracing::info!("Updating permissions for role: {}", id);

    ApiResponseBuilder::success(true)
}
