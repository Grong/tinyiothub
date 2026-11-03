use std::sync::Arc;

use crate::domains::agent::thing_agent::approval_port::{ApprovalError, TaskApprovalStore};
use async_trait::async_trait;
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde_json::json;
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

    async fn get_by_decision_event(
        &self,
        workspace_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskApproval>, ApprovalError> {
        self.db
            .task_approval_by_decision_event(workspace_id, event_id)
            .await
            .map_err(map_error)
    }

    async fn list_for_task(&self, workspace_id: &str, task_id: &str) -> Result<Vec<TaskApproval>, ApprovalError> {
        self.db
            .list_task_approvals(workspace_id, task_id)
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
        Ok(()) => ApiResponseBuilder::success(json!({"accepted": true})).into_response(),
        Err(error) => error_response(StatusCode::CONFLICT, error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_agent::port::task::TaskStore as _;
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::task_approval::{ApprovalState, TaskApproval};
    use tinyiothub_storage::Db;
    use tower::ServiceExt;

    use super::DbApprovalStore;
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

    #[derive(Default)]
    struct RuntimeProvider {
        prompts: Arc<Mutex<Vec<String>>>,
        block_next: Arc<AtomicBool>,
        release: Arc<tokio::sync::Notify>,
        started: Arc<tokio::sync::Notify>,
    }

    struct RuntimeAgent {
        prompts: Arc<Mutex<Vec<String>>>,
        block_next: Arc<AtomicBool>,
        release: Arc<tokio::sync::Notify>,
        started: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl tinyiothub_agent::port::runtime::AgentLoop for RuntimeAgent {
        async fn turn_streamed(
            &self,
            message: &str,
            _event_tx: tokio::sync::mpsc::Sender<tinyiothub_agent::port::events::TurnEvent>,
            _cancel: Option<tokio_util::sync::CancellationToken>,
        ) -> anyhow::Result<String> {
            self.prompts.lock().expect("prompts").push(message.into());
            self.started.notify_one();
            if self.block_next.swap(false, Ordering::SeqCst) {
                self.release.notified().await;
            }
            Ok("done".into())
        }

        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            Ok("done".into())
        }

        async fn clear_history(&self) {}

        async fn seed_history(
            &self,
            _messages: &[tinyiothub_agent::port::provider::ChatMessage],
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl crate::domains::agent::thing_agent::manager::AutonomousAgentProvider for RuntimeProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<crate::domains::agent::thing_agent::RunContextInner>>,
        ) -> anyhow::Result<crate::domains::agent::thing_agent::AgentHandle> {
            Ok(Arc::new(tokio::sync::Mutex::new(RuntimeAgent {
                prompts: self.prompts.clone(),
                block_next: self.block_next.clone(),
                release: self.release.clone(),
                started: self.started.clone(),
            })))
        }

        fn invalidate(&self, _workspace_id: &str) {}
    }

    struct ApprovalRuntime {
        store: Arc<crate::domains::agent::runtime::DbTaskStore>,
        approvals: Arc<DbApprovalStore>,
        coordinator: Arc<crate::domains::agent::thing_agent::task::coordinator::TaskCoordinator>,
        executor: Arc<crate::domains::agent::thing_agent::task::approval::ApprovalExecutor>,
        router: Arc<crate::domains::agent::thing_agent::task::wake::WakeRouter>,
        active: crate::domains::agent::thing_agent::active_runs::ActiveRunRegistry,
        jobs: tinyiothub_runtime::job::service::JobService,
    }

    impl ApprovalRuntime {
        async fn recover(&self) {
            self.coordinator.recover().await.expect("coordinator recovery");
            self.executor.recover(None).await.expect("approval recovery");
            self.router.run(64).await.expect("decision routing");
        }
    }

    impl Fixture {
        fn runtime(&self, worker: Arc<RecordingWorker>, provider: Arc<RuntimeProvider>) -> ApprovalRuntime {
            let store = Arc::new(crate::domains::agent::runtime::DbTaskStore::new(self.db.clone()));
            let approvals = Arc::new(DbApprovalStore::new(self.db.clone()));
            let manifests = Arc::new(tinyiothub_runtime::access::registry::ManifestRegistry::new());
            manifests
                .seed_builtin(include_str!(
                    "../../../../../../drivers/demo-declarative/driver.manifest.json"
                ))
                .expect("demo manifest");
            let access = Arc::new(tinyiothub_runtime::access::service::AccessService::new(
                std::path::PathBuf::from("."),
                manifests,
                Arc::new(crate::shared::access_ports::DbDriverStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbDeviceStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbProjectionStore(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbThingSource(self.db.clone())),
                Arc::new(crate::shared::access_ports::DbAcceptanceStore(self.db.clone())),
                Arc::new(tinyiothub_runtime::access::demand::DemandTracker::new()),
            ));
            let jobs = tinyiothub_runtime::job::service::JobService::new(
                Arc::new(crate::shared::runtime_ports::DbDeviceJobStore::new(
                    self.db.as_ref().clone(),
                )),
                Arc::new(RecordingWorkers { worker }),
                access.clone(),
                access,
                Arc::new(tinyiothub_runtime::arbitration::ArbitrationRegistry::default()),
            );
            let active = crate::domains::agent::thing_agent::active_runs::ActiveRunRegistry::new(8);
            let coordinator = Arc::new(
                crate::domains::agent::thing_agent::task::coordinator::TaskCoordinator::new(
                    store.clone(),
                    Some(jobs.clone()),
                    Arc::new(crate::shared::runtime_ports::WatchRuleStoreAdapter::new(
                        self.db.as_ref().clone(),
                    )),
                    Arc::new(dashmap::DashMap::new()),
                    Arc::new(crate::domains::agent::events::AgentEventBus::new(64)),
                    active.clone(),
                    Arc::new(crate::domains::agent::thing_agent::runner::Runner::new()),
                    provider,
                )
                .with_approvals(approvals.clone()),
            );
            let executor = Arc::new(
                crate::domains::agent::thing_agent::task::approval::ApprovalExecutor::new(
                    approvals.clone(),
                    Arc::new(jobs.clone()),
                    coordinator.clone(),
                ),
            );
            let router = Arc::new(
                crate::domains::agent::thing_agent::task::wake::WakeRouter::new(store.clone(), coordinator.clone())
                    .with_approvals(executor.clone()),
            );
            ApprovalRuntime {
                store,
                approvals,
                coordinator,
                executor,
                router,
                active,
                jobs,
            }
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
            dispatch_state: WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        fixture.db.create_task_with_wake(&task, &wake).await.expect("task");
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
                declaration_hash: "approval-test-hash".into(),
                status: "active".into(),
                updated_at: now.to_rfc3339(),
            })
            .await
            .expect("declaration row");
        let approval = TaskApproval {
            id: "approval-1".into(),
            workspace_id: WS.into(),
            task_id: task.id.clone(),
            run_id: "run-1".into(),
            tool_call_id: "call-1".into(),
            device_id: "device-1".into(),
            tool: "set_level".into(),
            args_json: json!({"level":7}),
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

    fn decision_wake(approval: &TaskApproval) -> TaskWake {
        let now = Utc::now();
        TaskWake {
            task_id: approval.task_id.clone(),
            event_id: approval.decision_event_id.clone(),
            signal_json: json!({
                "kind": "approval_decided",
                "workspace_id": approval.workspace_id,
                "approval_id": approval.id,
                "decision_event_id": approval.decision_event_id,
            })
            .to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        }
    }

    async fn decide(
        runtime: &ApprovalRuntime,
        approval: &TaskApproval,
        decision: tinyiothub_core::task_approval::ApprovalDecision,
    ) -> tinyiothub_core::task_approval::DecisionOutcome {
        crate::domains::agent::thing_agent::approval_port::TaskApprovalStore::decide(
            runtime.approvals.as_ref(),
            &approval.workspace_id,
            &approval.task_id,
            &approval.id,
            decision,
            "user-1",
            Utc::now(),
            &decision_wake(approval),
        )
        .await
        .expect("decision")
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
    async fn paused_pending_resume_restores_approval_before_active_jobs() {
        let fixture = fixture().await;
        let pending = pending(&fixture).await;
        let worker = Arc::new(RecordingWorker::default());
        let runtime = fixture.runtime(worker, Arc::new(RuntimeProvider::default()));
        runtime.coordinator.pause(WS, &pending.task_id).await.expect("pause");
        let job = tinyiothub_core::device_job::DeviceJob {
            id: "job-other".into(),
            workspace_id: WS.into(),
            device_id: pending.device_id.clone(),
            tool: pending.tool.clone(),
            args_json: json!({"level":1}),
            state: tinyiothub_core::device_job::DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(pending.task_id.clone()),
            run_id: Some(pending.run_id.clone()),
            tool_call_id: Some("call-other".into()),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: Utc::now(),
            ended_at: None,
        };
        fixture.db.create_job(&job).await.expect("active job");
        let resumed = runtime.coordinator.resume(WS, &pending.task_id).await.expect("resume");
        assert_eq!(resumed.status, TaskStatus::NeedsApproval);
        let decided = decide(
            &runtime,
            &pending,
            tinyiothub_core::task_approval::ApprovalDecision::Approve,
        )
        .await;
        assert_eq!(decided.approval.decision_event_id, pending.decision_event_id);
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = ?")
            .bind(&pending.decision_event_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("decision wake");
        assert_eq!(wakes, 1);
    }

    #[tokio::test]
    async fn paused_decisions_resume_with_the_original_decision_wake() {
        for action in ["approve", "reject", "expire"] {
            let fixture = fixture().await;
            let pending = pending(&fixture).await;
            let worker = Arc::new(RecordingWorker::default());
            let runtime = fixture.runtime(worker.clone(), Arc::new(RuntimeProvider::default()));
            runtime.coordinator.pause(WS, &pending.task_id).await.expect("pause");
            match action {
                "approve" => {
                    decide(
                        &runtime,
                        &pending,
                        tinyiothub_core::task_approval::ApprovalDecision::Approve,
                    )
                    .await;
                }
                "reject" => {
                    decide(
                        &runtime,
                        &pending,
                        tinyiothub_core::task_approval::ApprovalDecision::Reject,
                    )
                    .await;
                }
                "expire" => {
                    crate::domains::agent::thing_agent::approval_port::TaskApprovalStore::expire_due(
                        runtime.approvals.as_ref(),
                        pending.expires_at + chrono::Duration::seconds(1),
                    )
                    .await
                    .expect("expiry");
                }
                _ => unreachable!(),
            }
            assert_eq!(
                runtime.router.run(64).await.expect("paused dispatch"),
                0,
                "paused owners must not be claimed by the router"
            );
            let wake = runtime
                .store
                .wake_by_id(&pending.task_id, &pending.decision_event_id)
                .await
                .expect("wake read")
                .expect("original wake");
            assert_eq!(wake.dispatch_state, WakeDispatchState::Pending, "{action}");
            let resumed = runtime.coordinator.resume(WS, &pending.task_id).await.expect("resume");
            assert_eq!(resumed.status, TaskStatus::NeedsApproval, "{action}");
            assert_eq!(runtime.router.run(64).await.expect("resume dispatch"), 1, "{action}");
            let wake = runtime
                .store
                .wake_by_id(&pending.task_id, &pending.decision_event_id)
                .await
                .expect("wake read")
                .expect("original wake");
            assert_eq!(wake.dispatch_state, WakeDispatchState::Dispatched, "{action}");
            let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
                .bind(&pending.tool_call_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("jobs");
            assert_eq!(jobs, i64::from(action == "approve"), "{action}");
        }
    }

    #[tokio::test]
    async fn repeated_recovery_with_a_blocked_continuation_dispatches_once_and_supplies_approval_facts() {
        let fixture = fixture().await;
        let pending = pending(&fixture).await;
        let provider = Arc::new(RuntimeProvider::default());
        provider.block_next.store(true, Ordering::SeqCst);
        let runtime = fixture.runtime(Arc::new(RecordingWorker::default()), provider.clone());
        decide(
            &runtime,
            &pending,
            tinyiothub_core::task_approval::ApprovalDecision::Reject,
        )
        .await;
        runtime.recover().await;
        runtime.executor.recover(None).await.expect("repeat approval recovery");
        runtime.executor.recover(None).await.expect("third approval recovery");
        assert_eq!(runtime.router.run(64).await.expect("repeat routing"), 0);
        let task = fixture
            .db
            .get_task(WS, &pending.task_id)
            .await
            .expect("task")
            .expect("task row");
        let run_id = task.current_run_id.clone().expect("continuation run");
        assert!(runtime.active.get(&run_id).is_some());
        tokio::time::timeout(std::time::Duration::from_secs(5), provider.started.notified())
            .await
            .expect("continuation prompt starts");
        let prompts = provider.prompts.lock().expect("prompts");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains(&pending.id));
        assert!(prompts[0].contains("rejected"));
        drop(prompts);
        provider.release.notify_waiters();
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CrashPoint {
        BeforeDecisionCommit,
        AfterDecisionCommit,
        AfterDispatchBeforeJob,
        AfterJobBeforeWakeMark,
    }

    #[tokio::test]
    async fn approved_execution_recovers_from_every_commit_boundary_without_duplicate_effects() {
        for point in [
            CrashPoint::BeforeDecisionCommit,
            CrashPoint::AfterDecisionCommit,
            CrashPoint::AfterDispatchBeforeJob,
            CrashPoint::AfterJobBeforeWakeMark,
        ] {
            let fixture = fixture().await;
            let pending = pending(&fixture).await;
            let worker = Arc::new(RecordingWorker::default());
            let provider = Arc::new(RuntimeProvider::default());
            let crashed = fixture.runtime(worker.clone(), provider.clone());
            if point == CrashPoint::BeforeDecisionCommit {
                let mut invalid = decision_wake(&pending);
                invalid.event_id = "wrong-event".into();
                let result = crate::domains::agent::thing_agent::approval_port::TaskApprovalStore::decide(
                    crashed.approvals.as_ref(),
                    WS,
                    &pending.task_id,
                    &pending.id,
                    tinyiothub_core::task_approval::ApprovalDecision::Approve,
                    "user-1",
                    Utc::now(),
                    &invalid,
                )
                .await;
                assert!(result.is_err());
            } else {
                decide(
                    &crashed,
                    &pending,
                    tinyiothub_core::task_approval::ApprovalDecision::Approve,
                )
                .await;
                if matches!(
                    point,
                    CrashPoint::AfterDispatchBeforeJob | CrashPoint::AfterJobBeforeWakeMark
                ) {
                    let claimed = crashed.store.claim_pending_wakes(64).await.expect("claim");
                    assert!(claimed.iter().any(|wake| wake.event_id == pending.decision_event_id));
                }
                if point == CrashPoint::AfterJobBeforeWakeMark {
                    assert!(matches!(
                        crashed
                            .executor
                            .execute_decision(&pending.workspace_id, &pending.decision_event_id)
                            .await
                            .expect("job before crash"),
                        crate::domains::agent::thing_agent::task::approval::ExecutionOutcome::Job(_)
                    ));
                }
            }
            drop(crashed);
            let recovered = fixture.runtime(worker.clone(), provider);
            recovered.recover().await;
            recovered.recover().await;
            let current = crate::domains::agent::thing_agent::approval_port::TaskApprovalStore::get_scoped(
                recovered.approvals.as_ref(),
                WS,
                &pending.task_id,
                &pending.id,
            )
            .await
            .expect("approval")
            .expect("approval row");
            let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
                .bind(&pending.tool_call_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("job count");
            let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = ?")
                .bind(&pending.decision_event_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("wake count");
            if point == CrashPoint::BeforeDecisionCommit {
                assert_eq!(current.state, ApprovalState::Pending);
                assert_eq!(jobs, 0);
                assert_eq!(wakes, 0);
                assert!(worker.submitted.lock().expect("submissions").is_empty());
            } else {
                assert_eq!(current.state, ApprovalState::Approved, "{point:?}");
                assert_eq!(jobs, 1, "{point:?}");
                assert_eq!(wakes, 1, "{point:?}");
                assert_eq!(worker.submitted.lock().expect("submissions").len(), 1, "{point:?}");
                let args: String = sqlx::query_scalar("SELECT args_json FROM device_jobs WHERE tool_call_id = ?")
                    .bind(&pending.tool_call_id)
                    .fetch_one(fixture.db.pool())
                    .await
                    .expect("frozen args");
                assert_eq!(
                    serde_json::from_str::<Value>(&args).expect("args JSON"),
                    pending.args_json
                );
                let events = fixture
                    .db
                    .runtime_events_after(WS, None, Some(&pending.task_id), 100)
                    .await
                    .expect("events");
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.event_type == "approval.approved")
                        .count(),
                    1,
                    "{point:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn existing_active_terminal_and_unknown_jobs_are_not_reenqueued() {
        for state in [
            tinyiothub_core::device_job::DeviceJobState::Accepted,
            tinyiothub_core::device_job::DeviceJobState::Done,
            tinyiothub_core::device_job::DeviceJobState::CancelledUnknown,
        ] {
            let fixture = fixture().await;
            let pending = pending(&fixture).await;
            let worker = Arc::new(RecordingWorker::default());
            let runtime = fixture.runtime(worker.clone(), Arc::new(RuntimeProvider::default()));
            decide(
                &runtime,
                &pending,
                tinyiothub_core::task_approval::ApprovalDecision::Approve,
            )
            .await;
            let job = tinyiothub_core::device_job::DeviceJob {
                id: format!("job-existing-{state:?}"),
                workspace_id: WS.into(),
                device_id: pending.device_id.clone(),
                tool: pending.tool.clone(),
                args_json: pending.args_json.clone(),
                state,
                stop_requested_at: None,
                progress_json: None,
                result_json: None,
                task_id: Some(pending.task_id.clone()),
                run_id: Some(pending.run_id.clone()),
                tool_call_id: Some(pending.tool_call_id.clone()),
                driver_call_id: None,
                timeout_ms: 5_000,
                started_at: Utc::now(),
                ended_at: state.is_terminal().then(Utc::now),
            };
            fixture.db.create_job(&job).await.expect("existing job");
            runtime.recover().await;
            runtime.recover().await;
            assert!(worker.submitted.lock().expect("submissions").is_empty(), "{state:?}");
            let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
                .bind(&pending.tool_call_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("jobs");
            assert_eq!(jobs, 1, "{state:?}");
            let recovered = fixture
                .db
                .get_job(WS, &job.id)
                .await
                .expect("existing Job")
                .expect("existing Job row");
            if state.is_active() {
                assert_eq!(
                    recovered.state,
                    tinyiothub_core::device_job::DeviceJobState::Interrupted
                );
            } else {
                assert_eq!(recovered.state, state);
            }
        }
    }

    #[tokio::test]
    async fn claimed_approved_wake_cancel_race_stays_resolved_without_execution() {
        let fixture = fixture().await;
        let pending = pending(&fixture).await;
        let worker = Arc::new(RecordingWorker::default());
        let runtime = fixture.runtime(worker.clone(), Arc::new(RuntimeProvider::default()));
        decide(
            &runtime,
            &pending,
            tinyiothub_core::task_approval::ApprovalDecision::Approve,
        )
        .await;
        let claimed = runtime.store.claim_pending_wakes(64).await.expect("claim");
        assert!(claimed.iter().any(|wake| wake.event_id == pending.decision_event_id));
        runtime.coordinator.cancel(WS, &pending.task_id).await.expect("cancel");
        assert!(matches!(
            runtime
                .executor
                .execute_decision(&pending.workspace_id, &pending.decision_event_id)
                .await
                .expect("deferred cancelled execution"),
            crate::domains::agent::thing_agent::task::approval::ExecutionOutcome::Deferred
        ));
        runtime
            .store
            .mark_wake(&pending.task_id, &pending.decision_event_id, WakeDispatchState::Pending)
            .await
            .expect("deferred restoration");
        let wake = runtime
            .store
            .wake_by_id(&pending.task_id, &pending.decision_event_id)
            .await
            .expect("wake")
            .expect("wake row");
        assert!(wake.resolved_at.is_some());
        assert!(wake.dispatched_at.is_some());
        assert_eq!(runtime.store.claim_pending_wakes(64).await.expect("reclaim").len(), 0);
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("jobs");
        assert_eq!(jobs, 0);
        assert!(worker.submitted.lock().expect("submissions").is_empty());
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

    async fn wait_for_task_status(
        db: &Db,
        task_id: &str,
        expected: TaskStatus,
    ) -> tinyiothub_core::agent_task::AgentTask {
        for _ in 0..200 {
            let task = db.get_task(WS, task_id).await.expect("task read").expect("task row");
            if task.status == expected {
                return task;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        db.get_task(WS, task_id).await.expect("task read").expect("task row")
    }

    async fn seed_waiting_job_task(fixture: &Fixture, task_id: &str, job_id: &str) {
        let now = Utc::now();
        let task = AgentTask {
            id: task_id.into(),
            workspace_id: WS.into(),
            idempotency_key: format!("{task_id}-key"),
            objective: "observe a terminal device Job".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::WaitingJob,
            current_run_id: None,
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
            .create_task_with_wake(&task, &wake)
            .await
            .expect("waiting Task");
        fixture
            .db
            .create_job(&tinyiothub_core::device_job::DeviceJob {
                id: job_id.into(),
                workspace_id: WS.into(),
                device_id: "device-1".into(),
                tool: "set_level".into(),
                args_json: json!({"level":7}),
                state: tinyiothub_core::device_job::DeviceJobState::Running,
                stop_requested_at: None,
                progress_json: None,
                result_json: None,
                task_id: Some(task.id.clone()),
                run_id: Some("run-origin".into()),
                tool_call_id: Some(format!("call-{task_id}")),
                driver_call_id: None,
                timeout_ms: 5_000,
                started_at: now,
                ended_at: None,
            })
            .await
            .expect("running Job");
    }

    #[tokio::test]
    async fn terminal_job_wakes_start_one_continuation_for_done_stopped_and_interrupted() {
        use tinyiothub_runtime::job::service::JobFinal;

        for (label, final_result, expected_state, expected_result) in [
            (
                "done",
                JobFinal::Done(json!({"level": 7, "persisted": true})),
                "done",
                Some("\"level\":7"),
            ),
            ("stopped", JobFinal::Cancelled { within_grace: true }, "cancelled", None),
            (
                "interrupted",
                JobFinal::Interrupted("device connection lost".into()),
                "interrupted",
                None,
            ),
        ] {
            let fixture = fixture().await;
            let task_id = "task-job";
            let job_id = "job-terminal";
            seed_waiting_job_task(&fixture, task_id, job_id).await;
            let provider = Arc::new(RuntimeProvider::default());
            provider.block_next.store(true, Ordering::SeqCst);
            let runtime = fixture.runtime(Arc::new(RecordingWorker::default()), provider.clone());
            let finished = runtime.jobs.finish(job_id, final_result).await.expect("finish Job");
            assert!(finished.wake_created, "{label}");
            assert_eq!(runtime.router.run(64).await.expect("wake routing"), 1, "{label}");
            tokio::time::timeout(std::time::Duration::from_secs(5), provider.started.notified())
                .await
                .expect("continuation prompt starts");
            let task = fixture
                .db
                .get_task(WS, task_id)
                .await
                .expect("task read")
                .expect("task row");
            assert_eq!(task.status, TaskStatus::Active, "{label}");
            assert!(task.current_run_id.is_some(), "{label}");
            let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs WHERE task_id = ?")
                .bind(task_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("run count");
            assert_eq!(runs, 1, "{label}: exactly one continuation Run");
            let wake = runtime
                .store
                .wake_by_id(task_id, &format!("device_job:{job_id}:{expected_state}"))
                .await
                .expect("wake read")
                .expect("Job wake");
            assert_eq!(wake.dispatch_state, WakeDispatchState::Dispatched, "{label}");
            {
                let prompts = provider.prompts.lock().expect("prompts");
                assert_eq!(prompts.len(), 1, "{label}");
                assert!(prompts[0].contains(job_id), "{label}: {}", prompts[0]);
                assert!(prompts[0].contains(expected_state), "{label}: {}", prompts[0]);
                assert!(
                    prompts[0].contains("Terminal Job result"),
                    "{label}: the continuation must include terminal Job facts"
                );
                if let Some(expected_result) = expected_result {
                    assert!(
                        prompts[0].contains(expected_result),
                        "{label}: the persisted Job result must reach the continuation prompt: {}",
                        prompts[0]
                    );
                }
            }
            provider.release.notify_waiters();
            let completed = wait_for_task_status(&fixture.db, task_id, TaskStatus::Completed).await;
            assert_eq!(completed.status, TaskStatus::Completed, "{label}");
            let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs WHERE task_id = ?")
                .bind(task_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("run count");
            assert_eq!(runs, 1, "{label}: no duplicate continuation Run");
        }
    }

    #[tokio::test]
    async fn approved_decision_waits_on_its_job_and_terminal_wake_continues() {
        use tinyiothub_runtime::job::service::JobFinal;

        let fixture = fixture().await;
        let pending = pending(&fixture).await;
        let worker = Arc::new(RecordingWorker::default());
        let provider = Arc::new(RuntimeProvider::default());
        let runtime = fixture.runtime(worker.clone(), provider.clone());
        decide(
            &runtime,
            &pending,
            tinyiothub_core::task_approval::ApprovalDecision::Approve,
        )
        .await;
        assert_eq!(runtime.router.run(64).await.expect("decision routing"), 1);
        let task = fixture
            .db
            .get_task(WS, &pending.task_id)
            .await
            .expect("task read")
            .expect("task row");
        assert_eq!(task.status, TaskStatus::WaitingJob);
        let jobs = runtime
            .jobs
            .task_jobs(WS, &pending.task_id, false)
            .await
            .expect("task Jobs");
        assert_eq!(jobs.len(), 1, "exactly one approved Job");
        assert_eq!(worker.submitted.lock().expect("submitted").len(), 1);
        let wake = runtime
            .store
            .wake_by_id(&pending.task_id, &pending.decision_event_id)
            .await
            .expect("wake read")
            .expect("decision wake");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Dispatched);

        let job_id = jobs[0].id.clone();
        let finished = runtime
            .jobs
            .finish(&job_id, JobFinal::Done(json!({"level":7})))
            .await
            .expect("finish approved Job");
        assert!(finished.wake_created);
        assert_eq!(runtime.router.run(64).await.expect("Job wake routing"), 1);
        let completed = wait_for_task_status(&fixture.db, &pending.task_id, TaskStatus::Completed).await;
        assert_eq!(completed.status, TaskStatus::Completed);
        assert_eq!(provider.prompts.lock().expect("prompts").len(), 1);
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(jobs, 1, "the continuation must not submit a second Job");
        assert_eq!(worker.submitted.lock().expect("submitted").len(), 1);
    }

    #[tokio::test]
    async fn rejected_and_expired_decisions_continue_without_a_job() {
        for action in ["reject", "expire"] {
            let fixture = fixture().await;
            let pending = pending(&fixture).await;
            let provider = Arc::new(RuntimeProvider::default());
            let runtime = fixture.runtime(Arc::new(RecordingWorker::default()), provider.clone());
            match action {
                "reject" => {
                    decide(
                        &runtime,
                        &pending,
                        tinyiothub_core::task_approval::ApprovalDecision::Reject,
                    )
                    .await;
                }
                "expire" => {
                    crate::domains::agent::thing_agent::approval_port::TaskApprovalStore::expire_due(
                        runtime.approvals.as_ref(),
                        pending.expires_at + chrono::Duration::seconds(1),
                    )
                    .await
                    .expect("expiry");
                }
                _ => unreachable!(),
            }
            assert_eq!(runtime.router.run(64).await.expect("decision routing"), 1, "{action}");
            let completed = wait_for_task_status(&fixture.db, &pending.task_id, TaskStatus::Completed).await;
            assert_eq!(completed.status, TaskStatus::Completed, "{action}");
            assert_eq!(provider.prompts.lock().expect("prompts").len(), 1, "{action}");
            let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
                .bind(&pending.tool_call_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("job count");
            assert_eq!(jobs, 0, "{action}");
        }
    }
}
