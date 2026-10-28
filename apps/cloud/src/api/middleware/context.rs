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
    let user_info = extract_user_from_jwt(request.headers(), request.uri(), Some(&state.db), &state.jwt_service)
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

    db.token_blacklist_contains(&token_hash).await.unwrap_or(false)
}

/// JWT authentication middleware - requires valid JWT token
pub async fn jwt_auth_middleware(
    State(state): State<crate::state::AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let uri = request.uri().path().to_string();
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
            request
                .extensions_mut()
                .insert(tinyiothub_web::security::Claims::from(claims));
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

#[cfg(test)]
mod tests {
    use super::jwt_auth_middleware;
    use axum::{Router, body::Body, middleware, routing::get};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct LogBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for LogBuf {
        type Writer = LogBuf;

        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn jwt_logs_never_include_query_token_or_signed_query() {
        let (state, _pool) = crate::test_utils::setup_test_app_with_pool().await;
        let app = Router::new()
            .route("/workspaces/ws/evidence/evidence-1", get(|| async {}))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth_middleware))
            .with_state(state);
        let buf = LogBuf::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(buf.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/workspaces/ws/evidence/evidence-1?token=synthetic-query-token&sig=synthetic-signed-query")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        let logs = String::from_utf8(buf.0.lock().expect("log buffer").clone()).expect("UTF-8 logs");
        assert!(logs.contains("/workspaces/ws/evidence/evidence-1"));
        assert!(!logs.contains("synthetic-query-token"), "logged query token");
        assert!(!logs.contains("synthetic-signed-query"), "logged signed query");
    }
}
