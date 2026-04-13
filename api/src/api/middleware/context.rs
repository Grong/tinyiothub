use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};

use crate::{
    dto::response::{ReqCtx, UserInfo},
    shared::security::jwt::{validate_jwt, is_token_blacklisted_sync},
};

/// Context middleware for request processing with Axum
pub async fn context_middleware(
    State(state): State<crate::shared::app_state::AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract request information
    let uri = request.uri().to_string();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Try to extract and validate JWT token
    let user_info = extract_user_from_jwt(request.headers(), request.uri(), Some(&state.database)).unwrap_or_default();

    // Create context with user information
    let ctx = ReqCtx {
        ori_uri: uri,
        path,
        path_params: String::new(),
        method,
        user: user_info,
        data: String::new(),
    };

    // Add context to request extensions
    request.extensions_mut().insert(ctx);

    Ok(next.run(request).await)
}

/// Extract bearer token from Authorization header or query string fallback
fn extract_bearer_token<'a>(headers: &'a HeaderMap, uri: &'a axum::http::Uri) -> Option<String> {
    // Try Authorization header first
    if let Some(auth) = headers.typed_get::<Authorization<Bearer>>() {
        return Some(auth.token().to_string());
    }
    // Fallback: query string ?token=xxx (needed for EventSource which can't set headers)
    let query = uri.query()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("token") {
            if let Some(val) = parts.next() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Extract user information from JWT token in headers or query string
fn extract_user_from_jwt(headers: &HeaderMap, uri: &axum::http::Uri, db: Option<&crate::infrastructure::persistence::database::Database>) -> Option<UserInfo> {
    let token = extract_bearer_token(headers, uri)?;

    // Check token blacklist if DB is available
    if let Some(database) = db {
        if is_token_blacklisted_sync(database, &token) {
            tracing::warn!("Rejected blacklisted token");
            return None;
        }
    }

    // Validate JWT token
    let claims = validate_jwt(&token).ok()?;

    // Convert claims to UserInfo
    Some(UserInfo {
        id: claims.user_id,
        name: claims.username,
        token_id: claims.token_id,
        tenant_id: claims.tenant_id,
    })
}

/// Extract body data from request (helper function)
#[allow(dead_code)]
async fn extract_body_data(body: &[u8]) -> Result<String, String> {
    match std::str::from_utf8(body) {
        Ok(text) => Ok(text.to_string()),
        Err(_) => Ok("Binary data".to_string()),
    }
}

/// JWT authentication middleware - requires valid JWT token
pub async fn jwt_auth_middleware(mut request: Request, next: Next) -> Response {
    let uri = request.uri().to_string();
    tracing::debug!("JWT middleware called for: {}", uri);

    // Extract token from Authorization header or query string ?token=xxx
    let token = extract_bearer_token(request.headers(), request.uri());

    let Some(token) = token else {
        tracing::warn!("No authorization token found for: {}", uri);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": -1,
                "msg": "Missing authorization token",
                "result": serde_json::Value::Null
            })),
        )
            .into_response();
    };

    tracing::debug!("Found token for: {}, length: {}", uri, token.len());

    // Validate JWT token
    match validate_jwt(&token) {
        Ok(claims) => {
            tracing::debug!("JWT validation successful for user: {} at: {}", claims.username, uri);
            // Add claims to request extensions for handlers to use
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(e) => {
            tracing::warn!("JWT validation failed for: {} - Error: {}", uri, e);
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "code": -1,
                    "msg": format!("Invalid token: {}", e),
                    "result": serde_json::Value::Null
                })),
            )
                .into_response()
        }
    }
}
