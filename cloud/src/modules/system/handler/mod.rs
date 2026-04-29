use axum::Router;

use crate::shared::app_state::AppState;

mod configuration;
pub mod features; // 公开features模块
mod initialization;
// products — 已迁移至 modules/product/handler.rs
mod tasks;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(configuration::create_router())
        .merge(features::create_router())
        .merge(initialization::create_router())
        .nest("/tasks", tasks::create_router())
        .nest("/products", crate::modules::product::create_router())
}

// 重新导出初始化功能
pub use initialization::{ensure_default_admin_user, ensure_user_has_workspace};
