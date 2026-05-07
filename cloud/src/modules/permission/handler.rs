use tinyiothub_web::response::ApiResponseBuilder;
use super::types::{Permission, PermissionQuery};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::{
    shared::api_response::ApiResponse,
    shared::app_state::AppState,
};
use crate::shared::security::jwt::Claims;

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
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<UserPermission>>> {
    let mut user_permissions: Vec<UserPermission> = vec![];

    match state.role_service.find_roles_by_user_id(&user_id).await {
        Ok(roles) => {
            for role in roles {
                match state.role_service.get_permissions(&role.id).await {
                    Ok(permission_ids) => {
                        for permission_id in permission_ids {
                            if let Ok(Some(permission)) = state.permission_service.find_permission_by_id(&permission_id).await {
                                user_permissions.push(UserPermission {
                                    permission_id: permission.id.clone(),
                                    permission_name: permission.name.clone(),
                                    resource: permission.resource_type.clone(),
                                    action: permission.action_type.clone(),
                                    granted_by_role: true,
                                    granted_directly: false,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get permissions for role {}: {}", role.id, e);
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get roles for user {}: {}", user_id, e);
        }
    }

    tracing::info!("Retrieved {} permissions for user: {}", user_permissions.len(), user_id);
    ApiResponseBuilder::success(user_permissions)
}
