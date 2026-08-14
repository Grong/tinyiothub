// Agents API Module
//
// Provides agent management endpoints:
// - /agents - list agents
// - /agents/{id}/config - agent configuration
// - /workspaces/{id}/heartbeat/* — heartbeat configuration (in workspace handler)
// - /agents/{id}/files/* - workspace files

pub mod agent_tasks;
pub mod files;
pub mod skills;
pub mod types;
pub mod workspace_heartbeat;

#[cfg(test)]
mod tests;

use crate::state::AppState;
use axum::{Router, routing::get};
use tinyiothub_web::security::Claims;

use crate::domains::agent::chat::handler::proxy as chat_proxy;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    crate::state::AppState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        // /agents - list agents
        .route("/", get(list_agents))
        // /agents/{id}/config
        .route("/{id}/config", get(get_agent_config).put(set_agent_config))
        // /agents/{id}/files/*
        .route("/{id}/files", get(files::list_workspace_files))
        .route(
            "/{id}/files/{filename}",
            get(files::get_workspace_file)
                .put(files::put_workspace_file)
                .delete(files::delete_workspace_file),
        )
}

/// GET /api/v1/agents
async fn list_agents(
    state: axum::extract::State<AppState>,
    claims: Claims,
) -> axum::Json<tinyiothub_web::api_response::ApiResponse<serde_json::Value>> {
    chat_proxy::list_agents(state, claims).await
}

/// GET /api/v1/agents/{id}/config
async fn get_agent_config(
    state: axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    claims: Claims,
) -> axum::Json<tinyiothub_web::api_response::ApiResponse<serde_json::Value>> {
    chat_proxy::get_agent_config(state, path, claims).await
}

/// PUT /api/v1/agents/{id}/config
async fn set_agent_config(
    state: axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    claims: Claims,
    json: axum::Json<types::AgentConfigUpdateRequest>,
) -> axum::Json<tinyiothub_web::api_response::ApiResponse<serde_json::Value>> {
    chat_proxy::set_agent_config(state, path, claims, json).await
}
