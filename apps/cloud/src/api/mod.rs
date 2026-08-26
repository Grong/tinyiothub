// API Layer
// Contains all HTTP API handlers and middleware

use axum::{
    Router, middleware as axum_middleware,
    routing::{get, post},
};

use crate::state::AppState;

// agents — 已迁移至 domains/agent/host/handler/
// alarms — 已迁移至 modules/alarm/handler.rs
// alarm_rules — 已迁移至 modules/alarm/handler.rs
// auth — 已迁移至 crates/auth（tinyiothub_auth）
// batch — 已迁移至 modules/batch/handler.rs
// chat — 已迁移至 modules/chat/handler/
// things — 已迁移至 modules/device/handler/
// drivers — 已迁移至 modules/drivers/handler.rs
// events — 已迁移至 modules/event/handler/
// heartbeat — 已迁移至 modules/heartbeat/handler.rs
// jobs — 已迁移至 modules/jobs/handler.rs
// marketplace — 已迁移至 modules/marketplace/handler.rs
pub mod middleware;
// mcp — 已迁移至 crates/mcp/ (P4-Task23)
// monitoring — 已迁移至 modules/monitoring/handler/
// notification_channels — 已迁移至 modules/notification/handler.rs
// notifications — 已迁移至 modules/notification/handler.rs
// open — 已迁移至 modules/open/
// system — 已迁移至 modules/system/handler/
// tags — 已迁移至 modules/tag/handler.rs
// templates — 已迁移至 modules/template/handler.rs
// tenants — 已迁移至 modules/tenant/handler.rs
// users — 已迁移至 modules/user/handler.rs + modules/role/handler.rs + modules/permission/handler.rs
// workspaces — 已迁移至 modules/workspace/handler.rs

/// Create the main API router
pub fn create_router(app_state: &AppState) -> Router<AppState> {
    // 创建需要认证的路由
    let protected_routes = Router::new()
        .nest("/devices", crate::domains::admin::device::create_router())
        .nest("/drivers", crate::domains::driver::router())
        .nest("/alarms", crate::domains::alarm::router())
        .nest("/alarm-rules", crate::domains::alarm::rule_router())
        .nest(
            "/monitoring",
            crate::domains::admin::monitoring::handler::create_router(),
        )
        .nest("/users", crate::domains::user::router())
        .nest("/users/roles", crate::domains::user::role::create_router())
        .nest("/users/permissions", crate::domains::user::permission::create_router())
        .nest(
            "/device-templates",
            crate::domains::thing::template::handler::create_router(),
        )
        .nest("/marketplace", crate::domains::marketplace::handler::create_router())
        .nest("/notifications", crate::domains::notify::handler::create_router())
        .nest(
            "/notification-channels",
            crate::domains::notify::handler::create_channel_router(),
        )
        .nest("/tenants", crate::domains::tenant::router())
        .nest(
            "/events",
            crate::domains::event::router().merge(crate::domains::event::http::create_router()),
        )
        .nest("/jobs", crate::domains::admin::jobs::handler::create_router())
        .nest("/batch", crate::domains::admin::batch::handler::create_router())
        .nest("/heartbeat", crate::domains::driver::heartbeat_router())
        .nest("/workspaces", crate::domains::tenant::workspace_router())
        .nest(
            "/workspaces",
            crate::domains::agent::host::memory::handler::create_router(),
        )
        .nest(
            "/workspaces",
            crate::domains::agent::host::handler::agent_tasks::create_workspace_router(),
        )
        .nest(
            "/workspaces",
            crate::domains::agent::host::handler::workspace_heartbeat::create_router(),
        )
        .nest("/mcp", crate::domains::mcp::router())
        .nest("/chat", crate::domains::agent::chat::handler::create_router())
        .nest(
            "/agents/skills",
            crate::domains::agent::host::handler::skills::create_router(),
        )
        .nest("/tags", crate::domains::thing::tag::create_router())
        .nest("/api-keys", crate::domains::tenant::api_key_router())
        .nest("/agents", crate::domains::agent::host::handler::create_router())
        .nest("/driver-health", crate::domains::driver::driver_health_router())
        .nest("/things", crate::domains::thing::router())
        .route(
            "/tools/catalog",
            get(crate::domains::agent::chat::handler::proxy::tools_catalog),
        )
        .route(
            "/tools/effective",
            get(crate::domains::agent::chat::handler::proxy::tools_effective),
        )
        .route(
            "/tools/toggle",
            post(crate::domains::agent::chat::handler::proxy::tools_toggle),
        )
        .nest("/auth", crate::domains::auth::router())
        .route("/test-auth", get(test_auth_endpoint))
        .layer(axum_middleware::from_fn_with_state(
            app_state.clone(),
            crate::api::middleware::context::jwt_auth_middleware,
        ));

    // 创建v1版本的API路由
    let v1_routes = Router::new()
        .nest("/auth", crate::domains::auth::handler::login::create_router())
        .nest("/auth/token", crate::domains::auth::handler::token::create_router())
        .nest("/auth/sms", crate::domains::auth::handler::sms::create_router())
        .nest("/auth/social", crate::domains::auth::handler::social::create_router())
        .nest("/tenants", crate::domains::tenant::auth_router())
        .nest("/system", crate::domains::admin::system::create_router())
        .nest("/system", crate::shared::initialization::create_router())
        .route("/gateway/pair", post(crate::domains::driver::gateway::handler::pairing::pair_device))
        // 公开的SSE端点（不需要JWT header, 通过?token=鉴权）
        .route(
            "/events/sse/public",
            get(crate::domains::event::http::sse::handle_sse_connection_public),
        )
        // SSE token 认证端点（不需要 JWT header，通过 ?sse_token= 鉴权）
        .route(
            "/events/sse/token",
            get(crate::domains::event::http::sse::handle_sse_connection_token),
        )
        .merge(protected_routes);

    // 合并所有路由
    Router::new()
        .nest("/v1", v1_routes)
        .nest("/open", crate::domains::admin::open::create_open_router())
        .route("/health", get(health_check))
}

/// 测试认证端点
async fn test_auth_endpoint() -> &'static str {
    "Authentication successful!"
}

/// 简单的健康检查端点
async fn health_check() -> &'static str {
    "OK"
}
