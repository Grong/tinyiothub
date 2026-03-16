/// Gateway API Module
/// 网关管理 API

pub mod management;

// Re-export API handlers
pub use management::*;

use crate::shared::app_state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

/// Create gateway API router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Gateway CRUD
        .route("/gateways", get(management::list_gateways))
        .route("/gateways", post(management::create_gateway))
        .route("/gateways/:id", get(management::get_gateway))
        .route("/gateways/:id", put(management::update_gateway))
        .route("/gateways/:id", delete(management::delete_gateway))
        
        // Gateway devices
        .route("/gateways/:id/devices", get(management::get_gateway_devices))
        
        // Gateway status
        .route("/gateways/:id/status", put(management::update_gateway_status))
}
