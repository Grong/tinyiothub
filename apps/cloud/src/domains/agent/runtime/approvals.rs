use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde_json::json;
use tinyiothub_core::task_approval::ApprovalDecision;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::tasks::error_response;

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
    let answer = match decision {
        ApprovalDecision::Approve => "approved",
        ApprovalDecision::Reject => "denied",
    };
    let now = Utc::now();
    let outcome = match state
        .db
        .harness_answer_tool_approval(&workspace_id, &task_id, &approval_id, answer, &claims.user_id, now)
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => return harness_approval_error(error),
    };
    state.session_stream.notify(&workspace_id);
    // An approval answer is the fact that can unblock a Prepared Effect. Run
    // the bounded workspace dispatch pass now so the answer is not waiting for
    // the recovery interval. The continuation wake is owned by the answer
    // transaction (denial/expiry) or by the dispatcher/Job terminal wake; the
    // handler never infers an outcome from this pass's dispatch count.
    if let Some(dispatch) = &state.effect_dispatch
        && let Err(error) = dispatch.dispatch_task(&workspace_id, &task_id).await
    {
        tracing::warn!(%workspace_id, %task_id, %error, "effect dispatch after approval answer failed");
    }
    let projection = match state.db.harness_runtime_projection(&workspace_id, &task_id).await {
        Ok(Some(projection)) => projection,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        Err(error) => return harness_approval_error(error),
    };
    let approval = projection
        .approvals
        .into_iter()
        .find(|approval| approval.id == approval_id)
        .ok_or_else(|| "approval projection disappeared".to_string());
    match approval {
        Ok(approval) => ApiResponseBuilder::success(json!({
            "approval": approval,
            "deduplicated": outcome.deduplicated,
        }))
        .into_response(),
        Err(error) => {
            tracing::error!(%workspace_id, %approval_id, %error, "approval projection missing after answer");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Approval 服务不可用")
        }
    }
}

fn harness_approval_error(error: tinyiothub_storage::error::DbError) -> Response {
    use tinyiothub_storage::error::DbError;
    match error {
        DbError::NotFound { .. } => error_response(StatusCode::NOT_FOUND, "Approval 不存在"),
        DbError::Conflict { .. } | DbError::CasConflict { .. } => {
            error_response(StatusCode::CONFLICT, "Approval 决策冲突")
        }
        DbError::Validation { message } => error_response(StatusCode::BAD_REQUEST, message),
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

#[derive(serde::Deserialize)]
pub struct ReconcileEffectRequest {
    answer: String,
    evidence: Option<serde_json::Value>,
}

pub async fn reconcile_effect(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, effect_id)): Path<(String, String)>,
    Json(request): Json<ReconcileEffectRequest>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let recovery = super::EffectRecovery::new(state.db.clone());
    match recovery
        .answer_reconciliation(
            &workspace_id,
            &effect_id,
            &request.answer,
            &claims.user_id,
            request.evidence,
        )
        .await
    {
        Ok(()) => {
            state.session_stream.notify(&workspace_id);
            ApiResponseBuilder::success(json!({"accepted": true})).into_response()
        }
        Err(error) => error_response(StatusCode::CONFLICT, error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_storage::Db;
    use tower::ServiceExt;

    use crate::domains::agent::runtime::dispatch::EffectDispatch;
    use crate::domains::agent::runtime::session_stream::SessionStreamHub;
    use crate::test_utils::{
        auth_header, create_test_token, response_parts, seed_test_workspace, test_app_state_on_pool,
    };

    const WS: &str = "ws-approval";

    struct Fixture {
        app: Router,
        state: crate::state::AppState,
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
        let db = app_state.db.clone();
        db.create_workspace_store(WS).await.expect("workspace store");
        let app = Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state.clone());
        Fixture {
            app,
            state: app_state,
            db,
            token: create_test_token("user-1", "tenant-1"),
            _dir: dir,
        }
    }

    #[derive(Default)]
    struct RecordingWorker {
        submitted: Mutex<Vec<String>>,
    }

    impl tinyiothub_runtime::job::worker_adapter::JobDeviceWorker for RecordingWorker {
        fn submit(
            &self,
            _command: tinyiothub_core::models::thing_command::ThingCommand,
            job_id: &str,
            _lease: Option<tinyiothub_runtime::arbitration::ResourceLease>,
        ) -> Result<(), tinyiothub_core::error::Error> {
            self.submitted.lock().expect("submissions").push(job_id.into());
            Ok(())
        }

        fn cancel_job(&self, _job_id: &str) -> bool {
            true
        }

        fn has_recoverable_call(&self, _job_id: &str) -> bool {
            false
        }

        fn complete_job(&self, _job_id: &str) {}
    }

    struct RecordingWorkers {
        worker: Arc<RecordingWorker>,
    }

    impl tinyiothub_runtime::job::worker_adapter::JobWorkerLocator for RecordingWorkers {
        fn worker_for(
            &self,
            _device_id: &str,
        ) -> Option<Arc<dyn tinyiothub_runtime::job::worker_adapter::JobDeviceWorker>> {
            Some(self.worker.clone())
        }
    }

    impl Fixture {
        fn access(&self) -> Arc<tinyiothub_runtime::access::service::AccessService> {
            let manifests = Arc::new(tinyiothub_runtime::access::registry::ManifestRegistry::new());
            manifests
                .seed_builtin(include_str!(
                    "../../../../../../drivers/demo-declarative/driver.manifest.json"
                ))
                .expect("demo manifest");
            Arc::new(tinyiothub_runtime::access::service::AccessService::new(
                std::path::PathBuf::from("."),
                manifests,
                Arc::new(crate::shared::access_ports::DbDriverStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbDeviceStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbProjectionStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbThingSource(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbAcceptanceStore(self.db.clone())),
                Arc::new(tinyiothub_runtime::access::demand::DemandTracker::new()),
            ))
        }

        fn job_service(&self, worker: Arc<RecordingWorker>) -> tinyiothub_runtime::job::service::JobService {
            let access = self.access();
            tinyiothub_runtime::job::service::JobService::new(
                Arc::new(crate::shared::runtime_ports::WorkspaceDeviceJobStore::new(
                    self.db.clone(),
                )),
                Arc::new(RecordingWorkers { worker }),
                access.clone(),
                access,
                Arc::new(tinyiothub_runtime::arbitration::ArbitrationRegistry::default()),
            )
        }

        fn app_with_dispatch(&self, dispatch: Arc<EffectDispatch>) -> Router {
            let mut state = self.state.clone();
            state.effect_dispatch = Some(dispatch);
            Router::new()
                .nest("/api", crate::api::create_router(&state))
                .with_state(state)
        }
    }

    /// Seed the workspace Harness facts the HTTP approval routes read.
    async fn seed_harness_approval(fixture: &Fixture, task_id: &str, approval_id: &str, state: &str) {
        let now = Utc::now();
        let task = AgentTask {
            id: task_id.into(),
            workspace_id: WS.into(),
            idempotency_key: format!("{task_id}-key"),
            objective: "exercise approval".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::Active,
            current_run_id: Some("run-1".into()),
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: format!("create-{task_id}"),
            signal_json: json!({"workspace_id":WS}).to_string(),
            dispatch_state: WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        fixture
            .db
            .harness_create_task_with_wake(&task, &wake)
            .await
            .expect("harness task");
        let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
        sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, answer_evidence_json, consumed_at, consumed_by)
             VALUES (?, 'tool_approval', ?, 'session-1', 'run-1', ?, NULL, ?, '[\"approved\",\"denied\"]',
                     ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(approval_id)
        .bind(state)
        .bind(task_id)
        .bind(
            json!({
                "deviceId":"device-1",
                "tool":"set_level",
                "argsJson":{"level":7},
                "reason":"confirm",
            })
            .to_string(),
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("harness decision");
    }

    /// Seed a Harness Task, device/declaration, a Prepared Effect, and a
    /// linked requested `tool_approval` Decision. No platform Task/approval
    /// rows are written (Task 19C1 must not create them).
    async fn seed_dispatch_approval(fixture: &Fixture) {
        let now = Utc::now();
        let task = AgentTask {
            id: "task-approval".into(),
            workspace_id: WS.into(),
            idempotency_key: "dispatch-approval-key".into(),
            objective: "dispatch through approval".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::Active,
            current_run_id: Some("run-1".into()),
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "create-dispatch-approval".into(),
            signal_json: json!({"workspace_id":WS}).to_string(),
            dispatch_state: WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        fixture
            .db
            .harness_create_task_with_wake(&task, &wake)
            .await
            .expect("harness task");
        sqlx::query(
            "INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('device-1', 'Device', ?, 'device')",
        )
        .bind(WS)
        .execute(fixture.db.pool())
        .await
        .expect("device");
        let declaration: tinyiothub_core::driver::declaration::DeviceDeclaration = serde_json::from_value(json!({
            "manifestVersion": "1.1",
            "driver": "demo-declarative",
            "driverVersion": "1.0.0",
            "stateFields": [{"name":"level","type":"integer"}],
            "tools": [{
                "name":"set_level","kind":"set","targetField":"level",
                "parameters":{"type":"object","properties":{"level":{"type":"integer","minimum":0,"maximum":100}},"required":["level"],"additionalProperties":false},
                "resources":["actuator"]
            }]
        }))
        .expect("declaration");
        fixture
            .db
            .upsert_device_declaration(&tinyiothub_core::driver::declaration::DeviceDeclarationRecord {
                workspace_id: WS.into(),
                thing_id: "device-1".into(),
                driver_name: declaration.driver.clone(),
                driver_version: declaration.driver_version.clone(),
                declaration_json: serde_json::to_string(&declaration).expect("declaration JSON"),
                declaration_hash: "dispatch-approval-hash".into(),
                status: "active".into(),
                updated_at: now.to_rfc3339(),
            })
            .await
            .expect("declaration row");
        let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES ('run-1', 'task-approval', 'running')")
            .execute(&pool)
            .await
            .expect("run");
        sqlx::query(
            "INSERT INTO effect_intents
                (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                 request_digest, idempotency_key, external_ref, state, state_version, result_json,
                 created_at, updated_at)
             VALUES ('effect-1', 'session-1', 'task-approval', 'run-1', 'call-1', 'set_level', ?,
                     'digest-1', 'effect-1', NULL, 'prepared', 1, NULL, ?, ?)",
        )
        .bind(
            json!({
                "deviceId": "device-1",
                "tool": "set_level",
                "argsJson": {"level": 7},
                "reason": "confirm",
            })
            .to_string(),
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("effect");
        sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, answer_evidence_json, consumed_at, consumed_by)
             VALUES ('approval-1', 'tool_approval', 'requested', 'session-1', 'run-1',
                     'task-approval', 'effect-1', ?, '[\"approved\",\"denied\"]', ?, ?, NULL,
                     NULL, NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(
            json!({
                "deviceId": "device-1",
                "tool": "set_level",
                "argsJson": {"level": 7},
                "reason": "confirm",
            })
            .to_string(),
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("decision");
    }

    async fn post_to(
        app: &Router,
        token: &str,
        workspace: &str,
        task_id: &str,
        approval_id: &str,
        action: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/workspaces/{workspace}/agent/tasks/{task_id}/approvals/{approval_id}/{action}"
            ))
            .header("Authorization", auth_header(token))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
        response_parts(app.clone().oneshot(request).await.expect("response")).await
    }

    async fn post(fixture: &Fixture, token: &str, workspace: &str, action: &str, body: Value) -> (StatusCode, Value) {
        post_to(
            &fixture.app,
            token,
            workspace,
            "task-approval",
            "approval-1",
            action,
            body,
        )
        .await
    }

    #[tokio::test]
    async fn effect_recovery_reconciliation_requires_agent_admin_before_workspace_open() {
        let fixture = fixture().await;
        let reader = create_test_token("reader-1", "tenant-1");
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/workspaces/{WS}/agent/effects/effect-1/reconcile"))
            .header("Authorization", auth_header(&reader))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"answer":"applied"}).to_string()))
            .expect("request");
        let (status, body) = response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "unexpected response: {body}");
    }

    #[tokio::test]
    async fn expired_and_unavailable_decisions_return_current_projection_without_new_wake() {
        for unavailable in [false, true] {
            let fixture = fixture().await;
            seed_harness_approval(
                &fixture,
                "task-approval",
                "approval-1",
                if unavailable { "withdrawn" } else { "expired" },
            )
            .await;
            let (status, body) = post(&fixture, &fixture.token, WS, "approve", json!({})).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(
                body["result"]["approval"]["state"],
                if unavailable { "unavailable" } else { "expired" }
            );
            assert_eq!(body["result"]["deduplicated"], true);
            let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            // A deduplicated expired answer heals the crash window by
            // ensuring the one idempotent expiry wake exists; a withdrawn
            // decision (cancel owns closure) adds none.
            let wakes: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = 'approval:approval-1:expired'",
            )
            .fetch_one(&pool)
            .await
            .expect("wakes");
            assert_eq!(wakes, i64::from(!unavailable), "{body}");
            let platform: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
                .fetch_one(fixture.db.pool())
                .await
                .expect("platform approvals");
            assert_eq!(platform, 0, "HTTP approvals never write the platform table");
        }
    }

    #[tokio::test]
    async fn approval_routes_enforce_scope_admin_conflict_and_frozen_body_fields() {
        let fixture = fixture().await;
        seed_harness_approval(&fixture, "task-approval", "approval-1", "requested").await;
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
        assert_eq!(first["result"]["approval"]["argsJson"], json!({"level":7}));
        assert_eq!(first["result"]["approval"]["deciderId"], "user-1");
        assert_eq!(
            first["result"]["approval"]["decisionEventId"],
            "approval:approval-1:approved"
        );
        let (status, repeated) = post(&fixture, &fixture.token, WS, "approve", malicious).await;
        assert_eq!(status, StatusCode::OK, "{repeated}");
        assert_eq!(repeated["result"]["deduplicated"], true);
        assert_eq!(
            post(&fixture, &fixture.token, WS, "reject", json!({})).await.0,
            StatusCode::CONFLICT
        );
        let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
        // Approved answers do not write a handler wake; without a live
        // dispatcher/Job in this fixture there is no continuation wake.
        let wakes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id LIKE 'approval:approval-1:%'")
                .fetch_one(&pool)
                .await
                .expect("wakes");
        assert_eq!(wakes, 0);
        let detail = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/workspaces/{WS}/agent/tasks/task-approval"))
            .header("Authorization", auth_header(&fixture.token))
            .body(Body::empty())
            .expect("detail");
        let (_, detail) = response_parts(fixture.app.clone().oneshot(detail).await.expect("response")).await;
        assert_eq!(detail["result"]["approvals"][0]["argsJson"], json!({"level":7}));
        assert_eq!(detail["result"]["approvals"][0]["state"], "approved");
        let types: Vec<String> =
            sqlx::query_scalar("SELECT type FROM session_events WHERE task_id = 'task-approval' ORDER BY seq")
                .fetch_all(&pool)
                .await
                .expect("events");
        assert_eq!(
            types
                .iter()
                .filter(|event_type| event_type.as_str() == "approval.approved")
                .count(),
            1,
            "{types:?}"
        );
        let platform: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
            .fetch_one(fixture.db.pool())
            .await
            .expect("platform approvals");
        assert_eq!(platform, 0, "HTTP approvals never write the platform table");
    }

    #[tokio::test]
    async fn approval_projection_and_sse_snapshot_redact_nested_secrets() {
        let fixture = fixture().await;
        let now = Utc::now();
        let task = AgentTask {
            id: "task-secret".into(),
            workspace_id: WS.into(),
            idempotency_key: "secret-key".into(),
            objective: "redaction".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::Active,
            current_run_id: Some("run-secret".into()),
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "create-task-secret".into(),
            signal_json: json!({"workspace_id":WS}).to_string(),
            dispatch_state: WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        fixture
            .db
            .harness_create_task_with_wake(&task, &wake)
            .await
            .expect("harness task");
        let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
        // Historical raw subject: the public projection must redact on read.
        sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, answer_evidence_json, consumed_at, consumed_by)
             VALUES ('decision-secret', 'tool_approval', 'requested', 'session-1', 'run-secret',
                     'task-secret', NULL, ?, '[\"approved\",\"denied\"]', ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(
            json!({
                "deviceId":"device-1",
                "tool":"set_level",
                "argsJson":{"level":7,"credentials":{"password":"s3cret"}},
                "reason":"confirm",
            })
            .to_string(),
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("secret decision");
        // Effect fallback: a subject without argsJson must not leak the raw
        // effect request either.
        sqlx::query(
            "INSERT INTO effect_intents
                (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                 request_digest, idempotency_key, external_ref, state, state_version, result_json,
                 created_at, updated_at)
             VALUES ('effect-secret', 'session-1', 'task-secret', 'run-secret', 'call-secret',
                     'set_level', ?, 'digest-secret', 'effect-secret', NULL, 'prepared', 1, NULL, ?, ?)",
        )
        .bind(json!({"deviceId":"device-1","argsJson":{"level":7,"credentials":{"password":"s3cret"}}}).to_string())
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("secret effect");
        sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, answer_evidence_json, consumed_at, consumed_by)
             VALUES ('decision-fallback', 'tool_approval', 'requested', 'session-1', 'run-secret',
                     'task-secret', 'effect-secret', '{\"deviceId\":\"device-1\",\"tool\":\"set_level\"}',
                     '[\"approved\",\"denied\"]', ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("fallback decision");

        // This row deliberately models a historical unredacted subject; the
        // write path itself is covered by the tool test. The read projection
        // must still redact it.
        let _subject: String =
            sqlx::query_scalar("SELECT subject_json FROM decisions WHERE decision_id = 'decision-secret'")
                .fetch_one(&pool)
                .await
                .expect("subject");
        let projection = fixture
            .db
            .harness_runtime_projection(WS, "task-secret")
            .await
            .expect("projection")
            .expect("task projection");
        for approval in &projection.approvals {
            assert!(
                !approval.args_json.to_string().contains("s3cret"),
                "projection leaked a secret: {:?}",
                approval.args_json
            );
        }
        let fallback = projection
            .approvals
            .iter()
            .find(|approval| approval.id == "decision-fallback")
            .expect("fallback approval");
        assert_eq!(
            fallback.args_json["argsJson"]["credentials"]["password"], "[REDACTED]",
            "the effect request fallback must be redacted: {}",
            fallback.args_json
        );

        let hub = Arc::new(SessionStreamHub::default());
        let mut connection = hub
            .connect(fixture.db.clone(), WS, "task-secret", None)
            .await
            .expect("connect");
        let frame = connection.next_frame().await.expect("snapshot");
        let frame = serde_json::to_string(&frame).expect("frame JSON");
        assert!(!frame.contains("s3cret"), "SSE snapshot leaked a secret");

        let (status, body) = post_to(
            &fixture.app,
            &fixture.token,
            WS,
            "task-secret",
            "decision-secret",
            "approve",
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.to_string().contains("s3cret"),
            "HTTP approval response leaked a secret"
        );
    }

    /// Task 19C1: the HTTP answer is the unblock fact. Approval dispatches the
    /// Prepared Effect immediately; denial settles it without a Job. Neither
    /// flow writes a platform approval or the legacy runtime ledger.
    #[tokio::test]
    async fn approval_answer_runs_the_effect_dispatch_pass_for_that_workspace() {
        for (action, expected_effect, expected_jobs) in
            [("approve", "dispatching", 1_usize), ("reject", "not_applied", 0)]
        {
            let fixture = fixture().await;
            seed_dispatch_approval(&fixture).await;
            let worker = Arc::new(RecordingWorker::default());
            let dispatch = Arc::new(EffectDispatch::new(
                fixture.db.clone(),
                fixture.job_service(worker.clone()),
            ));

            // The unanswered decision blocks before the HTTP answer.
            let before = dispatch.dispatch_workspace(WS).await.expect("pre-answer pass");
            assert_eq!(before.blocked, 1, "{action}");
            assert!(worker.submitted.lock().expect("submissions").is_empty(), "{action}");

            // An unrelated Prepared Effect on another Task: the answer-driven
            // pass must be scoped to the answered Task and leave it alone.
            let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            sqlx::query(
                "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
                 VALUES ('task-unrelated', 'run-u', 'running', 1, 'production')",
            )
            .execute(&pool)
            .await
            .expect("unrelated task");
            sqlx::query(
                "INSERT INTO effect_intents
                    (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                     request_digest, idempotency_key, external_ref, state, state_version, result_json,
                     created_at, updated_at)
                 VALUES ('effect-unrelated', 'session-1', 'task-unrelated', 'run-u', 'call-u',
                         'set_level', '{\"deviceId\":\"device-1\",\"argsJson\":{\"level\":7}}',
                         'digest-u', 'effect-unrelated', NULL, 'prepared', 1, NULL, ?, ?)",
            )
            .bind(Utc::now())
            .bind(Utc::now())
            .execute(&pool)
            .await
            .expect("unrelated effect");

            let app = fixture.app_with_dispatch(dispatch);
            let (status, body) = post_to(
                &app,
                &fixture.token,
                WS,
                "task-approval",
                "approval-1",
                action,
                json!({}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{action}: {body}");

            let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            let (effect_state, external_ref): (String, Option<String>) =
                sqlx::query_as("SELECT state, external_ref FROM effect_intents WHERE effect_id = 'effect-1'")
                    .fetch_one(&pool)
                    .await
                    .expect("effect");
            assert_eq!(effect_state, expected_effect, "{action}");
            assert_eq!(
                worker.submitted.lock().expect("submissions").len(),
                expected_jobs,
                "{action}"
            );
            let unrelated_state: String =
                sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = 'effect-unrelated'")
                    .fetch_one(&pool)
                    .await
                    .expect("unrelated effect");
            assert_eq!(
                unrelated_state, "prepared",
                "{action}: an answer must not dispatch another Task's Effect"
            );
            if expected_jobs == 1 {
                assert_eq!(
                    external_ref.as_deref(),
                    Some("effect:effect-1"),
                    "dispatch must record the deterministic Job identity"
                );
            } else {
                assert!(external_ref.is_none(), "{action}");
            }

            let platform_approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
                .fetch_one(fixture.db.pool())
                .await
                .expect("platform approvals");
            assert_eq!(platform_approvals, 0, "{action}: no platform approval writes");
            let legacy_events: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM hub_ledger WHERE kind = 'runtime' || '.v1'")
                    .fetch_one(fixture.db.pool())
                    .await
                    .expect("legacy runtime events");
            assert_eq!(legacy_events, 0, "{action}: no legacy runtime ledger writes");
        }
    }
}
