// Jobs API Module
// 定时任务管理 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json,
    Router,
};

use std::sync::Arc;

use crate::{
    domain::device::service::DeviceService,
    dto::entity::job::{
        CreateJobRequest, Job, JobExecutionQueryParams, JobQueryParams,
        JobStatistics, UpdateJobRequest,
    },
    dto::response::{ApiResponse, builder::ApiResponseBuilder, PaginatedResponse, PaginationInfo},
    shared::{app_state::AppState, error::Error},
};

/// Create jobs router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Job CRUD
        .route("/", get(list_jobs))
        .route("/", post(create_job))
        .route("/{id}", get(get_job))
        .route("/{id}", put(update_job))
        .route("/{id}", delete(delete_job))
        // Job Actions (复杂业务动作，保持 RPC 风格)
        .route("/{id}/run", post(run_job_now))
        // Job Executions
        .route("/{id}/executions", get(list_job_executions))
        // Statistics
        .route("/statistics", get(get_statistics))
        // All Jobs Executions
        .route("/executions", get(list_all_executions))
}

/// List jobs with pagination and filters
async fn list_jobs(
    State(state): State<AppState>,
    Query(params): Query<JobQueryParams>,
) -> Json<ApiResponse<Vec<crate::dto::entity::job::Job>>> {
    match state.job_service.find_all(&params).await {
        Ok(jobs) => ApiResponseBuilder::success(jobs),
        Err(e) => {
            tracing::error!("Failed to list jobs: {}", e);
            ApiResponseBuilder::error("获取任务列表失败")
        }
    }
}

/// Get a single job by ID
async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Job>> {
    match state.job_service.find_by_id(&id).await {
        Ok(Some(job)) => ApiResponseBuilder::success(job),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            ApiResponseBuilder::error("获取任务失败")
        }
    }
}

/// Create a new job
async fn create_job(
    State(state): State<AppState>,
    Json(payload): Json<CreateJobRequest>,
) -> Json<ApiResponse<Job>> {
    // 验证 cron 表达式
    if let Err(e) = cron::Schedule::from_str(&payload.cron_expression) {
        tracing::error!("Invalid cron expression: {}", e);
        return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
    }

    match state.job_service.create(&payload).await {
        Ok(job) => {
            ApiResponseBuilder::success(job)
        }
        Err(e) => {
            tracing::error!("Failed to create job: {}", e);
            ApiResponseBuilder::error("创建任务失败")
        }
    }
}

/// Update an existing job
async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateJobRequest>,
) -> Json<ApiResponse<Job>> {
    // 如果更新了 cron 表达式，验证它
    if let Some(ref cron) = payload.cron_expression {
        if let Err(e) = cron::Schedule::from_str(cron) {
            tracing::error!("Invalid cron expression: {}", e);
            return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
        }
    }

    match state.job_service.update(&id, &payload).await {
        Ok(job) => {
            ApiResponseBuilder::success(job)
        }
        Err(Error::NotFound) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to update job: {}", e);
            ApiResponseBuilder::error("更新任务失败")
        }
    }
}

/// Delete a job
async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<bool>> {
    match state.job_service.delete(&id).await {
        Ok(_) => {
            if let Err(e) = state.job_execution_service.delete_by_job_id(&id).await {
                tracing::error!("Failed to delete job executions: {}", e);
                return ApiResponseBuilder::error("删除任务执行记录失败");
            }
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to delete job: {}", e);
            ApiResponseBuilder::error("删除任务失败")
        }
    }
}

/// Manually trigger a job to run now
async fn run_job_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<crate::dto::entity::job::JobExecution>> {
    // 检查 job 是否存在
    let job = match state.job_service.find_by_id(&id).await {
        Ok(Some(j)) => j,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            return ApiResponseBuilder::error("获取任务失败");
        }
    };

    // 检查是否已经在运行
    if job.is_running {
        return ApiResponseBuilder::error_with_code(409, "任务正在运行中");
    }

    // 创建执行记录
    let execution = match state.job_execution_service.create(&id, "manual", Some("user")).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to create execution: {}", e);
            return ApiResponseBuilder::error("创建执行记录失败");
        }
    };

    // 设置 job 为运行中
    let _ = state.job_service.set_running(&id, true).await;

    // 在后台执行任务
    let job_service = state.job_service.clone();
    let job_execution_service = state.job_execution_service.clone();
    let job_clone = job.clone();
    let exec_id = execution.id.clone();
    let device_service = state.device_service.clone();

    tokio::spawn(async move {
        let result = execute_job(&job_clone, device_service).await;

        // 更新执行状态
        let status = if result.is_ok() { "success" } else { "failed" };
        let error = result.err();

        let _ = job_execution_service.update_status(
            &exec_id,
            status,
            None,
            error.as_deref(),
            error.as_deref(),
        )
        .await;

        // 更新 job 统计
        let _ = job_service.update_run_stats(&job_clone.id, status, error.as_deref()).await;
    });

    ApiResponseBuilder::success(execution)
}

/// Execute a job based on its type
async fn execute_job(job: &Job, device_service: Arc<DeviceService>) -> Result<String, String> {
    match job.job_type.as_str() {
        "http" => execute_http_job(job).await,
        "script" => execute_script_job(job).await,
        "device_command" => execute_device_command_job(job, device_service).await,
        "sql" => execute_sql_job(job).await,
        _ => Err(format!("Unknown job type: {}", job.job_type)),
    }
}

/// Execute HTTP job
async fn execute_http_job(job: &Job) -> Result<String, String> {
    use serde_json::Value;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let url = config.get("url").and_then(|v| v.as_str()).ok_or("Missing URL in config")?;

    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("GET");

    let full_url =
        if url.starts_with("http") { url.to_string() } else { format!("http://localhost{}", url) };

    let client = reqwest::Client::new();

    let request = match method {
        "GET" => client.get(&full_url),
        "POST" => client.post(&full_url),
        "PUT" => client.put(&full_url),
        "DELETE" => client.delete(&full_url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    let timeout = std::time::Duration::from_secs(job.timeout_seconds as u64);

    let response =
        request.timeout(timeout).send().await.map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("HTTP error {}: {}", status, body))
    }
}

/// Execute script job
async fn execute_script_job(job: &Job) -> Result<String, String> {
    use std::process::Command;

    use serde_json::Value;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let script = config.get("script").and_then(|v| v.as_str()).ok_or("Missing script in config")?;

    let working_dir = config.get("working_dir").and_then(|v| v.as_str());

    let interpreter = config.get("interpreter").and_then(|v| v.as_str()).unwrap_or("bash");

    let output = match interpreter {
        "python" => Command::new("python")
            .arg("-c")
            .arg(script)
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        "powershell" => Command::new("powershell")
            .args(["-Command", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        "cmd" => Command::new("cmd")
            .args(["/C", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
        _ => Command::new("bash")
            .args(["-c", script])
            .current_dir(working_dir.unwrap_or("."))
            .output(),
    };

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        }
        Err(e) => Err(format!("Failed to execute script: {}", e)),
    }
}

/// Execute device command job
async fn execute_device_command_job(job: &Job, device_service: Arc<DeviceService>) -> Result<String, String> {
    let device_id = job.target_device_id.as_ref()
        .ok_or_else(|| "Missing target_device_id in job".to_string())?;
    let command_name = job.target_command_name.as_ref()
        .ok_or_else(|| "Missing target_command_name in job".to_string())?;

    let params = job.target_command_params.clone();

    match device_service.send_command(device_id, command_name, "custom", params).await {
        Ok(command_id) => Ok(format!("Device command '{}' sent to device '{}', command_id: {}",
            command_name, device_id, command_id)),
        Err(e) => Err(format!("Failed to send device command: {}", e)),
    }
}

/// Execute SQL job
async fn execute_sql_job(job: &Job) -> Result<String, String> {
    use serde_json::Value;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let sql = config.get("sql").and_then(|v| v.as_str()).ok_or("Missing SQL in config")?;

    Ok(format!("SQL would execute: {}", sql))
}

/// List job executions
async fn list_job_executions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<JobExecutionQueryParams>,
) -> Json<ApiResponse<Vec<crate::dto::entity::job::JobExecution>>> {
    let limit = params.page_size.unwrap_or(20) as i32;

    match state.job_execution_service.find_by_job(&id, limit).await {
        Ok(executions) => ApiResponseBuilder::success(executions),
        Err(e) => {
            tracing::error!("Failed to list executions: {}", e);
            ApiResponseBuilder::error("获取执行记录失败")
        }
    }
}

/// Get job statistics
async fn get_statistics(State(state): State<AppState>) -> Json<ApiResponse<JobStatistics>> {
    match state.job_service.get_statistics().await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get statistics: {}", e);
            ApiResponseBuilder::error("获取统计信息失败")
        }
    }
}

/// List all executions (with filters)
async fn list_all_executions(
    State(state): State<AppState>,
    Query(params): Query<JobExecutionQueryParams>,
) -> Json<ApiResponse<PaginatedResponse<crate::dto::entity::job::JobExecution>>> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);

    let (executions_result, count_result) = tokio::join!(
        state.job_execution_service.find_all(&params),
        state.job_execution_service.count(&params),
    );

    match executions_result {
        Ok(executions) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            ApiResponseBuilder::success(PaginatedResponse {
                data: executions,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list executions: {}", e);
            ApiResponseBuilder::error("获取执行记录失败")
        }
    }
}
