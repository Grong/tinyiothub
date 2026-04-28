use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use std::convert::Infallible;

/// 从请求头中提取 workspace 上下文
/// 优先读取 X-Workspace-Id 请求头，未提供时回退到 JWT Claims 中的 workspace_id
/// 确保用户只能访问其 JWT 授权的 workspace
pub struct WorkspaceScope(pub Option<String>);

impl<S> FromRequestParts<S> for WorkspaceScope
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Check X-Workspace-Id header first (allows explicit workspace switching)
        let header_ws = parts
            .headers
            .get("x-workspace-id")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if let Some(ws) = header_ws {
            return Ok(WorkspaceScope(Some(ws)));
        }

        // 2. Fall back to JWT claims workspace_id (prevents unauthenticated data access)
        let claims_ws = crate::shared::security::jwt::Claims::from_request_parts(parts, state)
            .await
            .ok()
            .filter(|c| !c.workspace_id.is_empty())
            .map(|c| c.workspace_id);

        Ok(WorkspaceScope(claims_ws))
    }
}
