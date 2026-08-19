// Thing CRUD handlers

use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tinyiothub_web::response::ApiResponse;

use super::super::{
    service::ThingService,
    types::{
        CreateThingRequest, ListThingsParams, ThingProfileResponse, ThingResponse, ThingTreeNode, UpdateThingRequest,
    },
};
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

// ──────────────────────────────────────────────
// GET /things
// ──────────────────────────────────────────────

pub async fn list_things(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Query(params): Query<ListThingsParams>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);

    let ws = workspace_id.unwrap_or_default();

    match svc.list_things(&ws, &params).await {
        Ok(result) => {
            let data = serde_json::to_value(&result).unwrap_or_default();
            (StatusCode::OK, ApiResponseBuilder::success(data))
        }
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to list things");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// POST /things
// ──────────────────────────────────────────────

pub async fn create_thing(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Json(req): Json<CreateThingRequest>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);

    let workspace_id = ws.unwrap_or_default();

    match svc.create_thing(&req, Some(workspace_id.as_str())).await {
        Ok(thing) => {
            let resp = ApiResponseBuilder::success(thing);
            (StatusCode::CREATED, resp)
        }
        Err(e) => {
            let status = e.status_code();
            let resp = ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string());
            (status, resp)
        }
    }
}

// ──────────────────────────────────────────────
// GET /things/:id
// ──────────────────────────────────────────────

pub async fn get_thing(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();

    match svc.get_thing(&id, &ws).await {
        Ok(thing) => (StatusCode::OK, ApiResponseBuilder::success(thing)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to get thing");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// PUT /things/:id
// ──────────────────────────────────────────────

pub async fn update_thing(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
    Json(req): Json<UpdateThingRequest>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();

    match svc.update_thing(&id, &req, &ws).await {
        Ok(thing) => (StatusCode::OK, ApiResponseBuilder::success(thing)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to update thing");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// DELETE /things/:id
// ──────────────────────────────────────────────

pub async fn delete_thing(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();

    match svc.delete_thing(&id, &ws).await {
        Ok(()) => (StatusCode::OK, ApiResponseBuilder::success(())),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to delete thing");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// GET /things/:id/ontology
// ──────────────────────────────────────────────

/// Alias for get_thing — returns the thing with its ontology summary.
pub async fn get_thing_ontology(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    get_thing(State(state), WorkspaceScope(workspace_id), Path(id)).await
}

// ──────────────────────────────────────────────
// GET /things/:id/profile
// ──────────────────────────────────────────────

pub async fn get_thing_profile(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<ThingProfileResponse>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();

    match svc.get_thing_profile(&id, &ws).await {
        Ok(profile) => {
            tracing::info!(
                thing_id = %id,
                props = profile.properties.as_ref().map_or(0, |v| v.len()),
                actions = profile.actions.as_ref().map_or(0, |v| v.len()),
                events = profile.recent_events.as_ref().map_or(0, |v| v.len()),
                docs = profile.knowledge_docs.as_ref().map_or(0, |v| v.len()),
                "Thing profile loaded"
            );
            (StatusCode::OK, ApiResponseBuilder::success(profile))
        }
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, thing_id = %id, "Failed to get thing profile");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// GET /things/:id/tree
// ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct TreeQuery {
    pub depth: Option<u32>,
}

pub async fn get_thing_tree(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
    Query(query): Query<TreeQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<ThingTreeNode>>>) {
    let pool = state.db.pool().clone();
    let svc = thing_service(&pool);

    let ws = workspace_id.unwrap_or_default();

    match svc.get_thing_tree(&ws, Some(&id), query.depth).await {
        Ok(tree) => (StatusCode::OK, ApiResponseBuilder::success(tree)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to get thing tree");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}
