use std::convert::Infallible;

use axum::{extract::FromRequestParts, http::request::Parts};

/// Workspace context extracted from JWT Claims — NOT from the X-Workspace-Id header.
///
/// The header was previously trusted without validation, allowing any authenticated
/// user to access arbitrary workspaces by forging the header value. Now workspace_id
/// is always sourced from the signed JWT, which is the authoritative source of the
/// user's authorized scope.
pub struct WorkspaceScope(pub Option<String>);

impl<S> FromRequestParts<S> for WorkspaceScope
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims_ws = crate::shared::security::jwt::Claims::from_request_parts(parts, state)
            .await
            .ok()
            .filter(|c| !c.workspace_id.is_empty())
            .map(|c| c.workspace_id);

        Ok(WorkspaceScope(claims_ws))
    }
}
