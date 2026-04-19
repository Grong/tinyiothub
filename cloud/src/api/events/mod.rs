// Events API module
// Provides REST API endpoints for event querying and statistics

use axum::{
    routing::{get, post, put},
    Router,
};

use crate::shared::app_state::AppState;

pub mod overview;
pub mod performance;
pub mod query;
pub mod real_time;
pub mod security;
pub mod sse;

/// Create the events API router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(query::get_events))
        .route("/", post(query::create_event))
        .route("/real-time", get(real_time::get_real_time_events))
        .route("/real-time/status", get(real_time::get_status_summary))
        .route("/real-time/{id}/acknowledge", post(real_time::acknowledge_event))
        .route("/overview", get(overview::get_event_overview))
        .route("/security/permissions", get(security::get_user_permissions))
        .route("/security/config", get(security::get_security_config))
        .route("/security/config", put(security::update_security_config))
        .route("/security/roles", get(security::get_user_roles))
        .route("/security/audit-logs/{id}", get(security::get_event_audit_logs))
        .route("/security/audit-logs", get(security::get_user_audit_logs))
        .route("/security/audit-logs/all", get(security::get_all_audit_logs))
        .route("/security/cleanup", post(security::cleanup_audit_logs))
        .nest("/performance", performance::create_router())
        // SSE endpoints for real-time event streaming
        .route("/sse", get(sse::handle_sse_connection))
        .route("/sse/overview", get(sse::get_sse_overview))
        .route("/sse/connections", get(sse::get_sse_connections))
}
