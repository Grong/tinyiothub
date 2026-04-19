use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        entity::user::{CreateUserRequest, UpdateUserRequest, UserDto},
        request::pagination::PaginationQuery,
        response::{ApiResponse, PaginatedResponse, PaginationInfo},
    },
    shared::security::jwt::Claims,
    shared::utils::password::verify_password,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserQuery {
    pub enabled: Option<bool>,
    pub search: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PasswordChangeRequest {
    pub old_password: String,
    pub new_password: String,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/test", get(test_users_endpoint))
        .route("/statistics", get(get_user_statistics))
        .route("/me", get(get_current_user))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
        .route("/{id}/enable", post(enable_user))
        .route("/{id}/disable", post(disable_user))
        .route("/{id}/password", put(change_user_password))
}

/// 测试用户端点
async fn test_users_endpoint() -> &'static str {
    "Users module is working!"
}

/// 获取用户列表
async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<UserQuery>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<UserDto>>> {
    tracing::info!("list_users called with query: {:?}", query);

    let page = query.pagination.page.unwrap_or(1);
    let page_size = query.pagination.page_size.unwrap_or(20);

    match state.user_service.list_users(query.enabled, query.search, page, page_size).await {
        Ok((users, total)) => {
            let user_dtos = crate::dto::entity::user::User::to_dto_list(users);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            tracing::info!("Retrieved {} users", user_dtos.len());
            ApiResponse::success(PaginatedResponse {
                data: user_dtos,
                pagination: PaginationInfo {
                    page,
                    page_size,
                    total_pages,
                    total_count,
                },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list users: {}", e);
            ApiResponse::error("获取用户列表失败".to_string())
        }
    }
}

/// 验证密码强度
fn validate_password_strength(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("密码长度不能少于8位".to_string());
    }

    if password.len() > 128 {
        return Err("密码长度不能超过128位".to_string());
    }

    // 检查是否包含数字
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    // 检查是否包含字母
    let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());

    if !has_digit || !has_letter {
        return Err("密码必须包含字母和数字".to_string());
    }

    Ok(())
}

/// 创建用户
async fn create_user(
    State(state): State<AppState>,
    _claims: Claims,
    Json(request): Json<CreateUserRequest>,
) -> Json<ApiResponse<UserDto>> {
    // 验证输入
    if request.username.trim().is_empty() {
        return ApiResponse::error("用户名不能为空".to_string());
    }

    // 验证密码强度
    if let Err(err) = validate_password_strength(&request.password) {
        return ApiResponse::error(err);
    }

    // 检查用户名是否已存在
    match state.user_service.exists_by_username(&request.username).await {
        Ok(true) => {
            return ApiResponse::error("用户名已存在".to_string());
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check user existence: {}", e);
            return ApiResponse::error("创建用户失败".to_string());
        }
    }

    // 创建用户
    match state.user_service.create_user(&request).await {
        Ok(user) => {
            tracing::info!("User created: {}", user.get_display_name());
            ApiResponse::success(user.to_dto())
        }
        Err(e) => {
            tracing::error!("Failed to create user: {}", e);
            ApiResponse::error("创建用户失败".to_string())
        }
    }
}

/// 获取用户统计信息
async fn get_user_statistics(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<crate::dto::entity::user::UserStatisticsNew>> {
    match state.user_service.get_user_statistics().await {
        Ok(statistics) => {
            tracing::debug!("Retrieved user statistics");
            ApiResponse::success(statistics)
        }
        Err(e) => {
            tracing::error!("Failed to get user statistics: {}", e);
            ApiResponse::error("获取用户统计失败".to_string())
        }
    }
}

/// 获取用户详情
async fn get_current_user(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<UserDto>> {
    match state.user_service.get_user_by_id(&claims.user_id).await {
        Ok(Some(user)) => ApiResponse::success(user.to_dto()),
        Ok(None) => ApiResponse::error("用户不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to get current user {}: {}", claims.user_id, e);
            ApiResponse::error("获取用户信息失败".to_string())
        }
    }
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<UserDto>> {
    // 用户只能查看自己的信息（users 表无 tenant_id，暂以 user_id 限制）
    if claims.user_id != id {
        tracing::warn!(
            "User {} attempted to access user {} without permission",
            claims.user_id,
            id
        );
        return ApiResponse::error("无权限查看此用户".to_string());
    }

    match state.user_service.get_user_by_id(&id).await {
        Ok(Some(user)) => {
            tracing::debug!("Retrieved user: {}", user.get_display_name());
            ApiResponse::success(user.to_dto())
        }
        Ok(None) => ApiResponse::error("用户不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to get user {}: {}", id, e);
            ApiResponse::error("获取用户信息失败".to_string())
        }
    }
}

/// 更新用户
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(request): Json<UpdateUserRequest>,
) -> Json<ApiResponse<UserDto>> {
    // 用户只能修改自己的信息（users 表无 tenant_id，暂以 user_id 限制）
    let is_admin =
        crate::shared::error_handling::AuthHelper::check_role(&state, &claims.user_id, "admin")
            .await
            .unwrap_or(false);
    if claims.user_id != id && !is_admin {
        tracing::warn!(
            "User {} attempted to update user {} without permission",
            claims.user_id,
            id
        );
        return ApiResponse::error("无权限修改此用户".to_string());
    }

    // 验证输入
    if let Some(username) = &request.username {
        if username.trim().is_empty() {
            return ApiResponse::error("用户名不能为空".to_string());
        }

        // 检查用户名是否已被其他用户使用
        match state.user_service.get_user_by_username(username).await {
            Ok(Some(existing_user)) if existing_user.id != id => {
                return ApiResponse::error("用户名已存在".to_string());
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Failed to check user name uniqueness: {}", e);
                return ApiResponse::error("更新用户失败".to_string());
            }
        }
    }

    match state.user_service.update_user(&id, &request).await {
        Ok(user) => {
            tracing::info!("User updated: {}", user.get_display_name());
            ApiResponse::success(user.to_dto())
        }
        Err(crate::shared::error::Error::NotFound) => ApiResponse::error("用户不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to update user {}: {}", id, e);
            ApiResponse::error("更新用户失败".to_string())
        }
    }
}

/// 删除用户
async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    // 用户不能删除自己，且需要管理员权限
    if claims.user_id == id {
        return ApiResponse::error("不能删除自己的账号".to_string());
    }
    let is_admin =
        crate::shared::error_handling::AuthHelper::check_role(&state, &claims.user_id, "admin")
            .await
            .unwrap_or(false);
    if !is_admin {
        tracing::warn!(
            "User {} attempted to delete user {} without admin permission",
            claims.user_id,
            id
        );
        return ApiResponse::error("无权限删除用户".to_string());
    }

    match state.user_service.delete_user(&id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                tracing::info!("User deleted: {}", id);
                ApiResponse::success(true)
            } else {
                ApiResponse::error("用户不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete user {}: {}", id, e);
            ApiResponse::error("删除用户失败".to_string())
        }
    }
}

/// 启用用户
async fn enable_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    let is_admin =
        crate::shared::error_handling::AuthHelper::check_role(&state, &claims.user_id, "admin")
            .await
            .unwrap_or(false);
    if !is_admin {
        return ApiResponse::error("无权限启用用户".to_string());
    }
    match state.user_service.update_enabled_status(&id, true).await {
        Ok(_user) => {
            tracing::info!("User enabled: {}", id);
            ApiResponse::success(true)
        }
        Err(crate::shared::error::Error::NotFound) => ApiResponse::error("用户不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to enable user {}: {}", id, e);
            ApiResponse::error("启用用户失败".to_string())
        }
    }
}

/// 禁用用户
async fn disable_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    let is_admin =
        crate::shared::error_handling::AuthHelper::check_role(&state, &claims.user_id, "admin")
            .await
            .unwrap_or(false);
    if !is_admin {
        return ApiResponse::error("无权限禁用用户".to_string());
    }
    match state.user_service.update_enabled_status(&id, false).await {
        Ok(_user) => {
            tracing::info!("User disabled: {}", id);
            ApiResponse::success(true)
        }
        Err(crate::shared::error::Error::NotFound) => ApiResponse::error("用户不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to disable user {}: {}", id, e);
            ApiResponse::error("禁用用户失败".to_string())
        }
    }
}

/// 修改用户密码
async fn change_user_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(request): Json<PasswordChangeRequest>,
) -> Json<ApiResponse<bool>> {
    // 验证新密码强度
    if let Err(err) = validate_password_strength(&request.new_password) {
        return ApiResponse::error(err);
    }

    if request.old_password == request.new_password {
        return ApiResponse::error("新密码不能与旧密码相同".to_string());
    }

    // 授权检查：用户只能修改自己的密码，或者需要管理员权限
    let requesting_user_id = &claims.user_id;
    let is_admin =
        crate::shared::error_handling::AuthHelper::check_role(&state, requesting_user_id, "admin")
            .await
            .unwrap_or(false);

    if requesting_user_id != &id && !is_admin {
        tracing::warn!(
            "User {} attempted to change password for user {} without permission",
            requesting_user_id,
            id
        );
        return ApiResponse::error("无权限修改此用户密码".to_string());
    }

    // 获取用户信息
    let user = match state.user_service.get_user_by_id(&id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return ApiResponse::error("用户不存在".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to find user {}: {}", id, e);
            return ApiResponse::error("修改密码失败".to_string());
        }
    };

    // 验证旧密码
    match verify_password(&request.old_password, &user.password_hash) {
        Ok(true) => {}
        Ok(false) => {
            return ApiResponse::error("旧密码错误".to_string());
        }
        Err(e) => {
            tracing::error!("Password verification failed for user {}: {}", id, e);
            return ApiResponse::error("密码验证失败".to_string());
        }
    }

    // 更新密码
    match state.user_service.update_password(&id, &request.new_password).await {
        Ok(()) => {
            tracing::info!("Password changed for user: {}", id);
            ApiResponse::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to update password for user {}: {}", id, e);
            ApiResponse::error("修改密码失败".to_string())
        }
    }
}
