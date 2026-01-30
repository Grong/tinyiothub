pub mod commands;
pub mod dashboard;
pub mod management;
pub mod monitoring;
pub mod profile;
pub mod properties;
pub mod trace;

use crate::shared::app_state::AppState;
use axum::Router;

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
