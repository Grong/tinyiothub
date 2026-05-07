// Jobs API — moved from api/jobs/mod.rs
// Compatibility layer over new cron system

use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use std::str::FromStr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use tinyiothub_core::models::cron_job::{
    CreateCronJobRequest, CronJob, CronJobQuery, CronRun, CronRunQuery, UpdateCronJobRequest,
};
use tinyiothub_core::models::job::{
    CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams, JobStatistics,
    UpdateJobRequest,
};
use crate::{
    shared::api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
    shared::{app_state::AppState, error::Error},
};
use tinyiothub_runtime::cron::ExecutorRegistry;

/// Create jobs router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_jobs).post(create_job))
        .route("/{id}", get(get_job).put(update_job).delete(delete_job))
        .route("/{id}/run", post(run_job_now))
        .route("/{id}/executions", get(list_job_executions))
        .route("/statistics", get(get_statistics))
        .route("/executions", get(list_all_executions))
}

// ─── Workspace resolution helper ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WorkspaceQuery {
    workspace_id: Option<String>,
}

async fn resolve_workspace(
    state: &AppState,
    tenant_id: &str,
    explicit: Option<String>,
) -> Result<String, (i32, String)> {
    if let Some(ws) = explicit {
        return Ok(ws);
    }
    match state.workspace_service.find_by_tenant(tenant_id, Some(1), Some(1)).await {
        Ok(workspaces) if !workspaces.is_empty() => Ok(workspaces[0].id.clone()),
        _ => {
            tracing::warn!("No workspace found for tenant {}", tenant_id);
            Err((400, "未找到工作空间".to_string()))
        }
    }
}

// ─── DTO mapping: legacy Job <-> CronJob ──────────────────────────────────

fn map_cron_job_to_job(cj: CronJob) -> Job {
    let mut target_device_id = None;
    let mut target_command_name = None;
    let mut target_command_params = None;

    if cj.job_type == "device_command" {
        target_device_id = cj.target_device_id();
        target_command_name = cj.target_command_name();
        target_command_params = cj.target_command_params();
    }

    Job {
        id: cj.id,
        name: cj.name,
        description: cj.description,
        job_type: if cj.job_type == "shell" {
            "script".to_string()
        } else {
            cj.job_type
        },
        cron_expression: cj.cron_expression,
        config: cj.config,
        timeout_seconds: cj.timeout_seconds,
        retry_count: cj.max_retries,
        retry_delay_seconds: 0,
        concurrency: 1,
        target_device_id,
        target_command_name,
        target_command_params,
        is_enabled: cj.is_enabled,
        is_running: cj.is_running,
        last_run_at: cj.last_run_at,
        last_run_status: cj.last_run_status,
        last_run_error: cj.last_run_error,
        next_run_at: cj.next_run_at,
        run_count: cj.run_count,
        success_count: cj.success_count,
        fail_count: cj.fail_count,
        tags: String::new(),
        alert_config: String::new(),
        created_at: cj.created_at,
        updated_at: cj.updated_at,
        created_by: cj.created_by,
    }
}

fn map_create_request(req: &CreateJobRequest, workspace_id: &str) -> CreateCronJobRequest {
    let job_type = if req.job_type == "script" {
        "shell".to_string()
    } else {
        req.job_type.clone()
    };

    let config = if job_type == "device_command" {
        let mut cfg = serde_json::Map::new();
        if let Some(ref did) = req.target_device_id {
            cfg.insert("device_id".to_string(), serde_json::Value::String(did.clone()));
        }
        if let Some(ref cn) = req.target_command_name {
            cfg.insert("command_name".to_string(), serde_json::Value::String(cn.clone()));
        }
        if let Some(ref params) = req.target_command_params {
            cfg.insert("params".to_string(), serde_json::Value::String(params.clone()));
        }
        serde_json::Value::Object(cfg).to_string()
    } else {
        req.config.clone()
    };

    CreateCronJobRequest {
        name: req.name.clone(),
        description: req.description.clone(),
        job_type,
        cron_expression: req.cron_expression.clone(),
        config,
        workspace_id: workspace_id.to_string(),
        timeout_seconds: req.timeout_seconds,
        max_retries: req.retry_count,
    }
}

fn map_update_request(req: &UpdateJobRequest) -> UpdateCronJobRequest {
    let job_type = req.job_type.as_ref().map(|t| {
        if t == "script" {
            "shell".to_string()
        } else {
            t.clone()
        }
    });

    let config = req.config.clone().or_else(|| {
        if job_type.as_deref() == Some("device_command") {
            let mut cfg = serde_json::Map::new();
            if let Some(ref did) = req.target_device_id {
                cfg.insert("device_id".to_string(), serde_json::Value::String(did.clone()));
            }
            if let Some(ref cn) = req.target_command_name {
                cfg.insert("command_name".to_string(), serde_json::Value::String(cn.clone()));
            }
            if let Some(ref params) = req.target_command_params {
                cfg.insert("params".to_string(), serde_json::Value::String(params.clone()));
            }
            if !cfg.is_empty() {
                Some(serde_json::Value::Object(cfg).to_string())
            } else {
                None
            }
        } else {
            None
        }
    });

    UpdateCronJobRequest {
        name: req.name.clone(),
        description: req.description.clone(),
        job_type,
        cron_expression: req.cron_expression.clone(),
        config,
        timeout_seconds: req.timeout_seconds,
        max_retries: req.retry_count,
        is_enabled: None,
    }
}

fn map_cron_job_query(params: &JobQueryParams, workspace_id: Option<String>) -> CronJobQuery {
    CronJobQuery {
        name: params.name.clone(),
        job_type: params.job_type.clone(),
        is_enabled: params.is_enabled,
        workspace_id,
        page: params.page,
        page_size: params.page_size,
    }
}

fn map_cron_run_to_execution(run: CronRun) -> JobExecution {
    JobExecution {
        id: run.id,
        job_id: run.job_id,
        started_at: run.started_at,
        ended_at: run.ended_at,
        duration_ms: run.duration_ms,
        status: run.status,
        result: run.output,
        error_message: run.error_message,
        error_trace: None,
        trigger_type: run.trigger_type,
        triggered_by: run.triggered_by,
        worker_id: None,
        memory_usage_bytes: None,
        cpu_time_ms: None,
        created_at: run.created_at,
    }
}

fn map_execution_query(params: &JobExecutionQueryParams) -> CronRunQuery {
    CronRunQuery {
        job_id: params.job_id.clone(),
        workspace_id: None,
        status: params.status.clone(),
        trigger_type: params.trigger_type.clone(),
        page: params.page,
        page_size: params.page_size,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────

async fn list_jobs(
    State(state): State<AppState>,
    Query(params): Query<JobQueryParams>,
    claims: Claims,
) -> Json<ApiResponse<Vec<Job>>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => Some(ws),
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    let query = map_cron_job_query(&params, ws_id);
    match state.cron_job_repo.find_all(&query).await {
        Ok(jobs) => ApiResponseBuilder::success(
            jobs.into_iter().map(map_cron_job_to_job).collect(),
        ),
        Err(e) => {
            tracing::error!("Failed to list jobs: {}", e);
            ApiResponseBuilder::error("获取任务列表失败")
        }
    }
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<WorkspaceQuery>,
    claims: Claims,
) -> Json<ApiResponse<Job>> {
    let _ws_id = match resolve_workspace(&state, &claims.tenant_id, q.workspace_id).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    match state.cron_job_repo.find_by_id(&id).await {
        Ok(Some(job)) => ApiResponseBuilder::success(map_cron_job_to_job(job)),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            ApiResponseBuilder::error("获取任务详情失败")
        }
    }
}

async fn create_job(
    claims: Claims,
    State(state): State<AppState>,
    Json(mut payload): Json<CreateJobRequest>,
) -> Json<ApiResponse<Job>> {
    // Normalize 5-field cron to 6-field (prepend seconds=0)
    {
        let fields: Vec<&str> = payload.cron_expression.split_whitespace().collect();
        if fields.len() == 5 {
            payload.cron_expression = format!("0 {}", payload.cron_expression);
        }
    }

    if let Err(e) = cron::Schedule::from_str(&payload.cron_expression) {
        tracing::error!("Invalid cron expression: {}", e);
        return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
    }

    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    let req = map_create_request(&payload, &ws_id);

    match state.cron_job_repo.create(&req, Some(&claims.user_id)).await {
        Ok(job) => ApiResponseBuilder::success(map_cron_job_to_job(job)),
        Err(e) => {
            tracing::error!("Failed to create job: {}", e);
            ApiResponseBuilder::error("创建任务失败")
        }
    }
}

async fn update_job(
    claims: Claims,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut payload): Json<UpdateJobRequest>,
) -> Json<ApiResponse<Job>> {
    // Normalize 5-field cron to 6-field (prepend seconds=0)
    if let Some(ref cron) = payload.cron_expression {
        let fields: Vec<&str> = cron.split_whitespace().collect();
        if fields.len() == 5 {
            payload.cron_expression = Some(format!("0 {}", cron));
        }
    }

    if let Some(ref cron) = payload.cron_expression
        && let Err(e) = cron::Schedule::from_str(cron) {
            tracing::error!("Invalid cron expression: {}", e);
            return ApiResponseBuilder::error_with_code(400, "无效的 Cron 表达式");
        }

    let _ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    let req = map_update_request(&payload);

    match state.cron_job_repo.update(&id, &req).await {
        Ok(job) => ApiResponseBuilder::success(map_cron_job_to_job(job)),
        Err(Error::NotFound) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to update job: {}", e);
            ApiResponseBuilder::error("更新任务失败")
        }
    }
}

async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<WorkspaceQuery>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, q.workspace_id).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    match state.cron_job_repo.delete(&id).await {
        Ok(true) => {
            let _ = state.cron_run_repo.delete_by_job_id(&id, &ws_id).await;
            ApiResponseBuilder::success(true)
        }
        Ok(false) => ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to delete job: {}", e);
            ApiResponseBuilder::error("删除任务失败")
        }
    }
}

async fn run_job_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<JobExecution>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let job = match state.cron_job_repo.find_by_id(&id).await {
        Ok(Some(j)) => j,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            return ApiResponseBuilder::error("获取任务详情失败");
        }
    };

    let claimed = match state.cron_job_repo.claim_job(&id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to claim job: {}", e);
            return ApiResponseBuilder::error("启动任务失败");
        }
    };

    if !claimed {
        return ApiResponseBuilder::error_with_code(409, "任务正在运行中");
    }

    let workspace_id = job.workspace_id.clone().unwrap_or_else(|| ws_id.clone());
    let run = match state
        .cron_run_repo
        .create(&job.id, &workspace_id, "manual", Some(&claims.user_id))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create run record: {}", e);
            let _ = state.cron_job_repo.set_running(&id, false).await;
            return ApiResponseBuilder::error("创建执行记录失败");
        }
    };

    let job_repo = state.cron_job_repo.clone();
    let run_repo = state.cron_run_repo.clone();
    let job_clone = job.clone();
    let run_id = run.id.clone();
    let registry = Arc::new(ExecutorRegistry::new());

    tokio::spawn(async move {
        let executor = registry.find(&job_clone.job_type);
        let timeout_secs = job_clone.timeout_seconds.max(1) as u64;
        let start = std::time::Instant::now();

        let result = if let Some(exec) = executor {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                exec.execute(&job_clone, &run_id),
            )
            .await
            {
                Ok(Ok(res)) => Ok(res),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(tinyiothub_runtime::cron::ExecutorError::Timeout(timeout_secs)),
            }
        } else {
            Err(tinyiothub_runtime::cron::ExecutorError::InvalidConfig(format!(
                "no executor for job type {}",
                job_clone.job_type
            )))
        };

        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(res) => {
                let _ = run_repo
                    .complete(
                        &run_id,
                        &workspace_id,
                        &res.status,
                        res.output.as_deref(),
                        res.error_message.as_deref(),
                        res.duration_ms,
                    )
                    .await;
                let _ = job_repo
                    .update_run_stats(&job_clone.id, &res.status, res.error_message.as_deref())
                    .await;
            }
            Err(err) => {
                let status = match err {
                    tinyiothub_runtime::cron::ExecutorError::Timeout(_) => "timeout",
                    _ => "failed",
                };
                let err_msg = err.to_string();
                let _ = run_repo
                    .complete(&run_id, &workspace_id, status, None, Some(&err_msg), duration_ms)
                    .await;
                let _ = job_repo
                    .update_run_stats(&job_clone.id, status, Some(&err_msg))
                    .await;
            }
        }

        let _ = job_repo.set_running(&job_clone.id, false).await;
    });

    ApiResponseBuilder::success(map_cron_run_to_execution(run))
}

async fn list_job_executions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<JobExecutionQueryParams>,
    claims: Claims,
) -> Json<ApiResponse<Vec<JobExecution>>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    let mut query = map_execution_query(&params);
    query.job_id = Some(id);

    match state.cron_run_repo.find_by_job_id(&params.job_id.unwrap_or_default(), &ws_id, &query).await {
        Ok(runs) => ApiResponseBuilder::success(
            runs.into_iter().map(map_cron_run_to_execution).collect(),
        ),
        Err(e) => {
            tracing::error!("Failed to list executions: {}", e);
            ApiResponseBuilder::error("获取执行记录失败")
        }
    }
}

async fn get_statistics(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<JobStatistics>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let total = state.cron_job_repo.count().await.unwrap_or(0);
    let success_runs = state
        .cron_run_repo
        .count_by_status(&ws_id, "success")
        .await
        .unwrap_or(0);
    let failed_runs = state
        .cron_run_repo
        .count_by_status(&ws_id, "failed")
        .await
        .unwrap_or(0);

    let enabled_jobs = state.cron_job_repo.count_by_enabled(true).await.unwrap_or(0);
    let disabled_jobs = state.cron_job_repo.count_by_enabled(false).await.unwrap_or(0);
    let running_jobs = state.cron_job_repo.count_running().await.unwrap_or(0);
    let avg_duration_ms = state.cron_run_repo.avg_duration_ms(&ws_id).await.unwrap_or(0);

    let stats = JobStatistics {
        total_jobs: total,
        enabled_jobs,
        disabled_jobs,
        running_jobs,
        total_executions: success_runs + failed_runs,
        success_executions: success_runs,
        failed_executions: failed_runs,
        avg_duration_ms,
    };

    ApiResponseBuilder::success(stats)
}

async fn list_all_executions(
    State(state): State<AppState>,
    Query(params): Query<JobExecutionQueryParams>,
    claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<JobExecution>>> {
    let ws_id = match resolve_workspace(&state, &claims.tenant_id, None).await {
        Ok(ws) => ws,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);

    let query = CronRunQuery {
        job_id: None,
        workspace_id: None,
        status: params.status.clone(),
        trigger_type: params.trigger_type.clone(),
        page: Some(page),
        page_size: Some(page_size),
    };

    match state.cron_run_repo.find_all(&ws_id, &query).await {
        Ok(runs) => {
            let total = runs.len() as i64;
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
            ApiResponseBuilder::success(PaginatedResponse {
                data: runs.into_iter().map(map_cron_run_to_execution).collect(),
                pagination: PaginationInfo {
                    page,
                    page_size,
                    total_pages,
                    total_count: total as u64,
                },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list all executions: {}", e);
            ApiResponseBuilder::error("获取执行记录失败")
        }
    }
}
