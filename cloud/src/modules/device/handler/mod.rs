pub mod commands;
pub mod dashboard;
pub mod management;
pub mod monitoring;
pub mod profile;
pub mod properties;
pub mod trace;

// Note: Tenant verification is now handled by TenantDeviceRepository adapter
// which automatically filters devices by workspace_id. The adapter ensures
// that all device queries are scoped to the current workspace, eliminating
// the need for explicit tenant verification in API handlers.
use axum::Router;

use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(management::create_router())
        .merge(properties::create_router())
        .merge(commands::create_router())
        .merge(dashboard::create_router())
        .merge(profile::create_router())
        .merge(trace::create_router())
        .merge(monitoring::create_router())
}
