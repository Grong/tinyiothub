// Thing resource handlers

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponse;

use super::super::{service::ThingService, types::ThingResource};
use crate::{
    api::middleware::WorkspaceScope,
    shared::{api_response::ApiResponseBuilder, app_state::AppState},
};

#[derive(Deserialize)]
pub struct AttachResourceRequest {
    pub resource_id: String,
}

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

// ──────────────────────────────────────────────
// GET /resources/unassigned
// ──────────────────────────────────────────────

pub async fn list_unassigned_resources(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> (StatusCode, Json<ApiResponse<Vec<ThingResource>>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let ws = workspace_id.unwrap_or_default();

    match svc.list_unassigned_resources(&ws).await {
        Ok(resources) => (StatusCode::OK, ApiResponseBuilder::success(resources)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to list unassigned resources");
            (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()))
        }
    }
}

#[derive(Deserialize)]
pub struct CreateResourceRequest {
    pub name: String,
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
}

// ──────────────────────────────────────────────
// POST /resources/upload (create + attach in one step)
// ──────────────────────────────────────────────

pub async fn upload_resource(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(thing_id): Path<String>,
    Json(req): Json<CreateResourceRequest>,
) -> (StatusCode, Json<ApiResponse<ThingResource>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let ws_id = ws.unwrap_or_default();
    let resource_type = req.resource_type.unwrap_or_else(|| "document".to_string());

    match svc.create_and_attach_resource(&ws_id, &thing_id, &req.name, req.content.as_deref(), &resource_type).await {
        Ok(resource) => (StatusCode::CREATED, ApiResponseBuilder::success(resource)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, thing_id = %thing_id, "Failed to upload resource");
            (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()))
        }
    }
}

// ──────────────────────────────────────────────
// POST /{id}/resources
// ──────────────────────────────────────────────

pub async fn attach_resource(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AttachResourceRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    match svc.attach_resource(&id, &req.resource_id).await {
        Ok(()) => (StatusCode::OK, ApiResponseBuilder::success(())),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, thing_id = %id, "Failed to attach resource");
            (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()))
        }
    }
}
