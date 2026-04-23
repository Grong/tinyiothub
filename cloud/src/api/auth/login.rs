use tinyiothub_web::response::ApiResponseBuilder;
use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        response::{
            login::{LoginResponse, UserInfo},
            ApiResponse,
        },
    },
    shared::security::jwt,
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
    Router::new().route("/login", post(login)).route("/logout", post(logout))
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
            if !crate::infrastructure::config::get().harmonyos.enabled {
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

            // 查找用户关联的租户，默认为 "default"
            let tenant_id: String = sqlx::query_scalar(
                "SELECT tenant_id FROM tenant_users WHERE user_id = ? LIMIT 1"
            )
            .bind(&user.id)
            .fetch_optional(state.database().pool())
            .await
            .map_err(|e| {
                tracing::error!("DB error fetching tenant_id: {}", e);
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| "default".to_string());

            tracing::debug!(
                "Found tenant_id {} for user: {}",
                tenant_id,
                user.id
            );

            // 查找该租户的第一个 workspace 作为默认 workspace
            let workspace_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM workspaces WHERE tenant_id = ? LIMIT 1"
            )
            .bind(&tenant_id)
            .fetch_optional(state.database().pool())
            .await
            .map_err(|e| {
                tracing::error!("DB error fetching workspace_id: {}", e);
            })
            .ok()
            .flatten();

            tracing::debug!(
                "Found default workspace_id {:?} for tenant {}",
                workspace_id,
                tenant_id
            );

            // 生成 JWT 令牌（HarmonyOS 会自动使用 HMAC-SHA256）
            match jwt::generate_token(&user.id, user.get_display_name(), &tenant_id) {
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
