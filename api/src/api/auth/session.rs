use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::AppState,
    dto::{
        entity::user::User,
        response::{
            login::{RefreshTokenResponse, UserInfo},
            ApiResponse,
        },
    },
    shared::security::jwt::Claims,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshRequest {
    pub refresh_token: String,
}

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
    match User::find_by_id(state.database(), &claims.user_id).await {
        Ok(Some(user)) => {
            tracing::debug!("Retrieved profile for user: {}", user.get_display_name());
            ApiResponse::success(UserInfo::from(user))
        }
        Ok(None) => {
            tracing::warn!(
                "Profile requested for non-existent user: {}",
                claims.user_id
            );
            ApiResponse::error("用户不存在".to_string())
        }
        Err(e) => {
            tracing::error!("Database error when fetching user profile: {}", e);
            ApiResponse::error("获取用户信息失败".to_string())
        }
    }
}

/// 刷新访问令牌
async fn refresh_token(
    State(_state): State<AppState>,
    Json(_request): Json<RefreshRequest>,
) -> Json<ApiResponse<RefreshTokenResponse>> {
    // TODO: 实现刷新令牌逻辑
    // 在实际应用中，需要：
    // 1. 验证 refresh_token 的有效性
    // 2. 检查用户状态
    // 3. 生成新的 access_token
    // 4. 可选：生成新的 refresh_token

    tracing::info!("Token refresh requested");
    ApiResponse::error("刷新令牌功能暂未实现".to_string())
}

/// 验证会话有效性
async fn validate_session(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<SessionInfo>> {
    // 检查用户是否仍然存在且未被禁用
    match User::find_by_id(state.database(), &claims.user_id).await {
        Ok(Some(user)) => {
            // 从 JWT claims 中获取真实的过期时间
            let token_expires_at = claims.exp.unwrap_or_else(|| {
                // 如果没有 exp（理论上不应该发生），返回当前时间 + 1小时
                chrono::Local::now().timestamp() + 3600
            });

            let session_info = SessionInfo {
                user_info: UserInfo::from(user.clone()),
                token_expires_at,
                is_valid: user.enabled(),
            };

            if user.enabled() {
                tracing::debug!("Session validated for user: {}", user.get_display_name());
                ApiResponse::success(session_info)
            } else {
                tracing::warn!(
                    "Session validation failed - user disabled: {}",
                    user.get_display_name()
                );
                ApiResponse::error("用户账户已被禁用".to_string())
            }
        }
        Ok(None) => {
            tracing::warn!(
                "Session validation failed - user not found: {}",
                claims.user_id
            );
            ApiResponse::error("用户不存在".to_string())
        }
        Err(e) => {
            tracing::error!("Database error during session validation: {}", e);
            ApiResponse::error("会话验证失败".to_string())
        }
    }
}
