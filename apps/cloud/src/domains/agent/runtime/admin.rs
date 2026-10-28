use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::Deserialize;
use serde_json::json;
use tinyiothub_core::agent_task::WakeDispatchState;
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
