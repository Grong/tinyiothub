use axum::{Json, extract::{Path, Query, State}, routing::get};
use serde::Deserialize;

use tinyiothub_web::response::ApiResponseBuilder;
use crate::shared::app_state::AppState;
use tinyiothub_core::memory::AgentMemory;

use super::types::ListMemoriesQuery;

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/{workspace_id}/memories", get(list_active_memories))
        .route("/{workspace_id}/memories/queue", get(get_pending_queue))
        .route(
            "/{workspace_id}/memories/queue/{queue_id}/resolve",
            axum::routing::post(resolve_queue_item),
        )
        .route(
            "/{workspace_id}/memories/{memory_id}/pin",
            axum::routing::put(pin_memory),
        )
}

/// GET /workspaces/{workspace_id}/memories?agent_id=...
async fn list_active_memories(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<ListMemoriesQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<Vec<AgentMemory>>> {
    match state.memory_store.list_active(&workspace_id, &query.agent_id).await {
        Ok(memories) => ApiResponseBuilder::success(memories),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list memories: {}", e)),
    }
}

/// GET /workspaces/{workspace_id}/memories/queue?agent_id=...
async fn get_pending_queue(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<ListMemoriesQuery>,
) -> Json<tinyiothub_web::response::ApiResponse<Vec<tinyiothub_core::memory::ReflectionQueueItem>>> {
    match state.memory_store.get_pending_queue(&workspace_id, &query.agent_id).await {
        Ok(items) => ApiResponseBuilder::success(items),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get queue: {}", e)),
    }
}

/// POST /workspaces/{workspace_id}/memories/queue/{queue_id}/resolve
#[derive(Deserialize)]
struct ResolveBody {
    approved: bool,
    #[serde(default)]
    reviewer_note: Option<String>,
}

async fn resolve_queue_item(
    State(state): State<AppState>,
    Path((_workspace_id, queue_id)): Path<(String, String)>,
    Json(body): Json<ResolveBody>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    match state.memory_store.resolve_queue_item(&queue_id, body.approved, body.reviewer_note.as_deref()).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"resolved": true})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to resolve queue item: {}", e)),
    }
}

/// PUT /workspaces/{workspace_id}/memories/{memory_id}/pin
#[derive(Deserialize)]
struct PinBody {
    pinned: bool,
}

async fn pin_memory(
    State(state): State<AppState>,
    Path((_workspace_id, memory_id)): Path<(String, String)>,
    Json(body): Json<PinBody>,
) -> Json<tinyiothub_web::response::ApiResponse<serde_json::Value>> {
    match state.memory_store.set_pinned(&memory_id, body.pinned).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"pinned": body.pinned})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to pin memory: {}", e)),
    }
}
