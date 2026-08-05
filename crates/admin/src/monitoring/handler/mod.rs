use axum::Router;

use crate::AdminState;

mod dashboard;
pub mod health;
mod logs;
mod metrics;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
{
    Router::new()
        .nest("/metrics", metrics::create_router())
        .nest("/health", health::create_router())
        .nest("/logs", logs::create_router())
        .merge(dashboard::create_router())
}
