// API Layer
// Contains all HTTP API handlers and middleware

use axum::{
    Router, middleware as axum_middleware,
    routing::{get, post},
};

use crate::shared::app_state::AppState;

// agents — 已迁移至 modules/agent/handler/
// alarms — 已迁移至 modules/alarm/handler.rs
// alarm_rules — 已迁移至 modules/alarm/handler.rs
// auth — 已迁移至 crates/auth（tinyiothub_auth）
// batch — 已迁移至 modules/batch/handler.rs
// chat — 已迁移至 modules/chat/handler/
// devices — 已迁移至 modules/device/handler/
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
pub fn create_router() -> Router<AppState> {
    // 创建需要认证的路由
    let protected_routes = Router::new()
        .nest("/devices", tinyiothub_admin::device::create_router())
        .nest("/drivers", tinyiothub_driver::router())
        .nest("/alarms", tinyiothub_alarm::router())
        .nest("/alarm-rules", tinyiothub_alarm::rule_router())
        .nest("/monitoring", tinyiothub_admin::monitoring::handler::create_router())
        .nest("/users", tinyiothub_user::router())
        .nest("/users/roles", tinyiothub_user::role::create_router())
        .nest("/users/permissions", tinyiothub_user::permission::create_router())
        .nest("/device-templates", tinyiothub_thing::template::handler::create_router())
        .nest("/marketplace", crate::modules::marketplace::handler::create_router())
        .nest("/notifications", tinyiothub_notify::router())
        .nest("/notification-channels", tinyiothub_notify::channel_router())
        .nest("/tenants", tinyiothub_tenant::router())
        .nest(
            "/events",
            tinyiothub_event::router().merge(crate::shared::event::http::create_router()),
        )
        .nest("/jobs", tinyiothub_admin::jobs::handler::create_router())
        .nest("/batch", tinyiothub_admin::batch::handler::create_router())
        .nest("/heartbeat", tinyiothub_driver::heartbeat_router())
        .nest("/workspaces", tinyiothub_tenant::workspace_router())
        .nest("/workspaces", tinyiothub_agent::host::memory::handler::create_router())
        .nest(
            "/workspaces",
            tinyiothub_agent::host::handler::agent_tasks::create_workspace_router(),
        )
        .nest("/workspaces", tinyiothub_agent::host::handler::workspace_heartbeat::create_router())
        .nest("/mcp", tinyiothub_mcp::router())
        .nest("/chat", tinyiothub_agent::chat::handler::create_router())
        .nest("/agents/skills", tinyiothub_agent::host::handler::skills::create_router())
        .nest("/tags", tinyiothub_thing::tag::create_router())
        .nest("/api-keys", tinyiothub_tenant::api_key_router())
        .nest("/agents", tinyiothub_agent::host::handler::create_router())
        .nest("/driver-health", tinyiothub_driver::driver_health_router())
        .nest("/things", tinyiothub_thing::router())
        .route("/tools/catalog", get(tinyiothub_agent::chat::handler::proxy::tools_catalog))
        .route("/tools/effective", get(tinyiothub_agent::chat::handler::proxy::tools_effective))
        .route("/tools/toggle", post(tinyiothub_agent::chat::handler::proxy::tools_toggle))
        .nest("/auth", tinyiothub_auth::router())
        .route("/test-auth", get(test_auth_endpoint))
        .layer(axum_middleware::from_fn(crate::api::middleware::context::jwt_auth_middleware));

    // 创建v1版本的API路由
    let v1_routes = Router::new()
        .nest("/auth", tinyiothub_auth::handler::login::create_router())
        .nest("/auth/token", tinyiothub_auth::handler::token::create_router())
        .nest("/auth/sms", tinyiothub_auth::handler::sms::create_router())
        .nest("/auth/social", tinyiothub_auth::handler::social::create_router())
        .nest("/tenants", tinyiothub_tenant::auth_router())
        .nest("/system", tinyiothub_admin::system::create_router())
        .nest("/system", crate::shared::initialization::create_router())
        .route("/gateway/pair", post(tinyiothub_driver::gateway::handler::pairing::pair_device))
        // 公开的SSE端点（不需要JWT header, 通过?token=鉴权）
        .route(
            "/events/sse/public",
            get(crate::shared::event::http::sse::handle_sse_connection_public),
        )
        // SSE token 认证端点（不需要 JWT header，通过 ?sse_token= 鉴权）
        .route(
            "/events/sse/token",
            get(crate::shared::event::http::sse::handle_sse_connection_token),
        )
        .merge(protected_routes);

    // 合并所有路由
    Router::new()
        .nest("/v1", v1_routes)
        .nest("/open", tinyiothub_admin::open::create_open_router())
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
