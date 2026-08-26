use axum::{
    Json, Router,
    routing::{get, post},
};

// /api/v1/devices management CRUD routes have been removed.
// Use the /api/v1/things endpoints instead.

/// Handler that returns 410 Gone with migration guidance in standard API format.
async fn device_endpoint_removed() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::GONE,
        Json(serde_json::json!({
            "code": 410,
            "msg": "/api/devices has been removed. Use /api/things instead.",
            "result": null
        })),
    )
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(device_endpoint_removed).post(device_endpoint_removed))
        .route(
            "/{id}",
            get(device_endpoint_removed)
                .put(device_endpoint_removed)
                .delete(device_endpoint_removed),
        )
        .route("/{id}/enable", post(device_endpoint_removed))
        .route("/{id}/disable", post(device_endpoint_removed))
        .route("/{id}/export-template", post(device_endpoint_removed))
        .route("/{id}/clone", post(device_endpoint_removed))
        .route("/from-template", post(device_endpoint_removed))
        .route("/from-template/{template_id}/preview", post(device_endpoint_removed))
        .route("/from-template/{template_id}/validate", post(device_endpoint_removed))
        .route(
            "/from-template/{template_id}/requirements",
            get(device_endpoint_removed),
        )
        .route(
            "/from-template/{template_id}/validate-field",
            post(device_endpoint_removed),
        )
}
