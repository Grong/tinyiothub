// Batch Command API — moved from api/batch/mod.rs

use crate::domains::admin::AdminState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

use crate::domains::admin::batch::batch_command::{
    BatchCommandExecutor, BatchCommandWithItems, CreateBatchCommandRequest,
};

/// Query params for listing batches
#[derive(Debug, Deserialize)]
pub struct ListBatchesQuery {
    pub workspace_id: String,
    pub limit: Option<i32>,
}

/// Create router
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", post(create_batch))
        .route("/", get(list_batches))
        .route("/{batch_id}", get(get_batch))
        .route("/{batch_id}/execute", post(execute_batch))
}

/// Create a new batch command
async fn create_batch(
    State(state): State<AdminState>,
    Json(payload): Json<CreateBatchCommandRequest>,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.db.clone();

    // Check idempotency
    if let Some(existing) = db
        .find_batch_command_by_idempotency_key(&payload.workspace_id, &payload.idempotency_key)
        .await
        .unwrap_or(None)
    {
        // Return existing batch (idempotent)
        let batch_with_items = db.get_batch_command_with_items(&existing.id).await.unwrap_or(None);
        if let Some(bwi) = batch_with_items {
            return ApiResponseBuilder::success(bwi);
        }
    }

    // Create new batch
    match db.create_batch_command(&payload).await {
        Ok(batch_with_items) => ApiResponseBuilder::success(batch_with_items),
        Err(e) => {
            tracing::error!("Failed to create batch command: {}", e);
            ApiResponseBuilder::error("创建批量命令失败")
        }
    }
}

/// List batches for a workspace
async fn list_batches(
    State(state): State<AdminState>,
    Query(params): Query<ListBatchesQuery>,
) -> Json<ApiResponse<Vec<crate::domains::admin::batch::batch_command::BatchCommand>>> {
    let db = state.db.clone();
    let limit = params.limit.unwrap_or(20);

    match db.list_batch_commands_by_workspace(&params.workspace_id, limit).await {
        Ok(batches) => ApiResponseBuilder::success(batches),
        Err(e) => {
            tracing::error!("Failed to list batches: {}", e);
            ApiResponseBuilder::error("获取批量命令列表失败")
        }
    }
}

/// Get a batch with its items
async fn get_batch(
    State(state): State<AdminState>,
    Path(batch_id): Path<String>,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.db.clone();

    match db.get_batch_command_with_items(&batch_id).await {
        Ok(Some(batch_with_items)) => ApiResponseBuilder::success(batch_with_items),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "批量命令不存在"),
        Err(e) => {
            tracing::error!("Failed to get batch: {}", e);
            ApiResponseBuilder::error("获取批量命令失败")
        }
    }
}

/// Execute a batch command (send commands to all devices)
async fn execute_batch(
    State(state): State<AdminState>,
    Path(batch_id): Path<String>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.db.clone();
    let tenant_device_service = state.tenant_device_service(&workspace_id);

    match BatchCommandExecutor::execute(&db, tenant_device_service, &batch_id).await {
        Ok(batch_with_items) => ApiResponseBuilder::success(batch_with_items),
        Err(e) => {
            tracing::error!("Failed to execute batch {}: {}", batch_id, e);
            ApiResponseBuilder::error("执行批量命令失败")
        }
    }
}
