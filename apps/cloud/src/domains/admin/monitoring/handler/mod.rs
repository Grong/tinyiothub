use axum::Router;

use crate::domains::admin::AdminState;

mod dashboard;
pub mod health;
mod logs;
mod metrics;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .nest("/metrics", metrics::create_router())
        .nest("/health", health::create_router())
        .nest("/logs", logs::create_router())
        .merge(dashboard::create_router())
}
