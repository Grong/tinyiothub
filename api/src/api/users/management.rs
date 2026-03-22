use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        entity::user::{CreateUserRequest, UpdateUserRequest, User, UserDto, UserStatisticsNew},
        request::pagination::PaginationQuery,
        response::ApiResponse,
    },
    shared::security::jwt::Claims,
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
        .route("/:id", get(get_user).put(update_user).delete(delete_user))
        .route("/:id/enable", post(enable_user))
        .route("/:id/disable", post(disable_user))
        .route("/:id/password", put(change_user_password))
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
) -> Json<ApiResponse<Vec<UserDto>>> {
    tracing::info!("list_users called with query: {:?}", query);

    match User::find_with_filters(
        state.database(),
        query.enabled,
        query.search,
        Some(query.pagination.page.unwrap_or(1)),
        Some(query.pagination.page_size.unwrap_or(20)),
    )
    .await
    {
        Ok(users) => {
            let user_dtos = User::to_dto_list(users);
            tracing::info!("Retrieved {} users", user_dtos.len());
            ApiResponse::success(user_dtos)
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
    match User::exists_by_username(state.database(), &request.username).await {
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
    match User::create(state.database(), &request).await {
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
) -> Json<ApiResponse<UserStatisticsNew>> {
    match User::get_user_statistics(state.database()).await {
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
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<UserDto>> {
    match User::find_by_id(state.database(), &id).await {
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
    _claims: Claims,
    Json(request): Json<UpdateUserRequest>,
) -> Json<ApiResponse<UserDto>> {
    // 验证输入
    if let Some(username) = &request.username {
        if username.trim().is_empty() {
            return ApiResponse::error("用户名不能为空".to_string());
        }

        // 检查用户名是否已被其他用户使用
        match User::find_by_username(state.database(), username).await {
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

    match User::update(state.database(), &id, &request).await {
        Ok(user) => {
            tracing::info!("User updated: {}", user.get_display_name());
            ApiResponse::success(user.to_dto())
        }
        Err(sqlx::Error::RowNotFound) => ApiResponse::error("用户不存在".to_string()),
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
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match User::delete(state.database(), &id).await {
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
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match User::update_enabled_status(state.database(), &id, true).await {
        Ok(_user) => {
            tracing::info!("User enabled: {}", id);
            ApiResponse::success(true)
        }
        Err(sqlx::Error::RowNotFound) => ApiResponse::error("用户不存在".to_string()),
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
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match User::update_enabled_status(state.database(), &id, false).await {
        Ok(_user) => {
            tracing::info!("User disabled: {}", id);
            ApiResponse::success(true)
        }
        Err(sqlx::Error::RowNotFound) => ApiResponse::error("用户不存在".to_string()),
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
    let user = match User::find_by_id(state.database(), &id).await {
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
    if !user.verify_password(&request.old_password) {
        return ApiResponse::error("旧密码错误".to_string());
    }

    // 更新密码
    match User::update_password(state.database(), &id, &request.new_password).await {
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
