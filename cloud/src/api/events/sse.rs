// API Layer - SSE Endpoints
// Handles HTTP requests for Server-Sent Events (SSE) connections

use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Query, State},
    response::Response,
    Json,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::{
    dto::response::ApiResponse,
    infrastructure::event::sse_manager::{SseConnectionInfo, SseOverview},
    shared::{app_state::AppState},
};
use crate::api::middleware::WorkspaceScope;

/// SSE connection query parameters
#[derive(Debug, Deserialize)]
pub struct SseConnectionQuery {
    /// User ID for the connection
    pub user_id: Option<String>,

    /// Workspace ID to scope events to (fallback: X-Workspace-Id header)
    pub workspace_id: Option<String>,

    /// Comma-separated list of event types to filter
    /// Example: "system.auth,device.connection,device.data"
    pub event_types: Option<String>,

    /// Comma-separated list of event levels to filter
    /// Example: "critical,error,warning"
    pub event_levels: Option<String>,
}

/// Handle authenticated SSE connection for real-time event notifications
///
/// This endpoint requires JWT authentication and creates a persistent
/// SSE connection for streaming events to the client.
#[axum::debug_handler]
pub async fn handle_sse_connection(
    Query(query): Query<SseConnectionQuery>,
    State(state): State<AppState>,
    workspace_scope: WorkspaceScope,
    claims: Claims,
) -> Response {
    // Use user_id from query or fall back to JWT claims
    let user_id = query.user_id.clone().unwrap_or_else(|| claims.user_id.clone());

    // Workspace: query param > X-Workspace-Id header > "default"
    // Frontend must pass workspace_id in the SSE URL so events are correctly filtered.
    let workspace_id = query
        .workspace_id
        .clone()
        .or(workspace_scope.0)
        .unwrap_or_else(|| "default".to_string());

    info!("New authenticated SSE connection from user: {} workspace: {}", user_id, workspace_id);

    // Parse event filters
    let event_types = parse_event_types(&query.event_types);
    let event_levels = parse_event_levels(&query.event_levels);

    // Create SSE connection through the manager
    let sse_manager = state.get_sse_manager();
    sse_manager.create_connection(user_id, workspace_id, event_types, event_levels).await
}

/// Handle public (unauthenticated) SSE connection
///
/// This endpoint does not require authentication and is intended for
/// testing or public event streams. Use with caution in production.
#[axum::debug_handler]
pub async fn handle_sse_connection_public(
    Query(query): Query<SseConnectionQuery>,
    State(state): State<AppState>,
) -> Response {
    let user_id = query.user_id.clone().unwrap_or_else(|| "anonymous".to_string());
    let workspace_id = query.workspace_id.clone().unwrap_or_else(|| "default".to_string());

    warn!("New public (unauthenticated) SSE connection from user: {} workspace: {}", user_id, workspace_id);

    // Parse event filters
    let event_types = parse_event_types(&query.event_types);
    let event_levels = parse_event_levels(&query.event_levels);

    // Create public SSE connection
    let sse_manager = state.get_sse_manager();
    sse_manager.create_public_connection(user_id, workspace_id, event_types, event_levels).await
}

/// Get SSE connection overview
///
/// Returns metrics about active SSE connections, including total count,
/// events sent, and average latency.
#[axum::debug_handler]
pub async fn get_sse_overview(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SseOverview>> {
    let sse_manager = state.get_sse_manager();
    let overview = sse_manager.get_overview().await;

    ApiResponseBuilder::success(overview)
}

/// Get list of active SSE connections
///
/// Returns information about all currently active SSE connections,
/// including user IDs, connection times, and filters.
#[axum::debug_handler]
pub async fn get_sse_connections(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<SseConnectionInfo>>> {
    let sse_manager = state.get_sse_manager();
    let connections = sse_manager.get_connections().await;

    ApiResponseBuilder::success(connections)
}

// === Helper Functions ===

/// Parse comma-separated event types from query string
fn parse_event_types(types_str: &Option<String>) -> Option<Vec<String>> {
    types_str
        .as_ref()
        .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
}

/// Parse comma-separated event levels from query string
fn parse_event_levels(levels_str: &Option<String>) -> Option<Vec<String>> {
    levels_str
        .as_ref()
        .map(|s| s.split(',').map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_types() {
        let types = parse_event_types(&Some("system.auth,device.connection".to_string()));
        assert_eq!(types, Some(vec!["system.auth".to_string(), "device.connection".to_string()]));

        let empty = parse_event_types(&None);
        assert_eq!(empty, None);
    }

    #[test]
    fn test_parse_event_levels() {
        let levels = parse_event_levels(&Some("CRITICAL,Error,warning".to_string()));
        assert_eq!(
            levels,
            Some(vec!["critical".to_string(), "error".to_string(), "warning".to_string()])
        );

        let empty = parse_event_levels(&None);
        assert_eq!(empty, None);
    }
}
