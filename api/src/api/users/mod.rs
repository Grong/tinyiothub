// 用户管理模块
// 包含用户CRUD、角色管理、权限管理等功能

use axum::Router;

use crate::shared::app_state::AppState;

pub mod management;
pub mod permissions;
pub mod roles;

/// 创建用户管理路由
pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(management::create_router())
        .nest("/roles", roles::create_router())
        .nest("/permissions", permissions::create_router())
}
