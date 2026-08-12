// Thing action handlers (invoke + confirm)

use crate::domains::agent::host::thing_action_hooks::ThingConfirmVerdict;
use crate::shared::app_state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use tinyiothub_web::response::ApiResponse;

use super::super::service::ThingService;
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use tinyiothub_web::response::ApiResponseBuilder;

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
    let pending = match state.thing_action_hooks.take_pending(&req.token) {
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
                format!(
                    "操作不支持: 物类型为 '{}'，仅 'device' 类型物支持操作",
                    thing.thing_type
                ),
            ),
        );
    }

    // 4. Verify the command exists
    let command_exists: bool =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?")
            .bind(&thing_id)
            .bind(&action_name)
            .fetch_one(&pool)
            .await
            .map(|c| c > 0)
            .unwrap_or(false);

    if !command_exists {
        return (
            StatusCode::NOT_FOUND,
            ApiResponseBuilder::error_with_code(404, format!("操作 '{}' 未在物 {} 上注册", action_name, thing_id)),
        );
    }

    // 5. Execute via DataServer if available
    match state.data_server.clone() {
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

// ──────────────────────────────────────────────
// POST /things/{id}/actions/{action_name}/invoke
// ──────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeActionRequest {
    pub params: Option<serde_json::Value>,
}

/// Direct invoke from the UI (eng-review T14). Applies the workspace's
/// require_action_confirm gate: when ON, mints a pending token and returns
/// `confirmation_required` (the UI opens the confirm modal with it); when
/// OFF, dispatches immediately.
pub async fn invoke_action(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path((thing_id, action_name)): Path<(String, String)>,
    Json(req): Json<InvokeActionRequest>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let ws = workspace_id.unwrap_or_default();
    let pool = state.database.pool().clone();
    let svc = thing_service(&pool);

    // 1. Thing must exist in this workspace and be a device
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
                format!(
                    "操作不支持: 物类型为 '{}'，仅 'device' 类型物支持操作",
                    thing.thing_type
                ),
            ),
        );
    }

    // 2. Action must be registered on the thing
    let registered: bool =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?")
            .bind(&thing_id)
            .bind(&action_name)
            .fetch_one(&pool)
            .await
            .map(|c| c > 0)
            .unwrap_or(false);
    if !registered {
        return (
            StatusCode::NOT_FOUND,
            ApiResponseBuilder::error_with_code(404, format!("操作 '{}' 未在物 {} 上注册", action_name, thing_id)),
        );
    }

    // 2b. If the action HAS a parameter schema, params must match it (same
    // validation as the agent-side InvokeActionTool — the two invoke paths
    // share one contract). NULL parameters = no schema = skip validation.
    let action_schema: Option<String> =
        sqlx::query_scalar("SELECT parameters FROM thing_actions WHERE device_id = ? AND name = ?")
            .bind(&thing_id)
            .bind(&action_name)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .flatten();
    if let Some(ref schema) = action_schema
        && let Err(msg) = state.thing_action_hooks.validate_params(schema, req.params.as_ref())
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiResponseBuilder::error_with_code(422, msg),
        );
    }

    // 3. Unified policy gate (X3/T16): a Block rule denies before any confirm
    // decision; a RequireApproval rule mints a confirmation token; otherwise
    // the legacy require_action_confirm toggle decides (fail closed — T7).
    let require_confirm: bool = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = ?")
        .bind(&ws)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(1i32)
        != 0;

    match state
        .thing_action_hooks
        .decide_confirm(&ws, &action_name, require_confirm)
        .await
    {
        ThingConfirmVerdict::Deny { reason } => {
            return (StatusCode::FORBIDDEN, ApiResponseBuilder::error_with_code(403, reason));
        }
        ThingConfirmVerdict::RequireToken => {
            let token = state.thing_action_hooks.store_pending(
                thing_id.clone(),
                action_name.clone(),
                req.params.clone(),
                ws.clone(),
            );
            return (
                StatusCode::OK,
                ApiResponseBuilder::success(json!({
                    "thingId": thing_id,
                    "actionName": action_name,
                    "status": "confirmation_required",
                    "token": token,
                    "params": req.params,
                })),
            );
        }
        ThingConfirmVerdict::Execute => {}
    }

    // 4. Dispatch immediately via the command channel
    match state.data_server.clone() {
        Some(data_server) => {
            let cmd = tinyiothub_core::models::device_command::DeviceCommand {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: thing_id.clone(),
                name: action_name.clone(),
                display_name: None,
                description: None,
                parameters: req.params.as_ref().map(|p| p.to_string()),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match data_server.execute_command(cmd) {
                Ok(()) => (
                    StatusCode::OK,
                    ApiResponseBuilder::success(json!({
                        "thingId": thing_id,
                        "actionName": action_name,
                        "status": "executed",
                    })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ApiResponseBuilder::error_with_code(500, format!("操作执行失败: {}", e)),
                ),
            }
        }
        None => (
            StatusCode::OK,
            ApiResponseBuilder::success(json!({
                "thingId": thing_id,
                "actionName": action_name,
                "status": "simulated",
            })),
        ),
    }
}
