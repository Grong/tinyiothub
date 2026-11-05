//! Harness overview and Workflow API.
//!
//! Routes (nested under `/workspaces` by the composition layer):
//!   GET  /{workspace_id}/harness/overview
//!   GET  /{workspace_id}/harness/main/{task_id}/subagents
//!   GET  /{workspace_id}/harness/workflows
//!   POST /{workspace_id}/harness/workflows
//!   GET  /{workspace_id}/harness/workflows/{workflow_id}
//!   PUT  /{workspace_id}/harness/workflows/{workflow_id}
//!   PUT  /{workspace_id}/harness/workflows/{workflow_id}/enabled
//!
//! Every handler verifies tenant/workspace access before opening the workspace
//! Harness store. Task controls stay on the shared Runtime handlers.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_core::harness::{WorkflowDefinition, WorkflowRevision, WorkflowTrigger};
use tinyiothub_skills::registry::Capability;
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;
use tinyiothub_storage::runtime_task::HarnessTaskRow;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::host::handler::is_admin;
use crate::domains::agent::runtime::tasks::error_response;
use crate::domains::agent::tools::resolve_capabilities;
use crate::domains::agent::workflows::{
    DryRunInput, ProjectionComparison, ProjectionInspector, RetryOutcome, ScriptedDryRunExecutor, SupervisorHealth,
    TemplateInput, WorkflowDryRun, WorkflowService, builtin_templates, next_wake_at,
};

const DEFAULT_ATTENTION_LIMIT: i64 = 5;
const MAX_ATTENTION_LIMIT: i64 = 50;
const SUBAGENT_PAGE_SIZE: i64 = 20;

pub fn create_workspace_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{workspace_id}/harness/overview", get(overview))
        .route("/{workspace_id}/harness/main/{task_id}/subagents", get(subagents))
        .route(
            "/{workspace_id}/harness/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route(
            "/{workspace_id}/harness/workflows/{workflow_id}",
            get(get_workflow).put(update_workflow),
        )
        .route(
            "/{workspace_id}/harness/workflows/{workflow_id}/enabled",
            put(set_workflow_enabled),
        )
        .route(
            "/{workspace_id}/harness/workflow-templates",
            get(list_workflow_templates),
        )
        .route(
            "/{workspace_id}/harness/workflow-templates/{template_id}/instantiate",
            post(instantiate_workflow_template),
        )
        .route(
            "/{workspace_id}/harness/workflows/{workflow_id}/versions/{version}",
            get(get_workflow_version),
        )
        .route(
            "/{workspace_id}/harness/workflows/{workflow_id}/versions/{version}/snapshots",
            get(list_workflow_snapshots),
        )
        .route(
            "/{workspace_id}/harness/workflows/{workflow_id}/versions/{version}/dry-run",
            post(start_workflow_dry_run),
        )
        .route("/{workspace_id}/harness/simulations/{task_id}", get(get_simulation))
        .route("/{workspace_id}/harness/supervisor", get(get_supervisor_health))
        .route(
            "/{workspace_id}/harness/supervisor/dead-wakes/{task_id}/{event_id}/retry",
            post(retry_supervisor_dead_wake),
        )
        .route("/{workspace_id}/harness/compaction", get(compare_compaction))
        .route(
            "/{workspace_id}/harness/compaction/replace",
            post(replace_compaction_snapshot),
        )
}

/// Request DTOs accept the snake_case bodies produced by the web ApiClient.
/// [`WorkflowDefinition`] itself stays camelCase for responses.
#[derive(Debug, Deserialize)]
pub struct WorkflowDefinitionRequest {
    pub objective: String,
    pub trigger: WorkflowTriggerRequest,
    pub device_selector: Value,
    pub source_selector: Value,
    pub tool_ids: Vec<String>,
    pub skill_refs: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub approval_policy: Value,
    pub evidence_policy: Value,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowTriggerRequest {
    Cron { trigger_id: String, expression: String },
    Watch { trigger_id: String },
    Event { trigger_id: String },
}

impl From<WorkflowTriggerRequest> for WorkflowTrigger {
    fn from(request: WorkflowTriggerRequest) -> Self {
        match request {
            WorkflowTriggerRequest::Cron { trigger_id, expression } => WorkflowTrigger::Cron { trigger_id, expression },
            WorkflowTriggerRequest::Watch { trigger_id } => WorkflowTrigger::Watch { trigger_id },
            WorkflowTriggerRequest::Event { trigger_id } => WorkflowTrigger::Event { trigger_id },
        }
    }
}

impl From<WorkflowDefinitionRequest> for WorkflowDefinition {
    fn from(request: WorkflowDefinitionRequest) -> Self {
        Self {
            objective: request.objective,
            trigger: request.trigger.into(),
            device_selector: request.device_selector,
            source_selector: request.source_selector,
            tool_ids: request.tool_ids,
            skill_refs: request.skill_refs,
            required_capabilities: request.required_capabilities,
            approval_policy: request.approval_policy,
            evidence_policy: request.evidence_policy,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WorkflowUpdateRequest {
    pub expected_version: i64,
    /// Echoed by the editor; the service reads the authoritative pointer.
    #[serde(default, rename = "enabled")]
    pub _enabled: bool,
    pub definition: WorkflowDefinitionRequest,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowEnabledRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub attention_limit: Option<u32>,
    pub attention_offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SubagentsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotsQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DryRunStartRequest {
    pub snapshot_seq: i64,
}

/// Fixed boundary for one projection comparison.
#[derive(Debug, Deserialize)]
pub struct CompactionQuery {
    pub projection_name: String,
    pub subject_id: String,
    pub last_seq: i64,
    pub projection_version: i64,
}

/// Explicit CAS replacement. The server re-runs the comparison before the
/// write, so a client-supplied hash or match flag is never trusted.
#[derive(Debug, Deserialize)]
pub struct CompactionReplaceRequest {
    pub projection_name: String,
    pub subject_id: String,
    pub last_seq: i64,
    pub projection_version: i64,
    pub expected_last_seq: i64,
    pub expected_state_version: i64,
}

async fn verify_workspace_read(state: &AgentState, claims: &Claims, workspace_id: &str) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(error_response(StatusCode::FORBIDDEN, "无权访问此工作空间")),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "工作空间不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check Harness workspace failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败"))
        }
    }
}

pub async fn overview(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(query): Query<OverviewQuery>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let limit =
        i64::from(query.attention_limit.unwrap_or(DEFAULT_ATTENTION_LIMIT as u32)).clamp(1, MAX_ATTENTION_LIMIT);
    let offset = i64::from(query.attention_offset.unwrap_or(0)).max(0);
    match overview_payload(&state.db, &workspace_id, limit, offset).await {
        Ok(payload) => ApiResponseBuilder::success(payload).into_response(),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "load Harness overview failed");
            db_error_response(error)
        }
    }
}

pub async fn subagents(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
    Query(query): Query<SubagentsQuery>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let limit = i64::from(query.limit.unwrap_or(SUBAGENT_PAGE_SIZE as u32)).clamp(1, 200);
    let offset = i64::from(query.offset.unwrap_or(0)).max(0);
    match subagents_payload(&state.db, &workspace_id, &task_id, limit, offset).await {
        Ok(payload) => ApiResponseBuilder::success(payload).into_response(),
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "load Harness subagents failed");
            db_error_response(error)
        }
    }
}

pub async fn list_workflows(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    match state.db.list_workflows(&workspace_id).await {
        Ok(workflows) => {
            let result = json!({
                "workflows": workflows.iter().map(workflow_revision_json).collect::<Vec<_>>(),
            });
            ApiResponseBuilder::success(result).into_response()
        }
        Err(error) => {
            tracing::error!(%workspace_id, %error, "list Harness workflows failed");
            workflow_db_error_response(error, None)
        }
    }
}

pub async fn get_workflow(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    match state.db.current_workflow(&workspace_id, &workflow_id).await {
        Ok(Some(revision)) => ApiResponseBuilder::success(workflow_revision_json(&revision)).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "Workflow 不存在"),
        Err(error) => {
            tracing::error!(%workspace_id, %workflow_id, %error, "get Harness workflow failed");
            workflow_db_error_response(error, None)
        }
    }
}

pub async fn create_workflow(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<WorkflowDefinitionRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = WorkflowService::new(state.db.clone(), workspace_id.as_str());
    match service.create(payload.into()).await {
        // Create deliberately ignores any supplied `enabled`; the service
        // always persists a disabled first revision.
        Ok(revision) => ApiResponseBuilder::success(workflow_revision_json(&revision)).into_response(),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "create Harness workflow failed");
            workflow_db_error_response(error, None)
        }
    }
}

pub async fn update_workflow(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id)): Path<(String, String)>,
    Json(payload): Json<WorkflowUpdateRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = WorkflowService::new(state.db.clone(), workspace_id.as_str());
    match service
        .edit(&workflow_id, payload.expected_version, payload.definition.into())
        .await
    {
        Ok(revision) => ApiResponseBuilder::success(workflow_revision_json(&revision)).into_response(),
        Err(DbError::CasConflict { message }) => {
            let latest = state
                .db
                .current_workflow(&workspace_id, &workflow_id)
                .await
                .ok()
                .flatten()
                .map(|revision| revision.version);
            tracing::warn!(%workspace_id, %workflow_id, error = %message, "Harness workflow version conflict");
            workflow_conflict_response(message, latest)
        }
        Err(error) => {
            tracing::error!(%workspace_id, %workflow_id, %error, "update Harness workflow failed");
            workflow_db_error_response(error, None)
        }
    }
}

pub async fn set_workflow_enabled(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id)): Path<(String, String)>,
    Json(payload): Json<WorkflowEnabledRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = WorkflowService::new(state.db.clone(), workspace_id.as_str());
    match service.set_enabled(&workflow_id, payload.enabled).await {
        Ok(revision) => ApiResponseBuilder::success(workflow_revision_json(&revision)).into_response(),
        Err(error) => {
            tracing::error!(%workspace_id, %workflow_id, %error, "set Harness workflow enabled failed");
            workflow_db_error_response(error, None)
        }
    }
}

/// Built-in template catalog. Read-only and workspace-scoped; the full
/// definition lets the first-run form prefill source/policy defaults without a
/// second request.
pub async fn list_workflow_templates(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    ApiResponseBuilder::success(json!({ "templates": builtin_templates() })).into_response()
}

/// DR4=B: a valid template creates an enabled Workflow directly. Reference,
/// capability, and policy validation happens before the revision write, so a
/// rejected request creates no Workflow and no scheduling artifact.
pub async fn instantiate_workflow_template(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, template_id)): Path<(String, String)>,
    Json(payload): Json<TemplateInput>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = WorkflowService::new(state.db.clone(), workspace_id.as_str());
    match service.instantiate_template(&template_id, payload).await {
        Ok(revision) => ApiResponseBuilder::success(workflow_revision_json(&revision)).into_response(),
        Err(error) => {
            tracing::warn!(%workspace_id, %template_id, %error, "instantiate Harness workflow template failed");
            workflow_db_error_response(error, None)
        }
    }
}

/// One fixed historical revision for the dry-run return path. This never
/// follows the current revision; `nextWakeAt` is omitted because scheduling
/// belongs to the current revision, not the historical content.
pub async fn get_workflow_version(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id, version)): Path<(String, String, i64)>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = WorkflowService::new(state.db.clone(), workspace_id.as_str());
    match service.revision(&workflow_id, version).await {
        Ok(revision) => ApiResponseBuilder::success(workflow_fixed_revision_json(&revision)).into_response(),
        Err(error) => {
            tracing::warn!(%workspace_id, %workflow_id, version, %error, "get fixed Workflow revision failed");
            dry_run_error_response(error)
        }
    }
}

/// Snapshot picker for one fixed revision. The sequence and observed time are
/// the only selection inputs; the picker never reads current platform state.
pub async fn list_workflow_snapshots(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id, version)): Path<(String, String, i64)>,
    Query(query): Query<SnapshotsQuery>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let limit = i64::from(query.limit.unwrap_or(50)).clamp(1, 200);
    let service = dry_run_service(&state, &workspace_id);
    match service.list_snapshots(&workflow_id, version, limit).await {
        Ok(snapshots) => ApiResponseBuilder::success(json!({
            "workflowId": workflow_id,
            "version": version,
            "snapshots": snapshots,
        }))
        .into_response(),
        Err(error) => {
            tracing::warn!(%workspace_id, %workflow_id, version, %error, "list dry-run snapshots failed");
            dry_run_error_response(error)
        }
    }
}

/// Start an isolated simulation. This is an Agent-management write surface,
/// exactly like the other Harness configuration writes.
pub async fn start_workflow_dry_run(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, workflow_id, version)): Path<(String, String, i64)>,
    Json(payload): Json<DryRunStartRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = dry_run_service(&state, &workspace_id);
    match service
        .start(DryRunInput {
            workflow_id,
            workflow_version: version,
            snapshot_seq: payload.snapshot_seq,
        })
        .await
    {
        Ok(result) => ApiResponseBuilder::success(result).into_response(),
        Err(error) => {
            tracing::warn!(%workspace_id, version, %error, "start Workflow dry-run failed");
            dry_run_error_response(error)
        }
    }
}

/// Read-only simulation Run projection. Production Task IDs are not readable
/// through this route, and no control or approval entry exists here.
pub async fn get_simulation(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let service = dry_run_service(&state, &workspace_id);
    match service.projection(&task_id).await {
        Ok(Some(projection)) => ApiResponseBuilder::success(projection).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "模拟运行不存在"),
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "load simulation projection failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用")
        }
    }
}

fn supervisor_health(state: &AgentState, workspace_id: &str) -> SupervisorHealth {
    SupervisorHealth::new(state.db.as_ref().clone(), workspace_id)
}

/// Read-only Supervisor panel: bounded health facts plus the current
/// dead-wake page. Retry availability is permission-driven per caller.
pub async fn get_supervisor_health(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let health = supervisor_health(&state, &workspace_id);
    let snapshot = match health.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::error!(%workspace_id, %error, "load Supervisor health failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用");
        }
    };
    let dead_wakes = match health.dead_wakes(200).await {
        Ok(dead_wakes) => dead_wakes,
        Err(error) => {
            tracing::error!(%workspace_id, %error, "load Supervisor dead wakes failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用");
        }
    };
    let can_retry = is_admin(&state.db, &claims.user_id).await;
    ApiResponseBuilder::success(json!({
        "health": snapshot,
        "deadWakes": dead_wakes,
        "canRetry": can_retry,
        "readOnly": !can_retry,
    }))
    .into_response()
}

/// Confirm exactly one original dead wake. Agent management permission and
/// the workspace/tenant check happen before the workspace store is opened.
pub async fn retry_supervisor_dead_wake(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id, event_id)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let health = supervisor_health(&state, &workspace_id);
    match health
        .retry_dead_wake(&workspace_id, &task_id, &event_id, &claims.user_id)
        .await
    {
        Ok(RetryOutcome::Retried | RetryOutcome::AlreadyPending) => ApiResponseBuilder::success(json!({
            "taskId": task_id,
            "eventId": event_id,
            "state": "pending",
            "retriedBy": claims.user_id,
        }))
        .into_response(),
        Ok(RetryOutcome::TargetNotEligible) => error_response(StatusCode::CONFLICT, "目标 Task 已结束或不可重试"),
        Ok(RetryOutcome::NotFound) => error_response(StatusCode::NOT_FOUND, "Dead wake 不存在"),
        Err(DbError::Validation { message }) => error_response(StatusCode::BAD_REQUEST, message),
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %event_id, %error, "retry Supervisor dead wake failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Wake retry 失败")
        }
    }
}

/// Rebuild and compare one projection at a fixed boundary. Read-only for
/// every durable fact; replacement permission is reported per caller.
///
/// Read authorization boundary: workspace-read scope (tenant + workspace),
/// because reads inherit workspace read permission. The canonical state is
/// derived only from production `session_events` rows and their already
/// sanitized/validated public payloads — the same event fields the read/SSE
/// surface exposes. It never reads raw Effect `request_json`, Decision
/// `subject_json`/`options_json`, provider secrets, or other non-public
/// tables. Replacement is a separate Agent-management-only action.
pub async fn compare_compaction(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(query): Query<CompactionQuery>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let inspector = ProjectionInspector::new(state.db.as_ref().clone(), workspace_id.as_str());
    let comparison = match inspector
        .compare(
            &workspace_id,
            &query.projection_name,
            &query.subject_id,
            query.last_seq,
            query.projection_version,
        )
        .await
    {
        Ok(comparison) => comparison,
        Err(error) => return compaction_error_response(error),
    };
    let admin = is_admin(&state.db, &claims.user_id).await;
    let mut body = match serde_json::to_value(&comparison) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%workspace_id, %error, "serialize projection comparison failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "比较结果序列化失败");
        }
    };
    body["canReplace"] = json!(admin && comparison.matched);
    // Read-only reflects the caller's permission, not whether this boundary
    // happened to match; a mismatch independently disables replacement.
    body["readOnly"] = json!(!admin);
    ApiResponseBuilder::success(body).into_response()
}

/// Explicit CAS replacement. Agent-management permission and the tenant
/// workspace check run before the workspace store is opened; the comparison
/// is recomputed so only a fresh exact match can write.
pub async fn replace_compaction_snapshot(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<CompactionReplaceRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let inspector = ProjectionInspector::new(state.db.as_ref().clone(), workspace_id.as_str());
    let comparison = match inspector
        .compare(
            &workspace_id,
            &payload.projection_name,
            &payload.subject_id,
            payload.last_seq,
            payload.projection_version,
        )
        .await
    {
        Ok(comparison) => comparison,
        Err(error) => return compaction_error_response(error),
    };
    if !comparison.matched {
        return compaction_conflict_response("比较结果不再匹配，保留最新快照", &comparison);
    }
    match inspector
        .replace_snapshot(&comparison, payload.expected_last_seq, payload.expected_state_version)
        .await
    {
        Ok(snapshot) => ApiResponseBuilder::success(snapshot).into_response(),
        Err(DbError::CasConflict { message }) => {
            // Re-run the read-only comparison once so the 409 body describes
            // the authoritative post-conflict state, not the pre-write view.
            let fresh = match inspector
                .compare(
                    &workspace_id,
                    &payload.projection_name,
                    &payload.subject_id,
                    payload.last_seq,
                    payload.projection_version,
                )
                .await
            {
                Ok(fresh) => fresh,
                Err(error) => {
                    tracing::warn!(%workspace_id, %error, "fresh comparison after CAS loss failed");
                    comparison.clone()
                }
            };
            compaction_conflict_response(&message, &fresh)
        }
        Err(error) => compaction_error_response(error),
    }
}

fn compaction_error_response(error: DbError) -> Response {
    match error {
        DbError::Validation { message } => error_response(StatusCode::BAD_REQUEST, message),
        DbError::Conflict { message } | DbError::CasConflict { message } => {
            error_response(StatusCode::CONFLICT, message)
        }
        DbError::NotFound { id } => error_response(StatusCode::NOT_FOUND, format!("投影快照不存在: {id}")),
        _ => {
            tracing::error!(%error, "Harness compaction failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用")
        }
    }
}

fn compaction_conflict_response(message: &str, comparison: &ProjectionComparison) -> Response {
    let body: ApiResponse<Value> = ApiResponse {
        code: i32::from(StatusCode::CONFLICT.as_u16()),
        msg: format!("{message}；保留最新快照，请重新比较"),
        result: serde_json::to_value(comparison).ok(),
    };
    (StatusCode::CONFLICT, Json(body)).into_response()
}

fn dry_run_service(state: &AgentState, workspace_id: &str) -> WorkflowDryRun {
    WorkflowDryRun::new(
        state.db.clone(),
        workspace_id,
        Arc::new(ScriptedDryRunExecutor),
        // The scripted provider is text-only. A Workflow that declares vision
        // or video fails explicitly with capability.unavailable rather than
        // inventing observations.
        resolve_capabilities(false, [Capability::Text]),
    )
}

fn dry_run_error_response(error: DbError) -> Response {
    match error {
        DbError::NotFound { id } if id.starts_with("snapshot:") => {
            error_response(StatusCode::NOT_FOUND, format!("快照不存在: {id}"))
        }
        DbError::NotFound { id } => error_response(StatusCode::NOT_FOUND, format!("Workflow 版本不存在: {id}")),
        DbError::Validation { message } => error_response(StatusCode::BAD_REQUEST, message),
        _ => error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用"),
    }
}

fn workflow_fixed_revision_json(revision: &WorkflowRevision) -> Value {
    json!({
        "workflowId": revision.workflow_id,
        "version": revision.version,
        "enabled": revision.enabled,
        "definition": revision.definition,
        "createdAt": revision.created_at.to_rfc3339(),
        "nextWakeAt": Value::Null,
        "fixedRevision": true,
    })
}

fn workflow_revision_json(revision: &WorkflowRevision) -> Value {
    json!({
        "workflowId": revision.workflow_id,
        "version": revision.version,
        "enabled": revision.enabled,
        "definition": revision.definition,
        "createdAt": revision.created_at.to_rfc3339(),
        "nextWakeAt": if revision.enabled {
            next_wake_at(&revision.definition, Utc::now()).map(|wake| wake.to_rfc3339())
        } else {
            None
        },
    })
}

fn workflow_conflict_response(message: String, current_version: Option<i64>) -> Response {
    let body: ApiResponse<Value> = ApiResponse {
        code: i32::from(StatusCode::CONFLICT.as_u16()),
        msg: vec![
            message,
            current_version
                .map(|v| format!("current_version={v}"))
                .unwrap_or_default(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("; "),
        result: current_version.map(|version| json!({ "currentVersion": version })),
    };
    (StatusCode::CONFLICT, Json(body)).into_response()
}

fn workflow_db_error_response(error: DbError, current_version: Option<i64>) -> Response {
    match error {
        DbError::Validation { message } => error_response(StatusCode::BAD_REQUEST, message),
        DbError::NotFound { id } => error_response(StatusCode::NOT_FOUND, format!("Workflow 不存在: {id}")),
        DbError::Conflict { message } => error_response(StatusCode::CONFLICT, message),
        DbError::CasConflict { message } => workflow_conflict_response(message, current_version),
        _ => error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用"),
    }
}

fn db_error_response(error: DbError) -> Response {
    match error {
        DbError::NotFound { .. } => error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        _ => error_response(StatusCode::SERVICE_UNAVAILABLE, "Harness 存储不可用"),
    }
}

pub(crate) async fn overview_payload(
    db: &Db,
    workspace_id: &str,
    attention_limit: i64,
    attention_offset: i64,
) -> Result<Value, DbError> {
    let workflows = db.list_workflows(workspace_id).await?;
    let recovery = db.list_effect_recovery(workspace_id).await?;
    let attention_total = recovery.len() as i64;
    let attention_items: Vec<Value> = recovery
        .iter()
        .skip(attention_offset as usize)
        .take(attention_limit as usize)
        .map(attention_item)
        .collect();

    let mut coordination = Vec::new();
    for task in db.harness_main_tasks(workspace_id).await? {
        let (children, subagent_total) = db
            .harness_subagents(workspace_id, &task.id, SUBAGENT_PAGE_SIZE, 0)
            .await?;
        coordination.push(main_task_json(&task, &children, subagent_total));
    }

    Ok(json!({
        "attention": {
            "items": attention_items,
            "total": attention_total,
            "limit": attention_limit,
            "offset": attention_offset,
        },
        "workflows": workflows.iter().map(workflow_json).collect::<Vec<_>>(),
        "coordination": coordination,
    }))
}

pub(crate) async fn subagents_payload(
    db: &Db,
    workspace_id: &str,
    task_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Value, DbError> {
    let (rows, total) = db.harness_subagents(workspace_id, task_id, limit, offset).await?;
    Ok(json!({
        "subagents": rows.iter().map(subagent_json).collect::<Vec<_>>(),
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

fn workflow_json(revision: &tinyiothub_core::harness::WorkflowRevision) -> Value {
    let next_run_at = if revision.enabled {
        next_wake_at(&revision.definition, Utc::now()).map(|wake| wake.to_rfc3339())
    } else {
        None
    };
    json!({
        "workflowId": revision.workflow_id,
        "objective": revision.definition.objective,
        "enabled": revision.enabled,
        "version": revision.version,
        "triggerLabel": trigger_label(&revision.definition.trigger),
        "nextRunAt": next_run_at,
        "lastResult": Value::Null,
        "updatedAt": revision.created_at.to_rfc3339(),
    })
}

fn trigger_label(trigger: &WorkflowTrigger) -> String {
    match trigger {
        WorkflowTrigger::Cron { expression, .. } => format!("定时 {expression}"),
        WorkflowTrigger::Watch { .. } => "设备观察".to_string(),
        WorkflowTrigger::Event { .. } => "事件驱动".to_string(),
    }
}

fn attention_item(item: &EffectRecoveryItem) -> Value {
    let state = item.intent.state.as_str();
    let (kind, title) = match state {
        "unknown" => ("effect_unknown", "外部动作结果未知"),
        "dispatching" => ("recovery", "外部动作发送中"),
        "prepared" => ("recovery", "外部动作待发送"),
        "cancelling" => ("recovery", "停止请求处理中"),
        _ => ("recovery", "外部动作需要处理"),
    };
    json!({
        "id": item.intent.effect_id,
        "kind": kind,
        "title": title,
        "detail": format!(
            "Task {} · {} · 状态 {}",
            item.intent.task_id,
            item.intent.tool_name,
            effect_state_label(state)
        ),
        "occurredAt": item.intent.updated_at.to_rfc3339(),
        "taskId": item.intent.task_id,
        "runId": item.intent.run_id,
        "eventId": Value::Null,
        "actionLabel": "查看并处理",
    })
}

fn effect_state_label(state: &str) -> &'static str {
    match state {
        "prepared" => "已准备",
        "dispatching" => "发送中",
        "cancelling" => "停止中",
        "unknown" => "结果未知",
        _ => "需要处理",
    }
}

fn main_task_json(row: &HarnessTaskRow, children: &[HarnessTaskRow], total: i64) -> Value {
    let mut value = task_json(row);
    let object = value.as_object_mut().expect("task json object");
    object.insert(
        "subagents".to_string(),
        Value::Array(children.iter().map(subagent_json).collect()),
    );
    object.insert("subagentTotal".to_string(), json!(total));
    value
}

fn subagent_json(row: &HarnessTaskRow) -> Value {
    task_json(row)
}

fn task_json(row: &HarnessTaskRow) -> Value {
    json!({
        "taskId": row.id,
        "runId": row.run_id,
        "objective": row.objective.clone().unwrap_or_else(|| "Workflow 任务".to_string()),
        "status": row.state,
        "waitReason": wait_reason(&row.state),
        "recentResult": outcome_summary(row.outcome_json.as_deref()),
        "updatedAt": row.updated_at,
    })
}

fn wait_reason(state: &str) -> Option<&'static str> {
    match state {
        "pending" => Some("等待调度"),
        "paused" => Some("已暂停"),
        "cancelling" => Some("正在取消"),
        _ => None,
    }
}

fn outcome_summary(raw: Option<&str>) -> Option<String> {
    let value: Value = serde_json::from_str(raw?).ok()?;
    let outcome = value.get("outcome").unwrap_or(&value);
    if let Some(text) = outcome.as_str() {
        return Some(truncate(text));
    }
    for key in ["summary", "message", "reason", "status"] {
        if let Some(text) = outcome.get(key).and_then(Value::as_str) {
            return Some(truncate(text));
        }
    }
    None
}

fn truncate(text: &str) -> String {
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(500).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use chrono::DateTime;
    use tinyiothub_core::harness::{DeviceFactsSnapshot, ExecutionMode, HarnessEventInput};
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};
    use tower::ServiceExt;

    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
    };

    async fn send(
        app: &axum::Router,
        method: &str,
        token: Option<&str>,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            builder = builder.header("Authorization", auth_header(token));
        }
        if body.is_some() {
            builder = builder.header("Content-Type", "application/json");
        }
        let request = builder
            .body(
                body.map(|value| Body::from(value.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
    }

    async fn get(app: &axum::Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
        send(app, "GET", token, uri, None).await
    }

    async fn workflow_test_app(workspace_id: &str) -> (axum::Router, WorkspaceFixture) {
        let fixture = fixture_workspace(workspace_id).await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", workspace_id).await;
        // Harness handlers only touch `state.db`; point it at the fixture's
        // workspace store while auth keeps using the seeded metadata pool.
        app_state.db = Arc::new(fixture.db.clone());
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        (app, fixture)
    }

    fn workflow_body(objective: &str) -> Value {
        json!({
            "objective": objective,
            "trigger": {"type": "cron", "trigger_id": "t1", "expression": "0 * * * *"},
            "device_selector": {"device_ids": ["pump-1"]},
            "source_selector": {},
            "tool_ids": ["read_device"],
            "skill_refs": ["inspection"],
            "required_capabilities": ["text"],
            "approval_policy": {},
            "evidence_policy": {},
        })
    }

    fn update_body(objective: &str, expected_version: i64, enabled: bool) -> Value {
        json!({
            "expected_version": expected_version,
            "enabled": enabled,
            "definition": workflow_body(objective),
        })
    }

    #[tokio::test]
    async fn overview_rejects_cross_tenant_and_missing_workspace_before_store_open() {
        let (app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", "ws-1").await;
        seed_test_workspace(&pool, "tenant-2", "ws-2").await;
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let token = create_test_token("user-1", "tenant-1");

        let (status, body) = get(&app, Some(&token), "/api/v1/workspaces/ws-2/harness/overview").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

        let (status, body) = get(&app, Some(&token), "/api/v1/workspaces/ws-missing/harness/overview").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

        let (status, _) = get(&app, None, "/api/v1/workspaces/ws-1/harness/overview").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn overview_empty_state_is_well_formed() {
        let fixture = fixture_workspace("ws-harness-empty").await;
        let payload = overview_payload(&fixture.db, "ws-harness-empty", 5, 0)
            .await
            .expect("empty overview");

        assert_eq!(payload["attention"]["total"], 0);
        assert_eq!(payload["attention"]["items"].as_array().unwrap().len(), 0);
        assert_eq!(payload["attention"]["limit"], 5);
        assert_eq!(payload["attention"]["offset"], 0);
        assert_eq!(payload["workflows"].as_array().unwrap().len(), 0);
        assert_eq!(payload["coordination"].as_array().unwrap().len(), 0);

        fixture.close().await;
    }

    #[tokio::test]
    async fn subagents_are_filtered_by_parent_and_missing_parent_is_not_found() {
        let fixture = fixture_workspace("ws-harness-subagents").await;
        let pool = fixture.workspace_pool().await;
        for (id, parent, state) in [
            ("main-1", None, "running"),
            ("child-1", Some("main-1"), "running"),
            ("child-2", Some("main-1"), "completed"),
            ("other-child", Some("other-main"), "running"),
        ] {
            sqlx::query(
                "INSERT INTO agent_tasks
                    (id, run_id, state, state_version, parent_task_id)
                 VALUES (?, ?, ?, 1, ?)",
            )
            .bind(id)
            .bind(format!("run-{id}"))
            .bind(state)
            .bind(parent)
            .execute(&pool)
            .await
            .expect("seed task");
        }

        let payload = subagents_payload(&fixture.db, "ws-harness-subagents", "main-1", 20, 0)
            .await
            .expect("subagent page");
        assert_eq!(payload["total"], 2);
        let ids: Vec<&str> = payload["subagents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["taskId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["child-1", "child-2"]);
        assert_eq!(payload["subagents"][1]["status"], "completed");

        let missing = fixture
            .db
            .harness_subagents("ws-harness-subagents", "missing", 20, 0)
            .await;
        assert!(matches!(missing, Err(DbError::NotFound { .. })));

        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_routes_reject_auth_and_tenant_before_opening_the_store() {
        // Metadata knows ws-1 and cross-tenant ws-2. The handler's store DB only
        // registers ws-1, so a store-first regression for ws-2 lands on
        // ensure_registered_workspace -> 404 instead of the expected 403.
        let fixture = fixture_workspace("ws-1").await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", "ws-1").await;
        seed_test_workspace(&pool, "tenant-2", "ws-2").await;
        app_state.db = Arc::new(fixture.db.clone());
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let token = create_test_token("user-1", "tenant-1");
        let body = workflow_body("跨租户");

        let (status, response) = send(
            &app,
            "POST",
            Some(&token),
            "/api/v1/workspaces/ws-2/harness/workflows",
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{response}");

        // The rejected request must not create workflows or workspace artifacts.
        let (status, listed) = get(&app, Some(&token), "/api/v1/workspaces/ws-1/harness/workflows").await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed["result"]["workflows"].as_array().unwrap().len(), 0);
        assert!(
            !fixture.root.join("data/agents/ws-2/harness/harness.db").exists(),
            "cross-tenant request must not open or create the ws-2 store"
        );

        let (status, response) = send(
            &app,
            "POST",
            Some(&token),
            "/api/v1/workspaces/ws-missing/harness/workflows",
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{response}");

        let (status, _) = send(
            &app,
            "POST",
            None,
            "/api/v1/workspaces/ws-1/harness/workflows",
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_create_edit_conflict_and_enable_disable_follow_r30() {
        let (app, fixture) = workflow_test_app("ws-workflow-http").await;
        let token = create_test_token("admin-user-001", "tenant-1");
        let base = "/api/v1/workspaces/ws-workflow-http/harness/workflows";

        let (status, created) = send(&app, "POST", Some(&token), base, Some(workflow_body("初始目标"))).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let workflow_id = created["result"]["workflowId"]
            .as_str()
            .expect("workflow id")
            .to_string();
        assert_eq!(created["result"]["enabled"], false);
        assert_eq!(created["result"]["version"], 1);

        let item = format!("{base}/{}", workflow_id.replace(':', "%3A"));
        let (status, fetched) = get(&app, Some(&token), &item).await;
        assert_eq!(status, StatusCode::OK, "{fetched}");
        assert_eq!(fetched["result"]["enabled"], false);

        // Disabled content edit stays disabled.
        let (status, edited) = send(
            &app,
            "PUT",
            Some(&token),
            &item,
            Some(update_body("disabled edit", 1, false)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{edited}");
        assert_eq!(edited["result"]["enabled"], false);
        assert_eq!(edited["result"]["version"], 2);

        // Stale submit returns 409 with the authoritative current version.
        let (status, conflict) = send(
            &app,
            "PUT",
            Some(&token),
            &item,
            Some(update_body("stale edit", 1, false)),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
        assert_eq!(conflict["code"], 409);
        assert_eq!(conflict["result"]["currentVersion"], 2);

        let enabled_uri = format!("{item}/enabled");
        let (status, enabled) = send(&app, "PUT", Some(&token), &enabled_uri, Some(json!({"enabled": true}))).await;
        assert_eq!(status, StatusCode::OK, "{enabled}");
        assert_eq!(enabled["result"]["enabled"], true);

        // Enabled content edit stays enabled; the supplied echo is not trusted
        // over the stored pointer.
        let (status, edited) = send(
            &app,
            "PUT",
            Some(&token),
            &item,
            Some(update_body("enabled edit", 2, true)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{edited}");
        assert_eq!(edited["result"]["enabled"], true);
        assert_eq!(edited["result"]["version"], 3);
        assert_eq!(edited["result"]["definition"]["objective"], "enabled edit");

        let (status, disabled) = send(&app, "PUT", Some(&token), &enabled_uri, Some(json!({"enabled": false}))).await;
        assert_eq!(status, StatusCode::OK, "{disabled}");
        assert_eq!(disabled["result"]["enabled"], false);
        assert_eq!(disabled["result"]["version"], 3);

        let (status, listed) = get(&app, Some(&token), base).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed["result"]["workflows"].as_array().unwrap().len(), 1);
        assert_eq!(listed["result"]["workflows"][0]["version"], 3);
        assert_eq!(listed["result"]["workflows"][0]["enabled"], false);

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_create_ignores_supplied_enabled_and_missing_get_is_not_found() {
        let (app, fixture) = workflow_test_app("ws-workflow-create").await;
        let token = create_test_token("admin-user-001", "tenant-1");
        let base = "/api/v1/workspaces/ws-workflow-create/harness/workflows";
        let mut body = workflow_body("强制启用尝试");
        body["enabled"] = json!(true);

        let (status, created) = send(&app, "POST", Some(&token), base, Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["result"]["enabled"], false);

        let (status, missing) = get(&app, Some(&token), &format!("{base}/missing-workflow")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");

        drop(app);
        fixture.close().await;
    }

    async fn seed_template_thing(fixture: &WorkspaceFixture, id: &str) {
        let workspace_id = fixture.workspace_id.as_str();
        let driver = format!("driver-{id}");
        sqlx::query("INSERT INTO things (id, name, thing_type, workspace_id) VALUES (?, ?, 'device', ?)")
            .bind(id)
            .bind(id)
            .bind(workspace_id)
            .execute(fixture.db.pool())
            .await
            .expect("seed thing");
        sqlx::query(
            "INSERT INTO driver_declarations
                (workspace_id, driver_name, driver_version, manifest_version, manifest_json,
                 manifest_hash, status, installed_at)
             VALUES (?, ?, '1.0.0', '1.0', ?, 'hash', 'active', 'now')",
        )
        .bind(workspace_id)
        .bind(&driver)
        .bind(json!({"sources":[{"id":"telemetry","kind":"poll","dataKind":"values"}],"tools":[]}).to_string())
        .execute(fixture.db.pool())
        .await
        .expect("seed driver declaration");
        sqlx::query(
            "INSERT INTO device_declarations
                (workspace_id, thing_id, driver_name, driver_version, declaration_json,
                 declaration_hash, status, updated_at)
             VALUES (?, ?, ?, '1.0.0', '{}', 'hash', 'active', 'now')",
        )
        .bind(workspace_id)
        .bind(id)
        .bind(&driver)
        .execute(fixture.db.pool())
        .await
        .expect("seed device declaration");
    }

    #[tokio::test]
    async fn workflow_write_routes_require_agent_admin() {
        let workspace_id = "ws-workflow-admin";
        let (app, fixture) = workflow_test_app(workspace_id).await;
        let base = format!("/api/v1/workspaces/{workspace_id}/harness");
        let member = create_test_token("user-2", "tenant-1");
        let admin = create_test_token("admin-user-001", "tenant-1");

        let instantiate = json!({
            "device_selector": {"device_ids": ["pump-1"]},
            "source_selector": {"sources": ["telemetry"]},
            "approval_policy": {},
        });
        for (method, uri, body) in [
            ("POST", format!("{base}/workflows"), Some(workflow_body("成员尝试"))),
            (
                "POST",
                format!("{base}/workflow-templates/inspection/instantiate"),
                Some(instantiate.clone()),
            ),
        ] {
            let (status, response) = send(&app, method, Some(&member), &uri, body).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "member must not write: {response}");
        }
        let (_, listed) = get(&app, Some(&admin), &format!("{base}/workflows")).await;
        assert_eq!(listed["result"]["workflows"].as_array().unwrap().len(), 0);

        let (status, created) = send(
            &app,
            "POST",
            Some(&admin),
            &format!("{base}/workflows"),
            Some(workflow_body("管理员创建")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let workflow_id = created["result"]["workflowId"].as_str().expect("workflow id");
        let item = format!("{base}/workflows/{}", workflow_id.replace(':', "%3A"));

        let (status, response) = send(
            &app,
            "PUT",
            Some(&member),
            &item,
            Some(update_body("成员编辑", 1, false)),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
        let (status, response) = send(
            &app,
            "PUT",
            Some(&member),
            &format!("{item}/enabled"),
            Some(json!({"enabled": true})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
        let (status, enabled) = send(
            &app,
            "PUT",
            Some(&admin),
            &format!("{item}/enabled"),
            Some(json!({"enabled": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{enabled}");
        assert_eq!(enabled["result"]["enabled"], true);

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_dry_run_is_admin_only_and_reads_only_a_fixed_revision_snapshot() {
        let workspace_id = "ws-dry-run-http";
        let (app, fixture) = workflow_test_app(workspace_id).await;
        let admin = create_test_token("admin-user-001", "tenant-1");
        let member = create_test_token("user-2", "tenant-1");
        let base = format!("/api/v1/workspaces/{workspace_id}/harness");

        let (status, created) = send(
            &app,
            "POST",
            Some(&admin),
            &format!("{base}/workflows"),
            Some(workflow_body("dry-run 巡检")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let workflow_id = created["result"]["workflowId"]
            .as_str()
            .expect("workflow id")
            .to_string();
        let version = created["result"]["version"].as_i64().expect("version");

        // F6: the dry-run return path must be able to pin this revision even
        // after a newer current revision exists.
        let item = format!("{base}/workflows/{workflow_id}");
        let (status, edited) = send(
            &app,
            "PUT",
            Some(&admin),
            &item,
            Some(update_body("dry-run 巡检 v2", version, false)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{edited}");
        assert_eq!(edited["result"]["version"], 2);
        let fixed_uri = format!("{base}/workflows/{workflow_id}/versions/{version}");
        let (status, fixed) = get(&app, Some(&member), &fixed_uri).await;
        assert_eq!(status, StatusCode::OK, "{fixed}");
        assert_eq!(fixed["result"]["version"], version);
        assert_eq!(fixed["result"]["definition"]["objective"], "dry-run 巡检");
        assert_eq!(fixed["result"]["fixedRevision"], true);
        assert!(fixed["result"]["nextWakeAt"].is_null());

        let observed_at = DateTime::parse_from_rfc3339("2026-11-02T08:30:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let snapshot_event = fixture
            .db
            .append_session_event(&HarnessEventInput {
                event_id: "production-snapshot-http".to_string(),
                workspace_id: workspace_id.to_string(),
                session_id: format!("supervisor:{workspace_id}"),
                execution_mode: ExecutionMode::Production,
                run_id: None,
                task_id: None,
                event_type: "run.started".to_string(),
                occurred_at: observed_at,
                cause_id: None,
                schema_v: 1,
                payload_digest: "digest-production-snapshot-http".to_string(),
                payload_json: json!({
                    "deviceFacts": DeviceFactsSnapshot {
                        observed_at,
                        declaration_hash: "declaration-http".to_string(),
                        driver_version: "driver-http".to_string(),
                        facts: json!({"temperature": 20}),
                    }
                }),
            })
            .await
            .expect("seed production snapshot");

        let snapshots_uri = format!("{base}/workflows/{workflow_id}/versions/{version}/snapshots");
        let (status, snapshots) = get(&app, Some(&member), &snapshots_uri).await;
        assert_eq!(status, StatusCode::OK, "{snapshots}");
        assert_eq!(snapshots["result"]["snapshots"][0]["seq"], snapshot_event.seq);
        assert_eq!(
            snapshots["result"]["snapshots"][0]["observedAt"],
            "2026-11-02T08:30:00Z"
        );

        let dry_run_uri = format!("{base}/workflows/{workflow_id}/versions/{version}/dry-run");
        let (status, forbidden) = send(
            &app,
            "POST",
            Some(&member),
            &dry_run_uri,
            Some(json!({"snapshot_seq": snapshot_event.seq})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{forbidden}");

        let (status, missing) = send(
            &app,
            "POST",
            Some(&admin),
            &dry_run_uri,
            Some(json!({"snapshot_seq": 999999})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
        assert!(missing["msg"].as_str().unwrap_or_default().contains("快照不存在"));

        let (status, started) = send(
            &app,
            "POST",
            Some(&admin),
            &dry_run_uri,
            Some(json!({"snapshot_seq": snapshot_event.seq})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let task_id = started["result"]["taskId"].as_str().expect("task id").to_string();
        let run_id = started["result"]["runId"].as_str().expect("run id").to_string();

        let simulation_uri = format!("{base}/simulations/{task_id}");
        let (status, simulation) = get(&app, Some(&member), &simulation_uri).await;
        assert_eq!(status, StatusCode::OK, "{simulation}");
        assert_eq!(simulation["result"]["executionMode"], "simulation");
        assert_eq!(simulation["result"]["readOnly"], true);
        assert_eq!(simulation["result"]["status"], "completed");
        assert_eq!(simulation["result"]["runId"], run_id);
        assert_eq!(simulation["result"]["snapshot"]["seq"], snapshot_event.seq);
        assert_eq!(
            simulation["result"]["outcome"]["evidenceRefs"],
            json!([format!("snapshot:{}", snapshot_event.seq)])
        );
        assert_eq!(
            simulation["result"]["outcome"]["recommendedActions"],
            json!(["read_device"])
        );
        assert!(
            simulation["result"]["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .any(|value| value == &format!("snapshot:{}", snapshot_event.seq))
        );

        // No production approval/execution entry is exposed on the simulation.
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/workspaces/{workspace_id}/harness/simulations/{task_id}/approve"
            ))
            .header("Authorization", auth_header(&admin))
            .header("Content-Type", "application/json")
            .body(Body::from("{}"))
            .expect("approval request");
        let response = app.clone().oneshot(request).await.expect("approval response");
        assert!(!response.status().is_success());

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn supervisor_health_http_is_member_readable_but_retry_requires_admin_before_store_open() {
        let workspace_id = "ws-supervisor-http";
        let fixture = fixture_workspace(workspace_id).await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", workspace_id).await;
        seed_test_workspace(&pool, "tenant-1", "ws-supervisor-member-nostore").await;
        seed_test_workspace(&pool, "tenant-2", "ws-supervisor-other").await;
        app_state.db = Arc::new(fixture.db.clone());
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let admin = create_test_token("admin-user-001", "tenant-1");
        let member = create_test_token("user-2", "tenant-1");
        let base = format!("/api/v1/workspaces/{workspace_id}/harness/supervisor");

        let (status, body) = get(&app, Some(&member), &base).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["health"]["status"], "idle");
        assert_eq!(body["result"]["canRetry"], false);
        assert_eq!(body["result"]["readOnly"], true);
        assert_eq!(body["result"]["deadWakes"].as_array().unwrap().len(), 0);

        let retry_uri = format!("{base}/dead-wakes/task-original/event-original/retry");
        let (status, body) = send(&app, "POST", Some(&member), &retry_uri, Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

        // A member is rejected on a same-tenant workspace that has no store;
        // the permission gate must run before any store open.
        let member_uri =
            "/api/v1/workspaces/ws-supervisor-member-nostore/harness/supervisor/dead-wakes/task-x/event-x/retry";
        let (status, body) = send(&app, "POST", Some(&member), member_uri, Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(
            !fixture
                .root
                .join("data/agents/ws-supervisor-member-nostore/harness/harness.db")
                .exists(),
            "a member retry must be rejected before the workspace store is opened"
        );

        let (status, body) = get(&app, Some(&admin), &base).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["canRetry"], true);
        assert_eq!(body["result"]["readOnly"], false);

        let (status, body) = send(&app, "POST", Some(&admin), &retry_uri, Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

        // Cross-tenant and missing Workspaces are rejected before their
        // harness store is opened or created.
        let (status, body) = get(
            &app,
            Some(&admin),
            "/api/v1/workspaces/ws-supervisor-other/harness/supervisor",
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let (status, body) = send(
            &app,
            "POST",
            Some(&admin),
            "/api/v1/workspaces/ws-supervisor-other/harness/supervisor/dead-wakes/task/event/retry",
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(
            !fixture
                .root
                .join("data/agents/ws-supervisor-other/harness/harness.db")
                .exists(),
            "cross-tenant requests must not open the workspace store"
        );

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn workflow_template_routes_list_and_instantiate_enabled_with_permission_checks() {
        let workspace_id = "ws-template-http";
        let fixture = fixture_workspace(workspace_id).await;
        seed_template_thing(&fixture, "pump-1").await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", workspace_id).await;
        seed_test_workspace(&pool, "tenant-2", "ws-template-other").await;
        app_state.db = Arc::new(fixture.db.clone());
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let token = create_test_token("admin-user-001", "tenant-1");

        let base = format!("/api/v1/workspaces/{workspace_id}/harness");
        let (status, _) = get(&app, None, &format!("{base}/workflow-templates")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, forbidden) = get(
            &app,
            Some(&token),
            "/api/v1/workspaces/ws-template-other/harness/workflow-templates",
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{forbidden}");

        let (status, listed) = get(&app, Some(&token), &format!("{base}/workflow-templates")).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        let templates = listed["result"]["templates"].as_array().expect("templates");
        assert_eq!(templates.len(), 3);
        assert_eq!(templates[0]["id"], "inspection");
        assert!(templates[0]["version"].as_i64().unwrap_or_default() >= 1);

        let instantiate = |device: &str| {
            json!({
                "device_selector": {"device_ids": [device]},
                "source_selector": {"sources": ["telemetry"]},
                "approval_policy": {},
            })
        };
        let (status, failed) = send(
            &app,
            "POST",
            Some(&token),
            &format!("{base}/workflow-templates/inspection/instantiate"),
            Some(instantiate("missing-device")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{failed}");
        let (_, workflows) = get(&app, Some(&token), &format!("{base}/workflows")).await;
        assert_eq!(workflows["result"]["workflows"].as_array().unwrap().len(), 0);

        let (status, created) = send(
            &app,
            "POST",
            Some(&token),
            &format!("{base}/workflow-templates/inspection/instantiate"),
            Some(instantiate("pump-1")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["result"]["enabled"], true, "DR4=B direct enable");
        assert_eq!(created["result"]["version"], 1);
        assert!(created["result"]["nextWakeAt"].is_string());
        let (_, workflows) = get(&app, Some(&token), &format!("{base}/workflows")).await;
        assert_eq!(workflows["result"]["workflows"].as_array().unwrap().len(), 1);
        assert_eq!(workflows["result"]["workflows"][0]["enabled"], true);

        drop(app);
        fixture.close().await;
    }

    #[tokio::test]
    async fn compaction_http_compare_member_readable_replace_admin_and_cas_conflict() {
        let workspace_id = "ws-compaction-http";
        let subject = format!("supervisor:{workspace_id}");
        let fixture = fixture_workspace(workspace_id).await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", workspace_id).await;
        seed_test_workspace(&pool, "tenant-1", "ws-compaction-member-nostore").await;
        seed_test_workspace(&pool, "tenant-2", "ws-compaction-other").await;
        app_state.db = Arc::new(fixture.db.clone());

        for (event_id, text) in [("compact-http-1", "one"), ("compact-http-2", "two")] {
            fixture
                .db
                .append_session_event(&HarnessEventInput {
                    event_id: event_id.to_string(),
                    workspace_id: workspace_id.to_string(),
                    session_id: subject.clone(),
                    execution_mode: ExecutionMode::Production,
                    run_id: Some("run-compact-http".to_string()),
                    task_id: Some("task-compact-http".to_string()),
                    event_type: "step.text".to_string(),
                    occurred_at: DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
                        .expect("timestamp")
                        .with_timezone(&Utc),
                    cause_id: None,
                    schema_v: 1,
                    payload_digest: format!("digest-{event_id}"),
                    payload_json: json!({ "text": text }),
                })
                .await
                .expect("append compaction event");
        }
        let last_seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE session_id = ?")
            .bind(&subject)
            .fetch_one(&fixture.workspace_pool().await)
            .await
            .expect("boundary seq");
        let inspector = crate::domains::agent::workflows::ProjectionInspector::new(fixture.db.clone(), workspace_id);
        let missing = inspector
            .compare(workspace_id, "supervisor", &subject, last_seq, 1)
            .await
            .expect("compare without snapshot");
        sqlx::query(
            "INSERT INTO projection_snapshots
                (projection_name, subject_id, last_seq, projection_version, state_version, state_json, updated_at)
             VALUES ('supervisor', ?, ?, 1, 3, ?, ?)",
        )
        .bind(&subject)
        .bind(last_seq)
        .bind(missing.canonical_state.to_string())
        .bind(
            DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
                .expect("timestamp")
                .with_timezone(&Utc)
                .to_rfc3339(),
        )
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("seed snapshot");

        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let admin = create_test_token("admin-user-001", "tenant-1");
        let member = create_test_token("user-2", "tenant-1");
        let encoded_subject = subject.replace(':', "%3A");
        let query_uri = format!(
            "/api/v1/workspaces/{workspace_id}/harness/compaction?projection_name=supervisor&subject_id={encoded_subject}&last_seq={last_seq}&projection_version=1"
        );
        let replace_uri = format!("/api/v1/workspaces/{workspace_id}/harness/compaction/replace");
        let replace_body = json!({
            "projection_name": "supervisor",
            "subject_id": subject,
            "last_seq": last_seq,
            "projection_version": 1,
            "expected_last_seq": last_seq,
            "expected_state_version": 3,
        });

        let (status, body) = get(&app, Some(&member), &query_uri).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["status"], "matched");
        assert_eq!(body["result"]["matched"], true);
        assert_eq!(body["result"]["canReplace"], false);
        assert_eq!(body["result"]["readOnly"], true);
        assert!(
            body["result"]["canonicalHash"]
                .as_str()
                .is_some_and(|hash| !hash.is_empty())
        );
        assert!(body["result"]["snapshotCanonicalHash"].as_str().is_some());
        assert_eq!(body["result"]["boundaryLastSeq"], last_seq);
        assert_eq!(body["result"]["evidenceRefs"].as_array().unwrap().len(), 2);

        let (status, body) = send(&app, "POST", Some(&member), &replace_uri, Some(replace_body.clone())).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

        // A member is rejected before a same-tenant workspace store is opened.
        let no_store_uri = "/api/v1/workspaces/ws-compaction-member-nostore/harness/compaction/replace";
        let (status, body) = send(&app, "POST", Some(&member), no_store_uri, Some(replace_body.clone())).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(
            !fixture
                .root
                .join("data/agents/ws-compaction-member-nostore/harness/harness.db")
                .exists(),
            "a member replace must be rejected before the workspace store is opened"
        );

        let (status, body) = get(&app, Some(&admin), &query_uri).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["canReplace"], true);
        assert_eq!(body["result"]["readOnly"], false);

        let (status, body) = send(&app, "POST", Some(&admin), &replace_uri, Some(replace_body.clone())).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["stateVersion"], 4);
        assert_eq!(body["result"]["lastSeq"], last_seq);

        // Replaying the old comparison must fail CAS and keep the replacement;
        // the 409 body carries a fresh read-only comparison of the new state.
        let (status, body) = send(&app, "POST", Some(&admin), &replace_uri, Some(replace_body)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["result"]["status"], "matched");
        assert_eq!(body["result"]["snapshotStateVersion"], 4);
        assert_eq!(body["result"]["boundaryLastSeq"], last_seq);
        let (state_version, state_json): (i64, String) = sqlx::query_as(
            "SELECT state_version, state_json FROM projection_snapshots
             WHERE projection_name = 'supervisor' AND subject_id = ?",
        )
        .bind(&subject)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("snapshot after CAS conflict");
        assert_eq!(state_version, 4);
        assert_eq!(state_json, missing.canonical_state.to_string());

        // Cross-tenant requests fail before opening or creating another store.
        let other_uri = "/api/v1/workspaces/ws-compaction-other/harness/compaction?projection_name=supervisor&subject_id=supervisor%3Aws-compaction-other&last_seq=0&projection_version=1";
        let (status, body) = get(&app, Some(&admin), other_uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let other_replace_uri = "/api/v1/workspaces/ws-compaction-other/harness/compaction/replace";
        let (status, body) = send(
            &app,
            "POST",
            Some(&admin),
            other_replace_uri,
            Some(json!({
                "projection_name": "supervisor",
                "subject_id": "supervisor:ws-compaction-other",
                "last_seq": 0,
                "projection_version": 1,
                "expected_last_seq": 0,
                "expected_state_version": 1,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(
            !fixture
                .root
                .join("data/agents/ws-compaction-other/harness/harness.db")
                .exists(),
            "cross-tenant requests must not open the workspace store"
        );

        drop(app);
        fixture.close().await;
    }
}
