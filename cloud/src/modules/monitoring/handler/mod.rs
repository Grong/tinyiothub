use axum::Router;

use crate::shared::app_state::AppState;

mod dashboard;
mod health;
mod logs;
mod metrics;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .nest("/metrics", metrics::create_router())
        .nest("/health", health::create_router())
        .nest("/logs", logs::create_router())
        .merge(dashboard::create_router())
}
