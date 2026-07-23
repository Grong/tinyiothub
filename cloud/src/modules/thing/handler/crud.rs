// Thing CRUD handlers

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use tinyiothub_web::response::ApiResponse;

use crate::shared::{api_response::ApiResponseBuilder, app_state::AppState};

use super::super::{
    service::ThingService,
    types::{
        CreateThingRequest, ListThingsParams, ThingProfileResponse, ThingResponse, ThingTreeNode,
        UpdateThingRequest,
    },
};

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

// ──────────────────────────────────────────────
// GET /things
// ──────────────────────────────────────────────

pub async fn list_things(
    State(state): State<AppState>,
    Query(params): Query<ListThingsParams>,
) -> Json<ApiResponse<serde_json::Value>> {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let workspace_id = "default"; // TODO: resolve from JWT claims / WorkspaceScope

    match svc.list_things(workspace_id, &params).await {
        Ok(result) => ApiResponseBuilder::success(serde_json::to_value(&result).unwrap_or_default()),
        Err(e) => {
            tracing::error!(?e, "Failed to list things");
            ApiResponseBuilder::error(e.to_string())
        }
    }
}

// ──────────────────────────────────────────────
// POST /things
// ──────────────────────────────────────────────

pub async fn create_thing(
    State(state): State<AppState>,
    Json(req): Json<CreateThingRequest>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let workspace_id = req.workspace_id.clone();

    match svc.create_thing(&req, workspace_id.as_deref()).await {
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
    Path(id): Path<String>,
) -> Json<ApiResponse<ThingResponse>> {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    match svc.get_thing(&id).await {
        Ok(thing) => ApiResponseBuilder::success(thing),
        Err(e) => {
            tracing::error!(?e, "Failed to get thing");
            ApiResponseBuilder::error(e.to_string())
        }
    }
}

// ──────────────────────────────────────────────
// PUT /things/:id
// ──────────────────────────────────────────────

pub async fn update_thing(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateThingRequest>,
) -> (StatusCode, Json<ApiResponse<ThingResponse>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    match svc.update_thing(&id, &req).await {
        Ok(thing) => (StatusCode::OK, ApiResponseBuilder::success(thing)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to update thing");
            (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()))
        }
    }
}

// ──────────────────────────────────────────────
// DELETE /things/:id
// ──────────────────────────────────────────────

pub async fn delete_thing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    match svc.delete_thing(&id).await {
        Ok(()) => (StatusCode::OK, ApiResponseBuilder::success(())),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to delete thing");
            (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()))
        }
    }
}

// ──────────────────────────────────────────────
// GET /things/:id/ontology
// ──────────────────────────────────────────────

/// Alias for get_thing — returns the thing with its ontology summary.
pub async fn get_thing_ontology(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ThingResponse>> {
    get_thing(State(state), Path(id)).await
}

// ──────────────────────────────────────────────
// GET /things/:id/profile
// ──────────────────────────────────────────────

pub async fn get_thing_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ThingProfileResponse>> {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    match svc.get_thing_profile(&id).await {
        Ok(profile) => ApiResponseBuilder::success(profile),
        Err(e) => {
            tracing::error!(?e, "Failed to get thing profile");
            ApiResponseBuilder::error(e.to_string())
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
    Path(id): Path<String>,
    Query(query): Query<TreeQuery>,
) -> Json<ApiResponse<Vec<ThingTreeNode>>> {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let workspace_id = "default"; // TODO: resolve from JWT claims

    match svc
        .get_thing_tree(workspace_id, Some(&id), query.depth)
        .await
    {
        Ok(tree) => ApiResponseBuilder::success(tree),
        Err(e) => {
            tracing::error!(?e, "Failed to get thing tree");
            ApiResponseBuilder::error(e.to_string())
        }
    }
}
