//! HTTP server configuration and router assembly.
//!
//! Extracted from `main.rs` to separate server wiring from runtime initialization.

use axum::{Router, extract::DefaultBodyLimit};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::shared::app_state::AppState;

/// Create the main application router with all middleware.
pub async fn create_app_router(app_state: AppState) -> Router {
    use tower_http::cors::AllowOrigin;

    tracing::info!("Creating CORS layer...");

    // Initialize MCP tools with AppState
    tracing::info!("Initializing MCP tools...");
    use std::sync::Arc;
    crate::modules::mcp::init_app_state(Arc::new(app_state.clone()));
    crate::modules::mcp::register_tools().await;
    tracing::info!("MCP tools initialized");

    // Refresh agent tools after MCP registration
    if let Err(e) = app_state.agent_pool.refresh_tools().await {
        tracing::error!("Failed to refresh agent tools: {}", e);
    }

    // Initialize self-healing state
    let db = app_state.database.clone();
    let _self_healing_state = crate::modules::self_healing::handler::init_self_healing_state(db);

    // CORS layer
    let config = crate::shared::config::get();
    let cors_origins = &config.server.cors_origins;

    let allowed_headers = [
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        axum::http::header::ACCEPT,
    ];
    let allowed_methods = [
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::DELETE,
        axum::http::Method::OPTIONS,
    ];

    let allow_any = cors_origins.contains(&"*".to_string());
    let explicit_origins: Vec<axum::http::HeaderValue> = if allow_any {
        Vec::new()
    } else {
        cors_origins.iter().filter_map(|o| o.parse().ok()).collect()
    };

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            if allow_any {
                return true;
            }
            explicit_origins.iter().any(|o| o == origin)
        }))
        .allow_credentials(true)
        .allow_methods(allowed_methods)
        .allow_headers(allowed_headers);

    tracing::info!("Creating API router...");
    let api_router = crate::api::create_router();

    tracing::info!("Building main router...");
    tracing::info!("Serving static files from wwwroot/ (SPA mode)");

    let agents_dir = crate::shared::paths::agents_base_dir();
    tokio::fs::create_dir_all(&agents_dir).await.unwrap_or_default();

    let mut router = Router::new()
        .nest("/api", api_router)
        .nest_service("/uploads", tower_http::services::ServeDir::new(&agents_dir))
        .fallback(spa_handler);

    tracing::info!("Adding middleware layers...");
    router = router
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50 MB for file uploads
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    tracing::info!("Adding application state...");
    router.with_state(app_state)
}

/// SPA fallback handler — serves static files or falls back to index.html.
async fn spa_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};

    let path = uri.path();

    if path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "API endpoint not found").into_response();
    }

    let file_path = if path == "/" {
        "wwwroot/index.html".to_string()
    } else if path.ends_with('/') {
        format!("wwwroot{}", path.trim_end_matches('/'))
    } else {
        format!("wwwroot{}", path)
    };

    match tokio::fs::read(&file_path).await {
        Ok(content) => {
            let content_type = if file_path.ends_with(".html") {
                "text/html"
            } else if file_path.ends_with(".js") {
                "application/javascript"
            } else if file_path.ends_with(".css") {
                "text/css"
            } else if file_path.ends_with(".json") {
                "application/json"
            } else if file_path.ends_with(".png") {
                "image/png"
            } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
                "image/jpeg"
            } else if file_path.ends_with(".svg") {
                "image/svg+xml"
            } else if file_path.ends_with(".ico") {
                "image/x-icon"
            } else {
                "application/octet-stream"
            };

            ([(axum::http::header::CONTENT_TYPE, content_type)], content).into_response()
        }
        Err(_) => {
            let html_path = format!("{}.html", file_path);
            match tokio::fs::read(&html_path).await {
                Ok(content) => {
                    ([(axum::http::header::CONTENT_TYPE, "text/html")], content).into_response()
                }
                Err(_) => {
                    tracing::info!("Serving index.html for SPA route: {}", path);
                    match tokio::fs::read("wwwroot/index.html").await {
                        Ok(content) => ([(axum::http::header::CONTENT_TYPE, "text/html")], content)
                            .into_response(),
                        Err(_) => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
                    }
                }
            }
        }
    }
}
