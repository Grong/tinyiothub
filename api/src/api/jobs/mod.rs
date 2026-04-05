// Jobs API Module
// 定时任务管理 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json,
    Router,
};
use serde::Deserialize;

use std::sync::Arc;

use crate::{
    domain::device::service::DeviceService,
    dto::entity::job::{
        CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams,
        JobStatistics, UpdateJobRequest,
    },
    dto::response::{ApiResponse, builder::ApiResponseBuilder},
    infrastructure::persistence::database::Database,
    shared::app_state::AppState,
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
) -> Json<ApiResponse<Vec<Job>>> {
    let db = state.database.clone();

    match Job::find_all(&db, &params).await {
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
    let db = state.database.clone();

    match Job::find_by_id(&db, &id).await {
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
    let db = state.database.clone();

    // 验证 cron 表达式
    if let Err(e) = cron::Schedule::from_str(&payload.cron_expression) {
        tracing::error!("Invalid cron expression: {}", e);
        return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
    }

    match Job::create(&db, &payload).await {
        Ok(job) => {
            // TODO: 通知调度器添加任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.add_job(job.clone());
            // }
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
    let db = state.database.clone();

    // 如果更新了 cron 表达式，验证它
    if let Some(ref cron) = payload.cron_expression {
        if let Err(e) = cron::Schedule::from_str(cron) {
            tracing::error!("Invalid cron expression: {}", e);
            return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
        }
    }

    match Job::update(&db, &id, &payload).await {
        Ok(job) => {
            // TODO: 通知调度器更新任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.upd_job(job.clone());
            // }
            ApiResponseBuilder::success(job)
        }
        Err(sqlx::Error::RowNotFound) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
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
    let db = state.database.clone();

    match Job::delete(&db, &id).await {
        Ok(_) => {
            // TODO: 通知调度器删除任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.del_job(id.clone());
            // }
            // 删除关联的执行记录 - 使用参数化查询防止 SQL 注入
            if let Err(e) = sqlx::query("DELETE FROM job_executions WHERE job_id = ?")
                .bind(&id)
                .execute(db.pool())
                .await
            {
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
) -> Json<ApiResponse<JobExecution>> {
    let db = state.database.clone();

    // 检查 job 是否存在
    let job = match Job::find_by_id(&db, &id).await {
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
    let execution = match JobExecution::create(&db, &id, "manual", Some("user")).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to create execution: {}", e);
            return ApiResponseBuilder::error("创建执行记录失败");
        }
    };

    // 设置 job 为运行中
    let _ = Job::set_running(&db, &id, true).await;

    // 在后台执行任务（这里简化处理，实际应该用 tokio::spawn）
    let db_clone = db.clone();
    let job_clone = job.clone();
    let exec_id = execution.id.clone();
    let device_service = state.device_service.clone();

    tokio::spawn(async move {
        let result = execute_job(&job_clone, device_service).await;

        // 更新执行状态
        let status = if result.is_ok() { "success" } else { "failed" };
        let error = result.err();

        let _ = JobExecution::update_status(
            &db_clone,
            &exec_id,
            status,
            None,
            error.as_deref(),
            error.as_deref(),
        )
        .await;

        // 更新 job 统计
        let _ = Job::update_run_stats(&db_clone, &job_clone.id, status, error.as_deref()).await;
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

    // 构建完整 URL（这里简化处理，实际应该支持完整的 URL）
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

    // 根据解释器执行脚本
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

    // Get parameters from config JSON or target_command_params
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

    // 注意：实际执行需要数据库连接
    // 这里只做验证
    Ok(format!("SQL would execute: {}", sql))
}

/// List job executions
async fn list_job_executions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<JobExecutionQueryParams>,
) -> Json<ApiResponse<Vec<JobExecution>>> {
    let db = state.database.clone();

    let limit = params.page_size.unwrap_or(20) as i32;

    match JobExecution::find_by_job(&db, &id, limit).await {
        Ok(executions) => ApiResponseBuilder::success(executions),
        Err(e) => {
            tracing::error!("Failed to list executions: {}", e);
            ApiResponseBuilder::error("获取执行记录失败")
        }
    }
}

/// Get job statistics
async fn get_statistics(State(state): State<AppState>) -> Json<ApiResponse<JobStatistics>> {
    let db = state.database.clone();

    match Job::get_statistics(&db).await {
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
) -> Json<ApiResponse<Vec<JobExecution>>> {
    let db = state.database.clone();

    // 如果指定了 job_id，使用 entity 的方法
    if let Some(ref job_id) = params.job_id {
        let limit = params.page_size.unwrap_or(20) as i32;
        match JobExecution::find_by_job(&db, job_id, limit).await {
            Ok(executions) => return ApiResponseBuilder::success(executions),
            Err(e) => {
                tracing::error!("Failed to list executions: {}", e);
                return ApiResponseBuilder::error("获取执行记录失败");
            }
        }
    }

    // 否则返回空列表（简化实现）
    ApiResponseBuilder::success(vec![])
}
