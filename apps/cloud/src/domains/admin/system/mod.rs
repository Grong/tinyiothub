use axum::Router;

use crate::domains::admin::AdminState;

mod configuration;
pub mod features; // 公开features模块
mod tasks;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .merge(configuration::create_router())
        .merge(features::create_router())
        .nest("/tasks", tasks::create_router())
}

// 初始化功能（initialize 端点 + ensure_default_admin_user /
// ensure_user_has_workspace）因 entangled with the agent plane
// (scaffold / AgentPool / shared::paths) 留在组合层
// `cloud::shared::initialization` — 见 crate::legacy 的边界文档。
