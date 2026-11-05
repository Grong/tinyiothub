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

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_core::harness::{WorkflowDefinition, WorkflowRevision, WorkflowTrigger};
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;
use tinyiothub_storage::runtime_task::HarnessTaskRow;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::runtime::tasks::error_response;
use crate::domains::agent::workflows::WorkflowService;

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
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
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
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
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
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
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

fn workflow_revision_json(revision: &WorkflowRevision) -> Value {
    json!({
        "workflowId": revision.workflow_id,
        "version": revision.version,
        "enabled": revision.enabled,
        "definition": revision.definition,
        "createdAt": revision.created_at.to_rfc3339(),
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
    json!({
        "workflowId": revision.workflow_id,
        "objective": revision.definition.objective,
        "enabled": revision.enabled,
        "version": revision.version,
        "triggerLabel": trigger_label(&revision.definition.trigger),
        "nextRunAt": Value::Null,
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
        let token = create_test_token("user-1", "tenant-1");
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
        let token = create_test_token("user-1", "tenant-1");
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
}
