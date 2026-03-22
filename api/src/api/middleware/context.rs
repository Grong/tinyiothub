use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};

use crate::dto::response::{ReqCtx, UserInfo};
use crate::shared::security::jwt::validate_jwt;

/// Context middleware for request processing with Axum
pub async fn context_middleware(
    State(_state): State<crate::shared::app_state::AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract request information
    let uri = request.uri().to_string();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Try to extract and validate JWT token
    let user_info = extract_user_from_jwt(request.headers()).unwrap_or_default();

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

/// Extract user information from JWT token in headers
fn extract_user_from_jwt(headers: &HeaderMap) -> Option<UserInfo> {
    // Try to get Authorization header
    let auth_header = headers.typed_get::<Authorization<Bearer>>()?;

    // Validate JWT token
    let claims = validate_jwt(auth_header.token()).ok()?;

    // Convert claims to UserInfo
    Some(UserInfo {
        id: claims.user_id,
        name: claims.username,
        ..Default::default()
    })
}

/// Extract body data from request (helper function)
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

    // Extract Authorization header
    let auth_header = request.headers().typed_get::<Authorization<Bearer>>();

    if auth_header.is_none() {
        tracing::warn!("No Authorization header found for: {}", uri);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": -1,
                "msg": "Missing authorization token",
                "result": serde_json::Value::Null
            })),
        )
            .into_response();
    }

    let auth_header = auth_header.unwrap();
    let token = auth_header.token();
    tracing::debug!(
        "Found Authorization header for: {}, token length: {}",
        uri,
        token.len()
    );

    // Validate JWT token
    match validate_jwt(token) {
        Ok(claims) => {
            tracing::debug!(
                "JWT validation successful for user: {} at: {}",
                claims.username,
                uri
            );
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
