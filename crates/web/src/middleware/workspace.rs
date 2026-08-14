//! Workspace scoping — resolves the caller's tenant/workspace from the JWT.
//!
//! Domain crates must not depend on the composition layer's auth
//! implementation, so the actual token validation is injected: the cloud
//! binary registers a tenant resolver once at startup (`set_tenant_resolver`),
//! and the extractors below call it.

use std::{convert::Infallible, sync::OnceLock};

use axum::{extract::FromRequestParts, http::request::Parts};
use headers::{Authorization, HeaderMapExt, authorization::Bearer};

use crate::security::AuthError;

/// Tenant-scoped identity resolved from the request token by the
/// composition layer's resolver.
#[derive(Debug, Clone)]
pub struct TenantClaims {
    pub user_id: String,
    pub tenant_id: String,
    pub workspace_id: String,
}

#[allow(clippy::type_complexity)]
static TENANT_RESOLVER: OnceLock<Box<dyn Fn(&str) -> Option<TenantClaims> + Send + Sync>> = OnceLock::new();

/// Register the tenant resolver (must be called once at application startup).
pub fn set_tenant_resolver(resolver: Box<dyn Fn(&str) -> Option<TenantClaims> + Send + Sync>) {
    let _ = TENANT_RESOLVER.set(resolver);
}

/// Extract the bearer token from the Authorization header, falling back to
/// the `?token=` query parameter (needed for EventSource which cannot set
/// headers).
fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(auth_header) = parts.headers.typed_get::<Authorization<Bearer>>() {
        return Some(auth_header.token().to_string());
    }
    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if kv.next() == Some("token")
                && let Some(token) = kv.next()
            {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn resolve(parts: &Parts) -> Option<TenantClaims> {
    let token = extract_token(parts)?;
    TENANT_RESOLVER.get().and_then(|resolver| resolver(&token))
}

/// Workspace context extracted from the signed JWT — NOT from the
/// X-Workspace-Id header.
///
/// The header was previously trusted without validation, allowing any
/// authenticated user to access arbitrary workspaces by forging the header
/// value. Now workspace_id is always sourced from the signed JWT, which is
/// the authoritative source of the user's authorized scope.
pub struct WorkspaceScope(pub Option<String>);

impl<S> FromRequestParts<S> for WorkspaceScope
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let claims_ws = resolve(parts)
            .filter(|c| !c.workspace_id.is_empty())
            .map(|c| c.workspace_id);
        Ok(WorkspaceScope(claims_ws))
    }
}

/// Authenticated tenant identity (user_id + tenant_id + workspace_id).
/// Rejects with 401 when no valid token is present.
pub struct AuthClaims(pub TenantClaims);

impl<S> FromRequestParts<S> for AuthClaims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        resolve(parts).map(AuthClaims).ok_or(AuthError::MissingToken)
    }
}
