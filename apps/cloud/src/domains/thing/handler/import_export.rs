// Thing template import/export handlers — DTDL and WoT Thing Description

use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;
use tinyiothub_web::response::ApiResponse;

use super::super::service::import_export::{self, ImportError};
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

// ──────────────────────────────────────────────
// POST /things/import/dtdl
// ──────────────────────────────────────────────

pub async fn import_dtdl(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Json(body): Json<Value>,
) -> (StatusCode, Json<ApiResponse<Value>>) {
    let pool = state.db.pool().clone();
    let workspace_id = ws.unwrap_or_default();

    // 场景包旁路：根级含非空 children → SceneTemplateFile 校验 → device_info 存原文注册
    if import_export::is_scene_template_json(&body) {
        return match import_export::import_scene_template(&pool, &body, Some(workspace_id.as_str())).await {
            Ok(outcome) => (
                StatusCode::CREATED,
                ApiResponseBuilder::success(serde_json::json!({
                    "id": outcome.id,
                    "name": outcome.name,
                    "thingType": outcome.thing_type,
                    "scene": true,
                })),
            ),
            Err(e) => import_error_response(e),
        };
    }

    let parsed = match import_export::parse_dtdl(&body) {
        Ok(p) => p,
        Err(e) => return import_error_response(e),
    };

    match import_export::save_template(&pool, &parsed, Some(workspace_id.as_str())).await {
        Ok(template_id) => (
            StatusCode::CREATED,
            ApiResponseBuilder::success(serde_json::json!({
                "id": template_id,
                "name": parsed.name,
                "thingType": parsed.thing_type,
            })),
        ),
        Err(e) => import_error_response(e),
    }
}

// ──────────────────────────────────────────────
// POST /things/import/wot
// ──────────────────────────────────────────────

pub async fn import_wot(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Json(body): Json<Value>,
) -> (StatusCode, Json<ApiResponse<Value>>) {
    let pool = state.db.pool().clone();

    let parsed = match import_export::parse_wot_td(&body) {
        Ok(p) => p,
        Err(e) => return import_error_response(e),
    };

    let workspace_id = ws.unwrap_or_default();

    match import_export::save_template(&pool, &parsed, Some(workspace_id.as_str())).await {
        Ok(template_id) => (
            StatusCode::CREATED,
            ApiResponseBuilder::success(serde_json::json!({
                "id": template_id,
                "name": parsed.name,
                "thingType": parsed.thing_type,
            })),
        ),
        Err(e) => import_error_response(e),
    }
}

// ──────────────────────────────────────────────
// GET /things/templates/{id}/export/dtdl
// ──────────────────────────────────────────────

pub async fn export_dtdl(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<Value>>) {
    let pool = state.db.pool().clone();

    let template = match import_export::load_template(&pool, &id).await {
        Ok(t) => t,
        Err(e) => return import_error_response(e),
    };

    let dtdl = match import_export::export_to_dtdl(&template) {
        Ok(v) => v,
        Err(e) => return import_error_response(e),
    };

    (StatusCode::OK, ApiResponseBuilder::success(dtdl))
}

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

fn import_error_response(e: ImportError) -> (StatusCode, Json<ApiResponse<Value>>) {
    let status = match &e {
        ImportError::NotFound(_) => StatusCode::NOT_FOUND,
        ImportError::NameConflict(_) => StatusCode::CONFLICT,
        ImportError::UnsupportedType(_) | ImportError::MissingField(_) | ImportError::InvalidJson(_) => {
            StatusCode::BAD_REQUEST
        }
        ImportError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    tracing::error!(?e, "Import/export error");
    (
        status,
        ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
    )
}
