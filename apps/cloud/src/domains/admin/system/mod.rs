use axum::Router;

mod configuration;
pub mod features; // 公开features模块
mod tasks;

pub fn create_router() -> Router<crate::state::AppState> {
    Router::new()
        .merge(configuration::create_router())
        .merge(features::create_router())
        .nest("/tasks", tasks::create_router())
}

// 初始化功能（initialize 端点 + ensure_default_admin_user /
// ensure_user_has_workspace）因 entangled with the agent plane
// (scaffold / AgentPool / shared::paths) 留在组合层
// `cloud::shared::initialization` — 见 crate::legacy 的边界文档。
