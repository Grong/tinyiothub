//! Workspace-Harness-backed `TaskStore` adapter (Task 19A).
//!
//! This adapter has the same port contract as [`super::DbTaskStore`] but every
//! fact is read from and written to the owning workspace's Harness SQLite
//! generation. It is registered alongside the legacy store so Task 19B can
//! swap the runtime wiring without changing handlers in the same step.

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
}

const _: fn() = || {
    fn assert_task_store<T: TaskStore>() {}
    assert_task_store::<HarnessTaskStore>();
};

#[cfg(test)]
mod tests {
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
    use crate::domains::agent::runtime::DbTaskStore;

    const WS: &str = "ws-task-store-parity";

    struct ParityFixture {
        _workspace: WorkspaceFixture,
        db: Arc<Db>,
        legacy: DbTaskStore,
        harness: HarnessTaskStore,
    }

    impl ParityFixture {
        async fn new() -> Self {
            let workspace = fixture_workspace(WS).await;
            let db = Arc::new(workspace.db.clone());
            let legacy = DbTaskStore::new(db.clone());
            let harness = HarnessTaskStore::new(db.clone());
            Self {
                _workspace: workspace,
                db,
                legacy,
                harness,
            }
        }

        async fn legacy_events(&self, task_id: &str) -> Vec<(String, String)> {
            self.db
                .runtime_events_after(WS, None, Some(task_id), 200)
                .await
                .expect("legacy events")
                .into_iter()
                .map(|event| (event.event_id, event.event_type))
                .collect()
        }

        async fn harness_events(&self, task_id: &str) -> Vec<(String, String)> {
            let pool = self.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            sqlx::query_as::<_, (String, String)>(
                "SELECT event_id, type FROM session_events
                 WHERE task_id = ? AND execution_mode = 'production' ORDER BY seq",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await
            .expect("harness events")
        }

        async fn assert_same_events(&self, task_id: &str) {
            assert_eq!(
                self.legacy_events(task_id).await,
                self.harness_events(task_id).await,
                "task {task_id}: event identity and ordering must match"
            );
        }

        async fn legacy_wake_count(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
                .fetch_one(self.db.pool())
                .await
                .expect("legacy wake count")
        }

        async fn harness_wake_count(&self) -> i64 {
            let pool = self.db.workspace_pool_for_testing(WS).await.expect("workspace pool");
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
                .fetch_one(&pool)
                .await
                .expect("harness wake count")
        }
    }

    /// Ops set the wall-clock `updated_at`/`ended_at` independently, so parity
    /// compares every durable field and only the presence of those timestamps.
    fn assert_task_parity(left: &AgentTask, right: &AgentTask) {
        assert_eq!(left.id, right.id);
        assert_eq!(left.workspace_id, right.workspace_id);
        assert_eq!(left.idempotency_key, right.idempotency_key);
        assert_eq!(left.objective, right.objective);
        assert_eq!(left.session_key, right.session_key);
        assert_eq!(left.created_by, right.created_by);
        assert_eq!(left.status, right.status);
        assert_eq!(left.current_run_id, right.current_run_id);
        assert_eq!(left.state_version, right.state_version);
        assert_eq!(left.created_at, right.created_at);
        assert_eq!(left.ended_at.is_some(), right.ended_at.is_some());
    }

    fn assert_wake_parity(left: &TaskWake, right: &TaskWake) {
        assert_eq!(left.task_id, right.task_id);
        assert_eq!(left.event_id, right.event_id);
        assert_eq!(left.signal_json, right.signal_json);
        assert_eq!(left.dispatch_state, right.dispatch_state);
        assert_eq!(left.created_at, right.created_at);
        assert_eq!(left.dispatched_at.is_some(), right.dispatched_at.is_some());
        assert_eq!(left.resolved_at.is_some(), right.resolved_at.is_some());
    }

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn sample_task(id: &str) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            workspace_id: WS.to_string(),
            idempotency_key: format!("key-{id}"),
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

    fn sample_wake(task_id: &str, event_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.to_string(),
            event_id: event_id.to_string(),
            signal_json: json!({"workspace_id": WS, "task_id": task_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: at(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn sample_report(task_id: &str, run_id: &str, wake_id: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: WS.to_string(),
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

    async fn create_both(fixture: &ParityFixture, task_id: &str, wake_id: &str) {
        let task = sample_task(task_id);
        let wake = sample_wake(task_id, wake_id);
        let legacy = fixture
            .legacy
            .create_task_with_wake(&task, &wake)
            .await
            .expect("legacy create");
        let harness = fixture
            .harness
            .create_task_with_wake(&task, &wake)
            .await
            .expect("harness create");
        assert_eq!(legacy.deduplicated, harness.deduplicated);
        assert_eq!(legacy.task, harness.task);
    }

    #[tokio::test]
    async fn create_get_list_and_idempotent_replay_match_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-1", "event-1").await;

        assert_eq!(fixture.legacy_wake_count().await, 1);
        assert_eq!(fixture.harness_wake_count().await, 1);
        fixture.assert_same_events("task-1").await;

        let legacy = fixture.legacy.get_task(WS, "task-1").await.expect("legacy get");
        let harness = fixture.harness.get_task(WS, "task-1").await.expect("harness get");
        assert_eq!(legacy, harness);

        let legacy_list = fixture.legacy.list_tasks(WS, 10, 0).await.expect("legacy list");
        let harness_list = fixture.harness.list_tasks(WS, 10, 0).await.expect("harness list");
        assert_eq!(legacy_list, harness_list);

        let replay_task = sample_task("task-1");
        let replay_wake = sample_wake("task-1", "event-1");
        let legacy_replay = fixture
            .legacy
            .create_task_with_wake(&replay_task, &replay_wake)
            .await
            .expect("legacy replay");
        let harness_replay = fixture
            .harness
            .create_task_with_wake(&replay_task, &replay_wake)
            .await
            .expect("harness replay");
        assert!(legacy_replay.deduplicated && harness_replay.deduplicated);
        assert_eq!(fixture.legacy_wake_count().await, 1);
        assert_eq!(fixture.harness_wake_count().await, 1);
        fixture.assert_same_events("task-1").await;

        let mut conflicting = sample_task("task-1");
        conflicting.objective = "different objective".to_string();
        assert_eq!(
            fixture.legacy.create_task_with_wake(&conflicting, &replay_wake).await,
            Err(TaskStoreError::ObjectiveConflict)
        );
        assert_eq!(
            fixture.harness.create_task_with_wake(&conflicting, &replay_wake).await,
            Err(TaskStoreError::ObjectiveConflict)
        );
    }

    #[tokio::test]
    async fn wake_claim_mark_recover_and_retry_match_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-wake", "event-wake").await;

        let legacy_claimed = fixture.legacy.claim_pending_wakes(10).await.expect("legacy claim");
        let harness_claimed = fixture.harness.claim_pending_wakes(10).await.expect("harness claim");
        assert_eq!(legacy_claimed.len(), 1);
        assert_eq!(harness_claimed.len(), 1);
        assert_wake_parity(&legacy_claimed[0], &harness_claimed[0]);
        assert!(harness_claimed[0].dispatched_at.is_some());

        assert!(
            fixture
                .legacy
                .claim_pending_wakes(10)
                .await
                .expect("legacy re-claim")
                .is_empty()
        );
        assert!(
            fixture
                .harness
                .claim_pending_wakes(10)
                .await
                .expect("harness re-claim")
                .is_empty()
        );

        fixture
            .legacy
            .mark_wake("task-wake", "event-wake", WakeDispatchState::Dispatched)
            .await
            .expect("legacy dispatch");
        fixture
            .harness
            .mark_wake("task-wake", "event-wake", WakeDispatchState::Dispatched)
            .await
            .expect("harness dispatch");
        assert_wake_parity(
            &fixture
                .legacy
                .wake_by_id("task-wake", "event-wake")
                .await
                .unwrap()
                .unwrap(),
            &fixture
                .harness
                .wake_by_id("task-wake", "event-wake")
                .await
                .unwrap()
                .unwrap(),
        );

        fixture
            .legacy
            .mark_wake_dead("task-wake", "event-wake", "provider unavailable")
            .await
            .expect("legacy dead");
        fixture
            .harness
            .mark_wake_dead("task-wake", "event-wake", "provider unavailable")
            .await
            .expect("harness dead");
        let legacy_dead = fixture
            .legacy
            .wake_by_id("task-wake", "event-wake")
            .await
            .unwrap()
            .unwrap();
        let harness_dead = fixture
            .harness
            .wake_by_id("task-wake", "event-wake")
            .await
            .unwrap()
            .unwrap();
        assert_wake_parity(&legacy_dead, &harness_dead);
        assert_eq!(harness_dead.dispatch_state, WakeDispatchState::Dead);

        assert!(
            fixture
                .legacy
                .retry_dead_wake(WS, "event-wake")
                .await
                .expect("legacy retry")
        );
        assert!(
            fixture
                .harness
                .retry_dead_wake(WS, "event-wake")
                .await
                .expect("harness retry")
        );
        assert_wake_parity(
            &fixture
                .legacy
                .wake_by_id("task-wake", "event-wake")
                .await
                .unwrap()
                .unwrap(),
            &fixture
                .harness
                .wake_by_id("task-wake", "event-wake")
                .await
                .unwrap()
                .unwrap(),
        );
        assert!(
            !fixture
                .legacy
                .retry_dead_wake(WS, "event-wake")
                .await
                .expect("legacy re-retry")
        );
        assert!(
            !fixture
                .harness
                .retry_dead_wake(WS, "event-wake")
                .await
                .expect("harness re-retry")
        );

        assert_eq!(fixture.legacy.recover_claimed_wakes().await.unwrap(), 0);
        assert_eq!(fixture.harness.recover_claimed_wakes().await.unwrap(), 0);
        assert_eq!(fixture.legacy.claim_pending_wakes(10).await.unwrap().len(), 1);
        assert_eq!(fixture.harness.claim_pending_wakes(10).await.unwrap().len(), 1);
        assert_eq!(fixture.legacy.recover_claimed_wakes().await.unwrap(), 1);
        assert_eq!(fixture.harness.recover_claimed_wakes().await.unwrap(), 1);

        assert_eq!(fixture.legacy.resolve_task_wakes("task-wake").await.unwrap(), 1);
        assert_eq!(fixture.harness.resolve_task_wakes("task-wake").await.unwrap(), 1);
        assert!(fixture.legacy.claim_pending_wakes(10).await.unwrap().is_empty());
        assert!(fixture.harness.claim_pending_wakes(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_lifecycle_matches_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-run", "event-run").await;

        let legacy_task = fixture.legacy.get_task(WS, "task-run").await.unwrap().unwrap();
        let harness_task = fixture.harness.get_task(WS, "task-run").await.unwrap().unwrap();
        assert!(
            fixture
                .legacy
                .begin_run(&legacy_task, "event-run", "run-1")
                .await
                .unwrap()
        );
        assert!(
            fixture
                .harness
                .begin_run(&harness_task, "event-run", "run-1")
                .await
                .unwrap()
        );
        assert!(
            !fixture
                .legacy
                .begin_run(&legacy_task, "event-run", "run-1")
                .await
                .unwrap()
        );
        assert!(
            !fixture
                .harness
                .begin_run(&harness_task, "event-run", "run-1")
                .await
                .unwrap()
        );

        let legacy_started = fixture.legacy.get_task(WS, "task-run").await.unwrap().unwrap();
        let harness_started = fixture.harness.get_task(WS, "task-run").await.unwrap().unwrap();
        assert_task_parity(&legacy_started, &harness_started);
        assert_eq!(harness_started.current_run_id.as_deref(), Some("run-1"));

        let report = sample_report("task-run", "run-1", "event-run");
        fixture.legacy.finish_run(&report).await.expect("legacy finish");
        fixture.harness.finish_run(&report).await.expect("harness finish");
        assert_eq!(
            serde_json::to_value(fixture.legacy.latest_task_run("task-run").await.unwrap()).unwrap(),
            serde_json::to_value(fixture.harness.latest_task_run("task-run").await.unwrap()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(fixture.legacy.completed_run_report("run-1").await.unwrap()).unwrap(),
            serde_json::to_value(fixture.harness.completed_run_report("run-1").await.unwrap()).unwrap()
        );
        assert_eq!(
            fixture.legacy.finish_run(&report).await,
            Err(TaskStoreError::ConcurrentUpdate)
        );
        assert_eq!(
            fixture.harness.finish_run(&report).await,
            Err(TaskStoreError::ConcurrentUpdate)
        );
        fixture.assert_same_events("task-run").await;
    }

    #[tokio::test]
    async fn transitions_binding_recovery_and_nonterminal_listing_match_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-move", "event-move").await;

        assert_eq!(
            fixture
                .legacy
                .transition_task(WS, "task-move", 9, TaskStatus::Paused, None)
                .await,
            Err(TaskStoreError::ConcurrentUpdate)
        );
        assert_eq!(
            fixture
                .harness
                .transition_task(WS, "task-move", 9, TaskStatus::Paused, None)
                .await,
            Err(TaskStoreError::ConcurrentUpdate)
        );

        let legacy_bound = fixture
            .legacy
            .bind_current_run("task-move", 0, Some("run-bound"))
            .await
            .unwrap();
        let harness_bound = fixture
            .harness
            .bind_current_run("task-move", 0, Some("run-bound"))
            .await
            .unwrap();
        assert_task_parity(&legacy_bound, &harness_bound);

        let legacy_paused = fixture
            .legacy
            .transition_task(WS, "task-move", 1, TaskStatus::Paused, None)
            .await
            .unwrap();
        let harness_paused = fixture
            .harness
            .transition_task(WS, "task-move", 1, TaskStatus::Paused, None)
            .await
            .unwrap();
        assert_task_parity(&legacy_paused, &harness_paused);

        fixture
            .legacy
            .transition_task(WS, "task-move", 2, TaskStatus::Active, None)
            .await
            .expect("legacy resume");
        fixture
            .harness
            .transition_task(WS, "task-move", 2, TaskStatus::Active, None)
            .await
            .expect("harness resume");

        let legacy_done = fixture
            .legacy
            .transition_task(WS, "task-move", 3, TaskStatus::Completed, None)
            .await
            .unwrap();
        let harness_done = fixture
            .harness
            .transition_task(WS, "task-move", 3, TaskStatus::Completed, None)
            .await
            .unwrap();
        assert_task_parity(&legacy_done, &harness_done);
        assert!(harness_done.ended_at.is_some());
        fixture.assert_same_events("task-move").await;

        create_both(&fixture, "task-crash", "event-crash").await;
        let legacy_crash = fixture.legacy.get_task(WS, "task-crash").await.unwrap().unwrap();
        let harness_crash = fixture.harness.get_task(WS, "task-crash").await.unwrap().unwrap();
        fixture
            .legacy
            .begin_run(&legacy_crash, "event-crash", "run-crash")
            .await
            .unwrap();
        fixture
            .harness
            .begin_run(&harness_crash, "event-crash", "run-crash")
            .await
            .unwrap();
        assert_eq!(fixture.legacy.interrupt_unfinished_runs(&[]).await.unwrap(), 1);
        assert_eq!(fixture.harness.interrupt_unfinished_runs(&[]).await.unwrap(), 1);
        assert_eq!(fixture.legacy.interrupt_unfinished_runs(&[]).await.unwrap(), 0);
        assert_eq!(fixture.harness.interrupt_unfinished_runs(&[]).await.unwrap(), 0);
        fixture.assert_same_events("task-crash").await;

        let legacy_active = fixture.legacy.list_nonterminal_tasks(Some(WS)).await.unwrap();
        let harness_active = fixture.harness.list_nonterminal_tasks(Some(WS)).await.unwrap();
        assert_eq!(legacy_active.len(), harness_active.len());
        for (legacy_task, harness_task) in legacy_active.iter().zip(harness_active.iter()) {
            assert_task_parity(legacy_task, harness_task);
        }
        assert_eq!(
            harness_active
                .iter()
                .find(|task| task.id == "task-crash")
                .map(|task| task.status),
            Some(TaskStatus::Active)
        );
        let all_legacy = fixture.legacy.list_nonterminal_tasks(None).await.unwrap();
        let all_harness = fixture.harness.list_nonterminal_tasks(None).await.unwrap();
        assert_eq!(all_legacy.len(), all_harness.len());
    }

    #[tokio::test]
    async fn finalize_intents_and_retention_match_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-finalize", "event-finalize").await;
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
            .legacy
            .requeue_with_finalize_intent(&intent)
            .await
            .expect("legacy requeue");
        fixture
            .harness
            .requeue_with_finalize_intent(&intent)
            .await
            .expect("harness requeue");
        assert_eq!(
            serde_json::to_value(
                fixture
                    .legacy
                    .finalize_intent("task-finalize", "event-finalize")
                    .await
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(
                fixture
                    .harness
                    .finalize_intent("task-finalize", "event-finalize")
                    .await
                    .unwrap()
            )
            .unwrap()
        );
        assert_eq!(
            serde_json::to_value(fixture.legacy.pending_finalize_intents().await.unwrap()).unwrap(),
            serde_json::to_value(fixture.harness.pending_finalize_intents().await.unwrap()).unwrap()
        );
        assert_eq!(fixture.legacy.count_finalize_intents().await.unwrap(), 1);
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 1);

        let mut updated = intent.clone();
        updated.report.summary = "retried".to_string();
        fixture.legacy.put_finalize_intent(&updated).await.expect("legacy put");
        fixture
            .harness
            .put_finalize_intent(&updated)
            .await
            .expect("harness put");
        assert_eq!(
            fixture
                .legacy
                .finalize_intent("task-finalize", "event-finalize")
                .await
                .unwrap()
                .unwrap()
                .report
                .summary,
            "retried"
        );
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
            .legacy
            .complete_finalize_intent("task-finalize", "event-finalize")
            .await
            .expect("legacy complete");
        fixture
            .harness
            .complete_finalize_intent("task-finalize", "event-finalize")
            .await
            .expect("harness complete");
        assert_eq!(fixture.legacy.count_finalize_intents().await.unwrap(), 0);
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 0);

        let retention_intent = TaskFinalizeIntent {
            task_id: "task-finalize".to_string(),
            workspace_id: WS.to_string(),
            wake_event_id: "event-finalize".to_string(),
            run_id: "run-finalize".to_string(),
            report: sample_report("task-finalize", "run-finalize", "event-finalize"),
            report_persisted: true,
            report_emitted: true,
            undelivered: vec![],
        };
        fixture
            .legacy
            .put_finalize_intent(&retention_intent)
            .await
            .expect("legacy retention intent");
        fixture
            .harness
            .put_finalize_intent(&retention_intent)
            .await
            .expect("harness retention intent");
        fixture
            .legacy
            .resolve_task_wakes("task-finalize")
            .await
            .expect("legacy resolve");
        fixture
            .harness
            .resolve_task_wakes("task-finalize")
            .await
            .expect("harness resolve");
        fixture
            .legacy
            .transition_task(WS, "task-finalize", 0, TaskStatus::Completed, None)
            .await
            .expect("legacy terminal");
        fixture
            .harness
            .transition_task(WS, "task-finalize", 0, TaskStatus::Completed, None)
            .await
            .expect("harness terminal");
        let cutoff = Utc::now() + Duration::days(1);
        assert_eq!(fixture.legacy.prune_finalize_intents(cutoff).await.unwrap(), 1);
        assert_eq!(fixture.harness.prune_finalize_intents(cutoff).await.unwrap(), 1);
        assert_eq!(fixture.legacy.count_finalize_intents().await.unwrap(), 0);
        assert_eq!(fixture.harness.count_finalize_intents().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn record_wake_and_wake_lookup_match_legacy() {
        let fixture = ParityFixture::new().await;
        create_both(&fixture, "task-record", "event-record").await;
        let extra = sample_wake("task-record", "event-extra");
        assert!(fixture.legacy.record_wake(&extra).await.unwrap());
        assert!(fixture.harness.record_wake(&extra).await.unwrap());
        assert!(!fixture.legacy.record_wake(&extra).await.unwrap());
        assert!(!fixture.harness.record_wake(&extra).await.unwrap());
        assert_wake_parity(
            &fixture
                .legacy
                .wake_by_id("task-record", "event-extra")
                .await
                .unwrap()
                .unwrap(),
            &fixture
                .harness
                .wake_by_id("task-record", "event-extra")
                .await
                .unwrap()
                .unwrap(),
        );
        assert_eq!(fixture.legacy_wake_count().await, 2);
        assert_eq!(fixture.harness_wake_count().await, 2);
    }
}
