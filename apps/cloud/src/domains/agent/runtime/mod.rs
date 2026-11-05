use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tinyiothub_agent::harness::{EffectReconciler, ReconcileOutcome, RecoveryFact, reduce_recovery};
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_core::harness::{DecisionKind, DecisionState, EffectIntent, EffectState, Transition};
use tinyiothub_storage::Db;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;

pub mod admin;
pub mod approvals;
pub mod controls;
pub mod projection;
pub mod stream;
pub mod tasks;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryMode {
    Restart,
    Restore,
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub dispatch_permits: Vec<EffectIntent>,
    pub blocked: usize,
}

#[async_trait]
pub trait EffectSender: Send + Sync {
    async fn send(&self, intent: &EffectIntent) -> Result<(), String>;
}

#[derive(Clone)]
pub struct EffectDispatcher {
    db: Arc<Db>,
}

impl EffectDispatcher {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn dispatch(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        sender: &dyn EffectSender,
    ) -> Result<bool, String> {
        let controls = controls::WorkflowControls::new(self.db.clone(), workspace_id);
        if !controls
            .dispatch_permitted(&intent.effect_id)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        sender.send(intent).await?;
        Ok(true)
    }
}

#[derive(Clone)]
pub struct EffectRecovery {
    db: Arc<Db>,
}

impl EffectRecovery {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    async fn item(&self, workspace_id: &str, effect_id: &str) -> Result<EffectRecoveryItem, String> {
        self.db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.intent.effect_id == effect_id)
            .ok_or_else(|| format!("active effect {effect_id} not found"))
    }

    async fn apply(&self, workspace_id: &str, mut transition: Transition) -> Result<(), String> {
        if let Some(event) = &mut transition.event {
            event.workspace_id = workspace_id.to_string();
        }
        for event in &mut transition.additional_events {
            event.workspace_id = workspace_id.to_string();
        }
        let precondition = transition.precondition.clone();
        self.db
            .apply_workspace_transition(workspace_id, &precondition, &transition)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn transition(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        fact: RecoveryFact,
    ) -> Result<(), String> {
        let transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            item.job_id.as_deref(),
            &format!("recovery:{}", uuid::Uuid::new_v4()),
            Utc::now(),
            &item.task_state,
            fact,
        )
        .map_err(|error| error.to_string())?;
        self.apply(workspace_id, transition).await
    }

    async fn recover_unknown(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        reconciler: &dyn EffectReconciler,
        report: &mut RecoveryReport,
    ) -> Result<(), String> {
        match item.decision.as_ref() {
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation && decision.state == DecisionState::Answered =>
            {
                if item.task_active {
                    self.transition(workspace_id, item, RecoveryFact::ConsumeAnswer).await?;
                } else {
                    report.blocked += 1;
                }
                return Ok(());
            }
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation
                    && decision.state == DecisionState::Requested => {}
            None => {
                self.transition(workspace_id, item, RecoveryFact::EnsureReconciliation)
                    .await?;
                let item = self.item(workspace_id, &item.intent.effect_id).await?;
                return Box::pin(self.recover_unknown(workspace_id, &item, reconciler, report)).await;
            }
            _ => return Err("unknown effect has an invalid reconciliation decision".to_string()),
        }
        match reconciler.settle(&item.intent).await {
            ReconcileOutcome::Unknown { reason } if item.intent.recovery_from == Some(EffectState::Cancelling) => {
                self.transition(
                    workspace_id,
                    item,
                    RecoveryFact::Reconcile(ReconcileOutcome::Unknown { reason }),
                )
                .await
            }
            ReconcileOutcome::Unknown { reason } => {
                self.db
                    .record_effect_reconcile_attempt(workspace_id, &item.intent, &reason)
                    .await
                    .map_err(|error| error.to_string())?;
                report.blocked += 1;
                Ok(())
            }
            outcome => {
                self.transition(workspace_id, item, RecoveryFact::Reconcile(outcome))
                    .await
            }
        }
    }

    async fn recover_cancelling(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        reconciler: &dyn EffectReconciler,
    ) -> Result<(), String> {
        match reconciler.settle(&item.intent).await {
            ReconcileOutcome::Unknown { .. } => {
                self.transition(
                    workspace_id,
                    item,
                    RecoveryFact::Reconcile(ReconcileOutcome::Unknown {
                        reason: "stop result unknown".to_string(),
                    }),
                )
                .await
            }
            outcome => {
                self.transition(workspace_id, item, RecoveryFact::Reconcile(outcome))
                    .await
            }
        }
    }

    pub async fn recover_workspace(
        &self,
        workspace_id: &str,
        mode: RecoveryMode,
        reconciler: &dyn EffectReconciler,
    ) -> Result<RecoveryReport, String> {
        // Fail closed on any persisted row outside the listed recovery
        // matrix before a single dispatch permit or decision consume occurs.
        self.db
            .guard_recovery_matrix(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let items = self
            .db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let mut report = RecoveryReport::default();
        if mode == RecoveryMode::Restore {
            for item in &items {
                let fact = if item.task_state == "cancelling"
                    && matches!(item.intent.state, EffectState::Prepared | EffectState::Dispatching)
                {
                    RecoveryFact::Cancel
                } else {
                    RecoveryFact::Fence
                };
                self.transition(workspace_id, item, fact).await?;
            }
            return Box::pin(self.recover_workspace(workspace_id, RecoveryMode::Restart, reconciler)).await;
        }
        for item in items {
            match item.intent.state {
                EffectState::Prepared => {
                    if item.task_state == "cancelling" {
                        self.transition(workspace_id, &item, RecoveryFact::Cancel).await?;
                    } else if item.task_active
                        && item.decision.as_ref().is_some_and(|decision| {
                            decision.kind == DecisionKind::ToolApproval
                                && matches!(
                                    decision.state,
                                    DecisionState::Answered | DecisionState::Expired | DecisionState::Withdrawn
                                )
                        })
                    {
                        let approved = item
                            .decision
                            .as_ref()
                            .is_some_and(|decision| decision.answer.as_deref() == Some("approved"));
                        self.transition(workspace_id, &item, RecoveryFact::ConsumeApproval)
                            .await?;
                        if approved {
                            let dispatched = self.item(workspace_id, &item.intent.effect_id).await?;
                            report.dispatch_permits.push(dispatched.intent);
                        }
                    }
                }
                EffectState::Dispatching if item.task_state == "cancelling" => {
                    self.transition(workspace_id, &item, RecoveryFact::Cancel).await?;
                    let cancelling = self.item(workspace_id, &item.intent.effect_id).await?;
                    self.recover_cancelling(workspace_id, &cancelling, reconciler).await?;
                }
                EffectState::Dispatching => {
                    self.transition(workspace_id, &item, RecoveryFact::Fence).await?;
                    let fenced = self.item(workspace_id, &item.intent.effect_id).await?;
                    self.recover_unknown(workspace_id, &fenced, reconciler, &mut report)
                        .await?;
                }
                EffectState::Unknown => {
                    self.recover_unknown(workspace_id, &item, reconciler, &mut report)
                        .await?;
                }
                EffectState::Cancelling => {
                    self.recover_cancelling(workspace_id, &item, reconciler).await?;
                }
                EffectState::Completed
                | EffectState::Failed
                | EffectState::NotApplied
                | EffectState::Cancelled
                | EffectState::CancelledUnknown => {}
            }
        }
        Ok(report)
    }

    pub async fn answer_reconciliation(
        &self,
        workspace_id: &str,
        effect_id: &str,
        answer: &str,
        actor: &str,
        evidence: Option<serde_json::Value>,
    ) -> Result<(), String> {
        self.db
            .answer_effect_reconciliation(workspace_id, effect_id, answer, actor, evidence)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub async fn request_cancellation(&self, workspace_id: &str, effect_id: &str) -> Result<(), String> {
        self.db
            .guard_recovery_matrix(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        if self
            .db
            .effect_intent(workspace_id, effect_id)
            .await
            .map_err(|error| error.to_string())?
            .is_some_and(|intent| intent.state.is_terminal())
        {
            return Ok(());
        }
        let item = self.item(workspace_id, effect_id).await?;
        if matches!(item.intent.state, EffectState::Unknown | EffectState::Cancelling) {
            return Ok(());
        }
        self.transition(workspace_id, &item, RecoveryFact::Cancel).await
    }
}

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

    async fn interrupt_unfinished_runs(&self, known_active: &[String]) -> Result<usize, TaskStoreError> {
        self.db
            .interrupt_zombie_running_agent_runs(known_active)
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

    async fn mark_wake_dead(&self, task_id: &str, event_id: &str, reason: &str) -> Result<(), TaskStoreError> {
        self.db.mark_wake_dead(task_id, event_id, reason).await
    }

    async fn requeue_with_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        self.db.requeue_with_finalize_intent(intent).await
    }

    async fn finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError> {
        self.db.finalize_intent(task_id, event_id).await
    }

    async fn complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError> {
        self.db.complete_finalize_intent(task_id, event_id).await
    }

    async fn put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        self.db.put_finalize_intent(intent).await
    }

    async fn pending_finalize_intents(&self) -> Result<Vec<TaskFinalizeIntent>, TaskStoreError> {
        self.db.pending_finalize_intents().await
    }

    async fn completed_run_report(&self, run_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        self.db.completed_run_report(run_id).await
    }

    async fn count_finalize_intents(&self) -> Result<i64, TaskStoreError> {
        self.db.count_finalize_intents().await
    }

    async fn prune_finalize_intents(&self, older_than: chrono::DateTime<chrono::Utc>) -> Result<usize, TaskStoreError> {
        self.db.prune_finalize_intents(older_than).await
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
