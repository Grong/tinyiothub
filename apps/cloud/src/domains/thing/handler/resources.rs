// Thing resource handlers — attach/detach existing resources.
// File upload is handled by the workspace module (POST /workspaces/{id}/resources/upload).

use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponse;

use super::super::{service::ThingService, types::ThingResource};
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

#[derive(Deserialize)]
pub struct AttachResourceRequest {
    pub resource_id: String,
}

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

pub async fn list_unassigned_resources(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> (StatusCode, Json<ApiResponse<Vec<ThingResource>>>) {
    let pool = state.db.pool().clone();
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
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
    Json(req): Json<AttachResourceRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.db.pool().clone();
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
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path((thing_id, resource_id)): Path<(String, String)>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let pool = state.db.pool().clone();
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

// ──────────────────────────────────────────────
// GET /resources/{id}/file — 资源文件服务面（workspace 隔离 + 防穿越）
// ──────────────────────────────────────────────

/// 资源文件的规范访问 URL（与 api/mod.rs 的 T5 路由一致；知识工具/A2UI 共用）。
pub fn resource_file_url(resource_id: &str) -> String {
    format!("/api/v1/resources/{resource_id}/file")
}

/// GET /api/v1/resources/{id}/file
///
/// `/uploads` 由裸 ServeDir 直出（无认证、无 workspace 隔离），本端点是带
/// JWT workspace 校验的服务面：resources.workspace_id 必须与调用方一致。
/// 越权/不存在/路径穿越一律 404（不区分存在性）。
pub async fn serve_resource_file(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> Response {
    let ws = workspace_id.unwrap_or_default();
    let db = tinyiothub_storage::Db::new(state.db.pool().clone());
    let doc = match db.find_thing_document(&id, &ws).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(?e, resource_id = %id, "Failed to load resource row");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(path) = resolve_resource_path(&state.agents_base_dir, &doc.file_path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type_for(&path))], bytes).into_response(),
        Err(e) => {
            tracing::warn!(?e, resource_id = %id, "Resource file unreadable");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// file_path（上传约定 `/uploads/{ws}/uploads/{name}`）→ agents_base_dir 下的安全路径。
/// 拒绝 `..`/根/前缀组件；canonicalize 后必须仍在资源根目录下（符号链接逃逸同拒）。
/// 文件不存在时 canonicalize 失败 → None → 404。
async fn resolve_resource_path(agents_base_dir: &std::path::Path, file_path: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path as FsPath};

    let rel = file_path.trim_start_matches('/');
    let rel = rel.strip_prefix("uploads/").unwrap_or(rel);
    let rel_path = FsPath::new(rel);
    if rel_path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let candidate = agents_base_dir.join(rel_path);
    let canonical = tokio::fs::canonicalize(&candidate).await.ok()?;
    let root = tokio::fs::canonicalize(agents_base_dir).await.ok()?;
    canonical.starts_with(&root).then_some(canonical)
}

/// Content-Type 按扩展名（glb/gltf → glTF 模型；常见图片 → image/*）。
fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("glb") => "model/gltf-binary",
        Some("gltf") => "model/gltf+json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}
