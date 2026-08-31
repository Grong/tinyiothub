use axum::{Json, Router, routing::any};

// /api/v1/devices management CRUD routes have been removed.
// Use the /api/v1/things endpoints instead.
// Removed surface (all return 410): /, /{id}, /{id}/enable, /{id}/disable,
// /{id}/export-template, /{id}/clone, /from-template/**.

/// Handler that returns 410 Gone with migration guidance in standard API format.
pub async fn device_endpoint_removed() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::GONE,
        Json(serde_json::json!({
            "code": 410,
            "msg": "/api/v1/devices has been removed. Use /api/v1/things instead.",
            "result": null
        })),
    )
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // Catch-alls: every method and every subpath under /devices 410s
    // (covers trailing slash and methods the old routes never had).
    Router::new()
        .route("/", any(device_endpoint_removed))
        .route("/{*rest}", any(device_endpoint_removed))
}
