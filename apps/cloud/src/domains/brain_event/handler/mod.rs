//! AI 大脑 P0 Task 3：/brain-events 只读端点 handler。
//!
//! ```text
//! GET /brain-events          tab 筛选 + 游标分页（默认 needs_you；list 不载证据）
//! GET /brain-events/summary  头部摘要（今日消化/需要你/判定延迟 p50/反馈对错）
//! GET /brain-events/{id}     详情（含完整 evidence）
//! ```

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};

use tinyiothub_storage::brain_event::BrainEventTab;
use tinyiothub_web::middleware::workspace::AuthClaims;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::domains::brain_event::dto::*;
use crate::state::AppState;

pub fn create_brain_event_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_brain_events))
        .route("/summary", get(brain_events_summary))
        .route("/{id}", get(brain_event_detail))
}

async fn list_brain_events(
    Query(params): Query<BrainEventQueryParams>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<Vec<BrainEventDto>>> {
    let ws = &claims.0.workspace_id;
    let tab = match params.tab.as_deref().unwrap_or("needs_you") {
        "needs_you" => BrainEventTab::NeedsYou,
        "all" => BrainEventTab::All,
        "patrol" => BrainEventTab::Patrol,
        "alarm" => BrainEventTab::Alarm,
        "directive" => BrainEventTab::Directive,
        // fail-closed：未知 tab 响亮 400，不静默返回全量（judgment 域先例）
        _ => {
            return ApiResponseBuilder::error_with_code(400, "未知 tab（可选：needs_you/all/patrol/alarm/directive）");
        }
    };
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    match state
        .db
        .list_brain_events(ws, tab, params.before.as_deref(), limit)
        .await
    {
        Ok(rows) => ApiResponseBuilder::success(rows.iter().map(BrainEventDto::from).collect()),
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}

async fn brain_events_summary(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<BrainEventsSummaryDto>> {
    let ws = &claims.0.workspace_id;
    match state.db.brain_events_summary(ws).await {
        Ok(s) => ApiResponseBuilder::success(BrainEventsSummaryDto::from(s)),
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}

async fn brain_event_detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<BrainEventDetailDto>> {
    let ws = &claims.0.workspace_id;
    match state.db.brain_event_detail(ws, &id).await {
        Ok(Some(d)) => ApiResponseBuilder::success(BrainEventDetailDto {
            event: BrainEventDto::from(&d.event),
            evidence: serde_json::from_str(&d.evidence_json).unwrap_or(serde_json::Value::Null),
        }),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "brain event 不存在"),
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}
