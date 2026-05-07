use tinyiothub_web::response::ApiResponseBuilder;
use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;

use crate::{
    shared::app_state::AppState,
    modules::auth::types::{LoginResponse, UserInfo},
    modules::user::types::CreateUserRequest,
    shared::api_response::ApiResponse,
    shared::security::jwt,
    shared::utils::validation,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogoutRequest {
    pub token: Option<String>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/logout", post(logout))
}

/// 用户注册（公开接口）
async fn register(
    State(state): State<AppState>,
    Json(request): Json<CreateUserRequest>,
) -> Json<ApiResponse<LoginResponse>> {
    let username = request.username.trim();
    let password = request.password.clone();
    let phone = request.phone.as_ref().map(|p| p.trim().to_string());
    let email = request.email.as_ref().map(|e| e.trim().to_string());

    // 用户名校验
    if username.is_empty() {
        return ApiResponseBuilder::error("用户名不能为空".to_string());
    }
    if !validation::is_valid_username(username) {
        return ApiResponseBuilder::error("用户名 3-32 字符，仅限字母、数字、下划线".to_string());
    }

    // 手机号校验（必填）
    let Some(ref phone) = phone else {
        return ApiResponseBuilder::error("请输入手机号".to_string());
    };
    if !validation::is_valid_phone(phone) {
        return ApiResponseBuilder::error("请输入正确的中国大陆手机号".to_string());
    }

    // 邮箱校验（选填，但提供时必须合法）
    if let Some(ref email) = email {
        if !validation::is_valid_email(email) {
            return ApiResponseBuilder::error("邮箱格式不正确".to_string());
        }
    }

    // 密码策略校验
    match validation::validate_password_policy(&password) {
        Err(validation::PasswordPolicyError::TooShort) => {
            return ApiResponseBuilder::error("密码至少 8 个字符".to_string());
        }
        Err(validation::PasswordPolicyError::HasWhitespace) => {
            return ApiResponseBuilder::error("密码不能包含空格".to_string());
        }
        Err(validation::PasswordPolicyError::NoLetter) => {
            return ApiResponseBuilder::error("密码必须包含字母".to_string());
        }
        Err(validation::PasswordPolicyError::NoDigit) => {
            return ApiResponseBuilder::error("密码必须包含数字".to_string());
        }
        Ok(()) => {}
    }

    // 检查用户名是否已存在
    match state.user_service.exists_by_username(username).await {
        Ok(true) => return ApiResponseBuilder::error("用户名已存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to check username existence: {}", e);
            return ApiResponseBuilder::error("注册失败，请稍后重试".to_string());
        }
        _ => {}
    }

    // 检查手机号是否已注册
    match state.user_service.exists_by_phone(phone).await {
        Ok(true) => return ApiResponseBuilder::error("手机号已注册".to_string()),
        Err(e) => {
            tracing::error!("Failed to check phone existence: {}", e);
            return ApiResponseBuilder::error("注册失败，请稍后重试".to_string());
        }
        _ => {}
    }

    // 检查邮箱是否已注册（若提供了邮箱）
    if let Some(ref email) = email {
        match state.user_service.exists_by_email(email).await {
            Ok(true) => return ApiResponseBuilder::error("邮箱已注册".to_string()),
            Err(e) => {
                tracing::error!("Failed to check email existence: {}", e);
                return ApiResponseBuilder::error("注册失败，请稍后重试".to_string());
            }
            _ => {}
        }
    }

    // 构造创建请求，确保 phone / display_name 正确填充
    let create_request = CreateUserRequest {
        username: username.to_string(),
        password,
        phone: Some(phone.to_string()),
        email: email.clone(),
        display_name: request.display_name.clone().or_else(|| Some(username.to_string())),
        is_enabled: request.is_enabled,
        parent_id: request.parent_id.clone(),
    };

    // 创建用户（依赖数据库 UNIQUE 约束兜底竞态条件）
    let user = match state.user_service.create_user(&create_request).await {
        Ok(u) => u,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed") {
                if msg.contains("users.username") {
                    return ApiResponseBuilder::error("用户名已存在".to_string());
                }
                if msg.contains("users.phone") {
                    return ApiResponseBuilder::error("手机号已注册".to_string());
                }
                if msg.contains("users.email") {
                    return ApiResponseBuilder::error("邮箱已注册".to_string());
                }
            }
            tracing::error!("Failed to create user: {}", e);
            return ApiResponseBuilder::error("注册失败，请稍后重试".to_string());
        }
    };

    // 确保新用户关联到默认租户和工作空间（幂等）
    if let Err(e) = crate::modules::system::handler::ensure_user_has_workspace(&state, &user.id).await {
        tracing::warn!("[REGISTER] Failed to ensure workspace for user {}: {}", user.id, e);
    }

    // 注册后自动登录 — 查找租户和用户自己的 workspace
    let (tenant_id, workspace_id) = match resolve_user_login_context(&state, &user.id).await {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!("[REGISTER] Failed to resolve user context: {}", e);
            return ApiResponseBuilder::error("注册成功，但登录失败".to_string());
        }
    };

    let workspace_id_for_token = workspace_id.clone().unwrap_or_default();
    let token = match jwt::generate_token(&user.id, user.get_display_name(), &tenant_id, &workspace_id_for_token) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to generate token: {}", e);
            return ApiResponseBuilder::error("注册成功，但登录失败".to_string());
        }
    };

    ApiResponseBuilder::success(LoginResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60,
        user_info: UserInfo::from(user),
        workspace_id,
    })
}

/// 用户登录
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Json<ApiResponse<LoginResponse>> {
    tracing::info!("Login attempt for user: {}", request.username);

    // 验证输入参数
    if request.username.trim().is_empty() || request.password.trim().is_empty() {
        return ApiResponseBuilder::error("用户名和密码不能为空".to_string());
    }

    tracing::debug!("Authenticating user: {}", request.username);

    // 验证用户凭据
    match state.user_service.authenticate(&request.username, &request.password).await {
        Ok(Some(user)) => {
            tracing::debug!("User authenticated: {}", user.id);

            // 检查用户是否被禁用
            if !user.is_enabled() {
                return ApiResponseBuilder::error("用户账户已被禁用".to_string());
            }

            tracing::debug!("Updating last login time for user: {}", user.id);

            // Skip database write on HarmonyOS (causes Signal 11)
            if !crate::shared::config::get().harmonyos.enabled {
                // 更新最后登录时间
                if let Err(e) = state.user_service.update_last_login(&user.id).await {
                    tracing::warn!("Failed to update last logon time for user {}: {}", user.id, e);
                }
            } else {
                tracing::warn!(
                    "⚠️  Skipping last login update on HarmonyOS (database write causes Signal 11)"
                );
            }

            tracing::debug!("Generating JWT token for user: {}", user.id);

            let (tenant_id, workspace_id) = match resolve_user_login_context(&state, &user.id).await {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("Failed to resolve user login context: {}", e);
                    return ApiResponseBuilder::error("登录失败，请稍后重试".to_string());
                }
            };

            tracing::debug!(
                "Found tenant_id {} workspace_id {:?} for user: {}",
                tenant_id,
                workspace_id,
                user.id
            );

            // 生成 JWT 令牌（HarmonyOS 会自动使用 HMAC-SHA256）
            let workspace_id_for_token = workspace_id.clone().unwrap_or_default();
            match jwt::generate_token(&user.id, user.get_display_name(), &tenant_id, &workspace_id_for_token) {
                Ok(token) => {
                    let login_response = LoginResponse {
                        access_token: token,
                        token_type: "Bearer".to_string(),
                        expires_in: 24 * 60 * 60, // 24小时
                        user_info: UserInfo::from(user),
                        workspace_id,
                    };

                    tracing::info!("User {} logged in successfully", request.username);
                    ApiResponseBuilder::success(login_response)
                }
                Err(e) => {
                    tracing::error!("Failed to generate JWT token: {}", e);
                    ApiResponseBuilder::error("登录失败，请稍后重试".to_string())
                }
            }
        }
        Ok(None) => {
            tracing::warn!(
                "Login attempt with invalid credentials for username: {}",
                request.username
            );
            ApiResponseBuilder::error("用户名或密码错误".to_string())
        }
        Err(e) => {
            tracing::error!("Database error during login: {}", e);
            ApiResponseBuilder::error("登录失败，请稍后重试".to_string())
        }
    }
}

/// 用户登出
async fn logout(
    State(_state): State<AppState>,
    Json(_request): Json<LogoutRequest>,
) -> Json<ApiResponse<String>> {
    // 在实际应用中，这里可能需要将 token 加入黑名单
    // 目前只是返回成功响应
    tracing::info!("User logged out");
    ApiResponseBuilder::success("登出成功".to_string())
}

/// 查询用户的 tenant_id 和 workspace_id（注册/登录后使用）
///
/// 返回 (tenant_id, workspace_id)。tenant_id 缺省为 "default"；
/// workspace_id 优先取用户自己的 ws-{user_id}，否则取租户下第一个。
async fn resolve_user_login_context(
    state: &AppState,
    user_id: &str,
) -> Result<(String, Option<String>), crate::shared::error::Error> {
    let pool = state.database().pool();

    let tenant_id: Option<String> = sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_users WHERE user_id = ? LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    let tenant_id = tenant_id.unwrap_or_else(|| "default".to_string());

    let user_ws_id = format!("ws-{}", user_id);
    let workspace_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? UNION ALL SELECT id FROM workspaces WHERE tenant_id = ? AND id != ? LIMIT 1"
    )
    .bind(&user_ws_id)
    .bind(&tenant_id)
    .bind(&user_ws_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    Ok((tenant_id, workspace_id))
}
