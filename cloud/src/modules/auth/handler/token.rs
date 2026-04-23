// Token 刷新模块
// 支持 Token 刷新和黑名单

use tinyiothub_web::response::ApiResponseBuilder;
use axum::{extract::State, response::Json, routing::post, Router};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{
    shared::app_state::AppState,
    shared::api_response::ApiResponse,
    shared::security::jwt::{generate_token, validate_jwt},
};

pub fn create_router() -> Router<AppState> {
    Router::new().route("/refresh", post(refresh_token)).route("/logout", post(logout))
}

/// 刷新 Token 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshTokenRequest {
    pub token: String,
}

/// 登出请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogoutRequest {
    pub token: Option<String>,
}

/// Token 刷新响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// 刷新 Token
async fn refresh_token(
    State(_state): State<AppState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Json<ApiResponse<RefreshTokenResponse>> {
    // 验证当前 token
    let claims = match validate_jwt(&request.token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Token refresh failed: {}", e);
            return ApiResponseBuilder::error("Invalid or expired token".to_string());
        }
    };

    // 生成新的 token
    match generate_token(&claims.user_id, &claims.username, &claims.tenant_id) {
        Ok(new_token) => {
            tracing::info!("Token refreshed for user: {}", claims.user_id);
            ApiResponseBuilder::success(RefreshTokenResponse {
                access_token: new_token,
                token_type: "Bearer".to_string(),
                expires_in: 86400, // 24 小时
            })
        }
        Err(e) => {
            tracing::error!("Failed to generate new token: {}", e);
            ApiResponseBuilder::error("Failed to refresh token".to_string())
        }
    }
}

/// 登出（将 token 加入黑名单）
async fn logout(
    State(state): State<AppState>,
    Json(request): Json<LogoutRequest>,
) -> Json<ApiResponse<String>> {
    if let Some(token) = request.token {
        // 将 token 加入黑名单
        let db = state.database();

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let expires_at = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(1))
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        // Store hashed token in blacklist
        use sha2::Sha256;
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));

        let result = sqlx::query(
            "INSERT INTO token_blacklist (id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&token_hash)
        .bind(&expires_at)
        .bind(&now)
        .execute(db.pool())
        .await;

        match result {
            Ok(_) => {
                tracing::info!("Token added to blacklist");
            }
            Err(e) => {
                // 表可能不存在，但不影响登出流程
                tracing::warn!("Failed to add token to blacklist: {}", e);
            }
        }
    }

    ApiResponseBuilder::success("Logged out successfully".to_string())
}
