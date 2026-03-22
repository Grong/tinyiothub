use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    domain::alarm::ResolutionType,
    dto::{
        request::{
            AcknowledgeAlarmRequest, BatchAcknowledgeRequest, BatchResolveRequest,
            ResolveAlarmRequest,
        },
        response::{api_response::ApiResponse, builder::ApiResponseBuilder, BatchOperationResult},
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

/// 确认报警
pub async fn acknowledge_alarm(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<AcknowledgeAlarmRequest>,
) -> Json<ApiResponse<()>> {
    match state.alarm_service.acknowledge_alarm(&id, claims.user_id, req.note).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("确认报警失败: {}", e)),
    }
}

/// 解决报警
pub async fn resolve_alarm(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<ResolveAlarmRequest>,
) -> Json<ApiResponse<()>> {
    let resolution_type = match req.resolution_type.as_str() {
        "fixed" => ResolutionType::Fixed,
        "false_alarm" => ResolutionType::FalseAlarm,
        "ignored" => ResolutionType::Ignored,
        "auto_resolved" => ResolutionType::AutoResolved,
        _ => ResolutionType::Fixed,
    };

    match state.alarm_service.resolve_alarm(&id, claims.user_id, resolution_type, req.note).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("解决报警失败: {}", e)),
    }
}

/// 批量确认报警
pub async fn batch_acknowledge(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<BatchAcknowledgeRequest>,
) -> Json<ApiResponse<BatchOperationResult>> {
    let total_count = req.alarm_ids.len();

    match state.alarm_service.batch_acknowledge(req.alarm_ids, claims.user_id).await {
        Ok(success_count) => {
            ApiResponseBuilder::success(BatchOperationResult { success_count, total_count })
        }
        Err(e) => ApiResponseBuilder::error(format!("批量确认失败: {}", e)),
    }
}

/// 批量解决报警
pub async fn batch_resolve(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<BatchResolveRequest>,
) -> Json<ApiResponse<BatchOperationResult>> {
    let total_count = req.alarm_ids.len();

    let resolution_type = match req.resolution_type.as_str() {
        "fixed" => ResolutionType::Fixed,
        "false_alarm" => ResolutionType::FalseAlarm,
        "ignored" => ResolutionType::Ignored,
        _ => ResolutionType::Fixed,
    };

    match state.alarm_service.batch_resolve(req.alarm_ids, claims.user_id, resolution_type).await {
        Ok(success_count) => {
            ApiResponseBuilder::success(BatchOperationResult { success_count, total_count })
        }
        Err(e) => ApiResponseBuilder::error(format!("批量解决失败: {}", e)),
    }
}
