use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::Serialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::auth::types::{RefreshTokenResponse, UserInfo},
    shared::{
        api_response::ApiResponse,
        app_state::AppState,
        security::jwt::{Claims, generate_token},
    },
};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionInfo {
    pub user_info: UserInfo,
    pub token_expires_at: i64,
    pub is_valid: bool,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/session/profile", get(get_profile))
        .route("/session/refresh", post(refresh_token))
        .route("/session/validate", get(validate_session))
}

/// 获取当前用户信息
async fn get_profile(State(state): State<AppState>, claims: Claims) -> Json<ApiResponse<UserInfo>> {
    match state.user_service.get_user_by_id(&claims.user_id).await {
        Ok(Some(user)) => {
            tracing::debug!("Retrieved profile for user: {}", user.get_display_name());
            ApiResponseBuilder::success(UserInfo::from(user))
        }
        Ok(None) => {
            tracing::warn!("Profile requested for non-existent user: {}", claims.user_id);
            ApiResponseBuilder::error("用户不存在".to_string())
        }
        Err(e) => {
            tracing::error!("Database error when fetching user profile: {}", e);
            ApiResponseBuilder::error("获取用户信息失败".to_string())
        }
    }
}

/// 刷新访问令牌
async fn refresh_token(claims: Claims) -> Json<ApiResponse<RefreshTokenResponse>> {
    match generate_token(&claims.user_id, &claims.username, &claims.tenant_id, &claims.workspace_id)
    {
        Ok(new_token) => {
            tracing::info!("Token refreshed for user: {}", claims.user_id);
            ApiResponseBuilder::success(RefreshTokenResponse {
                access_token: new_token,
                token_type: "Bearer".to_string(),
                expires_in: 24 * 60 * 60,
            })
        }
        Err(e) => {
            tracing::error!("Failed to generate new token: {}", e);
            ApiResponseBuilder::error("刷新令牌失败".to_string())
        }
    }
}

/// 验证会话有效性
async fn validate_session(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<SessionInfo>> {
    // 检查用户是否仍然存在且未被禁用
    match state.user_service.get_user_by_id(&claims.user_id).await {
        Ok(Some(user)) => {
            // 从 JWT claims 中获取真实的过期时间
            let token_expires_at = claims.exp.unwrap_or_else(|| {
                // 如果没有 exp（理论上不应该发生），返回当前时间 + 1小时
                chrono::Local::now().timestamp() + 3600
            });

            let session_info = SessionInfo {
                user_info: UserInfo::from(user.clone()),
                token_expires_at,
                is_valid: user.is_enabled(),
            };

            if user.is_enabled() {
                tracing::debug!("Session validated for user: {}", user.get_display_name());
                ApiResponseBuilder::success(session_info)
            } else {
                tracing::warn!(
                    "Session validation failed - user disabled: {}",
                    user.get_display_name()
                );
                ApiResponseBuilder::error("用户账户已被禁用".to_string())
            }
        }
        Ok(None) => {
            tracing::warn!("Session validation failed - user not found: {}", claims.user_id);
            ApiResponseBuilder::error("用户不存在".to_string())
        }
        Err(e) => {
            tracing::error!("Database error during session validation: {}", e);
            ApiResponseBuilder::error("会话验证失败".to_string())
        }
    }
}
