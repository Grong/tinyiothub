use async_trait::async_trait;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};

#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError>;
    async fn get_task(&self, workspace_id: &str, task_id: &str) -> Result<Option<AgentTask>, TaskStoreError>;
    async fn list_tasks(&self, workspace_id: &str, limit: i64, offset: i64) -> Result<Vec<AgentTask>, TaskStoreError>;
    async fn wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError>;
    async fn record_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError>;
    async fn resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError>;
    async fn interrupt_unfinished_runs(&self) -> Result<usize, TaskStoreError>;
    async fn transition_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<AgentTask, TaskStoreError>;
    async fn bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError>;
    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError>;
    async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError>;
    async fn latest_task_run(&self, task_id: &str) -> Result<Option<RunReport>, TaskStoreError>;
    async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError>;
    async fn recover_claimed_wakes(&self) -> Result<usize, TaskStoreError>;
    async fn mark_wake(&self, task_id: &str, event_id: &str, state: WakeDispatchState) -> Result<(), TaskStoreError>;
    /// Atomically persist a finalize intent and put its wake back on the
    /// outbox. The clear of `dispatched_at` is what makes a later Dispatched
    /// acknowledgement from the original claim a no-op.
    async fn requeue_with_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError>;
    async fn finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError>;
    async fn complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError>;
    /// Persist a finalize intent without touching its wake. Used by the
    /// in-memory staging retry: the wake is already claimed/pending, so only
    /// the durable intent write needs to be re-attempted.
    async fn put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError>;
    /// The completed Run report for `run_id`, used to distinguish a lost
    /// `finish_run` acknowledgement from a genuine newer-owner conflict.
    async fn completed_run_report(&self, run_id: &str) -> Result<Option<RunReport>, TaskStoreError>;
    /// Observability gauge for retained finalize intents.
    async fn count_finalize_intents(&self) -> Result<i64, TaskStoreError>;
    /// Bounded retention sweep: remove intents older than `older_than` whose
    /// wake is resolved/dead/missing or whose Task is terminal.
    async fn prune_finalize_intents(&self, older_than: chrono::DateTime<chrono::Utc>) -> Result<usize, TaskStoreError>;
    async fn retry_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, TaskStoreError>;
    async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError>;
}
