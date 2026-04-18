// Batch Command API Module
// REST API for batch command management


use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    dto::response::{ApiResponse, builder::ApiResponseBuilder},
    infrastructure::batch_command::{
        BatchCommandExecutor, BatchCommandRepository, BatchCommandWithItems, CreateBatchCommandRequest,
    },
    shared::app_state::AppState,
};

/// Query params for listing batches
#[derive(Debug, Deserialize)]
pub struct ListBatchesQuery {
    pub workspace_id: String,
    pub limit: Option<i32>,
}

/// Create router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_batch))
        .route("/", get(list_batches))
        .route("/{batch_id}", get(get_batch))
        .route("/{batch_id}/execute", post(execute_batch))
}

/// Create a new batch command
async fn create_batch(
    State(state): State<AppState>,
    Json(payload): Json<CreateBatchCommandRequest>,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.database.clone();

    // Check idempotency
    if let Some(existing) = BatchCommandRepository::find_by_idempotency_key(
        &db,
        &payload.workspace_id,
        &payload.idempotency_key,
    )
    .await
    .unwrap_or(None)
    {
        // Return existing batch (idempotent)
        let batch_with_items = BatchCommandRepository::get_batch_with_items(&db, &existing.id)
            .await
            .unwrap_or(None);
        if let Some(bwi) = batch_with_items {
            return ApiResponseBuilder::success(bwi);
        }
    }

    // Create new batch
    match BatchCommandRepository::create(&db, &payload).await {
        Ok(batch_with_items) => ApiResponseBuilder::success(batch_with_items),
        Err(e) => {
            tracing::error!("Failed to create batch command: {}", e);
            ApiResponseBuilder::error("创建批量命令失败")
        }
    }
}

/// List batches for a workspace
async fn list_batches(
    State(state): State<AppState>,
    Query(params): Query<ListBatchesQuery>,
) -> Json<ApiResponse<Vec<crate::infrastructure::batch_command::BatchCommand>>> {
    let db = state.database.clone();
    let limit = params.limit.unwrap_or(20);

    match BatchCommandRepository::list_by_workspace(&db, &params.workspace_id, limit).await {
        Ok(batches) => ApiResponseBuilder::success(batches),
        Err(e) => {
            tracing::error!("Failed to list batches: {}", e);
            ApiResponseBuilder::error("获取批量命令列表失败")
        }
    }
}

/// Get a batch with its items
async fn get_batch(
    State(state): State<AppState>,
    Path(batch_id): Path<String>,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.database.clone();

    match BatchCommandRepository::get_batch_with_items(&db, &batch_id).await {
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
    State(state): State<AppState>,
    Path(batch_id): Path<String>,
) -> Json<ApiResponse<BatchCommandWithItems>> {
    let db = state.database.clone();
    let device_service = state.device_service.clone();

    match BatchCommandExecutor::execute(&db, device_service, &batch_id).await {
        Ok(batch_with_items) => ApiResponseBuilder::success(batch_with_items),
        Err(e) => {
            tracing::error!("Failed to execute batch {}: {}", batch_id, e);
            ApiResponseBuilder::error("执行批量命令失败")
        }
    }
}
