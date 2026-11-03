use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use crate::domains::agent::thing_agent::ThingAgentManager;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tinyiothub_core::agent_task::WakeDispatchState;
use tinyiothub_storage::Db;
use tinyiothub_storage::runtime_admin::{RuntimeDrainCounts, UnknownDispositionOutcome};
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::tasks::error_response;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/outbox", get(list_outbox))
        .route("/outbox/{event_id}/retry", post(retry_dead_wake))
        .route("/jobs", get(list_jobs))
        .route("/drain", get(get_drain_status))
        .route("/drain/cancel", post(cancel_drain_batch))
        .route("/unknown/{job_id}/disposition", post(record_unknown_disposition))
        .route("/metrics", get(runtime_metrics))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainStatus {
    pub nonterminal_tasks: u64,
    pub active_runs: u64,
    pub active_jobs: u64,
    pub active_watches: u64,
    pub pending_approvals: u64,
    pub pending_outbox: u64,
    pub unresolved_unknown: u64,
}

impl DrainStatus {
    pub fn can_hide_workbench(&self) -> bool {
        self.nonterminal_tasks == 0
            && self.active_runs == 0
            && self.active_jobs == 0
            && self.active_watches == 0
            && self.pending_approvals == 0
            && self.pending_outbox == 0
            && self.unresolved_unknown == 0
    }

    /// Media shutdown uses the same gate: a nonzero category can still hold
    /// evidence or a physical side effect that has not settled.
    pub fn can_hide_media(&self) -> bool {
        self.can_hide_workbench()
    }
}

impl From<RuntimeDrainCounts> for DrainStatus {
    fn from(counts: RuntimeDrainCounts) -> Self {
        Self {
            nonterminal_tasks: counts.nonterminal_tasks,
            active_runs: counts.active_runs,
            active_jobs: counts.active_jobs,
            active_watches: counts.active_watches,
            pending_approvals: counts.pending_approvals,
            pending_outbox: counts.pending_outbox,
            unresolved_unknown: counts.unresolved_unknown,
        }
    }
}

fn drain_json(status: &DrainStatus) -> serde_json::Value {
    json!({
        "drain": status,
        "canHideWorkbench": status.can_hide_workbench(),
        "canHideMedia": status.can_hide_media(),
    })
}

async fn load_drain(state: &AgentState, workspace_id: &str) -> Result<DrainStatus, Response> {
    state
        .db
        .runtime_drain_counts(Some(workspace_id))
        .await
        .map(DrainStatus::from)
        .map_err(|error| {
            tracing::error!(%workspace_id, %error, "load runtime drain status failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Runtime drain 不可用")
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWorkspaceScope {
    pub workspace_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxQuery {
    pub state: String,
    pub workspace_id: Option<String>,
}

async fn check_admin_read_access(
    state: &AgentState,
    claims: &Claims,
    workspace_id: Option<&str>,
) -> Result<(), Response> {
    if let Some(workspace_id) = workspace_id {
        return check_agent_admin_response(state, claims, workspace_id).await;
    }
    if crate::domains::agent::host::handler::is_admin(&state.db, &claims.user_id).await {
        Ok(())
    } else {
        Err(error_response(StatusCode::FORBIDDEN, "需要管理员权限"))
    }
}

pub async fn list_outbox(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<OutboxQuery>,
) -> Response {
    if query.state != "dead" {
        return error_response(StatusCode::BAD_REQUEST, "state 仅支持 dead");
    }
    if let Err(response) = check_admin_read_access(&state, &claims, query.workspace_id.as_deref()).await {
        return response;
    }
    match state
        .db
        .runtime_outbox(query.workspace_id.as_deref(), WakeDispatchState::Dead, 200)
        .await
    {
        Ok(outbox) => ApiResponseBuilder::success(json!({"outbox": outbox})).into_response(),
        Err(error) => {
            tracing::error!(%error, "list runtime dead outbox failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Runtime outbox 不可用")
        }
    }
}

pub async fn retry_dead_wake(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(event_id): Path<String>,
    Query(scope): Query<AdminWorkspaceScope>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &scope.workspace_id).await {
        return response;
    }
    match state.db.retry_dead_wake(&scope.workspace_id, &event_id).await {
        Ok(true) => ApiResponseBuilder::success(json!({"eventId": event_id, "state": "pending"})).into_response(),
        Ok(false) => error_response(StatusCode::NOT_FOUND, "Dead wake 不存在"),
        Err(error) => {
            tracing::error!(%error, %event_id, "retry runtime dead wake failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Wake retry 失败")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobsQuery {
    pub state: String,
    pub workspace_id: Option<String>,
}

pub async fn list_jobs(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<JobsQuery>,
) -> Response {
    let unknown = match query.state.as_str() {
        "active" => false,
        "unknown" => true,
        _ => return error_response(StatusCode::BAD_REQUEST, "state 仅支持 active 或 unknown"),
    };
    if let Err(response) = check_admin_read_access(&state, &claims, query.workspace_id.as_deref()).await {
        return response;
    }
    match state.db.active_jobs(query.workspace_id.as_deref(), unknown).await {
        Ok(jobs) => ApiResponseBuilder::success(json!({"jobs": jobs})).into_response(),
        Err(error) => {
            tracing::error!(%error, "list runtime diagnostic Jobs failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Device Job 诊断不可用")
        }
    }
}

pub async fn get_drain_status(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Query(scope): Query<AdminWorkspaceScope>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &scope.workspace_id).await {
        return response;
    }
    match load_drain(&state, &scope.workspace_id).await {
        Ok(status) => ApiResponseBuilder::success(drain_json(&status)).into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainCancelRequest {
    pub workspace_id: String,
    pub task_ids: Vec<String>,
}

/// The per-Task Cancel operation used by the fixed-batch drain rollback.
/// Implemented for the real manager and for deterministic test doubles.
trait DrainTaskCancel: Send + Sync {
    fn cancel<'a>(
        &'a self,
        workspace_id: &'a str,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send + 'a>>;
}

impl DrainTaskCancel for ThingAgentManager {
    fn cancel<'a>(
        &'a self,
        workspace_id: &'a str,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send + 'a>> {
        Box::pin(async move {
            self.cancel_task(workspace_id, task_id)
                .await
                .map(|outcome| outcome.accepted)
                .map_err(|error| error.to_string())
        })
    }
}

struct DrainCancelReport {
    requested: usize,
    cancelled: usize,
    results: Vec<serde_json::Value>,
    drain: Option<DrainStatus>,
}

/// Cancel exactly the supplied IDs, one per-Task Cancel call each, then
/// recompute drain from the committed post-cancel state.
async fn cancel_fixed_task_batch<C: DrainTaskCancel + ?Sized>(
    db: &Db,
    cancellations: &C,
    workspace_id: &str,
    task_ids: &[String],
) -> DrainCancelReport {
    let requested = task_ids.len();
    let mut cancelled = 0usize;
    let mut results = Vec::with_capacity(requested);
    for task_id in task_ids {
        match cancellations.cancel(workspace_id, task_id).await {
            Ok(accepted) => {
                if accepted {
                    cancelled += 1;
                }
                let outcome = if accepted {
                    "cancelled"
                } else {
                    match db.get_task(workspace_id, task_id).await {
                        Ok(Some(task)) if task.status.is_terminal() => "already_terminal",
                        Ok(Some(_)) => "in_progress",
                        Ok(None) => "not_found",
                        Err(error) => {
                            tracing::warn!(%workspace_id, %task_id, %error, "drain cancel outcome lookup failed");
                            "failed"
                        }
                    }
                };
                results.push(json!({"taskId": task_id, "outcome": outcome}));
            }
            Err(error) => {
                tracing::warn!(
                    %workspace_id,
                    %task_id,
                    %error,
                    "drain batch cancel failed for Task"
                );
                results.push(json!({"taskId": task_id, "outcome": "failed"}));
            }
        }
    }
    tracing::info!(
        metric = "runtime_drain_cancel_batch",
        %workspace_id,
        requested,
        cancelled,
        "runtime drain cancel batch"
    );
    let drain = db
        .runtime_drain_counts(Some(workspace_id))
        .await
        .ok()
        .map(DrainStatus::from);
    DrainCancelReport {
        requested,
        cancelled,
        results,
        drain,
    }
}

/// Cancel a caller-fixed ID batch. Every ID goes through the existing
/// per-Task Cancel path; no bulk status update and no implicit discovery.
pub async fn cancel_drain_batch(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DrainCancelRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &request.workspace_id).await {
        return response;
    }
    if request.task_ids.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "taskIds 不能为空");
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    let report = cancel_fixed_task_batch(
        state.db.as_ref(),
        manager.as_ref(),
        &request.workspace_id,
        &request.task_ids,
    )
    .await;
    ApiResponseBuilder::success(json!({
        "requested": report.requested,
        "cancelled": report.cancelled,
        "results": report.results,
        "drain": report.drain,
        "canHideWorkbench": report.drain.as_ref().map(DrainStatus::can_hide_workbench),
        "canHideMedia": report.drain.as_ref().map(DrainStatus::can_hide_media),
    }))
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispositionRequest {
    pub workspace_id: String,
    pub disposition: String,
}

pub async fn record_unknown_disposition(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
    Json(request): Json<DispositionRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &request.workspace_id).await {
        return response;
    }
    let disposition = request.disposition.trim();
    if disposition.is_empty() || disposition.chars().count() > 500 {
        return error_response(StatusCode::BAD_REQUEST, "disposition 须为 1..=500 个字符");
    }
    match state
        .db
        .record_unknown_disposition(&request.workspace_id, &job_id, disposition, &claims.user_id)
        .await
    {
        Ok(UnknownDispositionOutcome::Recorded | UnknownDispositionOutcome::AlreadyRecorded) => {
            match load_drain(&state, &request.workspace_id).await {
                Ok(status) => ApiResponseBuilder::success(json!({
                    "jobId": job_id,
                    "dispositionRecorded": true,
                    "physicalStateReplayed": false,
                    "drain": status,
                    "canHideWorkbench": status.can_hide_workbench(),
                    "canHideMedia": status.can_hide_media(),
                }))
                .into_response(),
                Err(response) => response,
            }
        }
        Ok(UnknownDispositionOutcome::NotFound) => error_response(StatusCode::NOT_FOUND, "Job 不存在"),
        Ok(UnknownDispositionOutcome::NotUnknown) => {
            error_response(StatusCode::CONFLICT, "仅 cancelled_unknown/interrupted Job 可登记处置")
        }
        Err(error) => {
            tracing::error!(%job_id, %error, "record unknown disposition failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "未知结果处置不可用")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsQuery {
    pub workspace_id: Option<String>,
}

pub async fn runtime_metrics(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<MetricsQuery>,
) -> Response {
    if let Err(response) = check_admin_read_access(&state, &claims, query.workspace_id.as_deref()).await {
        return response;
    }
    let started = Instant::now();
    let snapshot = match state.db.runtime_metrics(query.workspace_id.as_deref()).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::error!(%error, "load runtime metrics failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Runtime 指标不可用");
        }
    };
    let db_query_ms = started.elapsed().as_millis() as u64;
    let stream = state.runtime_stream.metrics();
    let mut system = sysinfo::System::new_all();
    system.refresh_memory();
    let rss_bytes = system
        .process(sysinfo::Pid::from_u32(std::process::id()))
        .map_or(0, sysinfo::Process::memory);
    let total_memory = system.total_memory().max(1);
    let rss_percent = (rss_bytes as f64 / total_memory as f64) * 100.0;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let (disk_total, disk_used) = disks.iter().fold((0u64, 0u64), |(total, used), disk| {
        (
            total + disk.total_space(),
            used + disk.total_space().saturating_sub(disk.available_space()),
        )
    });
    let disk_used_percent = if disk_total == 0 {
        0.0
    } else {
        (disk_used as f64 / disk_total as f64) * 100.0
    };
    tracing::info!(
        metric = "runtime_host_metrics",
        rss_bytes,
        rss_percent,
        disk_used_percent,
        db_query_ms,
        "runtime host metrics"
    );
    ApiResponseBuilder::success(json!({
        "metrics": snapshot,
        "stream": stream,
        "host": {
            "rssBytes": rss_bytes,
            "rssPercent": rss_percent,
            "diskUsedPercent": disk_used_percent,
        },
        "dbQueryMs": db_query_ms,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};
    use tower::ServiceExt;

    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
    };

    const WS: &str = "ws-runtime-admin";

    async fn fixture() -> (axum::Router, tinyiothub_storage::Db, String, String) {
        let (app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", WS).await;
        sqlx::query("INSERT OR IGNORE INTO roles (id, name, description, is_administrator) VALUES ('role-admin','admin','admin',1)")
            .execute(&pool)
            .await
            .expect("role");
        sqlx::query(
            "INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES ('ur-runtime-admin','user-1','role-admin')",
        )
        .execute(&pool)
        .await
        .expect("grant");
        let db = tinyiothub_storage::Db::new(pool);
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: "task-admin".into(),
                workspace_id: WS.into(),
                idempotency_key: "admin-key".into(),
                objective: "inspect".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 1,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: "task-admin".into(),
                event_id: "dead-original".into(),
                signal_json: json!({"workspace_id":WS}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("task");
        db.mark_wake("task-admin", "dead-original", WakeDispatchState::Dead)
            .await
            .expect("dead wake");
        let api = crate::api::create_router(&app_state);
        let app = axum::Router::new().nest("/api", api).with_state(app_state);
        (
            app,
            db,
            create_test_token("user-1", "tenant-1"),
            create_test_token("user-2", "tenant-1"),
        )
    }

    async fn request(app: &axum::Router, token: &str, method: &str, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", auth_header(token))
            .body(Body::empty())
            .expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
    }

    async fn post_json(app: &axum::Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("Authorization", auth_header(token))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
    }

    #[test]
    fn drain_gate_blocks_every_nonzero_category() {
        let zero = super::DrainStatus {
            nonterminal_tasks: 0,
            active_runs: 0,
            active_jobs: 0,
            active_watches: 0,
            pending_approvals: 0,
            pending_outbox: 0,
            unresolved_unknown: 0,
        };
        assert!(zero.can_hide_workbench());
        assert!(zero.can_hide_media());
        let statuses = [
            super::DrainStatus {
                nonterminal_tasks: 1,
                ..zero
            },
            super::DrainStatus { active_runs: 1, ..zero },
            super::DrainStatus { active_jobs: 1, ..zero },
            super::DrainStatus {
                active_watches: 1,
                ..zero
            },
            super::DrainStatus {
                pending_approvals: 1,
                ..zero
            },
            super::DrainStatus {
                pending_outbox: 1,
                ..zero
            },
            super::DrainStatus {
                unresolved_unknown: 1,
                ..zero
            },
        ];
        for status in statuses {
            assert!(!status.can_hide_workbench(), "{status:?}");
            assert!(!status.can_hide_media(), "{status:?}");
        }
    }

    async fn insert_unknown_job(db: &tinyiothub_storage::Db, id: &str) {
        db.create_job(&job(id, DeviceJobState::Running))
            .await
            .expect("unknown insert");
        db.finish_job(
            id,
            DeviceJobState::CancelledUnknown,
            &json!({"reason":"cancel_unknown"}),
            None,
        )
        .await
        .expect("unknown finish");
    }

    async fn seed_task(db: &tinyiothub_storage::Db, id: &str, status: TaskStatus) {
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: id.into(),
                workspace_id: WS.into(),
                idempotency_key: format!("{id}-key"),
                objective: "drain fixture".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 1,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: id.into(),
                event_id: format!("{id}-wake"),
                signal_json: json!({"workspace_id":WS}).to_string(),
                dispatch_state: WakeDispatchState::Dispatched,
                created_at: now,
                dispatched_at: Some(now),
                resolved_at: Some(now),
            },
        )
        .await
        .expect("seed task");
        if status == TaskStatus::Active {
            return;
        }
        let cancelling = db
            .transition_task(WS, id, 1, TaskStatus::Cancelling, None)
            .await
            .expect("cancelling");
        if status == TaskStatus::Cancelled {
            db.transition_task(WS, id, cancelling.state_version, TaskStatus::Cancelled, None)
                .await
                .expect("cancelled");
        }
    }

    async fn seed_owned_watch(db: &tinyiothub_storage::Db, id: &str, owner_task_id: &str) {
        sqlx::query(
            "INSERT INTO watch_rules
                 (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
             VALUES (?, ?, 'availability', '{\"thing_id\":\"dev_1\",\"online\":true}', 0, 60, 'now', ?)",
        )
        .bind(id)
        .bind(WS)
        .bind(owner_task_id)
        .execute(db.pool())
        .await
        .expect("watch");
    }

    #[tokio::test]
    async fn drain_http_surfaces_every_durable_category_and_ignores_terminal_owner_watch() {
        let (app, db, admin, _) = fixture().await;
        seed_task(&db, "task-terminal", TaskStatus::Cancelled).await;
        seed_owned_watch(&db, "watch-live", "task-admin").await;
        seed_owned_watch(&db, "watch-terminal", "task-terminal").await;
        sqlx::query(
            "INSERT INTO task_approvals
                 (id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json, reason,
                  state, decision, decider_id, requested_at, decided_at, expires_at, decision_event_id, updated_at)
             VALUES ('approval-http', ?, 'task-admin', 'run-1', 'call-1', 'dev-1', 'set_speed', '{}',
                     'confirm', 'pending', NULL, NULL, '2026-10-29T00:00:00Z', NULL,
                     '2026-10-30T00:00:00Z', 'decision-1', '2026-10-29T00:00:00Z')",
        )
        .bind(WS)
        .execute(db.pool())
        .await
        .expect("approval");
        sqlx::query(
            "INSERT INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES ('task-admin', 'pending-http', '{\"workspace_id\":\"ws-runtime-admin\"}', 'pending',
                     '2026-10-29T00:00:00Z', NULL, NULL)",
        )
        .execute(db.pool())
        .await
        .expect("pending outbox");
        insert_unknown_job(&db, "unknown-http").await;

        let drain_uri = format!("/api/v1/admin/runtime/drain?workspaceId={WS}");
        let (status, body) = request(&app, &admin, "GET", &drain_uri).await;
        assert_eq!(status, StatusCode::OK, "drain: {body}");
        let drain = &body["result"]["drain"];
        assert_eq!(drain["activeWatches"], 1, "terminal Task watch must count zero: {body}");
        assert_eq!(drain["pendingApprovals"], 1, "{body}");
        assert_eq!(drain["pendingOutbox"], 1, "{body}");
        assert_eq!(drain["unresolvedUnknown"], 1, "{body}");
        assert_eq!(body["result"]["canHideWorkbench"], false);
        assert_eq!(body["result"]["canHideMedia"], false);
    }

    #[tokio::test]
    async fn drain_counts_unknown_until_administrator_records_disposition() {
        let (app, db, admin, reader) = fixture().await;
        insert_unknown_job(&db, "unknown-drain").await;
        let drain_uri = format!("/api/v1/admin/runtime/drain?workspaceId={WS}");
        let (status, _) = request(&app, &reader, "GET", &drain_uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = request(&app, &admin, "GET", &drain_uri).await;
        assert_eq!(status, StatusCode::OK, "drain: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 1);
        assert_eq!(body["result"]["canHideWorkbench"], false);
        assert_eq!(body["result"]["canHideMedia"], false);

        let uri = "/api/v1/admin/runtime/unknown/unknown-drain/disposition";
        let (status, body) = post_json(
            &app,
            &admin,
            uri,
            json!({"workspaceId": WS, "disposition": "inspected: valve physically closed"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "disposition: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 0);
        assert_eq!(body["result"]["physicalStateReplayed"], false);
        assert_eq!(
            db.get_job(WS, "unknown-drain").await.expect("get").expect("job").state,
            DeviceJobState::CancelledUnknown,
            "disposition must never rewrite or replay the Job"
        );
        let jobs = db.active_jobs(Some(WS), true).await.expect("unknown jobs");
        assert_eq!(jobs.len(), 1);

        let (status, body) = post_json(
            &app,
            &admin,
            uri,
            json!({"workspaceId": WS, "disposition": "inspected again"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "idempotent disposition: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 0);

        let (status, _) = post_json(&app, &admin, uri, json!({"workspaceId": WS, "disposition": "   "})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = post_json(
            &app,
            &admin,
            "/api/v1/admin/runtime/unknown/missing/disposition",
            json!({"workspaceId": WS, "disposition": "inspected"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let known = job("known-drain", DeviceJobState::Running);
        db.create_job(&known).await.expect("known job");
        let (status, _) = post_json(
            &app,
            &admin,
            "/api/v1/admin/runtime/unknown/known-drain/disposition",
            json!({"workspaceId": WS, "disposition": "inspected"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn drain_counts_interrupted_unknown_until_dispositioned() {
        let (app, db, admin, _) = fixture().await;
        db.create_job(&job("interrupted-drain", DeviceJobState::Running))
            .await
            .expect("interrupted insert");
        db.finish_job(
            "interrupted-drain",
            DeviceJobState::Interrupted,
            &json!({"reason":"worker call lost"}),
            None,
        )
        .await
        .expect("interrupted finish");

        let drain_uri = format!("/api/v1/admin/runtime/drain?workspaceId={WS}");
        let (status, body) = request(&app, &admin, "GET", &drain_uri).await;
        assert_eq!(status, StatusCode::OK, "drain: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 1);
        assert_eq!(body["result"]["canHideWorkbench"], false);
        assert_eq!(body["result"]["canHideMedia"], false);

        let uri = "/api/v1/admin/runtime/unknown/interrupted-drain/disposition";
        let (status, body) = post_json(
            &app,
            &admin,
            uri,
            json!({"workspaceId": WS, "disposition": "inspected: pump physically stopped"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "interrupted disposition: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 0);
        assert_eq!(body["result"]["physicalStateReplayed"], false);
        assert_eq!(
            db.get_job(WS, "interrupted-drain")
                .await
                .expect("get")
                .expect("job")
                .state,
            DeviceJobState::Interrupted,
            "disposition must never rewrite or replay an interrupted Job"
        );

        db.transition_task(WS, "task-admin", 1, TaskStatus::Cancelling, None)
            .await
            .expect("cancel task");
        db.transition_task(WS, "task-admin", 2, TaskStatus::Cancelled, None)
            .await
            .expect("finish cancel");
        let (status, body) = request(&app, &admin, "GET", &drain_uri).await;
        assert_eq!(status, StatusCode::OK, "settled drain: {body}");
        assert_eq!(body["result"]["drain"]["unresolvedUnknown"], 0);
        assert_eq!(body["result"]["canHideWorkbench"], true);
        assert_eq!(body["result"]["canHideMedia"], true);
    }

    #[tokio::test]
    async fn drain_cancel_requires_fixed_ids_and_never_bulk_mutates() {
        let (app, db, admin, reader) = fixture().await;
        let uri = "/api/v1/admin/runtime/drain/cancel";
        let (status, _) = post_json(
            &app,
            &reader,
            uri,
            json!({"workspaceId": WS, "taskIds": ["task-admin"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = post_json(&app, &admin, uri, json!({"workspaceId": WS, "taskIds": []})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, body) = post_json(&app, &admin, uri, json!({"workspaceId": WS, "taskIds": ["task-admin"]})).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "no manager is wired in this fixture: {body}"
        );
        let task = db.get_task(WS, "task-admin").await.expect("get").expect("task");
        assert_eq!(task.status, tinyiothub_core::agent_task::TaskStatus::Active);
        assert_eq!(task.state_version, 1);
    }

    struct StubCancel {
        db: tinyiothub_storage::Db,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl super::DrainTaskCancel for StubCancel {
        fn cancel<'a>(
            &'a self,
            workspace_id: &'a str,
            task_id: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, String>> + Send + 'a>> {
            Box::pin(async move {
                self.calls.lock().expect("calls").push(task_id.to_string());
                if task_id == "missing-task" {
                    return Err("task not found".to_string());
                }
                if task_id == "noop-missing" {
                    // A cancel path may report "nothing to do" for an ID that
                    // is not in the projection; the report must say not_found.
                    return Ok(false);
                }
                if task_id == "lookup-error" {
                    // Force the post-cancel outcome lookup to hit a DB error.
                    return Ok(false);
                }
                let task = self
                    .db
                    .get_task(workspace_id, task_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "task not found".to_string())?;
                if task.status.is_terminal() || task.status == TaskStatus::Cancelling {
                    return Ok(false);
                }
                let cancelling = self
                    .db
                    .transition_task(workspace_id, task_id, task.state_version, TaskStatus::Cancelling, None)
                    .await
                    .map_err(|error| error.to_string())?;
                self.db
                    .transition_task(
                        workspace_id,
                        task_id,
                        cancelling.state_version,
                        TaskStatus::Cancelled,
                        None,
                    )
                    .await
                    .map(|_| true)
                    .map_err(|error| error.to_string())
            })
        }
    }

    #[tokio::test]
    async fn drain_cancel_batch_calls_per_task_cancel_and_recomputes_drain() {
        let (_, db, _, _) = fixture().await;
        let before = db.runtime_drain_counts(Some(WS)).await.expect("before");
        assert_eq!(before.nonterminal_tasks, 1);
        assert!(!super::DrainStatus::from(before).can_hide_workbench());

        let stub = StubCancel {
            db: db.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let task_ids = vec!["task-admin".to_string(), "missing-task".to_string()];
        let report = super::cancel_fixed_task_batch(&db, &stub, WS, &task_ids).await;

        assert_eq!(report.requested, 2);
        assert_eq!(report.cancelled, 1);
        assert_eq!(report.results[0]["taskId"], "task-admin");
        assert_eq!(report.results[0]["outcome"], "cancelled");
        assert_eq!(report.results[1]["taskId"], "missing-task");
        assert_eq!(report.results[1]["outcome"], "failed");
        assert_eq!(
            stub.calls.lock().expect("calls").as_slice(),
            ["task-admin", "missing-task"],
            "exactly one Cancel call per supplied ID"
        );

        let drain = report.drain.expect("post-cancel drain");
        assert_eq!(drain.nonterminal_tasks, 0);
        assert!(drain.can_hide_workbench());
        assert!(drain.can_hide_media());
        assert_eq!(
            db.get_task(WS, "task-admin").await.expect("get").expect("task").status,
            TaskStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn drain_cancel_reports_cancelled_already_terminal_in_progress_not_found_and_failed() {
        let (_, db, _, _) = fixture().await;
        seed_task(&db, "task-terminal", TaskStatus::Cancelled).await;
        seed_task(&db, "task-cancelling", TaskStatus::Cancelling).await;
        let stub = StubCancel {
            db: db.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let task_ids = vec![
            "task-admin".to_string(),
            "task-terminal".to_string(),
            "task-cancelling".to_string(),
            "noop-missing".to_string(),
            "missing-task".to_string(),
        ];
        let report = super::cancel_fixed_task_batch(&db, &stub, WS, &task_ids).await;

        assert_eq!(report.requested, 5);
        assert_eq!(report.cancelled, 1, "only the accepted cancel counts as cancelled");
        let outcomes: Vec<&str> = report
            .results
            .iter()
            .map(|result| result["outcome"].as_str().expect("outcome"))
            .collect();
        assert_eq!(
            outcomes,
            ["cancelled", "already_terminal", "in_progress", "not_found", "failed"]
        );
        assert_eq!(
            db.get_task(WS, "task-terminal")
                .await
                .expect("get")
                .expect("task")
                .status,
            TaskStatus::Cancelled,
            "terminal Tasks must not be rewritten"
        );
        assert_eq!(
            db.get_task(WS, "task-cancelling")
                .await
                .expect("get")
                .expect("task")
                .status,
            TaskStatus::Cancelling,
            "an in-flight cancel must keep its status"
        );
    }

    #[tokio::test]
    async fn drain_cancel_reports_a_db_lookup_error_as_failed_not_in_progress() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        let db = tinyiothub_storage::Db::new(pool.clone());
        pool.close().await;
        let stub = StubCancel {
            db: db.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let report = super::cancel_fixed_task_batch(&db, &stub, WS, &["lookup-error".to_string()]).await;
        assert_eq!(report.results[0]["outcome"], "failed");
        assert_eq!(report.cancelled, 0);
    }

    #[tokio::test]
    async fn runtime_metrics_expose_durable_gauges_and_bound_access() {
        let (app, db, admin, reader) = fixture().await;
        insert_unknown_job(&db, "unknown-metrics").await;
        let uri = format!("/api/v1/admin/runtime/metrics?workspaceId={WS}");
        let (status, _) = request(&app, &reader, "GET", &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = request(&app, &admin, "GET", &uri).await;
        assert_eq!(status, StatusCode::OK, "metrics: {body}");
        assert_eq!(body["result"]["metrics"]["drain"]["unresolvedUnknown"], 1);
        assert_eq!(body["result"]["metrics"]["outbox"]["dead"], 1);
        assert!(body["result"]["stream"]["activeConnections"].is_number());
        assert!(body["result"]["host"]["rssBytes"].is_number());
        assert!(body["result"]["host"]["diskUsedPercent"].is_number());
        assert!(body["result"]["dbQueryMs"].is_number());
    }

    #[tokio::test]
    async fn admin_retry_preserves_original_dead_wake_and_enforces_guards() {
        let (app, db, admin, reader) = fixture().await;
        let unscoped_outbox_uri = "/api/v1/admin/runtime/outbox?state=dead";
        let (status, _) = request(&app, &reader, "GET", unscoped_outbox_uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = request(&app, &admin, "GET", unscoped_outbox_uri).await;
        assert_eq!(status, StatusCode::OK, "unscoped outbox: {body}");
        assert_eq!(body["result"]["outbox"][0]["eventId"], "dead-original");

        let outbox_uri = format!("/api/v1/admin/runtime/outbox?state=dead&workspaceId={WS}");
        let (status, _) = request(&app, &reader, "GET", &outbox_uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = request(&app, &admin, "GET", &outbox_uri).await;
        assert_eq!(status, StatusCode::OK, "outbox: {body}");
        assert_eq!(body["result"]["outbox"][0]["eventId"], "dead-original");

        let retry_uri = format!("/api/v1/admin/runtime/outbox/dead-original/retry?workspaceId={WS}");
        let (status, body) = request(&app, &admin, "POST", &retry_uri).await;
        assert_eq!(status, StatusCode::OK, "retry: {body}");
        let wake = db
            .wake_by_id("task-admin", "dead-original")
            .await
            .expect("wake")
            .expect("original row");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("count");
        assert_eq!(count, 1, "retry must never create a replacement event");
        let (status, _) = request(&app, &admin, "POST", &retry_uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "pending rows are not retried as dead");

        let (status, _) = request(
            &app,
            &admin,
            "GET",
            "/api/v1/admin/runtime/outbox?state=pending&workspaceId=ws-runtime-admin",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    fn job(id: &str, state: DeviceJobState) -> DeviceJob {
        DeviceJob {
            id: id.into(),
            workspace_id: WS.into(),
            device_id: "device-1".into(),
            tool: "inspect".into(),
            args_json: json!({}),
            state,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some("task-admin".into()),
            run_id: None,
            tool_call_id: Some(format!("call-{id}")),
            driver_call_id: None,
            timeout_ms: 1_000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    #[tokio::test]
    async fn admin_job_diagnostics_expose_active_and_unknown_without_replay() {
        let (app, db, admin, _) = fixture().await;
        db.create_job(&job("active-job", DeviceJobState::Running))
            .await
            .expect("active");
        let unknown = job("unknown-job", DeviceJobState::Running);
        db.create_job(&unknown).await.expect("unknown insert");
        db.finish_job(
            "unknown-job",
            DeviceJobState::CancelledUnknown,
            &json!({"reason":"cancel_unknown"}),
            None,
        )
        .await
        .expect("unknown finish");

        let active_uri = "/api/v1/admin/runtime/jobs?state=active";
        let (status, body) = request(&app, &admin, "GET", active_uri).await;
        assert_eq!(status, StatusCode::OK, "active: {body}");
        assert_eq!(body["result"]["jobs"].as_array().expect("jobs").len(), 1);
        assert_eq!(body["result"]["jobs"][0]["id"], "active-job");

        let unknown_uri = "/api/v1/admin/runtime/jobs?state=unknown";
        let (status, body) = request(&app, &admin, "GET", unknown_uri).await;
        assert_eq!(status, StatusCode::OK, "unknown: {body}");
        assert_eq!(body["result"]["jobs"].as_array().expect("jobs").len(), 1);
        assert_eq!(body["result"]["jobs"][0]["id"], "unknown-job");
        assert_eq!(
            db.get_job(WS, "unknown-job").await.expect("get").expect("job").state,
            DeviceJobState::CancelledUnknown
        );

        let (status, _) = request(
            &app,
            &admin,
            "GET",
            "/api/v1/admin/runtime/jobs?state=failed&workspaceId=ws-runtime-admin",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
