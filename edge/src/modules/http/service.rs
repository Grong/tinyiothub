use axum::{
    Router, middleware,
    routing::{get, post},
};
use std::sync::Arc;

use crate::app_state::AppState;

use super::{auth, handlers};

/// Create the local HTTP API router with 12 endpoints and API key auth middleware.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(handlers::get_health))
        .route("/api/v1/devices", get(handlers::get_devices))
        .route("/api/v1/devices/{id}", get(handlers::get_device))
        .route(
            "/api/v1/devices/{id}/properties",
            get(handlers::get_device_properties).post(handlers::post_device_properties),
        )
        .route("/api/v1/devices/{id}/command", post(handlers::post_device_command))
        .route("/api/v1/drivers", get(handlers::get_drivers))
        .route("/api/v1/drivers/scan", post(handlers::post_driver_scan))
        .route("/api/v1/alarms", get(handlers::get_alarms))
        .route("/api/v1/config", get(handlers::get_config).put(handlers::put_config))
        .route("/api/v1/offline-buffer", get(handlers::get_offline_buffer))
        .layer(middleware::from_fn(auth::auth_middleware))
        .with_state(state)
}
