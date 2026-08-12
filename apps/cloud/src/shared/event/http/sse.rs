// API Layer - SSE Endpoints
// Handles HTTP requests for Server-Sent Events (SSE) connections
//
// 认证方式（按优先级）：
// 1. 短期 SSE token（?sse_token=xxx）— 推荐，不暴露 JWT 到 URL
// 2. JWT Authorization header + WorkspaceScope middleware
//    用于向后兼容
// 3. JWT 在 URL 中（?token=xxx）— 仅用于不支持 header 的场景

use crate::domains::auth::security::jwt::Claims;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json as JsonResponse, Response},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;
use tracing::{info, warn};

use crate::{
    api::middleware::WorkspaceScope,
    shared::{
        api_response::ApiResponse,
        app_state::AppState,
        event::sse_manager::{SseConnectionInfo, SseOverview},
    },
};

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

    /// 短期 SSE token（替代 ?token=xxx 中的 JWT 暴露）
    /// 通过 POST /api/v1/auth/sse-token 获取，有效期 5 分钟
    pub sse_token: Option<String>,
}

/// 旧的 SSE 端点（受 JWT middleware 保护）— 保留向后兼容
///
/// 这个端点在受保护的路由组中，由 JWT middleware 认证。
/// 前端的 DeviceCache 使用它。
#[axum::debug_handler]
pub async fn handle_sse_connection(
    Query(query): Query<SseConnectionQuery>,
    State(state): State<AppState>,
    workspace_scope: WorkspaceScope,
    claims: Claims,
) -> Response {
    // Workspace: query param > X-Workspace-Id header > claims.workspace_id
    let user_id = claims.user_id.clone();
    let workspace_id = query
        .workspace_id
        .clone()
        .or_else(|| workspace_scope.0.clone())
        .unwrap_or_else(|| {
            if claims.workspace_id.is_empty() {
                "default".to_string()
            } else {
                claims.workspace_id.clone()
            }
        });

    info!(
        "New JWT-authenticated SSE connection from user: {} workspace: {}",
        user_id, workspace_id
    );

    let event_types = parse_event_types(&query.event_types);
    let event_levels = parse_event_levels(&query.event_levels);

    let sse_manager = state.get_sse_manager();
    sse_manager
        .create_connection(user_id, workspace_id, event_types, event_levels)
        .await
}

/// Handle SSE connection via SSE token（无需 JWT middleware）
///
/// 这个端点在公共路由组中，不支持 JWT header。
/// 客户端先通过 POST /api/v1/auth/sse-token 获取 token，
/// 然后使用 ?sse_token=xxx 连接此端点。
#[axum::debug_handler]
pub async fn handle_sse_connection_token(
    Query(query): Query<SseConnectionQuery>,
    State(state): State<AppState>,
) -> Response {
    let sse_token = match query.sse_token.as_ref() {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                JsonResponse(serde_json::json!({
                    "error": "Missing sse_token — use POST /api/v1/auth/sse-token first"
                })),
            )
                .into_response();
        }
    };

    let (user_id, workspace_id) = match state.get_sse_token_manager().validate_and_consume(sse_token) {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                JsonResponse(serde_json::json!({
                    "error": "Invalid or expired SSE token"
                })),
            )
                .into_response();
        }
    };

    let workspace_id = query.workspace_id.clone().unwrap_or(workspace_id);

    info!(
        "New SSE token connection from user: {} workspace: {}",
        user_id, workspace_id
    );

    let event_types = parse_event_types(&query.event_types);
    let event_levels = parse_event_levels(&query.event_levels);

    let sse_manager = state.get_sse_manager();
    sse_manager
        .create_connection(user_id, workspace_id, event_types, event_levels)
        .await
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

    warn!(
        "New public (unauthenticated) SSE connection from user: {} workspace: {}",
        user_id, workspace_id
    );

    // Parse event filters
    let event_types = parse_event_types(&query.event_types);
    let event_levels = parse_event_levels(&query.event_levels);

    // Create public SSE connection
    let sse_manager = state.get_sse_manager();
    sse_manager
        .create_public_connection(user_id, workspace_id, event_types, event_levels)
        .await
}

/// Get SSE connection overview
///
/// Returns metrics about active SSE connections, including total count,
/// events sent, and average latency.
#[axum::debug_handler]
pub async fn get_sse_overview(State(state): State<AppState>, _claims: Claims) -> Json<ApiResponse<SseOverview>> {
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
    types_str.as_ref().map(|s| {
        s.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    })
}

/// Parse comma-separated event levels from query string
fn parse_event_levels(levels_str: &Option<String>) -> Option<Vec<String>> {
    levels_str.as_ref().map(|s| {
        s.split(',')
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_types() {
        let types = parse_event_types(&Some("system.auth,device.connection".to_string()));
        assert_eq!(
            types,
            Some(vec!["system.auth".to_string(), "device.connection".to_string()])
        );

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
