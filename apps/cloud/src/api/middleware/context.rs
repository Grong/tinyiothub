use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use headers::{Authorization, HeaderMapExt, authorization::Bearer};

use crate::shared::api_response::{ReqCtx, UserInfo};

/// Context middleware for request processing with Axum
pub async fn context_middleware(
    State(state): State<crate::state::AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract request information
    let uri = request.uri().to_string();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Try to extract and validate JWT token
    let user_info = extract_user_from_jwt(
        request.headers(),
        request.uri(),
        Some(&state.db),
        &state.jwt_service,
    )
    .await
    .unwrap_or_default();

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
        if parts.next() == Some("token")
            && let Some(val) = parts.next()
        {
            return Some(val.to_string());
        }
    }
    None
}

/// Extract user information from JWT token in headers or query string
async fn extract_user_from_jwt(
    headers: &HeaderMap,
    uri: &axum::http::Uri,
    db: Option<&tinyiothub_storage::Db>,
    jwt: &tinyiothub_authn::jwt::JwtService,
) -> Option<UserInfo> {
    let token = extract_bearer_token(headers, uri)?;

    // Check token blacklist if DB is available（异步查询，不阻塞线程）
    if let Some(database) = db
        && is_token_blacklisted(database, &token).await
    {
        tracing::warn!("Rejected blacklisted token");
        return None;
    }

    // Validate JWT token
    let claims = jwt.validate_jwt(&token).ok()?;

    // Convert claims to UserInfo
    Some(UserInfo {
        id: claims.user_id,
        name: claims.username,
        token_id: claims.token_id,
    })
}

/// 检查 token 是否在黑名单中（G4：自 authn 迁入 —— 业务查询住 apps/cloud）
async fn is_token_blacklisted(db: &tinyiothub_storage::Db, token: &str) -> bool {
    use sha2::Digest;

    let token_hash = format!("{:x}", sha2::Sha256::digest(token.as_bytes()));

    sqlx::query("SELECT 1 FROM token_blacklist WHERE token_hash = ? LIMIT 1")
        .bind(&token_hash)
        .fetch_optional(db.pool())
        .await
        .map(|r| r.is_some())
        .unwrap_or(false)
}

/// JWT authentication middleware - requires valid JWT token
pub async fn jwt_auth_middleware(
    State(state): State<crate::state::AppState>,
    mut request: Request,
    next: Next,
) -> Response {
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
    match state.jwt_service.validate_jwt(&token) {
        Ok(claims) => {
            tracing::debug!("JWT validation successful for user: {} at: {}", claims.username, uri);
            // Add claims to request extensions for handlers to use
            request.extensions_mut().insert(tinyiothub_web::security::Claims::from(claims));
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
