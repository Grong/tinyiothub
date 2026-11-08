//! Workspace 级的 Assistant Schedule 与 Global HUD 端点。
//!
//! 只读聚合：设备/告警/待答/待批/活动会话/最近事件都来自既有存储，不新增状态表。
//! 所有端点先做 workspace 归属校验。

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tinyiothub_storage::alarm::{AlarmLevel, AlarmQueryCriteria, AlarmStatus};
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::session::hud::{
    HudActivity, HudAlarms, HudAssistant, HudDevices, HudPending, HudResponse, HudSchedule, HudStatusInputs,
    MAX_RECENT_EVENTS, assistant_status,
};
use crate::domains::agent::session::wake::assistant_agent_id;

pub fn create_workspace_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{workspace_id}/agent/hud", get(get_hud))
        .route("/{workspace_id}/agent/schedule", get(get_schedule).put(update_schedule))
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

async fn verify_workspace_read(state: &AgentState, claims: &Claims, workspace_id: &str) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(error_response(StatusCode::FORBIDDEN, "无权访问此工作空间")),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "工作空间不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check workspace failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败"))
        }
    }
}

fn validate_interval(interval_seconds: u32) -> Result<u32, String> {
    if (tinyiothub_storage::assistant_schedule::MIN_ASSISTANT_INTERVAL_SECONDS
        ..=tinyiothub_storage::assistant_schedule::MAX_ASSISTANT_INTERVAL_SECONDS)
        .contains(&interval_seconds)
    {
        Ok(interval_seconds)
    } else {
        Err("intervalSeconds 必须在 60 到 86400 之间".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    pub enabled: bool,
    pub interval_seconds: u32,
    /// Wire value of [`tinyiothub_core::policy::PermissionMode`].
    pub permission_mode: String,
    pub prompt: String,
    pub next_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

impl From<&AssistantSchedule> for ScheduleDto {
    fn from(schedule: &AssistantSchedule) -> Self {
        Self {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            permission_mode: schedule.permission_mode.as_str().to_string(),
            prompt: schedule.prompt.clone(),
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScheduleRequest {
    pub enabled: Option<bool>,
    pub interval_seconds: Option<u32>,
    pub prompt: Option<String>,
}

pub async fn get_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => Json(ScheduleDto::from(&schedule)).into_response(),
        Err(error) => internal(error),
    }
}

pub async fn update_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    let current = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let interval_seconds = match request.interval_seconds {
        Some(value) => match validate_interval(value) {
            Ok(value) => value,
            Err(message) => return error_response(StatusCode::BAD_REQUEST, &message),
        },
        None => current.interval_seconds,
    };
    let recompute = request.enabled == Some(true) || request.interval_seconds.is_some();
    let next_wake_at = recompute.then(|| now + chrono::Duration::seconds(i64::from(interval_seconds)));

    match state
        .db
        .update_assistant_schedule(
            &workspace_id,
            request.enabled,
            Some(interval_seconds),
            request.prompt.as_deref(),
            next_wake_at,
            None,
        )
        .await
    {
        Ok(Some(schedule)) => Json(ScheduleDto::from(&schedule)).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "Schedule 不存在"),
        Err(error) => internal(error),
    }
}

pub async fn get_hud(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    let schedule = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let paused = !schedule.enabled;

    let total = match state.db.count_things_total(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let online = match state.db.count_online_things(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let criteria = AlarmQueryCriteria {
        workspace_id: Some(workspace_id.clone()),
        alarm_levels: Some(vec![AlarmLevel::Critical, AlarmLevel::Error]),
        statuses: Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]),
        ..Default::default()
    };
    let alarms = match state.db.count_alarms_by_criteria(&criteria).await {
        Ok(value) => value as i64,
        Err(error) => return internal(error),
    };
    let harness_approvals = match state.db.count_pending_tool_approvals(&workspace_id).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let broker_approvals = state
        .session_runtime
        .approvals
        .count_pending_for_workspace(&workspace_id) as i64;
    let approvals = harness_approvals + broker_approvals;
    let questions = state
        .session_runtime
        .questions
        .count_pending_for_workspace(&workspace_id);
    let active_sessions = state.session_runtime.contexts.count_for_workspace(&workspace_id);
    let recent_events = match state.db.recent_session_events(&workspace_id, MAX_RECENT_EVENTS).await {
        Ok(events) => events,
        Err(error) => return internal(error),
    };

    let last_outcome = schedule.last_outcome.clone();
    let status = assistant_status(HudStatusInputs {
        paused,
        active_sessions,
        pending_questions: questions,
        pending_approvals: approvals,
        last_outcome: last_outcome.as_deref(),
    });

    Json(HudResponse {
        assistant: HudAssistant { status, last_outcome },
        devices: HudDevices {
            total,
            online,
            offline: (total - online).max(0),
        },
        alarms: HudAlarms { high_priority: alarms },
        pending: HudPending { questions, approvals },
        activity: HudActivity { active_sessions },
        schedule: HudSchedule {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome,
        },
        recent_events,
    })
    .into_response()
}

fn internal(error: impl std::fmt::Display) -> Response {
    tracing::warn!(%error, "assistant HUD/schedule query failed");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取 Assistant 状态失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_bounds_are_enforced() {
        assert!(validate_interval(60).is_ok());
        assert!(validate_interval(300).is_ok());
        assert!(validate_interval(86_400).is_ok());
        assert!(validate_interval(59).is_err());
        assert!(validate_interval(86_401).is_err());
    }

    #[test]
    fn schedule_dto_serializes_camel_case() {
        let dto = ScheduleDto {
            enabled: true,
            interval_seconds: 300,
            permission_mode: "ask".to_string(),
            prompt: String::new(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        };
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["intervalSeconds"], 300);
        assert!(value.get("nextWakeAt").is_some());
    }
}
