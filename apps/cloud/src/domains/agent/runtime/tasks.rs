use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_agent::runtime::thing_agent::{Priority, TriggerSource, WakeSignal};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState};
use tinyiothub_web::{api_response::ApiResponse, response::ApiResponseBuilder, security::Claims};

use super::DbTaskStore;
use crate::domains::agent::AgentState;
use crate::verify_workspace_access;

pub(crate) fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let code = i32::from(status.as_u16());
    (status, ApiResponseBuilder::error_with_code::<Value>(code, message)).into_response()
}

pub async fn create_persistent(
    state: &AgentState,
    claims: &Claims,
    workspace_id: &str,
    headers: &HeaderMap,
    text: &str,
) -> Response {
    let Some(idempotency_key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return error_response(StatusCode::BAD_REQUEST, "Idempotency-Key 不能为空");
    };
    if text.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "指令内容不能为空");
    }

    let now = Utc::now();
    let task_id = uuid::Uuid::new_v4().to_string();
    let event_id = uuid::Uuid::new_v4().to_string();
    let signal = WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority: Priority::High,
        source: TriggerSource::UserDirective {
            user_id: claims.user_id.clone(),
            text: text.to_string(),
            session_key: None,
            source: None,
            problem_key: None,
        },
        dedup_key: None,
    };
    let signal_json = match serde_json::to_string(&signal) {
        Ok(json) => json,
        Err(error) => {
            tracing::error!(%workspace_id, %error, "serialize task wake failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 存储不可用");
        }
    };
    let task = AgentTask {
        id: task_id,
        workspace_id: workspace_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        objective: text.to_string(),
        session_key: None,
        created_by: claims.user_id.clone(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: task.id.clone(),
        event_id,
        signal_json,
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    };
    let store = DbTaskStore::new(state.db.clone()).with_recorder(state.runtime_recorder.clone());
    match store.create_task_with_wake(&task, &wake).await {
        Ok(outcome) => ApiResponseBuilder::success(json!({
            "taskId": outcome.task.id,
            "status": "accepted",
            "deduplicated": outcome.deduplicated,
        }))
        .into_response(),
        Err(TaskStoreError::ObjectiveConflict) => {
            error_response(StatusCode::CONFLICT, "Idempotency-Key 已用于不同指令")
        }
        Err(TaskStoreError::Validation(message)) => error_response(StatusCode::BAD_REQUEST, message),
        Err(TaskStoreError::ConcurrentUpdate) => error_response(StatusCode::CONFLICT, "Task 已并发更新"),
        Err(TaskStoreError::Unavailable(error)) => {
            tracing::error!(%workspace_id, %error, "persistent task create failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 存储不可用")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListTasksParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn list_tasks(
    State(state): State<AgentState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<ListTasksParams>,
) -> Json<ApiResponse<Value>> {
    verify_workspace_access!(state, claims, workspace_id);
    let limit = i64::from(params.limit.unwrap_or(50).clamp(1, 200));
    let offset = i64::from(params.offset.unwrap_or(0));
    let store = DbTaskStore::new(state.db.clone());
    match store.list_tasks(&workspace_id, limit, offset).await {
        Ok(tasks) => ApiResponseBuilder::success(json!({
            "tasks": tasks,
            "limit": limit,
            "offset": offset,
        })),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "list persistent tasks failed");
            ApiResponseBuilder::error_with_code(503, "Task 存储不可用")
        }
    }
}

pub async fn get_task(
    State(state): State<AgentState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return error_response(StatusCode::FORBIDDEN, "无权访问此工作空间"),
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "工作空间不存在"),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check Task workspace failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败");
        }
    }
    super::projection::response(&state, &workspace_id, &task_id).await
}

#[derive(Debug, Deserialize)]
pub struct ListEventsParams {
    pub limit: Option<u32>,
    pub before: Option<i64>,
}

/// Bounded initial-timeline page for a Task. Newest page by default; `before`
/// is an exclusive cursor for "load earlier". Never replays the whole ledger.
pub async fn list_events(
    State(state): State<AgentState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
    Query(params): Query<ListEventsParams>,
) -> Response {
    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return error_response(StatusCode::FORBIDDEN, "无权访问此工作空间"),
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "工作空间不存在"),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check Task events workspace failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败");
        }
    }
    if let Err(response) = super::projection::load(&state, &workspace_id, &task_id).await {
        return response;
    }
    let limit = i64::from(params.limit.unwrap_or(100).clamp(1, 200));
    match state
        .db
        .runtime_events_page(&workspace_id, &task_id, params.before, limit)
        .await
    {
        Ok(page) => {
            let oldest_cursor = page.events.first().map(|event| event.cursor);
            ApiResponseBuilder::success(json!({
                "events": page.events,
                "oldestCursor": oldest_cursor,
                "hasMore": page.has_more,
            }))
            .into_response()
        }
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "load Task events failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 事件不可用")
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_storage::Db;
    use tinyiothub_storage::runtime_event::RuntimeEventInput;
    use tower::ServiceExt;

    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
    };

    async fn seed_task(db: &Db, workspace_id: &str, task_id: &str) {
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: task_id.into(),
                workspace_id: workspace_id.into(),
                idempotency_key: format!("{task_id}-key"),
                objective: "timeline".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: task_id.into(),
                event_id: format!("{task_id}-wake"),
                signal_json: json!({"workspace_id": workspace_id}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("task");
    }

    /// A Task with no persisted runtime events (created outside the recorder
    /// path) exercises the empty-timeline response.
    async fn seed_bare_task(db: &Db, workspace_id: &str, task_id: &str) {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, session_key, created_by,
                  status, current_run_id, state_version, created_at, updated_at, ended_at)
             VALUES (?, ?, ?, 'bare', NULL, 'user-1', 'active', NULL, 0, ?, ?, NULL)",
        )
        .bind(task_id)
        .bind(workspace_id)
        .bind(format!("{task_id}-key"))
        .bind(&now)
        .bind(&now)
        .execute(db.pool())
        .await
        .expect("bare task");
    }

    fn event(workspace_id: &str, task_id: &str, event_id: &str) -> RuntimeEventInput {
        RuntimeEventInput {
            event_id: event_id.into(),
            event_type: "task.updated".into(),
            occurred_at: Utc::now(),
            workspace_id: workspace_id.into(),
            task_id: Some(task_id.into()),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: json!({"stateVersion": 0}),
        }
    }

    async fn fixture() -> (axum::Router, Db, String, String) {
        let (app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", "ws-1").await;
        seed_test_workspace(&pool, "tenant-2", "ws-2").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        seed_task(&db, "ws-1", "task-2").await;
        seed_task(&db, "ws-2", "task-other").await;
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        (
            app,
            db,
            create_test_token("user-1", "tenant-1"),
            create_test_token("user-2", "tenant-2"),
        )
    }

    async fn record(db: &Db, workspace_id: &str, task_id: &str, event_id: &str) {
        db.append_runtime_event(event(workspace_id, task_id, event_id))
            .await
            .expect("record event");
    }

    async fn get(app: &axum::Router, token: &str, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .header("Authorization", auth_header(token))
            .body(Body::empty())
            .expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
    }

    #[tokio::test]
    async fn timeline_latest_page_is_bounded_and_ascending() {
        let (app, db, token, _) = fixture().await;
        for index in 0..205 {
            record(&db, "ws-1", "task-1", &format!("event-{index}")).await;
        }
        let (status, body) = get(
            &app,
            &token,
            "/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=50",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let result = &body["result"];
        let events = result["events"].as_array().expect("events");
        assert_eq!(events.len(), 50);
        let cursors: Vec<i64> = events
            .iter()
            .map(|event| event["cursor"].as_i64().expect("cursor"))
            .collect();
        assert!(
            cursors.windows(2).all(|pair| pair[0] < pair[1]),
            "cursors must ascend: {cursors:?}"
        );
        assert_eq!(events.last().expect("last")["eventId"], "event-204");
        assert_eq!(result["oldestCursor"], cursors[0]);
        assert_eq!(result["hasMore"], true);

        let (status, body) = get(
            &app,
            &token,
            "/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=999",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["events"].as_array().expect("events").len(), 200);
        assert_eq!(body["result"]["hasMore"], true);

        let (status, body) = get(
            &app,
            &token,
            "/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=0",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["events"].as_array().expect("events").len(), 1);
        assert_eq!(body["result"]["events"][0]["eventId"], "event-204");
    }

    #[tokio::test]
    async fn timeline_before_pagination_walks_older_pages() {
        let (app, db, token, _) = fixture().await;
        for index in 0..5 {
            record(&db, "ws-1", "task-1", &format!("event-{index}")).await;
        }
        let (status, body) = get(
            &app,
            &token,
            "/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=2",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let newest = &body["result"];
        let newest_ids: Vec<&str> = newest["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|event| event["eventId"].as_str().expect("id"))
            .collect();
        assert_eq!(newest_ids, ["event-3", "event-4"]);
        assert_eq!(newest["hasMore"], true);
        let oldest = newest["oldestCursor"].as_i64().expect("oldest cursor");

        let (status, body) = get(
            &app,
            &token,
            &format!("/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=2&before={oldest}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let middle = &body["result"];
        let middle_ids: Vec<&str> = middle["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|event| event["eventId"].as_str().expect("id"))
            .collect();
        assert_eq!(middle_ids, ["event-1", "event-2"]);
        assert_eq!(middle["hasMore"], true);
        let oldest = middle["oldestCursor"].as_i64().expect("oldest cursor");

        let (status, body) = get(
            &app,
            &token,
            &format!("/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=2&before={oldest}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let oldest_page_ids: Vec<&str> = body["result"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|event| event["eventId"].as_str().expect("id"))
            .collect();
        assert_eq!(
            oldest_page_ids,
            ["task:task-1:created", "event-0"],
            "the Task creation event is the oldest persisted row"
        );
        assert_eq!(body["result"]["hasMore"], false);
        let oldest = body["result"]["oldestCursor"].as_i64().expect("oldest cursor");

        let (status, body) = get(
            &app,
            &token,
            &format!("/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=2&before={oldest}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["events"], json!([]));
        assert_eq!(body["result"]["oldestCursor"], Value::Null);
        assert_eq!(body["result"]["hasMore"], false);
    }

    #[tokio::test]
    async fn timeline_is_task_and_workspace_scoped_and_empty_task_has_no_cursor() {
        let (app, db, token, other_token) = fixture().await;
        record(&db, "ws-1", "task-1", "a-1").await;
        record(&db, "ws-1", "task-1", "a-2").await;
        record(&db, "ws-1", "task-2", "b-1").await;
        record(&db, "ws-2", "task-other", "c-1").await;

        let (status, body) = get(&app, &token, "/api/v1/workspaces/ws-1/agent/tasks/task-1/events").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let ids: Vec<&str> = body["result"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|event| event["eventId"].as_str().expect("id"))
            .collect();
        assert_eq!(
            ids,
            ["task:task-1:created", "a-1", "a-2"],
            "foreign-task events must never leak"
        );
        assert_eq!(body["result"]["hasMore"], false);

        let (status, body) = get(&app, &token, "/api/v1/workspaces/ws-1/agent/tasks/task-2/events").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let ids: Vec<&str> = body["result"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .map(|event| event["eventId"].as_str().expect("id"))
            .collect();
        assert_eq!(ids, ["task:task-2:created", "b-1"]);
        assert!(!ids.iter().any(|id| id.starts_with('a') || id.starts_with('c')));

        let (status, body) = get(
            &app,
            &token,
            "/api/v1/workspaces/ws-1/agent/tasks/task-1/events?limit=10",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.to_string().contains("c-1"),
            "cross-workspace event leaked: {body}"
        );

        let (status, _) = get(&app, &token, "/api/v1/workspaces/ws-2/agent/tasks/task-other/events").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "tenant boundary must hold");
        let (status, _) = get(&app, &token, "/api/v1/workspaces/ws-1/agent/tasks/task-other/events").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign Task id must not resolve");
        let (status, body) = get(
            &app,
            &other_token,
            "/api/v1/workspaces/ws-2/agent/tasks/task-other/events",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["events"].as_array().expect("events").len(), 2);
        assert_eq!(body["result"]["events"][1]["eventId"], "c-1");

        let (status, body) = get(&app, &token, "/api/v1/workspaces/ws-1/agent/tasks/task-empty/events").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "unknown Task must 404: {body}");
        seed_bare_task(&db, "ws-1", "task-empty").await;
        let (status, body) = get(&app, &token, "/api/v1/workspaces/ws-1/agent/tasks/task-empty/events").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["events"], json!([]));
        assert_eq!(body["result"]["oldestCursor"], Value::Null);
        assert_eq!(body["result"]["hasMore"], false);
    }
}
