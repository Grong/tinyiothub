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
// auth — 已迁移至 modules/auth/handler/
// batch — 已迁移至 modules/batch/handler.rs
// chat — 已迁移至 modules/chat/handler/
// devices — 已迁移至 modules/device/handler/
// drivers — 已迁移至 modules/drivers/handler.rs
// events — 已迁移至 modules/event/handler/
// heartbeat — 已迁移至 modules/heartbeat/handler.rs
// jobs — 已迁移至 modules/jobs/handler.rs
// marketplace — 已迁移至 modules/marketplace/handler.rs
pub mod middleware;
// mcp — 已迁移至 modules/mcp/
// monitoring — 已迁移至 modules/monitoring/handler/
// notification_channels — 已迁移至 modules/notification/handler.rs
// notifications — 已迁移至 modules/notification/handler.rs
// open — 已迁移至 modules/open/
// self_healing — 已迁移至 modules/self_healing/handler.rs
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
        .nest("/devices", crate::modules::device::handler::create_router())
        .nest("/drivers", crate::modules::drivers::handler::create_router())
        .nest("/alarms", crate::modules::alarm::handler::create_alarm_router())
        .nest("/alarm-rules", crate::modules::alarm::handler::create_alarm_rule_router())
        .nest("/monitoring", crate::modules::monitoring::handler::create_router())
        .nest("/users", crate::modules::user::create_router())
        .nest("/users/roles", crate::modules::role::create_router())
        .nest("/users/permissions", crate::modules::permission::create_router())
        .nest("/device-templates", crate::modules::template::handler::create_router())
        .nest("/marketplace", crate::modules::marketplace::handler::create_router())
        .nest("/notifications", crate::modules::notification::handler::create_router())
        .nest(
            "/notification-channels",
            crate::modules::notification::handler::create_channel_router(),
        )
        .nest("/tenants", crate::modules::tenant::create_router())
        .nest("/events", crate::modules::event::handler::create_router())
        .nest("/jobs", crate::modules::jobs::handler::create_router())
        .nest("/batch", crate::modules::batch::handler::create_router())
        .nest("/heartbeat", crate::modules::heartbeat::handler::create_router())
        .nest("/self-healing", crate::modules::self_healing::handler::create_router())
        .nest("/workspaces", crate::modules::workspace::create_router()) // 工作空间端点
        .nest("/workspaces", crate::modules::agent::memory::handler::create_router()) // Agent 记忆
        .nest("/mcp", crate::modules::mcp::create_router())
        .nest("/chat", crate::modules::chat::handler::create_router())
        .nest("/agents/skills", crate::modules::agent::handler::skills::create_router())
        .nest("/tags", crate::modules::tag::create_router()) // 标签端点
        // API Keys — 直接在 /v1/api-keys/ 下，不嵌套在 /tenants 下
        .nest("/api-keys", crate::modules::tenant::create_api_key_router())
        .nest("/agents", crate::modules::agent::handler::create_router())
        .nest("/driver-health", crate::modules::driver_health::handler::create_router())
        .route("/tools/catalog", get(crate::modules::chat::handler::proxy::tools_catalog))
        .route("/tools/effective", get(crate::modules::chat::handler::proxy::tools_effective))
        .route("/tools/toggle", post(crate::modules::chat::handler::proxy::tools_toggle))
        .nest("/auth", crate::modules::auth::handler::session::create_router())
        .route("/test-auth", get(test_auth_endpoint))
        .layer(axum_middleware::from_fn(crate::api::middleware::context::jwt_auth_middleware));

    // 创建v1版本的API路由
    let v1_routes = Router::new()
        .nest("/auth", crate::modules::auth::handler::login::create_router())
        .nest("/auth/token", crate::modules::auth::handler::token::create_router())
        .nest("/auth/sms", crate::modules::auth::handler::sms::create_router())
        .nest("/auth/social", crate::modules::auth::handler::social::create_router())
        .nest("/tenants", crate::modules::tenant::create_auth_router()) // 租户注册登录
        .nest("/system", crate::modules::system::handler::create_router())
        .route("/gateway/pair", post(crate::modules::gateway::handler::pairing::pair_device))
        // 公开的SSE端点（不需要认证）
        .route(
            "/events/sse/public",
            get(crate::modules::event::handler::sse::handle_sse_connection_public),
        )
        .merge(protected_routes);

    // 合并所有路由
    Router::new()
        .nest("/v1", v1_routes)
        .nest("/open", crate::modules::open::create_open_router())
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
