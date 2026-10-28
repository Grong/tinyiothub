use tinyiothub_storage::runtime_event::TaskProjection;

use crate::domains::agent::AgentState;
use crate::domains::agent::runtime::tasks::error_response;

pub async fn load(
    state: &AgentState,
    workspace_id: &str,
    task_id: &str,
) -> Result<TaskProjection, axum::response::Response> {
    match state.db.runtime_projection(workspace_id, task_id).await {
        Ok(Some(projection)) => Ok(projection),
        Ok(None) => Err(error_response(axum::http::StatusCode::NOT_FOUND, "Task 不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "load Task projection failed");
            Err(error_response(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Task 投影不可用",
            ))
        }
    }
}

pub async fn response(state: &AgentState, workspace_id: &str, task_id: &str) -> axum::response::Response {
    match load(state, workspace_id, task_id).await {
        Ok(projection) => axum::response::IntoResponse::into_response(
            tinyiothub_web::response::ApiResponseBuilder::success(projection),
        ),
        Err(response) => response,
    }
}
