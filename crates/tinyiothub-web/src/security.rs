use std::sync::OnceLock;

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use headers::{Authorization, HeaderMapExt, authorization::Bearer};
use serde::{Deserialize, Serialize};

/// JWT Claims 结构体 — 从认证 token 中提取的用户身份
///
/// 由认证中间件填充后通过 Axum extensions 传递到 handler。
/// `exp` 字段不参与 JWT 序列化，仅在验证后填充供业务层使用。
/// 注意：这个通用Claims结构体不包含租户信息，符合"零租户污染"原则。
/// 具体的SaaS实现应在cloud crate中扩展此结构体。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: String,
    pub token_id: String,
    pub username: String,
    /// 从 JWT 验证结果中提取的过期时间（不参与序列化到 JWT）
    #[serde(skip_serializing)]
    pub exp: Option<i64>,
}

/// JWT token payload — 创建 token 时使用的数据
#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub id: String,
    pub name: String,
}

/// 认证响应体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthBody {
    pub token: String,
    pub token_type: String,
    pub exp: i64,
    pub expired: i64,
}

impl AuthBody {
    pub fn new(access_token: String, exp: i64, exp_in: i64) -> Self {
        Self {
            token: access_token,
            token_type: "Bearer".to_string(),
            exp,
            expired: exp_in,
        }
    }
}

/// Axum 认证错误类型
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken(_msg) => (StatusCode::UNAUTHORIZED, "Invalid token"),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
            "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}

/// JWT 验证器回调类型 — 由 cloud binary 在启动时注入
#[allow(clippy::type_complexity)]
static JWT_VALIDATOR: OnceLock<Box<dyn Fn(&str) -> Result<Claims, String> + Send + Sync>> = OnceLock::new();

/// 设置全局 JWT 验证器（必须在应用启动时调用一次）
pub fn set_jwt_validator(validator: Box<dyn Fn(&str) -> Result<Claims, String> + Send + Sync>) {
    let _ = JWT_VALIDATOR.set(validator);
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try Authorization header first
        if let Some(auth_header) = parts.headers.typed_get::<Authorization<Bearer>>() {
            let token = auth_header.token();
            let validator = JWT_VALIDATOR
                .get()
                .ok_or_else(|| AuthError::InvalidToken("JWT validator not initialized".to_string()))?;
            return validator(token).map_err(AuthError::InvalidToken);
        }

        // Fallback: query string ?token=xxx (needed for EventSource which can't set headers)
        if let Some(query) = parts.uri.query() {
            for pair in query.split('&') {
                let mut kv = pair.splitn(2, '=');
                if kv.next() == Some("token")
                    && let Some(token) = kv.next()
                {
                    let validator = JWT_VALIDATOR
                        .get()
                        .ok_or_else(|| AuthError::InvalidToken("JWT validator not initialized".to_string()))?;
                    return validator(token).map_err(AuthError::InvalidToken);
                }
            }
        }

        Err(AuthError::MissingToken)
    }
}
