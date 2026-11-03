use crate::domains::agent::thing_agent::task::coordinator::TaskError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Response,
};
use tinyiothub_core::agent_task::TaskStoreError;
use tinyiothub_core::device_job::DeviceJobError;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::{projection, tasks::error_response};

fn task_error(error: TaskError) -> Response {
    match error {
        TaskError::NotFound => error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        TaskError::Store(TaskStoreError::ConcurrentUpdate) => error_response(StatusCode::CONFLICT, "Task 已并发更新"),
        TaskError::Store(TaskStoreError::Validation(message)) => error_response(StatusCode::CONFLICT, message),
        other => {
            tracing::warn!(%other, "runtime Task control failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用")
        }
    }
}

async fn pause_resume(
    state: AgentState,
    claims: Claims,
    workspace_id: String,
    task_id: String,
    pause: bool,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    let result = if pause {
        manager.pause_task(&workspace_id, &task_id).await
    } else {
        manager.resume_task(&workspace_id, &task_id).await
    };
    match result {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(error) => task_error(error),
    }
}

pub async fn pause(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    pause_resume(state, claims, workspace_id, task_id, true).await
}

pub async fn resume(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    pause_resume(state, claims, workspace_id, task_id, false).await
}

pub async fn cancel(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    match manager.cancel_task(&workspace_id, &task_id).await {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(error) => task_error(error),
    }
}

pub async fn stop_job(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id, job_id)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    match manager.stop_task_job(&workspace_id, &task_id, &job_id).await {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(DeviceJobError::NotFound) => error_response(StatusCode::NOT_FOUND, "Device Job 不存在"),
        Err(DeviceJobError::Conflict(message)) | Err(DeviceJobError::Validation(message)) => {
            error_response(StatusCode::CONFLICT, message)
        }
        Err(error) => {
            tracing::warn!(%error, "single Device Job stop failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Device Job 服务不可用")
        }
    }
}
