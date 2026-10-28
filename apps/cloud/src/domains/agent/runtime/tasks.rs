use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_agent::runtime::thing_agent::{Priority, TriggerSource, WakeSignal};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState};
use tinyiothub_web::{api_response::ApiResponse, response::ApiResponseBuilder, security::Claims};

use super::DbTaskStore;
use crate::domains::agent::AgentState;
use crate::verify_workspace_access;

pub(crate) fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let code = i32::from(status.as_u16());
    (status, ApiResponseBuilder::error_with_code::<Value>(code, message)).into_response()
}

pub async fn create_persistent(
    state: &AgentState,
    claims: &Claims,
    workspace_id: &str,
    headers: &HeaderMap,
    text: &str,
) -> Response {
    let Some(idempotency_key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return error_response(StatusCode::BAD_REQUEST, "Idempotency-Key 不能为空");
    };
    if text.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "指令内容不能为空");
    }

    let now = Utc::now();
    let task_id = uuid::Uuid::new_v4().to_string();
    let event_id = uuid::Uuid::new_v4().to_string();
    let signal = WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority: Priority::High,
        source: TriggerSource::UserDirective {
            user_id: claims.user_id.clone(),
            text: text.to_string(),
            session_key: None,
            source: None,
            problem_key: None,
        },
        dedup_key: None,
    };
    let signal_json = match serde_json::to_string(&signal) {
        Ok(json) => json,
        Err(error) => {
            tracing::error!(%workspace_id, %error, "serialize task wake failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 存储不可用");
        }
    };
    let task = AgentTask {
        id: task_id,
        workspace_id: workspace_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        objective: text.to_string(),
        session_key: None,
        created_by: claims.user_id.clone(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: task.id.clone(),
        event_id,
        signal_json,
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    };
    let store = DbTaskStore::new(state.db.clone()).with_recorder(state.runtime_recorder.clone());
    match store.create_task_with_wake(&task, &wake).await {
        Ok(outcome) => ApiResponseBuilder::success(json!({
            "taskId": outcome.task.id,
            "status": "accepted",
            "deduplicated": outcome.deduplicated,
        }))
        .into_response(),
        Err(TaskStoreError::ObjectiveConflict) => {
            error_response(StatusCode::CONFLICT, "Idempotency-Key 已用于不同指令")
        }
        Err(TaskStoreError::Validation(message)) => error_response(StatusCode::BAD_REQUEST, message),
        Err(TaskStoreError::ConcurrentUpdate) => error_response(StatusCode::CONFLICT, "Task 已并发更新"),
        Err(TaskStoreError::Unavailable(error)) => {
            tracing::error!(%workspace_id, %error, "persistent task create failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 存储不可用")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListTasksParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn list_tasks(
    State(state): State<AgentState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<ListTasksParams>,
) -> Json<ApiResponse<Value>> {
    verify_workspace_access!(state, claims, workspace_id);
    let limit = i64::from(params.limit.unwrap_or(50).clamp(1, 200));
    let offset = i64::from(params.offset.unwrap_or(0));
    let store = DbTaskStore::new(state.db.clone());
    match store.list_tasks(&workspace_id, limit, offset).await {
        Ok(tasks) => ApiResponseBuilder::success(json!({
            "tasks": tasks,
            "limit": limit,
            "offset": offset,
        })),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "list persistent tasks failed");
            ApiResponseBuilder::error_with_code(503, "Task 存储不可用")
        }
    }
}

pub async fn get_task(
    State(state): State<AgentState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return error_response(StatusCode::FORBIDDEN, "无权访问此工作空间"),
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "工作空间不存在"),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check Task workspace failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败");
        }
    }
    super::projection::response(&state, &workspace_id, &task_id).await
}
