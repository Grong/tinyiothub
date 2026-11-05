//! Read-only Harness overview API.
//!
//! Routes (nested under `/workspaces` by the composition layer):
//!   GET /{workspace_id}/harness/overview
//!   GET /{workspace_id}/harness/main/{task_id}/subagents
//!
//! Both handlers verify tenant/workspace access before opening the workspace
//! Harness store, then reuse the existing `Db` read queries. There is no write
//! path here; Task controls stay on the shared Runtime handlers.

use axum::{
    Extension, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_core::harness::WorkflowTrigger;
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;
use tinyiothub_storage::runtime_task::HarnessTaskRow;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::runtime::tasks::error_response;

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
    use axum::body::Body;
    use axum::http::Request;
    use tinyiothub_storage::test_helpers::fixture_workspace;
    use tower::ServiceExt;

    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
    };

    async fn get(app: &axum::Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder().method("GET").uri(uri);
        if let Some(token) = token {
            builder = builder.header("Authorization", auth_header(token));
        }
        let request = builder.body(Body::empty()).expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
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
}
