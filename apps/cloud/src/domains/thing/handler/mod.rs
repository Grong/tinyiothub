// Thing API routes

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use tinyiothub_storage::Db;
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::domains::thing::service::export_template::{self, ExportError};
use crate::state::AppState;

pub mod actions;
pub mod crud;
pub mod import_export;
pub mod resources;

/// Create the thing API router at /api/v1/things.
///
/// Generic over the composition layer's state `S`: handlers extract
/// `State<AppState>`, which axum derives from `S` via `FromRef`.
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(crud::list_things))
        .route("/", post(crud::create_thing))
        .route("/{id}", get(crud::get_thing))
        .route("/{id}", put(crud::update_thing))
        .route("/{id}", delete(crud::delete_thing))
        .route("/{id}/ontology", get(crud::get_thing_ontology))
        .route("/{id}/profile", get(crud::get_thing_profile))
        .route("/{id}/tree", get(crud::get_thing_tree))
        .route("/{id}/export-as-template", post(export_as_template))
        .route("/{id}/actions/{action_name}/invoke", post(actions::invoke_action))
        .route("/{id}/actions/{action_name}/confirm", post(actions::confirm_action))
        .route("/import/dtdl", post(import_export::import_dtdl))
        .route("/import/wot", post(import_export::import_wot))
        .route("/templates/{id}/export/dtdl", get(import_export::export_dtdl))
        .route("/resources/unassigned", get(resources::list_unassigned_resources))
        .route("/{id}/resources", post(resources::attach_resource))
        .route("/{id}/resources/{rid}", delete(resources::detach_resource))
}

// ──────────────────────────────────────────────
// POST /things/{id}/export-as-template — 反向导出场景包模板 JSON 下载
// ──────────────────────────────────────────────

pub async fn export_as_template(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
) -> Response {
    let db = Db::new(state.db.pool().clone());
    let workspace_id = ws.unwrap_or_default();

    match export_template::export_subtree_as_template(&db, &workspace_id, &id).await {
        Ok(outcome) => {
            // warnings 作为附加顶层键随文件下发（SceneTemplateFile 解析时忽略未知键）
            let mut value = serde_json::to_value(&outcome.file).unwrap_or_default();
            if !outcome.warnings.is_empty() {
                value["warnings"] = serde_json::json!(outcome.warnings);
            }
            let body = serde_json::to_string_pretty(&value).unwrap_or_default();
            let mut resp = (StatusCode::OK, body).into_response();
            let headers = resp.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"scene-template-{id}.json\""))
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            );
            resp
        }
        Err(e) => {
            let status = match &e {
                ExportError::NotFound(_) => StatusCode::NOT_FOUND,
                ExportError::TooLarge(_) => StatusCode::BAD_REQUEST,
                ExportError::Database(_) | ExportError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            tracing::error!(?e, thing_id = %id, "export-as-template failed");
            (
                status,
                ApiResponseBuilder::error_with_code::<serde_json::Value>(status.as_u16() as i32, e.to_string()),
            )
                .into_response()
        }
    }
}
