//! Harness overview, Supervisor, compaction, and subagent API.
//!
//! Routes (nested under `/workspaces` by the composition layer):
//!   GET  /{workspace_id}/harness/overview
//!   GET  /{workspace_id}/harness/main/{task_id}/subagents
//!   GET  /{workspace_id}/harness/supervisor
//!   POST /{workspace_id}/harness/supervisor/dead-wakes/{task_id}/{event_id}/retry
//!   GET  /{workspace_id}/harness/compaction
//!   POST /{workspace_id}/harness/compaction/replace
//!
//! Every handler verifies tenant/workspace access before opening the workspace
//! Harness store. Task controls stay on the shared Runtime handlers.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;
use tinyiothub_storage::harness_workflow::HarnessTaskRow;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::host::handler::is_admin;
use crate::domains::agent::runtime::tasks::error_response;
use crate::domains::agent::workflows::{ProjectionComparison, ProjectionInspector, RetryOutcome, SupervisorHealth};

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
        "workflows": [],
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
    use chrono::{DateTime, Utc};
    use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
    use tinyiothub_storage::test_helpers::fixture_workspace;
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
    async fn retired_workflow_routes_are_not_mounted() {
        let (app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", "ws-1").await;
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let token = create_test_token("admin-user-001", "tenant-1");
        for uri in [
            "/api/v1/workspaces/ws-1/harness/workflows",
            "/api/v1/workspaces/ws-1/harness/workflows/wf-1",
            "/api/v1/workspaces/ws-1/harness/workflow-templates",
            "/api/v1/workspaces/ws-1/harness/workflows/wf-1/versions/1/dry-run",
            "/api/v1/workspaces/ws-1/harness/simulations/task-1",
        ] {
            let request = Request::builder()
                .method("GET")
                .uri(uri)
                .header("Authorization", auth_header(&token))
                .body(Body::empty())
                .expect("request");
            let status = app.clone().oneshot(request).await.expect("response").status();
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri} must be retired");
        }
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
    async fn harness_write_routes_reject_non_admin_or_cross_tenant_before_opening_the_workspace_store() {
        let workspace_id = "ws-harness-admin-order";
        let cross_tenant_id = "ws-harness-admin-cross-tenant";
        let fixture = fixture_workspace(workspace_id).await;
        let (mut app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", workspace_id).await;
        seed_test_workspace(&pool, "tenant-2", cross_tenant_id).await;
        // Fresh store cache over the same platform pool: the workspace stays
        // registered, but any premature open would recreate the directory we
        // remove below and fail the assertion.
        app_state.db = Arc::new(
            tinyiothub_storage::Db::new(fixture.db.pool().clone())
                .with_workspace_store_root(fixture.root.join("data/agents")),
        );
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let harness_dir = fixture.root.join("data/agents").join(workspace_id).join("harness");
        tokio::fs::remove_dir_all(&harness_dir)
            .await
            .expect("remove store so a premature open is observable");
        let member = create_test_token("user-2", "tenant-1");
        let base = format!("/api/v1/workspaces/{workspace_id}/harness");
        let writes = [
            (
                "POST",
                format!("{base}/supervisor/dead-wakes/task%3A1/event%3A1/retry"),
                None,
            ),
            (
                "POST",
                format!("{base}/compaction/replace"),
                Some(json!({
                    "projection_name": "task",
                    "subject_id": "task:1",
                    "last_seq": 1,
                    "projection_version": 1,
                    "expected_last_seq": 1,
                    "expected_state_version": 1,
                })),
            ),
        ];
        for (method, uri, body) in &writes {
            let (status, response) = send(&app, method, Some(&member), uri, body.clone()).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri} must be admin-only: {response}");
            assert!(
                !harness_dir.exists(),
                "non-admin write {uri} must be rejected before the workspace DB opens"
            );
        }
        let cross_tenant = create_test_token("admin-user-002", "tenant-2");
        let cross_tenant_dir = fixture.root.join("data/agents").join(cross_tenant_id);
        for (method, uri, body) in &writes {
            let cross_uri = uri.replacen(workspace_id, cross_tenant_id, 1);
            let (status, response) = send(&app, method, Some(&cross_tenant), &cross_uri, body.clone()).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{cross_uri} must reject a cross-tenant request: {response}"
            );
            assert!(
                !cross_tenant_dir.exists(),
                "cross-tenant write {cross_uri} must be rejected before any workspace DB opens"
            );
        }
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
