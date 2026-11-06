//! Workspace-Harness-backed `TaskStore` adapter (Task 19A).
//!
//! Every fact is read from and written to the owning workspace's Harness
//! SQLite generation. Since Task 19C4 this is the only `TaskStore`
//! implementation: the legacy platform store and its writers are deleted.

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_storage::Db;

#[derive(Clone)]
pub struct HarnessTaskStore {
    db: Arc<Db>,
}

impl HarnessTaskStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl TaskStore for HarnessTaskStore {
    async fn create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError> {
        let outcome = self.db.harness_create_task_with_wake(task, wake).await?;
        tracing::info!(
            metric = "runtime_task_created",
            workspace_id = %task.workspace_id,
            task_id = %outcome.task.id,
            created_by = %outcome.task.created_by,
            deduplicated = outcome.deduplicated,
            "harness runtime task created"
        );
        Ok(outcome)
    }

    async fn get_task(&self, workspace_id: &str, task_id: &str) -> Result<Option<AgentTask>, TaskStoreError> {
        self.db.harness_get_task(workspace_id, task_id).await
    }

    async fn list_tasks(&self, workspace_id: &str, limit: i64, offset: i64) -> Result<Vec<AgentTask>, TaskStoreError> {
        self.db.harness_list_tasks(workspace_id, limit, offset).await
    }

    async fn wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError> {
        self.db.harness_wake_by_id(task_id, event_id).await
    }

    async fn record_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError> {
        self.db.harness_record_wake(wake).await
    }

    async fn resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError> {
        self.db.harness_resolve_task_wakes(task_id).await
    }

    async fn interrupt_unfinished_runs(&self, known_active: &[String]) -> Result<usize, TaskStoreError> {
        self.db.harness_interrupt_unfinished_runs(known_active).await
    }

    async fn transition_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<AgentTask, TaskStoreError> {
        let task = self
            .db
            .harness_transition_task(workspace_id, task_id, expected_version, next, wake)
            .await?;
        tracing::info!(
            metric = "runtime_task_transition",
            workspace_id = %workspace_id,
            task_id = %task_id,
            status = ?task.status,
            state_version = task.state_version,
            "harness runtime task transition"
        );
        Ok(task)
    }

    async fn bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError> {
        self.db
            .harness_bind_current_run(task_id, expected_version, run_id)
            .await
    }

    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError> {
        self.db.harness_begin_run(task, wake_event_id, run_id).await
    }

    async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError> {
        self.db.harness_finish_run(report).await
    }

    async fn latest_task_run(&self, task_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        self.db.harness_latest_task_run(task_id).await
    }

    async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError> {
        self.db.harness_claim_pending_wakes(limit).await
    }

    async fn recover_claimed_wakes(&self) -> Result<usize, TaskStoreError> {
        self.db.harness_recover_claimed_wakes().await
    }

    async fn mark_wake(&self, task_id: &str, event_id: &str, state: WakeDispatchState) -> Result<(), TaskStoreError> {
        self.db.harness_mark_wake(task_id, event_id, state).await
    }

    async fn mark_wake_dead(&self, task_id: &str, event_id: &str, reason: &str) -> Result<(), TaskStoreError> {
        self.db.harness_mark_wake_dead(task_id, event_id, reason).await
    }

    async fn requeue_with_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        self.db.harness_requeue_with_finalize_intent(intent).await
    }

    async fn finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError> {
        self.db.harness_finalize_intent(task_id, event_id).await
    }

    async fn complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError> {
        self.db.harness_complete_finalize_intent(task_id, event_id).await
    }

    async fn put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        self.db.harness_put_finalize_intent(intent).await
    }

    async fn pending_finalize_intents(&self) -> Result<Vec<TaskFinalizeIntent>, TaskStoreError> {
        self.db.harness_pending_finalize_intents().await
    }

    async fn completed_run_report(&self, run_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        self.db.harness_completed_run_report(run_id).await
    }

    async fn count_finalize_intents(&self) -> Result<i64, TaskStoreError> {
        self.db.harness_count_finalize_intents().await
    }

    async fn prune_finalize_intents(&self, older_than: chrono::DateTime<chrono::Utc>) -> Result<usize, TaskStoreError> {
        self.db.harness_prune_finalize_intents(older_than).await
    }

    async fn retry_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, TaskStoreError> {
        self.db.harness_retry_dead_wake(workspace_id, event_id).await
    }

    async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError> {
        self.db.harness_list_nonterminal_tasks(workspace_id).await
    }

    async fn task_effect_wait(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError> {
        self.db
            .task_effect_wait(workspace_id, task_id)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }

    async fn task_dispatch_wait(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError> {
        self.db
            .task_dispatch_wait(workspace_id, task_id)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }

    async fn task_reconciliation_wait(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError> {
        self.db
            .task_reconciliation_wait(workspace_id, task_id)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }
}

const _: fn() = || {
    fn assert_task_store<T: TaskStore>() {}
    assert_task_store::<HarnessTaskStore>();
};
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;
    use tinyiothub_agent::port::task::TaskStore;
    use tinyiothub_core::agent_runs::{Outcome, RunReport};
    use tinyiothub_core::agent_task::{
        AgentTask, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
    };
    use tinyiothub_storage::Db;
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

    use super::HarnessTaskStore;

    const WS: &str = "ws-task-store";

    struct Fixture {
        _workspace: WorkspaceFixture,
        db: Arc<Db>,
        harness: HarnessTaskStore,
    }

    impl Fixture {
        async fn new() -> Self {
            let workspace = fixture_workspace(WS).await;
            let db = Arc::new(workspace.db.clone());
            let harness = HarnessTaskStore::new(db.clone());
            Self {
                _workspace: workspace,
                db,
                harness,
            }
        }

        async fn harness_events(&self, task_id: &str) -> Vec<(String, String, serde_json::Value, Option<String>)> {
            let pool = self.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
                "SELECT event_id, type, payload_json, run_id FROM session_events
                 WHERE task_id = ? AND execution_mode = 'production' ORDER BY seq",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await
            .expect("harness events");
            rows.into_iter()
                .map(|(event_id, event_type, payload, run_id)| {
                    (
                        event_id,
                        event_type,
                        serde_json::from_str(&payload).expect("event payload"),
                        run_id,
                    )
                })
                .collect()
        }

        async fn harness_wake_count(&self) -> i64 {
            let pool = self.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
                .fetch_one(&pool)
                .await
                .expect("harness wake count")
        }
    }

    /// One `Db` with several registered, migrated workspace stores. Used for
    /// cross-workspace ambiguity and degrade-isolation tests.
    struct MultiStoreFixture {
        db: Arc<Db>,
        root: PathBuf,
        workspaces: Vec<String>,
    }

    impl MultiStoreFixture {
        async fn new(workspace_ids: &[&str]) -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-task-{}", uuid::Uuid::new_v4()));
            let pool = tinyiothub_storage::test_helpers::test_pool().await;
            let db = Db::new(pool).with_workspace_store_root(root.join("data/agents"));
            tinyiothub_storage::seed::seed_system(&db).await.expect("seed system");
            for workspace_id in workspace_ids {
                sqlx::query(
                    "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
                     VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
                )
                .bind(workspace_id)
                .execute(db.pool())
                .await
                .expect("insert workspace");
                db.create_workspace_store(workspace_id)
                    .await
                    .expect("create workspace store");
            }
            Self {
                db: Arc::new(db),
                root,
                workspaces: workspace_ids.iter().map(|id| (*id).to_string()).collect(),
            }
        }

        fn harness(&self) -> HarnessTaskStore {
            HarnessTaskStore::new(self.db.clone())
        }

        fn database_path(&self, workspace_id: &str) -> PathBuf {
            self.root
                .join("data/agents")
                .join(workspace_id)
                .join("harness/harness.db")
        }

        async fn close(self) {
            for workspace_id in &self.workspaces {
                let _ = self.db.close_workspace_store(workspace_id).await;
            }
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn sample_task_in(workspace_id: &str, id: &str) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            idempotency_key: format!("key-{workspace_id}-{id}"),
            objective: "inspect pump".to_string(),
            session_key: Some(format!("session-{id}")),
            created_by: "user-1".to_string(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: at(),
            updated_at: at(),
            ended_at: None,
        }
    }

    fn sample_task(id: &str) -> AgentTask {
        sample_task_in(WS, id)
    }

    fn sample_wake_in(workspace_id: &str, task_id: &str, event_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.to_string(),
            event_id: event_id.to_string(),
            signal_json: json!({"workspace_id": workspace_id, "task_id": task_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: at(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn sample_wake(task_id: &str, event_id: &str) -> TaskWake {
        sample_wake_in(WS, task_id, event_id)
    }

    fn sample_report_in(workspace_id: &str, task_id: &str, run_id: &str, wake_id: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            trigger: format!("task:{wake_id}"),
            outcome: Outcome::NoActionNeeded,
            summary: "healthy".to_string(),
            actions: vec![],
            verified: true,
            duration_ms: 12,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some(task_id.to_string()),
            wake_event_id: Some(wake_id.to_string()),
        }
    }

    fn sample_report(task_id: &str, run_id: &str, wake_id: &str) -> RunReport {
        sample_report_in(WS, task_id, run_id, wake_id)
    }

    async fn create_task(fixture: &Fixture, task_id: &str, wake_id: &str) {
        fixture
            .harness
            .create_task_with_wake(&sample_task(task_id), &sample_wake(task_id, wake_id))
            .await
            .expect("create");
    }

    #[tokio::test]
    async fn create_get_list_and_idempotent_replay() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-1", "event-1").await;

        assert_eq!(fixture.harness_wake_count().await, 1);
        let events = fixture.harness_events("task-1").await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "task:task-1:created");
        assert_eq!(events[0].1, "task.created");

        let task = fixture
            .harness
            .get_task(WS, "task-1")
            .await
            .expect("get")
            .expect("task");
        assert_eq!(task, sample_task("task-1"));
        let list = fixture.harness.list_tasks(WS, 10, 0).await.expect("list");
        assert_eq!(list, vec![sample_task("task-1")]);

        let harness_replay = fixture
            .harness
            .create_task_with_wake(&sample_task("task-1"), &sample_wake("task-1", "event-1"))
            .await
            .expect("replay");
        assert!(harness_replay.deduplicated);
        assert_eq!(fixture.harness_wake_count().await, 1);
        assert_eq!(fixture.harness_events("task-1").await.len(), 1);

        let mut conflicting = sample_task("task-1");
        conflicting.objective = "different objective".to_string();
        assert_eq!(
            fixture
                .harness
                .create_task_with_wake(&conflicting, &sample_wake("task-1", "event-1"))
                .await,
            Err(TaskStoreError::ObjectiveConflict)
        );
    }

    #[tokio::test]
    async fn blank_idempotency_key_is_rejected_without_writes() {
        let fixture = Fixture::new().await;
        let mut task = sample_task("task-blank");
        task.idempotency_key = "   ".to_string();
        for _ in 0..2 {
            let error = fixture
                .harness
                .create_task_with_wake(&task, &sample_wake("task-blank", "event-blank"))
                .await
                .expect_err("blank key must be rejected");
            assert!(matches!(error, TaskStoreError::Validation(_)), "{error:?}");
        }
        assert_eq!(fixture.harness_wake_count().await, 0);
    }

    #[tokio::test]
    async fn wake_claim_mark_recover_and_retry() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-wake", "event-wake").await;

        let claimed = fixture.harness.claim_pending_wakes(10).await.expect("claim");
        assert_eq!(claimed.len(), 1);
        assert!(claimed[0].dispatched_at.is_some());
        assert!(
            fixture
                .harness
                .claim_pending_wakes(10)
                .await
                .expect("re-claim")
                .is_empty()
        );

        fixture
            .harness
            .mark_wake("task-wake", "event-wake", WakeDispatchState::Dispatched)
            .await
            .expect("dispatch");
        fixture
            .harness
            .mark_wake_dead("task-wake", "event-wake", "provider unavailable")
            .await
            .expect("dead");
        let dead = fixture
            .harness
            .wake_by_id("task-wake", "event-wake")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dead.dispatch_state, WakeDispatchState::Dead);

        assert!(fixture.harness.retry_dead_wake(WS, "event-wake").await.expect("retry"));
        assert!(
            !fixture
                .harness
                .retry_dead_wake(WS, "event-wake")
                .await
                .expect("re-retry")
        );

        assert_eq!(fixture.harness.recover_claimed_wakes().await.unwrap(), 0);
        assert_eq!(fixture.harness.claim_pending_wakes(10).await.unwrap().len(), 1);
        assert_eq!(fixture.harness.recover_claimed_wakes().await.unwrap(), 1);
        assert_eq!(fixture.harness.resolve_task_wakes("task-wake").await.unwrap(), 1);
        assert!(fixture.harness.claim_pending_wakes(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_lifecycle() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-run", "event-run").await;

        let task = fixture.harness.get_task(WS, "task-run").await.unwrap().unwrap();
        assert!(fixture.harness.begin_run(&task, "event-run", "run-1").await.unwrap());
        assert!(!fixture.harness.begin_run(&task, "event-run", "run-1").await.unwrap());

        let started = fixture.harness.get_task(WS, "task-run").await.unwrap().unwrap();
        assert_eq!(started.current_run_id.as_deref(), Some("run-1"));

        let report = sample_report("task-run", "run-1", "event-run");
        fixture.harness.finish_run(&report).await.expect("finish");
        assert_eq!(
            serde_json::to_value(fixture.harness.latest_task_run("task-run").await.unwrap()).unwrap(),
            serde_json::to_value(&report).unwrap()
        );
        assert_eq!(
            serde_json::to_value(fixture.harness.completed_run_report("run-1").await.unwrap()).unwrap(),
            serde_json::to_value(&report).unwrap()
        );
        assert_eq!(
            fixture.harness.finish_run(&report).await,
            Err(TaskStoreError::ConcurrentUpdate)
        );

        let events = fixture.harness_events("task-run").await;
        let types: Vec<&str> = events.iter().map(|event| event.1.as_str()).collect();
        assert_eq!(types, ["task.created", "run.started", "run.finished"]);
    }

    #[tokio::test]
    async fn transitions_binding_recovery_and_nonterminal_listing() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-move", "event-move").await;

        assert_eq!(
            fixture
                .harness
                .transition_task(WS, "task-move", 9, TaskStatus::Paused, None)
                .await,
            Err(TaskStoreError::ConcurrentUpdate)
        );

        let bound = fixture
            .harness
            .bind_current_run("task-move", 0, Some("run-bound"))
            .await
            .unwrap();
        assert_eq!(bound.current_run_id.as_deref(), Some("run-bound"));

        let paused = fixture
            .harness
            .transition_task(WS, "task-move", 1, TaskStatus::Paused, None)
            .await
            .unwrap();
        assert_eq!(paused.status, TaskStatus::Paused);
        fixture
            .harness
            .transition_task(WS, "task-move", 2, TaskStatus::Active, None)
            .await
            .expect("resume");
        let done = fixture
            .harness
            .transition_task(WS, "task-move", 3, TaskStatus::Completed, None)
            .await
            .unwrap();
        assert!(done.ended_at.is_some());

        create_task(&fixture, "task-crash", "event-crash").await;
        let crash = fixture.harness.get_task(WS, "task-crash").await.unwrap().unwrap();
        fixture
            .harness
            .begin_run(&crash, "event-crash", "run-crash")
            .await
            .unwrap();
        assert_eq!(fixture.harness.interrupt_unfinished_runs(&[]).await.unwrap(), 1);
        assert_eq!(fixture.harness.interrupt_unfinished_runs(&[]).await.unwrap(), 0);

        let active = fixture.harness.list_nonterminal_tasks(Some(WS)).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "task-crash");
        assert_eq!(active[0].status, TaskStatus::Active);
    }

    #[tokio::test]
    async fn finalize_intents_and_retention() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-finalize", "event-finalize").await;
        let intent = TaskFinalizeIntent {
            task_id: "task-finalize".to_string(),
            workspace_id: WS.to_string(),
            wake_event_id: "event-finalize".to_string(),
            run_id: "run-finalize".to_string(),
            report: sample_report("task-finalize", "run-finalize", "event-finalize"),
            report_persisted: false,
            report_emitted: false,
            undelivered: vec!["provider output".to_string()],
        };

        fixture
            .harness
            .requeue_with_finalize_intent(&intent)
            .await
            .expect("requeue");
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 1);
        assert_eq!(fixture.harness.pending_finalize_intents().await.unwrap().len(), 1);

        let mut updated = intent.clone();
        updated.report.summary = "retried".to_string();
        fixture.harness.put_finalize_intent(&updated).await.expect("put");
        assert_eq!(
            fixture
                .harness
                .finalize_intent("task-finalize", "event-finalize")
                .await
                .unwrap()
                .unwrap()
                .report
                .summary,
            "retried"
        );

        fixture
            .harness
            .complete_finalize_intent("task-finalize", "event-finalize")
            .await
            .expect("complete");
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 0);

        let retention_intent = TaskFinalizeIntent {
            report_persisted: true,
            report_emitted: true,
            undelivered: vec![],
            ..intent.clone()
        };
        fixture
            .harness
            .put_finalize_intent(&retention_intent)
            .await
            .expect("retention intent");
        fixture
            .harness
            .resolve_task_wakes("task-finalize")
            .await
            .expect("resolve");
        fixture
            .harness
            .transition_task(WS, "task-finalize", 0, TaskStatus::Completed, None)
            .await
            .expect("terminal");
        let cutoff = Utc::now() + Duration::days(1);
        assert_eq!(fixture.harness.prune_finalize_intents(cutoff).await.unwrap(), 1);
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn record_wake_and_wake_lookup() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-record", "event-record").await;
        let extra = sample_wake("task-record", "event-extra");
        assert!(fixture.harness.record_wake(&extra).await.unwrap());
        assert!(!fixture.harness.record_wake(&extra).await.unwrap());
        assert_eq!(fixture.harness_wake_count().await, 2);
        let wake = fixture
            .harness
            .wake_by_id("task-record", "event-extra")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wake.event_id, "event-extra");
    }

    #[tokio::test]
    async fn idempotency_conflicts_are_scoped_to_the_key() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-key", "event-key").await;

        let mut replay = sample_task("task-key-new-id");
        replay.idempotency_key = sample_task("task-key").idempotency_key;
        let replayed = fixture
            .harness
            .create_task_with_wake(&replay, &sample_wake("task-key-new-id", "event-key-new"))
            .await
            .expect("key replay");
        assert!(replayed.deduplicated);
        assert_eq!(replayed.task.id, "task-key");

        let mut conflicting = sample_task("task-key");
        conflicting.idempotency_key = "key-other".to_string();
        let error = fixture
            .harness
            .create_task_with_wake(&conflicting, &sample_wake("task-key", "event-other"))
            .await
            .expect_err("same-id conflict");
        assert!(matches!(error, TaskStoreError::Unavailable(_)), "{error:?}");
        assert_eq!(fixture.harness_wake_count().await, 1);
    }

    #[tokio::test]
    async fn concurrent_transitions_serialize_on_the_first_write() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-race", "event-race").await;
        let store = Arc::new(fixture.harness.clone());
        let (left, right) = tokio::join!(
            store.transition_task(WS, "task-race", 0, TaskStatus::Paused, None),
            store.transition_task(WS, "task-race", 0, TaskStatus::Paused, None),
        );
        let results = [left, right];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(TaskStoreError::ConcurrentUpdate)))
                .count(),
            1
        );
        let events = fixture.harness_events("task-race").await;
        assert_eq!(events.len(), 2, "exactly one transition event commits");
        assert_eq!(events[1].1, "task.paused");
    }

    #[tokio::test]
    async fn same_task_id_in_two_workspaces_is_refused() {
        let fixture = MultiStoreFixture::new(&["ws-amb-a", "ws-amb-b"]).await;
        let store = fixture.harness();
        for workspace_id in ["ws-amb-a", "ws-amb-b"] {
            store
                .create_task_with_wake(
                    &sample_task_in(workspace_id, "task-dup"),
                    &sample_wake_in(workspace_id, "task-dup", "event-dup"),
                )
                .await
                .expect("create per workspace");
            assert!(
                store
                    .get_task(workspace_id, "task-dup")
                    .await
                    .expect("scoped read")
                    .is_some()
            );
        }

        assert!(matches!(
            store.bind_current_run("task-dup", 0, Some("run-dup")).await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.wake_by_id("task-dup", "event-dup").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.resolve_task_wakes("task-dup").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.latest_task_run("task-dup").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.finalize_intent("task-dup", "event-dup").await,
            Err(TaskStoreError::Unavailable(_))
        ));

        for workspace_id in ["ws-amb-a", "ws-amb-b"] {
            let pool = fixture
                .db
                .workspace_pool_for_testing(workspace_id)
                .await
                .expect("workspace pool");
            let resolved: Option<String> =
                sqlx::query_scalar("SELECT resolved_at FROM task_wake_outbox WHERE task_id = 'task-dup'")
                    .fetch_one(&pool)
                    .await
                    .expect("wake row");
            assert!(resolved.is_none(), "{workspace_id} wake must stay unresolved");
        }
        fixture.close().await;
    }

    async fn corrupt_workspace(fixture: &MultiStoreFixture, workspace_id: &str) {
        fixture
            .db
            .close_workspace_store(workspace_id)
            .await
            .expect("close workspace to corrupt");
        let path = fixture.database_path(workspace_id);
        tokio::fs::write(&path, b"not a sqlite database")
            .await
            .expect("corrupt store");
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = tokio::fs::remove_file(sidecar).await;
        }
    }

    #[tokio::test]
    async fn unreadable_workspace_is_skipped_by_runtime_sweeps() {
        let fixture = MultiStoreFixture::new(&["ws-good", "ws-bad"]).await;
        let store = fixture.harness();
        store
            .create_task_with_wake(
                &sample_task_in("ws-good", "task-good"),
                &sample_wake_in("ws-good", "task-good", "event-good"),
            )
            .await
            .expect("good task");
        let intent = TaskFinalizeIntent {
            task_id: "task-good".to_string(),
            workspace_id: "ws-good".to_string(),
            wake_event_id: "event-good".to_string(),
            run_id: "run-good".to_string(),
            report: sample_report_in("ws-good", "task-good", "run-good", "event-good"),
            report_persisted: false,
            report_emitted: false,
            undelivered: vec!["retained".to_string()],
        };
        store.put_finalize_intent(&intent).await.expect("retained intent");
        corrupt_workspace(&fixture, "ws-bad").await;

        let claimed = store.claim_pending_wakes(10).await.expect("claim degrades");
        assert_eq!(claimed.len(), 1, "good workspace still claims");
        assert!(store.recover_claimed_wakes().await.is_ok());
        assert_eq!(store.pending_finalize_intents().await.expect("pending").len(), 1);
        assert_eq!(store.count_finalize_intents().await.expect("count"), 1);
        assert!(
            store
                .prune_finalize_intents(Utc::now() - Duration::days(1))
                .await
                .is_ok()
        );
        assert!(store.interrupt_unfinished_runs(&[]).await.is_ok());
        assert_eq!(store.list_nonterminal_tasks(None).await.expect("list").len(), 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn unreadable_owning_workspace_fails_point_lookups_closed() {
        let fixture = MultiStoreFixture::new(&["ws-only"]).await;
        let store = fixture.harness();
        store
            .create_task_with_wake(
                &sample_task_in("ws-only", "task-only"),
                &sample_wake_in("ws-only", "task-only", "event-only"),
            )
            .await
            .expect("create");
        let task = store.get_task("ws-only", "task-only").await.unwrap().unwrap();
        store
            .begin_run(&task, "event-only", "run-only")
            .await
            .expect("begin run");
        corrupt_workspace(&fixture, "ws-only").await;

        assert!(matches!(
            store.get_task("ws-only", "task-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.bind_current_run("task-only", 0, Some("run-next")).await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.wake_by_id("task-only", "event-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.resolve_task_wakes("task-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.latest_task_run("task-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.finalize_intent("task-only", "event-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store
                .mark_wake("task-only", "event-only", WakeDispatchState::Dispatched)
                .await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.completed_run_report("run-only").await,
            Err(TaskStoreError::Unavailable(_))
        ));

        // Sweeps keep the per-workspace skip and must stay Ok.
        assert!(store.claim_pending_wakes(10).await.expect("claim").is_empty());
        assert!(store.recover_claimed_wakes().await.is_ok());
        assert!(store.pending_finalize_intents().await.is_ok());
        assert_eq!(store.count_finalize_intents().await.expect("count"), 0);
        assert!(
            store
                .prune_finalize_intents(Utc::now() - Duration::days(1))
                .await
                .is_ok()
        );
        assert!(store.interrupt_unfinished_runs(&[]).await.is_ok());
        assert!(store.list_nonterminal_tasks(None).await.is_ok());
        fixture.close().await;
    }

    #[tokio::test]
    async fn mutation_is_refused_when_a_duplicate_workspace_is_unreadable() {
        let fixture = MultiStoreFixture::new(&["ws-readable", "ws-shadow"]).await;
        let store = fixture.harness();
        for workspace_id in ["ws-readable", "ws-shadow"] {
            store
                .create_task_with_wake(
                    &sample_task_in(workspace_id, "task-shadow"),
                    &sample_wake_in(workspace_id, "task-shadow", "event-shadow"),
                )
                .await
                .expect("create duplicate");
        }
        corrupt_workspace(&fixture, "ws-shadow").await;

        let wake = store
            .wake_by_id("task-shadow", "event-shadow")
            .await
            .expect("read lookup")
            .expect("readable match");
        assert_eq!(wake.task_id, "task-shadow");

        assert!(matches!(
            store.bind_current_run("task-shadow", 0, Some("run-shadow")).await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.resolve_task_wakes("task-shadow").await,
            Err(TaskStoreError::Unavailable(_))
        ));
        assert!(matches!(
            store
                .mark_wake("task-shadow", "event-shadow", WakeDispatchState::Dispatched)
                .await,
            Err(TaskStoreError::Unavailable(_))
        ));
        let pool = fixture
            .db
            .workspace_pool_for_testing("ws-readable")
            .await
            .expect("readable pool");
        let resolved: Option<String> =
            sqlx::query_scalar("SELECT resolved_at FROM task_wake_outbox WHERE task_id = 'task-shadow'")
                .fetch_one(&pool)
                .await
                .expect("wake row");
        assert!(resolved.is_none(), "refused mutation must not resolve the wake");
        fixture.close().await;
    }

    #[tokio::test]
    async fn dedup_replay_after_transition_keeps_the_created_event() {
        let fixture = Fixture::new().await;
        create_task(&fixture, "task-created", "event-created").await;
        fixture
            .harness
            .transition_task(WS, "task-created", 0, TaskStatus::Paused, None)
            .await
            .expect("transition");

        let replay = fixture
            .harness
            .create_task_with_wake(
                &sample_task("task-created"),
                &sample_wake("task-created", "event-created"),
            )
            .await
            .expect("replay after transition");
        assert!(replay.deduplicated);
        assert_eq!(replay.task.status, TaskStatus::Paused);
        assert_eq!(replay.task.state_version, 1);

        let events = fixture.harness_events("task-created").await;
        assert_eq!(events.len(), 2, "dedup must not append a new event");
        assert_eq!(events[0].0, "task:task-created:created");
        assert_eq!(events[0].2["stateVersion"], 0, "created payload stays at v0");
    }
}
