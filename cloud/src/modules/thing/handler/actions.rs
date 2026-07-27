// Thing action handlers (invoke + confirm)

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use tinyiothub_web::response::ApiResponse;

use super::super::service::ThingService;
use crate::{
    api::middleware::WorkspaceScope,
    modules::agent::tools::take_pending_action,
    shared::{api_response::ApiResponseBuilder, app_state::AppState},
};

fn thing_service(pool: &sqlx::SqlitePool) -> ThingService {
    ThingService::new(pool.clone())
}

// ──────────────────────────────────────────────
// POST /things/{id}/actions/{action_name}/confirm
// ──────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmActionRequest {
    pub token: String,
}

pub async fn confirm_action(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path((thing_id, action_name)): Path<(String, String)>,
    Json(req): Json<ConfirmActionRequest>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let ws = workspace_id.unwrap_or_default();

    // 1. Validate token and retrieve pending action
    let pending = match take_pending_action(&req.token) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                ApiResponseBuilder::error_with_code(404, "确认令牌无效或已过期"),
            );
        }
    };

    // 2. Verify the token matches the requested thing + action
    if pending.thing_id != thing_id || pending.action_name != action_name {
        return (
            StatusCode::BAD_REQUEST,
            ApiResponseBuilder::error_with_code(400, "确认令牌与请求的操作不匹配"),
        );
    }

    // 2b. Verify the token was issued for THIS workspace (eng-review T1)
    if pending.workspace_id != ws {
        return (
            StatusCode::FORBIDDEN,
            ApiResponseBuilder::error_with_code(403, "确认令牌不属于当前工作区"),
        );
    }

    // 3. Verify the thing exists and is a device
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    let thing = match svc.get_thing(&thing_id, &ws).await {
        Ok(t) => t,
        Err(e) => {
            let status = e.status_code();
            return (
                status,
                ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
            );
        }
    };

    if thing.thing_type != "device" {
        return (
            StatusCode::BAD_REQUEST,
            ApiResponseBuilder::error_with_code(
                400,
                format!("操作不支持: 物类型为 '{}'，仅 'device' 类型物支持操作", thing.thing_type),
            ),
        );
    }

    // 4. Verify the command exists
    let command_exists: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?",
    )
    .bind(&thing_id)
    .bind(&action_name)
    .fetch_one(&pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false);

    if !command_exists {
        return (
            StatusCode::NOT_FOUND,
            ApiResponseBuilder::error_with_code(
                404,
                format!("操作 '{}' 未在物 {} 上注册", action_name, thing_id),
            ),
        );
    }

    // 5. Execute via DataServer if available
    let app_state = crate::modules::mcp::get_app_state();
    match app_state.and_then(|s| s.data_server().cloned()) {
        Some(data_server) => {
            let cmd = tinyiothub_core::models::device_command::DeviceCommand {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: thing_id.clone(),
                name: action_name.clone(),
                display_name: None,
                description: None,
                parameters: pending.params.as_ref().map(|p| p.to_string()),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match data_server.execute_command(cmd) {
                Ok(()) => (
                    StatusCode::OK,
                    ApiResponseBuilder::success(json!({
                        "thingId": thing_id,
                        "actionName": action_name,
                        "status": "executed",
                        "message": "操作已确认并下发执行"
                    })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ApiResponseBuilder::error_with_code(500, format!("操作执行失败: {}", e)),
                ),
            }
        }
        None => {
            tracing::warn!("DataServer not available, action execution is simulated");
            (
                StatusCode::OK,
                ApiResponseBuilder::success(json!({
                    "thingId": thing_id,
                    "actionName": action_name,
                    "status": "simulated",
                    "message": "操作已确认（DataServer 未就绪，实际执行已模拟）"
                })),
            )
        }
    }
}
