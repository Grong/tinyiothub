use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_storage::Db;
use tinyiothub_storage::runtime_event::RuntimeEventInput;

pub mod admin;
pub mod controls;
pub mod projection;
pub mod stream;
pub mod tasks;
pub mod types;

#[derive(Clone)]
pub(crate) struct DbTaskStore {
    db: Arc<Db>,
    recorder: Option<stream::RuntimeRecorder>,
}

impl DbTaskStore {
    pub(crate) fn new(db: Arc<Db>) -> Self {
        Self { db, recorder: None }
    }

    pub(crate) fn with_recorder(mut self, recorder: stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    async fn record_task(&self, task: &AgentTask, event_type: &str, event_id: String) {
        let input = RuntimeEventInput {
            event_id,
            event_type: event_type.to_string(),
            occurred_at: chrono::Utc::now(),
            workspace_id: task.workspace_id.clone(),
            task_id: Some(task.id.clone()),
            run_id: task.current_run_id.clone(),
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: serde_json::json!({"stateVersion": task.state_version}),
        };
        self.record(input).await;
    }

    async fn record(&self, input: RuntimeEventInput) {
        let Some(recorder) = &self.recorder else {
            return;
        };
        let event_id = input.event_id.clone();
        if let Err(error) = recorder.record(input).await {
            tracing::error!(%event_id, %error, "record Task runtime event failed");
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
        let outcome = self.db.create_task_with_wake(task, wake).await?;
        self.record_task(
            &outcome.task,
            "task.created",
            format!("task:{}:created", outcome.task.id),
        )
        .await;
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
        let task = self
            .db
            .transition_task(workspace_id, task_id, expected_version, next, wake)
            .await?;
        self.record_task(
            &task,
            &format!("task.{}", task.status.as_str()),
            format!("task:{}:v{}:{}", task.id, task.state_version, task.status.as_str()),
        )
        .await;
        Ok(task)
    }

    async fn bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError> {
        let task = self.db.bind_current_run(task_id, expected_version, run_id).await?;
        self.record_task(
            &task,
            "task.run_bound",
            format!("task:{}:v{}:run_bound", task.id, task.state_version),
        )
        .await;
        Ok(task)
    }

    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError> {
        let started = self.db.begin_run(task, wake_event_id, run_id).await?;
        if started {
            self.record(RuntimeEventInput {
                event_id: format!("run:{run_id}:started"),
                event_type: "run.started".into(),
                occurred_at: chrono::Utc::now(),
                workspace_id: task.workspace_id.clone(),
                task_id: Some(task.id.clone()),
                run_id: Some(run_id.to_string()),
                tool_call_id: None,
                job_id: None,
                approval_id: None,
                device_id: None,
                source_id: None,
                source_seq: None,
                evidence_id: None,
                data: serde_json::json!({}),
            })
            .await;
        }
        Ok(started)
    }

    async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError> {
        self.db.finish_run(report).await?;
        self.record(RuntimeEventInput {
            event_id: format!("run:{}:finished", report.run_id),
            event_type: "run.finished".into(),
            occurred_at: chrono::Utc::now(),
            workspace_id: report.workspace_id.clone(),
            task_id: report.task_id.clone(),
            run_id: Some(report.run_id.clone()),
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: report.thing_id.clone(),
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: serde_json::json!({"outcome": report.outcome.as_str()}),
        })
        .await;
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
