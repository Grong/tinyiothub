use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_storage::Db;

pub mod admin;
pub mod approvals;
pub mod controls;
pub mod projection;
pub mod stream;
pub mod tasks;
pub mod types;

#[derive(Clone)]
pub struct DbTaskStore {
    db: Arc<Db>,
    recorder: Option<stream::RuntimeRecorder>,
}

impl DbTaskStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db, recorder: None }
    }

    pub fn with_recorder(mut self, recorder: stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    fn publish(&self, event: tinyiothub_storage::runtime_event::RuntimeEvent) {
        if let Some(recorder) = &self.recorder {
            recorder.publish(event);
        }
    }
}

#[async_trait]
impl TaskStore for DbTaskStore {
    async fn create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError> {
        let (outcome, event) = self.db.create_task_with_wake_and_event(task, wake).await?;
        tracing::info!(
            metric = "runtime_task_created",
            workspace_id = %task.workspace_id,
            task_id = %task.id,
            created_by = %task.created_by,
            "runtime task created"
        );
        self.publish(event);
        Ok(outcome)
    }

    async fn get_task(&self, workspace_id: &str, task_id: &str) -> Result<Option<AgentTask>, TaskStoreError> {
        self.db.get_task(workspace_id, task_id).await
    }

    async fn list_tasks(&self, workspace_id: &str, limit: i64, offset: i64) -> Result<Vec<AgentTask>, TaskStoreError> {
        self.db.list_tasks(workspace_id, limit, offset).await
    }

    async fn wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError> {
        self.db.wake_by_id(task_id, event_id).await
    }

    async fn record_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError> {
        self.db.record_task_wake(wake).await
    }

    async fn resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError> {
        self.db.resolve_task_wakes(task_id).await
    }

    async fn interrupt_unfinished_runs(&self) -> Result<usize, TaskStoreError> {
        self.db
            .interrupt_zombie_running_agent_runs(&[])
            .await
            .map(|count| count as usize)
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }

    async fn transition_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<AgentTask, TaskStoreError> {
        let (task, event) = self
            .db
            .transition_task_and_event(workspace_id, task_id, expected_version, next, wake)
            .await?;
        tracing::info!(
            metric = "runtime_task_transition",
            workspace_id = %workspace_id,
            task_id = %task_id,
            status = ?task.status,
            state_version = task.state_version,
            "runtime task transition"
        );
        self.publish(event);
        Ok(task)
    }

    async fn bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError> {
        let (task, event) = self
            .db
            .bind_current_run_and_event(task_id, expected_version, run_id)
            .await?;
        self.publish(event);
        Ok(task)
    }

    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError> {
        let (started, event) = self.db.begin_run_and_event(task, wake_event_id, run_id).await?;
        if started {
            tracing::info!(
                metric = "runtime_run_started",
                workspace_id = %task.workspace_id,
                task_id = %task.id,
                run_id = %run_id,
                "runtime run started"
            );
        }
        if let Some(event) = event {
            self.publish(event);
        }
        Ok(started)
    }

    async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError> {
        let event = self.db.finish_run_and_event(report).await?;
        tracing::info!(
            metric = "runtime_run_final",
            workspace_id = %report.workspace_id,
            run_id = %report.run_id,
            outcome = ?report.outcome,
            duration_ms = report.duration_ms,
            verified = report.verified,
            "runtime run final"
        );
        self.publish(event);
        Ok(())
    }

    async fn latest_task_run(&self, task_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        self.db.latest_task_run(task_id).await
    }

    async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError> {
        self.db.claim_pending_wakes(limit).await
    }

    async fn recover_claimed_wakes(&self) -> Result<usize, TaskStoreError> {
        self.db.recover_claimed_wakes().await
    }

    async fn mark_wake(&self, task_id: &str, event_id: &str, state: WakeDispatchState) -> Result<(), TaskStoreError> {
        self.db.mark_wake(task_id, event_id, state).await
    }

    async fn retry_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, TaskStoreError> {
        self.db.retry_dead_wake(workspace_id, event_id).await
    }

    async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError> {
        self.db.list_nonterminal_tasks(workspace_id).await
    }
}

const _: fn() = || {
    fn assert_task_store<T: TaskStore>() {}
    assert_task_store::<DbTaskStore>();
};
