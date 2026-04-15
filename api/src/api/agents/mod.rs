// Agents API Module
//
// Provides agent management endpoints:
// - /agents - list agents
// - /agents/{id}/config - agent configuration
// - /agents/{id}/heartbeat/* - heartbeat configuration
// - /agents/{id}/files/* - workspace files

pub mod heartbeat;
pub mod files;
pub mod skills;
pub mod types;

#[cfg(test)]
mod tests;

use axum::{
    routing::get,
    Router,
};
use crate::shared::app_state::AppState;

use crate::api::chat::proxy as chat_proxy;

pub fn create_router() -> Router<AppState> {
    Router::new()
        // /agents - list agents
        .route("/", get(list_agents))
        // /agents/{id}/config
        .route("/{id}/config", get(get_agent_config).put(set_agent_config))
        // /agents/{id}/heartbeat/*
        .nest("/{id}/heartbeat", heartbeat::create_router())
        // /agents/{id}/files/*
        .route("/{id}/files", get(files::list_workspace_files))
        .route("/{id}/files/{filename}", get(files::get_workspace_file).put(files::put_workspace_file))
}

/// GET /api/v1/agents
async fn list_agents(
    state: axum::extract::State<AppState>,
    claims: crate::shared::security::jwt::Claims,
) -> axum::Json<crate::dto::response::ApiResponse<serde_json::Value>> {
    chat_proxy::list_agents(state, claims).await
}

/// GET /api/v1/agents/{id}/config
async fn get_agent_config(
    state: axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    claims: crate::shared::security::jwt::Claims,
) -> axum::Json<crate::dto::response::ApiResponse<serde_json::Value>> {
    chat_proxy::get_agent_config(state, path, claims).await
}

/// PUT /api/v1/agents/{id}/config
async fn set_agent_config(
    state: axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    claims: crate::shared::security::jwt::Claims,
    json: axum::Json<crate::api::agents::types::AgentConfigUpdateRequest>,
) -> axum::Json<crate::dto::response::ApiResponse<serde_json::Value>> {
    chat_proxy::set_agent_config(state, path, claims, json).await
}
