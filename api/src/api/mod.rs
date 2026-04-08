// API Layer
// Contains all HTTP API handlers and middleware

use axum::{middleware as axum_middleware, routing::{get, post, put}, Router};

use crate::{application::data_context::DataContext, shared::app_state::AppState};

pub mod chat;
pub mod alarm_rules;
pub mod alarms;
pub mod auth;
pub mod batch;
pub mod devices;
pub mod drivers;
pub mod events;
pub mod heartbeat;
pub mod jobs;
pub mod marketplace;
pub mod middleware;
pub mod mcp;
pub mod monitoring;
pub mod notification_channels;
pub mod notifications;
pub mod open;
pub mod self_healing;
pub mod system;
pub mod tags;
pub mod templates;
pub mod tenants;
pub mod users;
pub mod workspaces;

/// Create the main API router
pub fn create_router() -> Router<AppState> {
    // 创建需要认证的路由
    let protected_routes = Router::new()
        .nest("/devices", devices::create_router())
        .nest("/drivers", drivers::create_router())
        .nest("/alarms", alarms::create_router())
        .nest("/alarm-rules", alarm_rules::create_router())
        .nest("/monitoring", monitoring::create_router())
        .nest("/users", users::create_router())
        .nest("/device-templates", templates::create_router())
        .nest("/marketplace", marketplace::create_router())
        .nest("/notifications", notifications::create_router())
        .nest("/notification-channels", notification_channels::create_router())
        .nest("/tenants", tenants::create_router())
        .nest("/events", events::create_router())
        .nest("/jobs", jobs::create_router())
        .nest("/batch", batch::create_router())
        .nest("/heartbeat", heartbeat::create_router()) // 心跳端点
        .nest("/self-healing", self_healing::create_router()) // 自愈端点
        .nest("/workspaces", workspaces::create_router()) // 工作空间端点
        .nest("/mcp", mcp::create_router()) // MCP 工具端点
        .nest("/chat", chat::create_router()) // Chat 代理端点
        // API Keys — 直接在 /v1/api-keys/ 下，不嵌套在 /tenants 下
        .nest("/api-keys", tenants::create_api_key_router())
        .route("/agents", get(chat::proxy::list_agents))
        .route("/agents/:id/config", get(chat::proxy::get_agent_config).put(chat::proxy::set_agent_config))
        .route("/tools/catalog", get(chat::proxy::tools_catalog))
        .route("/tools/effective", get(chat::proxy::tools_effective))
        .route("/tools/toggle", post(chat::proxy::tools_toggle))
        .nest("/auth", auth::session::create_router()) // 需要认证的会话路由
        .route("/test-auth", get(test_auth_endpoint))
        .layer(axum_middleware::from_fn(crate::api::middleware::context::jwt_auth_middleware));

    // 创建v1版本的API路由
    let v1_routes = Router::new()
        .nest("/auth", auth::login::create_router()) // 公开的登录路由
        .nest("/auth/sms", auth::sms::create_router()) // 短信验证码登录
        .nest("/auth/social", auth::social::create_router()) // 第三方登录
        .nest("/tenants", tenants::auth::create_auth_router()) // 租户注册登录
        .nest("/system", system::create_router())
        .nest("/tags", tags::create_router())
        // 公开的SSE端点（不需要认证）
        .route("/events/sse/public", get(events::sse::handle_sse_connection_public))
        .merge(protected_routes);

    // 合并所有路由
    Router::new()
        .nest("/v1", v1_routes)
        .nest("/open", open::create_open_router()) // 开放 API (需要 API Key)
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
