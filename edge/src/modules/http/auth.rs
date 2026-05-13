use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// API key authentication middleware for local HTTP API.
///
/// Reads `EDGE_LOCAL_API_KEY` from the environment. If the variable is not set,
/// all requests pass through without authentication. If set, requests must
/// include an `Authorization: Bearer <key>` header matching the configured key.
pub async fn auth_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if no API key is configured
    let expected_key = std::env::var("EDGE_LOCAL_API_KEY").ok();
    if let Some(key) = expected_key {
        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if auth_header != format!("Bearer {}", key) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}
