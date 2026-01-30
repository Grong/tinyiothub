use crate::shared::app_state::AppState;
use axum::Router;

mod configuration;
pub mod features; // 公开features模块
mod initialization;
mod products;
mod tasks;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(configuration::create_router())
        .merge(features::create_router())
        .merge(initialization::create_router())
        .nest("/tasks", tasks::create_router())
        .nest("/products", products::create_router())
}

// 重新导出初始化功能
pub use initialization::ensure_default_admin_user;
