use axum::{
    Json,
    extract::{Path, Query, State},
    routing::get,
};
use serde::Deserialize;
use tinyiothub_core::memory::AgentMemory;
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::ListMemoriesQuery;
use crate::{
    api::middleware::WorkspaceScope,
    shared::{agent::config::default_model, app_state::AppState},
};

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/memories", get(list_active_memories))
        .route("/memories/queue", get(get_pending_queue))
        .route("/memories/queue/{queue_id}/resolve", axum::routing::post(resolve_queue_item))
        .route("/memories/{memory_id}/pin", axum::routing::put(pin_memory))
        .route("/memories/profile/compile", axum::routing::post(compile_profile))
        .route("/memories/digest/weekly", axum::routing::post(generate_weekly_digest))
}

/// GET /memories?agent_id=...
async fn list_active_memories(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Query(query): Query<ListMemoriesQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<Vec<AgentMemory>>> {
    let ws = workspace_id.unwrap_or_default();
    match state.memory_store.list_active(&ws, &query.agent_id).await {
        Ok(memories) => ApiResponseBuilder::success(memories),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list memories: {}", e)),
    }
}

/// GET /memories/queue?agent_id=...
async fn get_pending_queue(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Query(query): Query<ListMemoriesQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<Vec<tinyiothub_core::memory::ReflectionQueueItem>>>
{
    let ws = workspace_id.unwrap_or_default();
    match state.memory_store.get_pending_queue(&ws, &query.agent_id).await {
        Ok(items) => ApiResponseBuilder::success(items),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get queue: {}", e)),
    }
}

/// POST /memories/queue/{queue_id}/resolve
#[derive(Deserialize)]
struct ResolveBody {
    approved: bool,
    #[serde(default)]
    reviewer_note: Option<String>,
}

async fn resolve_queue_item(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(queue_id): Path<String>,
    Json(body): Json<ResolveBody>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    let ws = workspace_id.unwrap_or_default();
    match state
        .memory_store
        .resolve_queue_item(&queue_id, &ws, body.approved, body.reviewer_note.as_deref())
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"resolved": true})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to resolve queue item: {}", e)),
    }
}

/// PUT /memories/{memory_id}/pin
#[derive(Deserialize)]
struct PinBody {
    pinned: bool,
}

async fn pin_memory(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(memory_id): Path<String>,
    Json(body): Json<PinBody>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    let _ws = workspace_id.unwrap_or_default();
    match state.memory_store.set_pinned(&memory_id, body.pinned).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"pinned": body.pinned})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to pin memory: {}", e)),
    }
}

/// POST /memories/profile/compile?agent_id=...
#[derive(Deserialize)]
struct ProfileCompileQuery {
    agent_id: String,
}

async fn compile_profile(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Query(query): Query<ProfileCompileQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    let ws = workspace_id.unwrap_or_default();

    let model =
        crate::modules::agent::config::service::get_config(&state.db_pool(), &query.agent_id)
            .await
            .map(|c| c.model)
            .unwrap_or_else(|_| default_model());

    let svc = match &state.agent_pool.reflection_service {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Reflection engine not enabled".to_string()),
    };

    match svc.compile_profile(&ws, &query.agent_id, &model).await {
        Ok(profile) => ApiResponseBuilder::success(serde_json::json!({"profile": profile})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to compile profile: {}", e)),
    }
}

/// POST /memories/digest/weekly?agent_id=...
async fn generate_weekly_digest(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Query(query): Query<ProfileCompileQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    let ws = workspace_id.unwrap_or_default();

    let model =
        crate::modules::agent::config::service::get_config(&state.db_pool(), &query.agent_id)
            .await
            .map(|c| c.model)
            .unwrap_or_else(|_| default_model());

    let prompt = match crate::modules::agent::reflection::notifications::generate_weekly_digest(
        &*state.memory_store,
        &ws,
        &query.agent_id,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => return ApiResponseBuilder::error(format!("Failed to build digest: {}", e)),
    };

    let svc = match &state.agent_pool.reflection_service {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Reflection engine not enabled".to_string()),
    };

    match svc.generate_digest(&prompt, &model).await {
        Ok(digest) => ApiResponseBuilder::success(serde_json::json!({"digest": digest})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to generate digest: {}", e)),
    }
}
