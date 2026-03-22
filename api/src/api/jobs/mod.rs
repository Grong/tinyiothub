// Jobs API Module
// 定时任务管理 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;

use crate::dto::entity::job::{
    CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams, JobStatistics,
    UpdateJobRequest,
};
use crate::infrastructure::persistence::database::Database;
use crate::shared::app_state::AppState;

/// Create jobs router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Job CRUD
        .route("/jobs", get(list_jobs))
        .route("/jobs", post(create_job))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}", put(update_job))
        .route("/jobs/{id}", delete(delete_job))
        // Job Actions
        .route("/jobs/{id}/enable", post(enable_job))
        .route("/jobs/{id}/disable", post(disable_job))
        .route("/jobs/{id}/run", post(run_job_now))
        // Job Executions
        .route("/jobs/{id}/executions", get(list_job_executions))
        // Statistics
        .route("/jobs/statistics", get(get_statistics))
        // All Jobs Executions
        .route("/executions", get(list_all_executions))
}

/// List jobs with pagination and filters
async fn list_jobs(
    State(state): State<AppState>,
    Query(params): Query<JobQueryParams>,
) -> Result<Json<Vec<Job>>, StatusCode> {
    let db = state.database.clone();

    match Job::find_all(&db, &params).await {
        Ok(jobs) => Ok(Json(jobs)),
        Err(e) => {
            tracing::error!("Failed to list jobs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a single job by ID
async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Job>, StatusCode> {
    let db = state.database.clone();

    match Job::find_by_id(&db, &id).await {
        Ok(Some(job)) => Ok(Json(job)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a new job
async fn create_job(
    State(state): State<AppState>,
    Json(payload): Json<CreateJobRequest>,
) -> Result<Json<Job>, StatusCode> {
    let db = state.database.clone();

    // 验证 cron 表达式
    if let Err(e) = cron::Schedule::from_str(&payload.cron_expression) {
        tracing::error!("Invalid cron expression: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }

    match Job::create(&db, &payload).await {
        Ok(job) => {
            // TODO: 通知调度器添加任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.add_job(job.clone());
            // }
            Ok(Json(job))
        }
        Err(e) => {
            tracing::error!("Failed to create job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update an existing job
async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateJobRequest>,
) -> Result<Json<Job>, StatusCode> {
    let db = state.database.clone();

    // 如果更新了 cron 表达式，验证它
    if let Some(ref cron) = payload.cron_expression {
        if let Err(e) = cron::Schedule::from_str(cron) {
            tracing::error!("Invalid cron expression: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    match Job::update(&db, &id, &payload).await {
        Ok(job) => {
            // TODO: 通知调度器更新任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.upd_job(job.clone());
            // }
            Ok(Json(job))
        }
        Err(sqlx::Error::RowNotFound) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a job
async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();

    match Job::delete(&db, &id).await {
        Ok(_) => {
            // TODO: 通知调度器删除任务
            // if let Some(ref scheduler) = *state.time_task.lock().unwrap() {
            //     scheduler.del_job(id.clone());
            // }
            // 删除关联的执行记录
            let sql = format!("DELETE FROM job_executions WHERE job_id = '{}'", id);
            let _ = db.execute(&sql).await;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            tracing::error!("Failed to delete job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Enable a job
async fn enable_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Job>, StatusCode> {
    let db = state.database.clone();

    match Job::set_enabled(&db, &id, true).await {
        Ok(job) => Ok(Json(job)),
        Err(sqlx::Error::RowNotFound) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to enable job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Disable a job
async fn disable_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Job>, StatusCode> {
    let db = state.database.clone();

    match Job::set_enabled(&db, &id, false).await {
        Ok(job) => Ok(Json(job)),
        Err(sqlx::Error::RowNotFound) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to disable job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Manually trigger a job to run now
async fn run_job_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobExecution>, StatusCode> {
    let db = state.database.clone();

    // 检查 job 是否存在
    let job = match Job::find_by_id(&db, &id).await {
        Ok(Some(j)) => j,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // 检查是否已经在运行
    if job.is_running {
        return Err(StatusCode::CONFLICT);
    }

    // 创建执行记录
    let execution = match JobExecution::create(&db, &id, "manual", Some("user")).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to create execution: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // 设置 job 为运行中
    let _ = Job::set_running(&db, &id, true).await;

    // 在后台执行任务（这里简化处理，实际应该用 tokio::spawn）
    let db_clone = db.clone();
    let job_clone = job.clone();
    let exec_id = execution.id.clone();

    tokio::spawn(async move {
        let result = execute_job(&job_clone).await;

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

    Ok(Json(execution))
}

/// Execute a job based on its type
async fn execute_job(job: &Job) -> Result<String, String> {
    match job.job_type.as_str() {
        "http" => execute_http_job(job).await,
        "script" => execute_script_job(job).await,
        "device_command" => execute_device_command_job(job).await,
        "sql" => execute_sql_job(job).await,
        _ => Err(format!("Unknown job type: {}", job.job_type)),
    }
}

/// Execute HTTP job
async fn execute_http_job(job: &Job) -> Result<String, String> {
    use serde_json::Value;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing URL in config")?;

    let method = config
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");

    // 构建完整 URL（这里简化处理，实际应该支持完整的 URL）
    let full_url = if url.starts_with("http") {
        url.to_string()
    } else {
        format!("http://localhost{}", url)
    };

    let client = reqwest::Client::new();

    let request = match method {
        "GET" => client.get(&full_url),
        "POST" => client.post(&full_url),
        "PUT" => client.put(&full_url),
        "DELETE" => client.delete(&full_url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    let timeout = std::time::Duration::from_secs(job.timeout_seconds as u64);

    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

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
    use serde_json::Value;
    use std::process::Command;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let script = config
        .get("script")
        .and_then(|v| v.as_str())
        .ok_or("Missing script in config")?;

    let working_dir = config.get("working_dir").and_then(|v| v.as_str());

    let interpreter = config
        .get("interpreter")
        .and_then(|v| v.as_str())
        .unwrap_or("bash");

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
async fn execute_device_command_job(job: &Job) -> Result<String, String> {
    // 这里需要调用设备命令服务
    // 暂时返回模拟结果
    Ok(format!(
        "Device command executed for device: {:?}",
        job.target_device_id
    ))
}

/// Execute SQL job
async fn execute_sql_job(job: &Job) -> Result<String, String> {
    use serde_json::Value;

    let config: Value =
        serde_json::from_str(&job.config).map_err(|e| format!("Invalid config JSON: {}", e))?;

    let sql = config
        .get("sql")
        .and_then(|v| v.as_str())
        .ok_or("Missing SQL in config")?;

    // 注意：实际执行需要数据库连接
    // 这里只做验证
    Ok(format!("SQL would execute: {}", sql))
}

/// List job executions
async fn list_job_executions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<JobExecutionQueryParams>,
) -> Result<Json<Vec<JobExecution>>, StatusCode> {
    let db = state.database.clone();

    let limit = params.page_size.unwrap_or(20) as i32;

    match JobExecution::find_by_job(&db, &id, limit).await {
        Ok(executions) => Ok(Json(executions)),
        Err(e) => {
            tracing::error!("Failed to list executions: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get job statistics
async fn get_statistics(State(state): State<AppState>) -> Result<Json<JobStatistics>, StatusCode> {
    let db = state.database.clone();

    match Job::get_statistics(&db).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            tracing::error!("Failed to get statistics: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List all executions (with filters)
async fn list_all_executions(
    State(state): State<AppState>,
    Query(params): Query<JobExecutionQueryParams>,
) -> Result<Json<Vec<JobExecution>>, StatusCode> {
    let db = state.database.clone();

    // 如果指定了 job_id，使用 entity 的方法
    if let Some(ref job_id) = params.job_id {
        let limit = params.page_size.unwrap_or(20) as i32;
        match JobExecution::find_by_job(&db, job_id, limit).await {
            Ok(executions) => return Ok(Json(executions)),
            Err(e) => {
                tracing::error!("Failed to list executions: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // 否则返回空列表（简化实现）
    Ok(Json(vec![]))
}
