//! HTTP 安全层：Claims 类型、认证错误、JWT extractor。
//! G4：Claims extractor 自 authn 迁入（传输层胶水乡 web，机制住 authn）。

pub mod jwt_extractor;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// JWT Claims 结构体 — 从认证 token 中提取的用户身份（含租户/工作空间隔离）
///
/// 由 `jwt_extractor` 或认证中间件填充后传递到 handler。
/// `exp` 字段不参与 JWT 序列化，仅在验证后填充供业务层使用。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: String,
    pub token_id: String,
    pub username: String,
    pub tenant_id: String,
    pub workspace_id: String,
    /// 从 JWT 验证结果中提取的过期时间（不参与序列化到 JWT）
    #[serde(skip_serializing)]
    pub exp: Option<i64>,
}

impl From<tinyiothub_authn::Claims> for Claims {
    fn from(claims: tinyiothub_authn::Claims) -> Self {
        Self {
            user_id: claims.user_id,
            token_id: claims.token_id,
            username: claims.username,
            tenant_id: claims.tenant_id,
            workspace_id: claims.workspace_id,
            exp: claims.exp,
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
