use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use std::convert::Infallible;

/// 从请求头中提取 workspace 上下文
/// 读取 X-Workspace-Id 请求头，值为可选的 workspace ID
/// 未提供时返回 None（显示整个租户的数据，向后兼容）
pub struct WorkspaceScope(pub Option<String>);

#[axum::async_trait]
impl<S> FromRequestParts<S> for WorkspaceScope
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let workspace_id = parts
            .headers
            .get("x-workspace-id")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        Ok(WorkspaceScope(workspace_id))
    }
}
