// Thing resource handlers

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponse;

use super::super::{
    service::ThingService,
    types::ThingResource,
};
use crate::shared::{api_response::ApiResponseBuilder, app_state::AppState};

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
) -> (StatusCode, Json<ApiResponse<Vec<ThingResource>>>) {
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let workspace_id = "default"; // TODO: resolve from JWT

    match svc.list_unassigned_resources(workspace_id).await {
        Ok(resources) => (StatusCode::OK, ApiResponseBuilder::success(resources)),
        Err(e) => {
            let status = e.status_code();
            tracing::error!(?e, "Failed to list unassigned resources");
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
