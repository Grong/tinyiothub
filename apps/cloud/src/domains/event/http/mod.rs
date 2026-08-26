// HTTP routes for the event security plane and SSE streaming.
//
// These handlers were part of `modules::event::handler` before the event
// domain crate extraction (P4-Task18). They stay in cloud because they depend
// on the event security plane (`shared::event::security`) and the SSE
// manager/token manager (`shared::event::sse_manager`, auth SSE tokens).
// The notification module was extracted in P4-Task21 (notify crate); the SSE
// manager still consumes `tinyiothub_notify::channels::sse_channel`.
//
// Reclaim task: a future security-plane extraction should move these
// routes (and the `shared::event` infrastructure) out of cloud.

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::state::AppState;

pub mod security;
pub mod sse;

/// Create the cloud-resident event routes (security + SSE), nested at
/// `/events` alongside `crate::domains::event::router()`.
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/security/permissions", get(security::get_user_permissions))
        .route("/security/config", get(security::get_security_config))
        .route("/security/config", put(security::update_security_config))
        .route("/security/roles", get(security::get_user_roles))
        .route("/security/audit-logs/{id}", get(security::get_event_audit_logs))
        .route("/security/audit-logs", get(security::get_user_audit_logs))
        .route("/security/audit-logs/all", get(security::get_all_audit_logs))
        .route("/security/cleanup", post(security::cleanup_audit_logs))
        // SSE endpoints for real-time event streaming
        .route("/sse", get(sse::handle_sse_connection))
        .route("/sse/overview", get(sse::get_sse_overview))
        .route("/sse/connections", get(sse::get_sse_connections))
}
