// Token 刷新模块
// 支持 Token 刷新、登出和 SSE token 生成

use crate::shared::app_state::AppState;
use axum::{Router, extract::State, response::Json, routing::post};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tinyiothub_web::{api_response::ApiResponse, response::ApiResponseBuilder};

use tinyiothub_web::security::Claims;

/// 创建不受 JWT middleware 保护的路由（login, logout, refresh）
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout))
}

/// 创建受 JWT middleware 保护的路由（需要已验证的 Claims）
pub fn create_protected_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new().route("/sse-token", post(generate_sse_token))
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

/// SSE token 响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SseTokenResponse {
    pub token: String,
    pub expires_in_seconds: u64,
}

/// 刷新 Token
async fn refresh_token(
    State(state): State<AppState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Json<ApiResponse<RefreshTokenResponse>> {
    // 验证当前 token
    let claims = match state.jwt_service.validate_jwt(&request.token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Token refresh failed: {}", e);
            return ApiResponseBuilder::error("Invalid or expired token".to_string());
        }
    };

    // 生成新的 token
    match state.jwt_service.generate_token(
        &claims.user_id,
        &claims.username,
        &claims.tenant_id,
        &claims.workspace_id,
    ) {
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

/// 生成用于 SSE 连接的短期 token（受 JWT middleware 保护）
///
/// SSE 使用 EventSource API 无法设置自定义 HTTP headers，因此 JWT
/// 通过 URL 查询参数传递会导致 token 泄露到日志中。这个端点返回
/// 一个短期（5分钟）、一次性使用的 token，在 SSE 连接中使用。
async fn generate_sse_token(State(state): State<AppState>, claims: Claims) -> Json<ApiResponse<SseTokenResponse>> {
    let user_id = claims.user_id;
    let workspace_id = claims.workspace_id;

    // 生成短期 SSE token
    let token = state.sse_token_manager.generate_token(&user_id, &workspace_id);

    tracing::debug!("SSE token generated for user: {} workspace: {}", user_id, workspace_id);

    ApiResponseBuilder::success(SseTokenResponse {
        token,
        expires_in_seconds: 300, // 5分钟
    })
}

/// 登出（将 token 加入黑名单）
async fn logout(State(state): State<AppState>, Json(request): Json<LogoutRequest>) -> Json<ApiResponse<String>> {
    if let Some(token) = request.token {
        // 将 token 加入黑名单
        let db = &state.database;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let expires_at = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(1))
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        // Store hashed token in blacklist
        use sha2::Sha256;
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));

        let result =
            sqlx::query("INSERT INTO token_blacklist (id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?)")
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
