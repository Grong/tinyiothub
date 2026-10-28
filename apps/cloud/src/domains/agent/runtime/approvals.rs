use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde_json::json;
use tinyiothub_agent::port::approval::{ApprovalError, TaskApprovalStore};
use tinyiothub_core::agent_task::{AgentTask, TaskWake, WakeDispatchState};
use tinyiothub_core::task_approval::{ApprovalDecision, DecisionOutcome, TaskApproval, TaskApprovalStoreError};
use tinyiothub_storage::Db;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::{stream, tasks::error_response};

#[derive(Clone)]
pub(crate) struct DbApprovalStore {
    db: Arc<Db>,
    recorder: Option<stream::RuntimeRecorder>,
}

impl DbApprovalStore {
    pub(crate) fn new(db: Arc<Db>) -> Self {
        Self { db, recorder: None }
    }

    pub(crate) fn with_recorder(mut self, recorder: stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    fn publish(&self, events: Vec<tinyiothub_storage::runtime_event::RuntimeEvent>) {
        if let Some(recorder) = &self.recorder {
            for event in events {
                recorder.publish(event);
            }
        }
    }
}

fn map_error(error: TaskApprovalStoreError) -> ApprovalError {
    match error {
        TaskApprovalStoreError::NotFound => ApprovalError::NotFound,
        TaskApprovalStoreError::DecisionConflict => ApprovalError::DecisionConflict,
        TaskApprovalStoreError::Storage(message) => ApprovalError::Storage(message),
    }
}

#[async_trait]
impl TaskApprovalStore for DbApprovalStore {
    async fn create_pending(&self, approval: &TaskApproval, expected_version: i64) -> Result<AgentTask, ApprovalError> {
        let (task, events) = self
            .db
            .create_task_approval_and_events(approval, expected_version)
            .await
            .map_err(map_error)?;
        self.publish(events);
        Ok(task)
    }

    async fn get_scoped(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
    ) -> Result<Option<TaskApproval>, ApprovalError> {
        self.db
            .task_approval_scoped(workspace_id, task_id, approval_id)
            .await
            .map_err(map_error)
    }

    async fn get_by_decision_event(&self, event_id: &str) -> Result<Option<TaskApproval>, ApprovalError> {
        self.db
            .task_approval_by_decision_event(event_id)
            .await
            .map_err(map_error)
    }

    async fn decide(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        decider_id: &str,
        now: DateTime<Utc>,
        wake: &TaskWake,
    ) -> Result<DecisionOutcome, ApprovalError> {
        let (outcome, event) = self
            .db
            .decide_task_approval_and_event(workspace_id, task_id, approval_id, decision, decider_id, now, wake)
            .await
            .map_err(map_error)?;
        self.publish(event.into_iter().collect());
        Ok(outcome)
    }

    async fn list_recoverable(&self, workspace_id: Option<&str>) -> Result<Vec<TaskApproval>, ApprovalError> {
        self.db
            .list_recoverable_task_approvals(workspace_id)
            .await
            .map_err(map_error)
    }

    async fn expire_due(&self, now: DateTime<Utc>) -> Result<u64, ApprovalError> {
        let (expired, events) = self.db.expire_task_approvals_and_events(now).await.map_err(map_error)?;
        self.publish(events);
        Ok(expired)
    }

    async fn cancel(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<AgentTask, ApprovalError> {
        let (task, events) = self
            .db
            .cancel_task_approvals_and_events(workspace_id, task_id, expected_version, now)
            .await
            .map_err(map_error)?;
        self.publish(events);
        Ok(task)
    }
}

async fn decide(
    state: AgentState,
    claims: Claims,
    workspace_id: String,
    task_id: String,
    approval_id: String,
    decision: ApprovalDecision,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let store = DbApprovalStore::new(state.db.clone()).with_recorder(state.runtime_recorder.clone());
    let approval = match store.get_scoped(&workspace_id, &task_id, &approval_id).await {
        Ok(Some(approval)) => approval,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Approval 不存在"),
        Err(error) => return approval_error(error),
    };
    let now = Utc::now();
    let wake = TaskWake {
        task_id: task_id.clone(),
        event_id: approval.decision_event_id.clone(),
        signal_json: json!({
            "kind":"approval_decided",
            "workspace_id":workspace_id,
            "task_id":task_id,
            "approval_id":approval_id,
        })
        .to_string(),
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    };
    match store
        .decide(
            &workspace_id,
            &task_id,
            &approval_id,
            decision,
            &claims.user_id,
            now,
            &wake,
        )
        .await
    {
        Ok(outcome) => ApiResponseBuilder::success(json!({
            "approval": outcome.approval,
            "deduplicated": outcome.deduplicated,
        }))
        .into_response(),
        Err(error) => approval_error(error),
    }
}

fn approval_error(error: ApprovalError) -> Response {
    match error {
        ApprovalError::NotFound => error_response(StatusCode::NOT_FOUND, "Approval 不存在"),
        ApprovalError::DecisionConflict => error_response(StatusCode::CONFLICT, "Approval 决策冲突"),
        other => {
            tracing::warn!(%other, "Task approval decision failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Approval 服务不可用")
        }
    }
}

pub async fn approve(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id, approval_id)): Path<(String, String, String)>,
) -> Response {
    decide(
        state,
        claims,
        workspace_id,
        task_id,
        approval_id,
        ApprovalDecision::Approve,
    )
    .await
}

pub async fn reject(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id, approval_id)): Path<(String, String, String)>,
) -> Response {
    decide(
        state,
        claims,
        workspace_id,
        task_id,
        approval_id,
        ApprovalDecision::Reject,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::task_approval::{ApprovalState, TaskApproval};
    use tinyiothub_storage::Db;
    use tower::ServiceExt;

    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, test_app_state_on_pool,
    };

    const WS: &str = "ws-approval";

    struct Fixture {
        app: Router,
        db: Arc<Db>,
        token: String,
        _dir: tempfile::TempDir,
    }

    async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("approval test directory");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.path().join("approval.db"))
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5))
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .expect("approval test pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("approval test migrations");
        let app_state = test_app_state_on_pool(pool.clone()).await;
        seed_test_workspace(&pool, "tenant-1", WS).await;
        seed_test_workspace(&pool, "tenant-1", "ws-other").await;
        sqlx::query("INSERT OR IGNORE INTO roles (id, name, description, is_administrator) VALUES ('role-admin', 'admin', 'admin', 1)")
            .execute(&pool).await.expect("role");
        sqlx::query("INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES ('ur-approval-admin', 'user-1', 'role-admin')")
            .execute(&pool).await.expect("admin");
        sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = ?")
            .bind(json!({"runtime_tasks_enabled":true}).to_string())
            .bind(WS)
            .execute(&pool)
            .await
            .expect("flag");
        let app = Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        Fixture {
            app,
            db: Arc::new(Db::new(pool)),
            token: create_test_token("user-1", "tenant-1"),
            _dir: dir,
        }
    }

    async fn pending(fixture: &Fixture) -> TaskApproval {
        let now = Utc::now();
        let task = AgentTask {
            id: "task-approval".into(),
            workspace_id: WS.into(),
            idempotency_key: "approval-key".into(),
            objective: "exercise approval".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "create-approval".into(),
            signal_json: json!({"workspace_id":WS}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        fixture.db.create_task_with_wake(&task, &wake).await.expect("task");
        let approval = TaskApproval {
            id: "approval-1".into(),
            workspace_id: WS.into(),
            task_id: task.id.clone(),
            run_id: "run-1".into(),
            tool_call_id: "call-1".into(),
            device_id: "device-1".into(),
            tool: "set_speed".into(),
            args_json: json!({"speed":7,"frozen":true}),
            reason: "confirm".into(),
            state: ApprovalState::Pending,
            decision: None,
            decider_id: None,
            requested_at: now,
            decided_at: None,
            expires_at: now + chrono::Duration::hours(1),
            decision_event_id: "decision-server-1".into(),
            updated_at: now,
        };
        fixture
            .db
            .create_task_approval_and_events(&approval, 0)
            .await
            .expect("approval");
        approval
    }

    async fn post(fixture: &Fixture, token: &str, workspace: &str, action: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/workspaces/{workspace}/agent/tasks/task-approval/approvals/approval-1/{action}"
            ))
            .header("Authorization", auth_header(token))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
        response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await
    }

    #[tokio::test]
    async fn expired_and_unavailable_decisions_return_current_projection_without_new_wake() {
        for unavailable in [false, true] {
            let fixture = fixture().await;
            let pending = pending(&fixture).await;
            if unavailable {
                fixture
                    .db
                    .cancel_task_approvals_and_events(WS, &pending.task_id, 1, Utc::now())
                    .await
                    .expect("cancel");
            } else {
                fixture
                    .db
                    .expire_task_approvals_and_events(Utc::now() + chrono::Duration::hours(2))
                    .await
                    .expect("expire");
            }
            let (status, body) = post(&fixture, &fixture.token, WS, "approve", json!({})).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(
                body["result"]["approval"]["state"],
                if unavailable { "unavailable" } else { "expired" }
            );
            assert_eq!(body["result"]["deduplicated"], true);
            let wakes: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = 'decision-server-1'")
                    .fetch_one(fixture.db.pool())
                    .await
                    .expect("wakes");
            assert_eq!(wakes, 1);
        }
    }

    #[tokio::test]
    async fn approval_routes_enforce_scope_admin_conflict_and_frozen_body_fields() {
        let fixture = fixture().await;
        let pending = pending(&fixture).await;
        let reader = create_test_token("user-2", "tenant-1");
        assert_eq!(
            post(&fixture, &reader, WS, "approve", json!({})).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            post(&fixture, &fixture.token, "ws-other", "approve", json!({})).await.0,
            StatusCode::NOT_FOUND
        );
        let malicious = json!({"args_json":{"speed":999},"decider_id":"attacker","decision_event_id":"attacker-event"});
        let (status, first) = post(&fixture, &fixture.token, WS, "approve", malicious.clone()).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["result"]["approval"]["argsJson"], pending.args_json);
        assert_eq!(first["result"]["approval"]["deciderId"], "user-1");
        assert_eq!(first["result"]["approval"]["decisionEventId"], "decision-server-1");
        assert_eq!(
            post(&fixture, &fixture.token, WS, "approve", malicious).await.0,
            StatusCode::OK
        );
        assert_eq!(
            post(&fixture, &fixture.token, WS, "reject", json!({})).await.0,
            StatusCode::CONFLICT
        );
        let wakes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = 'decision-server-1'")
                .fetch_one(fixture.db.pool())
                .await
                .expect("wakes");
        assert_eq!(wakes, 1);
        let detail = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/workspaces/{WS}/agent/tasks/task-approval"))
            .header("Authorization", auth_header(&fixture.token))
            .body(Body::empty())
            .expect("detail");
        let (_, detail) = response_parts(fixture.app.clone().oneshot(detail).await.expect("response")).await;
        assert_eq!(detail["result"]["approvals"][0]["argsJson"], pending.args_json);
        assert_eq!(detail["result"]["approvals"][0]["state"], "approved");
        let events = fixture
            .db
            .runtime_events_after(WS, None, Some("task-approval"), 20)
            .await
            .expect("events");
        let types = events.iter().map(|event| event.event_type.as_str()).collect::<Vec<_>>();
        assert!(
            types
                .windows(2)
                .any(|pair| pair == ["approval.requested", "approval.approved"]),
            "{types:?}"
        );
    }
}
