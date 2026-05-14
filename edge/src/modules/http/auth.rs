use std::sync::Mutex;
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// Cached API key — read once from env, cached for subsequent requests.
/// Uses Mutex instead of OnceLock so tests can reset the cache.
static API_KEY_CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

fn get_api_key() -> Option<String> {
    let mut cache = API_KEY_CACHE.lock().unwrap();
    if let Some(ref cached) = *cache {
        return cached.clone();
    }
    let key = std::env::var("EDGE_LOCAL_API_KEY").ok();
    *cache = Some(key.clone());
    key
}

/// Reset the API key cache. Only used in tests.
pub fn reset_api_key_cache() {
    let mut cache = API_KEY_CACHE.lock().unwrap();
    *cache = None;
}

/// Constant-time string comparison to prevent timing side-channel attacks.
/// Accumulates differences across all bytes instead of short-circuiting.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// API key authentication middleware for local HTTP API.
///
/// Reads `EDGE_LOCAL_API_KEY` from the environment once at first request.
/// If the variable is not set, all requests pass through.
/// If set, requests must include an `Authorization: Bearer <key>` header
/// matching the configured key. Uses constant-time comparison.
pub async fn auth_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if no API key is configured
    if let Some(expected_key) = get_api_key() {
        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let bearer_prefix = "Bearer ";
        if !auth_header.starts_with(bearer_prefix) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let token = &auth_header[bearer_prefix.len()..];

        if !constant_time_eq(token.as_bytes(), expected_key.as_bytes()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}
