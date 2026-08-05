// Thing resource handlers — attach/detach existing resources.
// File upload is handled by the workspace module (POST /workspaces/{id}/resources/upload).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponse;

use super::super::{service::ThingService, types::ThingResource};
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::ThingState;

#[derive(Deserialize)]
pub struct AttachResourceRequest {
    pub resource_id: String,
}

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

pub async fn list_unassigned_resources(
    State(state): State<ThingState>,
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
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

pub async fn attach_resource(
    State(state): State<ThingState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
    Json(req): Json<AttachResourceRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();
    match svc.attach_resource(&id, &req.resource_id, &ws).await {
        Ok(()) => (StatusCode::OK, ApiResponseBuilder::success(())),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, thing_id = %id, "Failed to attach resource");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}

// ──────────────────────────────────────────────
// DELETE /{id}/resources/{rid} — detach resource from thing
// ──────────────────────────────────────────────

pub async fn detach_resource(
    State(state): State<ThingState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path((thing_id, resource_id)): Path<(String, String)>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);
    let ws = workspace_id.unwrap_or_default();
    match svc.detach_resource(&thing_id, &resource_id, &ws).await {
        Ok(()) => (StatusCode::OK, ApiResponseBuilder::success(())),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, thing_id = %thing_id, resource_id = %resource_id, "Failed to detach resource");
            (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            )
        }
    }
}
