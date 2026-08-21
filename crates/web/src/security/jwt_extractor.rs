//! Claims 的 axum extractor（G4：自 authn 迁入）。
//! 通过 `Arc<JwtService>: FromRef<S>` 构造注入获取机制服务，无全局态。

use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use headers::{Authorization, HeaderMapExt, authorization::Bearer};
use tinyiothub_authn::JwtService;

use super::{AuthError, Claims};

/// 使 Claims 可以直接在 handler 中作为 extractor 使用
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
    Arc<JwtService>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let service = Arc::<JwtService>::from_ref(state);
        // Try Authorization header first
        if let Some(auth_header) = parts.headers.typed_get::<Authorization<Bearer>>() {
            let token = auth_header.token();
            return service
                .validate_jwt(token)
                .map(Claims::from)
                .map_err(AuthError::InvalidToken);
        }

        // Fallback: query string ?token=xxx (needed for EventSource which can't set headers)
        if let Some(query) = parts.uri.query() {
            for pair in query.split('&') {
                let mut kv = pair.splitn(2, '=');
                if kv.next() == Some("token")
                    && let Some(token) = kv.next()
                {
                    return service
                        .validate_jwt(token)
                        .map(Claims::from)
                        .map_err(AuthError::InvalidToken);
                }
            }
        }

        Err(AuthError::MissingToken)
    }
}
