pub mod cache;
pub mod handler;
pub mod service;
pub mod types;

use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<cache::SledCache>,
    pub sync: Arc<service::SyncService>,
}

impl AppState {
    pub fn new(cache: Arc<cache::SledCache>, sync: Arc<service::SyncService>) -> Self {
        Self { cache, sync }
    }
}

pub fn build_app(state: AppState) -> Router {
    Router::new().merge(handler::routes()).with_state(state)
}
