pub mod cache;
pub mod handler;
pub mod service;
pub mod types;

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<cache::SledCache>,
    pub sync: Arc<service::SyncService>,
}
