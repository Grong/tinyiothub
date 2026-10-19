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
            // Task 7 评审 ⚠️ 钩子②：新增子物 → 含 descendant_count 的派生重算
            if let (Some(manager), Some(parent)) = (&state.derived_manager, &req.parent_id) {
                manager.notify_tree_changed(parent).await;
            }
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

    // 换父 diff（Task 7 评审 ⚠️ 钩子②）：只有请求带 parent_id 才可能换父，
    // 先取旧值，更新成功后比对。
    let old_parent = if req.parent_id.is_some() {
        svc.get_thing(&id, &ws).await.ok().and_then(|t| t.parent_id)
    } else {
        None
    };

    match svc.update_thing(&id, &req, &ws).await {
        Ok(thing) => {
            let parent_changed = req.parent_id.is_some() && req.parent_id != old_parent;
            if parent_changed && let Some(manager) = &state.derived_manager {
                // 新父/旧父两侧子树都变了；Manager 内部对全部含 dc 的 owner
                // 重算（不按 root 过滤），以新父（缺省回退旧父）作 root 记日志。
                let root = req
                    .parent_id
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .or(old_parent.as_deref());
                if let Some(root) = root {
                    manager.notify_tree_changed(root).await;
                }
            }
            (StatusCode::OK, ApiResponseBuilder::success(thing))
        }
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

    // 删除前取 parent（F-B 钩子③ + 树变更钩子② 要用；物不存在时 delete 会
    // 报 NotFound，此处拿不到 parent 也无妨）
    let old_parent = svc.get_thing(&id, &ws).await.ok().and_then(|t| t.parent_id);

    match svc.delete_thing(&id, &ws).await {
        Ok(()) => {
            // Task 7 评审 ⚠️ 钩子③：受影响派生标 stale + 索引/refs 热更新；
            // 删除子物同时是树变更（含 descendant_count 的派生重算）。
            if let Some(manager) = &state.derived_manager {
                manager.notify_thing_deleted(&id).await;
                if let Some(parent) = &old_parent {
                    manager.notify_tree_changed(parent).await;
                }
            }
            (StatusCode::OK, ApiResponseBuilder::success(()))
        }
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
