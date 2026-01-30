// API Layer
// Contains all HTTP API handlers and middleware

use axum::{middleware as axum_middleware, routing::get, Router};

use crate::{application::data_context::DataContext, shared::app_state::AppState};

pub mod alarm_rules;
pub mod alarms;
pub mod auth;
pub mod devices;
pub mod drivers;
pub mod events;
pub mod marketplace;
pub mod middleware;
pub mod monitoring;
pub mod notifications;
pub mod system;
pub mod tags;
pub mod templates;
pub mod users;

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
        .nest("/events", events::create_router())
        .nest("/auth", auth::session::create_router()) // 需要认证的会话路由
        .route("/test-auth", get(test_auth_endpoint))
        .layer(axum_middleware::from_fn(
            crate::api::middleware::context::jwt_auth_middleware,
        ));

    // 创建v1版本的API路由
    let v1_routes = Router::new()
        .nest("/auth", auth::login::create_router()) // 公开的登录路由
        .nest("/system", system::create_router())
        .nest("/tags", tags::create_router())
        // 公开的SSE端点（不需要认证）
        .route(
            "/events/sse/public",
            get(events::sse::handle_sse_connection_public),
        )
        .merge(protected_routes);

    // 合并所有路由
    Router::new()
        .nest("/v1", v1_routes)
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
