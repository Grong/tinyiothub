use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    dto::{request::pagination::PaginationQuery, response::ApiResponse},
    shared::app_state::AppState,
    shared::security::jwt::Claims,
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TimeTask {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub task_type: String,
    pub parameters: Option<String>,
    pub enabled: bool,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskQuery {
    pub enabled: Option<bool>,
    pub task_type: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTaskRequest {
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub task_type: String,
    pub parameters: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cron_expression: Option<String>,
    pub task_type: Option<String>,
    pub parameters: Option<String>,
    pub enabled: Option<bool>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks).post(create_task))
        .route("/:id", get(get_task).put(update_task).delete(delete_task))
        .route("/:id/enable", post(enable_task))
        .route("/:id/disable", post(disable_task))
        .route("/:id/run", post(run_task_now))
}

/// 获取定时任务列表
async fn list_tasks(
    State(_state): State<AppState>,
    Query(_query): Query<TaskQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<TimeTask>>> {
    // TODO: 实现定时任务查询逻辑
    tracing::info!("Listing time tasks with filters");

    let tasks = vec![];
    ApiResponse::success(tasks)
}

/// 创建定时任务
async fn create_task(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(request): Json<CreateTaskRequest>,
) -> Json<ApiResponse<TimeTask>> {
    // TODO: 实现定时任务创建逻辑
    tracing::info!("Creating time task: {}", request.name);

    let task = TimeTask {
        id: uuid::Uuid::new_v4().to_string(),
        name: request.name,
        description: request.description,
        cron_expression: request.cron_expression,
        task_type: request.task_type,
        parameters: request.parameters,
        enabled: request.enabled.unwrap_or(true),
        last_run: None,
        next_run: None,
        created_at: chrono::Utc::now(),
    };

    ApiResponse::success(task)
}

/// 获取定时任务详情
async fn get_task(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<TimeTask>>> {
    // TODO: 实现定时任务详情查询逻辑
    tracing::info!("Getting time task details for: {}", id);

    ApiResponse::success(None)
}

/// 更新定时任务
async fn update_task(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(_request): Json<UpdateTaskRequest>,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现定时任务更新逻辑
    tracing::info!("Updating time task: {}", id);

    ApiResponse::success(true)
}

/// 删除定时任务
async fn delete_task(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现定时任务删除逻辑
    tracing::info!("Deleting time task: {}", id);

    ApiResponse::success(true)
}

/// 启用定时任务
async fn enable_task(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现定时任务启用逻辑
    tracing::info!("Enabling time task: {}", id);

    ApiResponse::success(true)
}

/// 禁用定时任务
async fn disable_task(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现定时任务禁用逻辑
    tracing::info!("Disabling time task: {}", id);

    ApiResponse::success(true)
}

/// 立即运行定时任务
async fn run_task_now(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现立即运行定时任务逻辑
    tracing::info!("Running time task now: {}", id);

    ApiResponse::success(true)
}
