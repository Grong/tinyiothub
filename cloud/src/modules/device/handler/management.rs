use axum::{Json, Router, routing::{get, post}};
use crate::shared::app_state::AppState;

// /api/v1/devices management CRUD routes have been removed.
// Use the /api/v1/things endpoints instead.

/// Handler that returns 404 with migration guidance.
async fn device_endpoint_removed() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "error": {
            "code": "ENDPOINT_REMOVED",
            "message": "/api/devices has been removed. Use /api/things instead."
        }
    }))
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(device_endpoint_removed).post(device_endpoint_removed))
        .route("/{id}", get(device_endpoint_removed).put(device_endpoint_removed).delete(device_endpoint_removed))
        .route("/{id}/enable", post(device_endpoint_removed))
        .route("/{id}/disable", post(device_endpoint_removed))
        .route("/{id}/export-template", post(device_endpoint_removed))
        .route("/{id}/clone", post(device_endpoint_removed))
        .route("/from-template", post(device_endpoint_removed))
        .route("/from-template/{template_id}/preview", post(device_endpoint_removed))
        .route("/from-template/{template_id}/validate", post(device_endpoint_removed))
        .route("/from-template/{template_id}/requirements", get(device_endpoint_removed))
        .route("/from-template/{template_id}/validate-field", post(device_endpoint_removed))
}
