use crate::dto::entity::permission::{Permission, PermissionQuery};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router
};
use serde::Serialize;

use crate::{
    api::AppState,
    dto::{
        response::{ApiResponse, ApiResponseBuilder}
    },
    shared::security::jwt::Claims
};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UserPermission {
    pub permission_id: String,
    pub permission_name: String,
    pub resource: String,
    pub action: String,
    pub granted_by_role: bool,
    pub granted_directly: bool,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_permissions))
        .route("/{id}/permissions", get(get_user_permissions))
}

/// 获取所有权限列表
async fn list_permissions(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<Permission>>> {
    let query = PermissionQuery::default();
    match state.permission_service.find_all_permissions(&query).await {
        Ok(permissions) => ApiResponseBuilder::success(permissions),
        Err(e) => {
            tracing::error!("Failed to list permissions: {}", e);
            ApiResponseBuilder::error("获取权限列表失败".to_string())
        }
    }
}

/// 获取用户权限
async fn get_user_permissions(
    State(_state): State<AppState>,
    Path(user_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<UserPermission>>> {
    // TODO: 实现获取用户权限逻辑
    // 需要查询用户直接权限和通过角色获得的权限
    tracing::info!("Getting permissions for user: {}", user_id);

    let permissions = vec![];
    ApiResponseBuilder::success(permissions)
}
