use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::agent_runs::{EndReason, Outcome, RunReport};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskStoreError, TaskWake};
use tinyiothub_core::watch::WatchRule;
use tinyiothub_runtime::job::service::JobService;
use tinyiothub_runtime::ports::WatchRuleStore;
use tokio_util::sync::CancellationToken;

use super::prompt::build_task_prompt;
use super::state::validate_transition;
use crate::port::approval::TaskApprovalStore;
use crate::port::task::TaskStore;
use crate::runtime::events::{AgentEventBus, AgentEventKind};
use crate::runtime::thing_agent::active_runs::{ActiveRunGuard, ActiveRunRegistry};
use crate::runtime::thing_agent::manager::{AutonomousAgentProvider, fetch_workspace_snapshot};
use crate::runtime::thing_agent::runner::{RunContext, Runner};
use crate::runtime::thing_agent::types::WakeSignal;
use crate::runtime::thing_agent::watch_loop::WatchCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advance {
    Started { run_id: String },
    Steered,
    Duplicate,
    Recorded,
    Deferred,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskError {
    #[error("task wake not found")]
    WakeNotFound,
    #[error("invalid task wake: {0}")]
    InvalidWake(String),
    #[error("task not found")]
    NotFound,
    #[error("task wake belongs to another workspace")]
    CrossWorkspace,
    #[error(transparent)]
    Store(#[from] TaskStoreError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelOutcome {
    pub accepted: bool,
}

#[derive(Clone)]
pub struct TaskCoordinator {
    store: Arc<dyn TaskStore>,
    approvals: Option<Arc<dyn TaskApprovalStore>>,
    jobs: Option<JobService>,
    watches: Arc<dyn WatchRuleStore>,
    watch_cmds: Arc<DashMap<String, tokio::sync::mpsc::Sender<WatchCommand>>>,
    events: Arc<AgentEventBus>,
    active_runs: ActiveRunRegistry,
    runner: Arc<Runner>,
    agent_provider: Arc<dyn AutonomousAgentProvider>,
    reports: Arc<DashMap<String, Vec<RunReport>>>,
}

impl TaskCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Arc<dyn TaskStore>,
        jobs: Option<JobService>,
        watches: Arc<dyn WatchRuleStore>,
        watch_cmds: Arc<DashMap<String, tokio::sync::mpsc::Sender<WatchCommand>>>,
        events: Arc<AgentEventBus>,
        active_runs: ActiveRunRegistry,
        runner: Arc<Runner>,
        agent_provider: Arc<dyn AutonomousAgentProvider>,
    ) -> Self {
        Self {
            store,
            approvals: None,
            jobs,
            watches,
            watch_cmds,
            events,
            active_runs,
            runner,
            agent_provider,
            reports: Arc::new(DashMap::new()),
        }
    }

    pub fn with_approvals(mut self, approvals: Arc<dyn TaskApprovalStore>) -> Self {
        self.approvals = Some(approvals);
        self
    }

    pub async fn pause(&self, workspace_id: &str, task_id: &str) -> Result<AgentTask, TaskError> {
        let task = self
            .store
            .get_task(workspace_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        if task.status == TaskStatus::Paused {
            return Ok(task);
        }
        validate_transition(task.status, TaskStatus::Paused)
            .map_err(|error| TaskStoreError::Validation(error.to_string()))?;
        Ok(self
            .store
            .transition_task(workspace_id, task_id, task.state_version, TaskStatus::Paused, None)
            .await?)
    }

    pub async fn resume(&self, workspace_id: &str, task_id: &str) -> Result<AgentTask, TaskError> {
        let task = self
            .store
            .get_task(workspace_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        if task.status != TaskStatus::Paused {
            return Ok(task);
        }
        let next = self
            .waiting_status(workspace_id, task_id)
            .await?
            .unwrap_or(TaskStatus::Active);
        validate_transition(task.status, next).map_err(|error| TaskStoreError::Validation(error.to_string()))?;
        Ok(self
            .store
            .transition_task(workspace_id, task_id, task.state_version, next, None)
            .await?)
    }

    async fn waiting_status(&self, workspace_id: &str, task_id: &str) -> Result<Option<TaskStatus>, TaskError> {
        if let Some(approvals) = &self.approvals {
            let pending = approvals
                .list_for_task(workspace_id, task_id)
                .await
                .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?
                .iter()
                .any(|approval| approval.state == tinyiothub_core::task_approval::ApprovalState::Pending);
            let decided = !pending
                && approvals
                    .list_recoverable(Some(workspace_id))
                    .await
                    .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?
                    .iter()
                    .any(|approval| approval.task_id == task_id);
            if pending || decided {
                return Ok(Some(TaskStatus::NeedsApproval));
            }
        }
        if let Some(jobs) = &self.jobs
            && !jobs
                .task_jobs(workspace_id, task_id, false)
                .await
                .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?
                .is_empty()
        {
            return Ok(Some(TaskStatus::WaitingJob));
        }
        let watches = self.watches.list_all().await.map_err(TaskStoreError::Unavailable)?;
        if watches
            .iter()
            .any(|rule| rule.workspace_id == workspace_id && rule.owner_task_id.as_deref() == Some(task_id))
        {
            return Ok(Some(TaskStatus::WaitingWatch));
        }
        Ok(None)
    }

    pub async fn cancel(&self, workspace_id: &str, task_id: &str) -> Result<CancelOutcome, TaskError> {
        let task = self
            .store
            .get_task(workspace_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        if task.status.is_terminal() {
            return Ok(CancelOutcome { accepted: false });
        }
        if task.status == TaskStatus::Cancelling {
            self.finish_cancellation(&task).await?;
            return Ok(CancelOutcome { accepted: false });
        }
        validate_transition(task.status, TaskStatus::Cancelling)
            .map_err(|error| TaskStoreError::Validation(error.to_string()))?;
        let task = if let Some(approvals) = &self.approvals {
            approvals
                .cancel(workspace_id, task_id, task.state_version, chrono::Utc::now())
                .await
                .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?
        } else {
            self.store
                .transition_task(workspace_id, task_id, task.state_version, TaskStatus::Cancelling, None)
                .await?
        };
        if let Some(run_id) = task.current_run_id.as_deref() {
            self.active_runs.cancel(run_id);
        }
        let owned_rules = self
            .watches
            .list_all()
            .await
            .map_err(TaskStoreError::Unavailable)?
            .into_iter()
            .filter(|rule| rule.workspace_id == workspace_id && rule.owner_task_id.as_deref() == Some(task_id))
            .collect::<Vec<_>>();
        if let Some(commands) = self.watch_cmds.get(workspace_id) {
            for rule in &owned_rules {
                let _ = commands.send(WatchCommand::Remove(rule.id.clone())).await;
            }
        }
        self.watches
            .delete_task_watches(workspace_id, task_id)
            .await
            .map_err(TaskStoreError::Unavailable)?;
        if self.approvals.is_none() {
            self.store.resolve_task_wakes(task_id).await?;
        }
        if let Some(jobs) = &self.jobs {
            for job in jobs
                .task_jobs(workspace_id, task_id, false)
                .await
                .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?
            {
                jobs.stop(workspace_id, task_id, &job.id)
                    .await
                    .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?;
            }
        }
        self.finish_cancellation(&task).await?;
        Ok(CancelOutcome { accepted: true })
    }

    async fn finish_cancellation(&self, task: &AgentTask) -> Result<(), TaskError> {
        let Some(jobs) = &self.jobs else {
            self.store
                .transition_task(
                    &task.workspace_id,
                    &task.id,
                    task.state_version,
                    TaskStatus::Cancelled,
                    None,
                )
                .await?;
            return Ok(());
        };
        let active = jobs
            .task_jobs(&task.workspace_id, &task.id, false)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?;
        if !active.is_empty() {
            return Ok(());
        }
        let unknown = jobs
            .task_jobs(&task.workspace_id, &task.id, true)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?;
        let terminal = if unknown.is_empty() {
            TaskStatus::Cancelled
        } else {
            tracing::warn!(task_id = %task.id, reason = "cancel_unknown", "Task cancellation left an unknown device outcome");
            TaskStatus::Failed
        };
        self.store
            .transition_task(&task.workspace_id, &task.id, task.state_version, terminal, None)
            .await?;
        Ok(())
    }

    pub async fn stop_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<tinyiothub_core::device_job::DeviceJob, tinyiothub_core::device_job::DeviceJobError> {
        self.jobs
            .as_ref()
            .ok_or_else(|| tinyiothub_core::device_job::DeviceJobError::Storage("Job service is not started".into()))?
            .stop(workspace_id, task_id, job_id)
            .await
    }

    pub async fn recover(&self) -> Result<usize, TaskError> {
        self.store.recover_claimed_wakes().await?;
        self.store.interrupt_unfinished_runs().await?;
        if let Some(jobs) = &self.jobs {
            jobs.recover()
                .await
                .map_err(|error| TaskStoreError::Unavailable(format!("job recovery failed: {error}")))?;
        }
        let mut started = 0;
        for mut task in self.store.list_nonterminal_tasks(None).await? {
            if let Some(run_id) = task.current_run_id.as_deref() {
                if self.active_runs.get(run_id).is_some() {
                    continue;
                }
                task = self.store.bind_current_run(&task.id, task.state_version, None).await?;
            }
            if task.status == TaskStatus::Cancelling {
                self.finish_cancellation(&task).await?;
                continue;
            }
            if task.status != TaskStatus::Active {
                continue;
            }
            if let Some(waiting) = self.waiting_status(&task.workspace_id, &task.id).await? {
                self.store
                    .transition_task(&task.workspace_id, &task.id, task.state_version, waiting, None)
                    .await?;
                continue;
            }
            let event_id = format!("recovery:{}", task.id);
            let signal = crate::runtime::thing_agent::types::WakeSignal {
                workspace_id: task.workspace_id.clone(),
                priority: crate::runtime::thing_agent::types::Priority::Normal,
                dedup_key: Some(format!("task:{}:recovery", task.id)),
                source: crate::runtime::thing_agent::types::TriggerSource::UserDirective {
                    user_id: task.created_by.clone(),
                    text: "Recover this task after a runtime restart.".into(),
                    session_key: task.session_key.clone(),
                    source: Some("task_recovery".into()),
                    problem_key: None,
                },
            };
            let wake = TaskWake {
                task_id: task.id.clone(),
                event_id: event_id.clone(),
                signal_json: serde_json::to_string(&signal)
                    .map_err(|error| TaskStoreError::Unavailable(format!("recovery wake encoding failed: {error}")))?,
                dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
                created_at: chrono::Utc::now(),
                dispatched_at: None,
                resolved_at: None,
            };
            self.store.record_wake(&wake).await?;
            if matches!(self.advance(&task.id, &event_id).await?, Advance::Started { .. }) {
                started += 1;
            }
        }
        Ok(started)
    }

    pub fn advance<'a>(
        &'a self,
        task_id: &'a str,
        wake_event_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Advance, TaskError>> + Send + 'a>> {
        Box::pin(async move {
            let wake = self
                .store
                .wake_by_id(task_id, wake_event_id)
                .await?
                .ok_or(TaskError::WakeNotFound)?;
            let raw_signal: serde_json::Value =
                serde_json::from_str(&wake.signal_json).map_err(|error| TaskError::InvalidWake(error.to_string()))?;
            let approval_decision =
                raw_signal.get("kind").and_then(serde_json::Value::as_str) == Some("approval_decided");
            let workspace_id = raw_signal
                .get("workspace_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| TaskError::InvalidWake("wake workspace is missing".into()))?;
            let mut task = self
                .store
                .get_task(workspace_id, task_id)
                .await?
                .ok_or(TaskError::NotFound)?;
            if workspace_id != task.workspace_id {
                return Err(TaskError::CrossWorkspace);
            }
            if task.status == TaskStatus::Paused {
                return Ok(Advance::Deferred);
            }
            if task.status.is_terminal() {
                return Ok(Advance::Recorded);
            }
            if task.status == TaskStatus::Cancelling {
                self.finish_cancellation(&task).await?;
                return Ok(Advance::Recorded);
            }
            let signal: WakeSignal = if approval_decision {
                if task.status == TaskStatus::NeedsApproval {
                    task = self
                        .store
                        .transition_task(workspace_id, task_id, task.state_version, TaskStatus::Active, None)
                        .await?;
                }
                WakeSignal {
                    workspace_id: task.workspace_id.clone(),
                    priority: crate::runtime::thing_agent::types::Priority::Normal,
                    dedup_key: Some(format!("task:{task_id}:approval:{wake_event_id}")),
                    source: crate::runtime::thing_agent::types::TriggerSource::UserDirective {
                        user_id: task.created_by.clone(),
                        text: "Continue after the recorded approval decision.".into(),
                        session_key: task.session_key.clone(),
                        source: Some("approval_decided".into()),
                        problem_key: None,
                    },
                }
            } else {
                serde_json::from_value(raw_signal).map_err(|error| TaskError::InvalidWake(error.to_string()))?
            };
            if let Some(run_id) = task.current_run_id.as_deref() {
                return self.steer_existing(run_id, wake_event_id, signal).await;
            }
            let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
            self.start_run(task, wake, signal, run_id).await
        })
    }

    async fn steer_existing(
        &self,
        run_id: &str,
        wake_event_id: &str,
        signal: WakeSignal,
    ) -> Result<Advance, TaskError> {
        let facts = serde_json::to_string_pretty(&signal)
            .map_err(|error| TaskStoreError::Unavailable(format!("steer encoding failed: {error}")))?;
        let targeted =
            format!("wake_event_id: {wake_event_id}\n<untrusted_task_context>\n{facts}\n</untrusted_task_context>");
        if !self.active_runs.steer(run_id, targeted) {
            return Err(TaskStoreError::Unavailable(format!("active Task Run {run_id} is not registered")).into());
        }
        Ok(Advance::Steered)
    }

    async fn start_run(
        &self,
        task: AgentTask,
        wake: TaskWake,
        signal: WakeSignal,
        run_id: String,
    ) -> Result<Advance, TaskError> {
        let ctx = RunContext {
            active_runs: Some(self.active_runs.clone()),
            task_id: Some(task.id.clone()),
            wake_event_id: Some(wake.event_id.clone()),
            ..RunContext::new(run_id.clone(), task.workspace_id.clone(), format!("task:{}", task.id))
        };
        let cancel = CancellationToken::new();
        {
            let mut inner = ctx.inner.write().await;
            inner.task_id = Some(task.id.clone());
            inner.task_session_key = task.session_key.clone();
        }
        let guard = self
            .active_runs
            .register(ctx.clone(), cancel.clone())
            .ok_or_else(|| TaskStoreError::Unavailable("active Run registry is at capacity".into()))?;
        if !self.store.begin_run(&task, &wake.event_id, &run_id).await? {
            return Ok(Advance::Duplicate);
        }
        let task = self
            .store
            .bind_current_run(&task.id, task.state_version, Some(&run_id))
            .await?;
        let coordinator = self.clone();
        let spawned_run_id = run_id.clone();
        tokio::spawn(async move {
            coordinator
                .execute_run(task, wake, signal, spawned_run_id, ctx, cancel, guard)
                .await;
        });
        Ok(Advance::Started { run_id })
    }

    async fn execute_run(
        &self,
        task: AgentTask,
        wake: TaskWake,
        signal: WakeSignal,
        run_id: String,
        ctx: RunContext,
        cancel: CancellationToken,
        guard: ActiveRunGuard,
    ) {
        let steering = ctx.steering.clone();
        self.events.emit(AgentEventKind::RunStarted {
            run_id: run_id.clone(),
            workspace_id: task.workspace_id.clone(),
            problem_keys: vec![],
            task_id: Some(task.id.clone()),
            wake_event_id: Some(wake.event_id.clone()),
        });

        let runs = match self.store.latest_task_run(&task.id).await {
            Ok(Some(report)) => vec![report],
            Ok(None) => self
                .reports
                .get(&task.id)
                .map(|entries| entries.clone())
                .unwrap_or_default(),
            Err(error) => {
                tracing::warn!(task_id = %task.id, %error, "Task run history read failed");
                vec![]
            }
        };
        let jobs = match &self.jobs {
            Some(jobs) => jobs
                .task_jobs(&task.workspace_id, &task.id, false)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(task_id = %task.id, %error, "Task Job context read failed");
                    vec![]
                }),
            None => vec![],
        };
        let watches = self
            .watches
            .list_all()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|rule| {
                rule.workspace_id == task.workspace_id && rule.owner_task_id.as_deref() == Some(task.id.as_str())
            })
            .collect::<Vec<WatchRule>>();
        let (snapshot, snapshot_metrics) = fetch_workspace_snapshot(&self.watch_cmds, &task.workspace_id, None).await;
        if let Some(metrics) = snapshot_metrics {
            ctx.inner.write().await.snapshot = Some(metrics);
        }
        let approval_facts = match &self.approvals {
            Some(approvals) => approvals
                .list_for_task(&task.workspace_id, &task.id)
                .await
                .map(|records| {
                    records
                        .into_iter()
                        .map(|approval| super::prompt::TaskPromptApproval {
                            approval_id: approval.id,
                            status: approval.state.as_str().into(),
                            detail: format!("{}: {}", approval.tool, approval.reason),
                        })
                        .collect()
                })
                .unwrap_or_else(|error| {
                    tracing::warn!(task_id = %task.id, %error, "Task approval context read failed");
                    vec![]
                }),
            None => vec![],
        };
        let prompt = build_task_prompt(
            &task,
            &runs,
            &jobs,
            &watches,
            &approval_facts,
            snapshot.as_deref().unwrap_or(""),
            Some(&signal),
        );
        let report = if cancel.is_cancelled() {
            RunReport {
                run_id: run_id.clone(),
                workspace_id: task.workspace_id.clone(),
                trigger: format!("task:{}", task.id),
                outcome: Outcome::Failed,
                summary: "Task run cancelled before model execution".into(),
                actions: vec![],
                verified: false,
                duration_ms: 0,
                tool_calls: 0,
                tokens: 0,
                end_reason: None,
                thing_id: None,
                trace: vec![],
                task_count: None,
                snapshot: None,
                task_id: Some(task.id.clone()),
                wake_event_id: Some(wake.event_id.clone()),
            }
        } else {
            match self
                .agent_provider
                .get_or_create(&task.workspace_id, Arc::clone(&ctx.inner))
                .await
            {
                Ok(agent) => self.runner.execute(agent, prompt, ctx, cancel).await.report,
                Err(error) => RunReport {
                    run_id: run_id.clone(),
                    workspace_id: task.workspace_id.clone(),
                    trigger: format!("task:{}", task.id),
                    outcome: Outcome::Failed,
                    summary: format!("Task agent unavailable: {error}"),
                    actions: vec![],
                    verified: false,
                    duration_ms: 0,
                    tool_calls: 0,
                    tokens: 0,
                    end_reason: Some(EndReason::AgentUnavailable),
                    thing_id: None,
                    trace: vec![],
                    task_count: None,
                    snapshot: None,
                    task_id: Some(task.id.clone()),
                    wake_event_id: Some(wake.event_id.clone()),
                },
            }
        };
        drop(guard);
        let undelivered = {
            let mut steering = steering.lock().unwrap_or_else(|error| error.into_inner());
            std::mem::take(&mut *steering)
        };
        if let Err(error) = self.store.finish_run(&report).await {
            tracing::warn!(run_id = %report.run_id, %error, "Task run final report persistence failed");
        }
        self.reports.entry(task.id.clone()).or_default().push(report.clone());
        self.events.emit(AgentEventKind::RunRecorded {
            report: Box::new(report.clone()),
            problem_key: None,
            dedup_key: signal.dedup_key.clone(),
        });
        self.finish_task_run_with_steering(&task, &run_id, report.outcome, undelivered)
            .await;
    }

    #[cfg(test)]
    async fn finish_task_run(&self, task: &AgentTask, run_id: &str, outcome: Outcome) {
        self.finish_task_run_with_steering(task, run_id, outcome, Vec::new())
            .await;
    }

    async fn finish_task_run_with_steering(
        &self,
        task: &AgentTask,
        run_id: &str,
        outcome: Outcome,
        undelivered: Vec<String>,
    ) {
        let Ok(Some(current)) = self.store.get_task(&task.workspace_id, &task.id).await else {
            return;
        };
        if current.current_run_id.as_deref() != Some(run_id) {
            return;
        }
        let current = match self.store.bind_current_run(&task.id, current.state_version, None).await {
            Ok(task) => task,
            Err(TaskStoreError::ConcurrentUpdate) => return,
            Err(error) => {
                tracing::warn!(task_id = %task.id, %error, "Task run binding cleanup failed");
                return;
            }
        };
        if current.status.is_terminal() {
            return;
        }
        if matches!(current.status, TaskStatus::Paused | TaskStatus::Cancelling) {
            for facts in undelivered {
                if let Some(event_id) = facts
                    .strip_prefix("wake_event_id: ")
                    .and_then(|facts| facts.lines().next())
                    && let Err(error) = self
                        .store
                        .mark_wake(
                            &current.id,
                            event_id,
                            tinyiothub_core::agent_task::WakeDispatchState::Pending,
                        )
                        .await
                {
                    tracing::warn!(task_id = %current.id, event_id, %error, "Undelivered Task wake restore failed");
                }
            }
            return;
        }
        let mut follow_up_started = false;
        for facts in undelivered {
            let Some(event_id) = facts
                .strip_prefix("wake_event_id: ")
                .and_then(|facts| facts.lines().next())
            else {
                tracing::warn!(task_id = %current.id, "Undelivered Task steering had no wake ID");
                continue;
            };
            match Box::pin(self.advance(&current.id, event_id)).await {
                Ok(Advance::Started { .. } | Advance::Steered) => follow_up_started = true,
                Ok(Advance::Deferred) => {
                    if let Err(error) = self
                        .store
                        .mark_wake(
                            &current.id,
                            event_id,
                            tinyiothub_core::agent_task::WakeDispatchState::Pending,
                        )
                        .await
                    {
                        tracing::warn!(task_id = %current.id, event_id, %error, "Deferred Task wake restore failed");
                    }
                }
                Ok(Advance::Duplicate | Advance::Recorded) => {}
                Err(error) => {
                    tracing::warn!(task_id = %current.id, event_id, %error, "Undelivered Task wake dispatch failed");
                    if let Err(error) = self
                        .store
                        .mark_wake(
                            &current.id,
                            event_id,
                            tinyiothub_core::agent_task::WakeDispatchState::Pending,
                        )
                        .await
                    {
                        tracing::warn!(task_id = %current.id, event_id, %error, "Failed Task wake restore failed");
                    }
                }
            }
        }
        if follow_up_started {
            return;
        }
        let next = if outcome == Outcome::Failed {
            TaskStatus::Failed
        } else {
            match self.waiting_status(&current.workspace_id, &current.id).await {
                Ok(Some(waiting)) => waiting,
                Ok(None) => TaskStatus::Completed,
                Err(error) => {
                    tracing::warn!(task_id = %task.id, %error, "Task waiting relationship read failed");
                    return;
                }
            }
        };
        if let Err(error) = validate_transition(current.status, next) {
            tracing::warn!(task_id = %current.id, from = ?current.status, to = ?next, %error, "Task completion requested an illegal transition");
            return;
        }
        if let Err(error) = self
            .store
            .transition_task(&current.workspace_id, &current.id, current.state_version, next, None)
            .await
        {
            tracing::debug!(task_id = %task.id, %error, "Task completion lost to a newer state");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use tinyiothub_core::agent_runs::{Outcome, RunReport};
    use tinyiothub_core::agent_task::{
        AgentTask, CreateTaskOutcome, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
    };
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
    use tinyiothub_core::error::Error;
    use tinyiothub_core::models::thing::Thing;
    use tinyiothub_core::models::thing_command::ThingCommand;
    use tinyiothub_core::watch::WatchRule;
    use tinyiothub_runtime::access::resources::ResourceResolver;
    use tinyiothub_runtime::access::validator::AccessCallValidator;
    use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
    use tinyiothub_runtime::job::service::JobService;
    use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobWorkerLocator};
    use tinyiothub_runtime::ports::{DeviceJobStore, HubLedger, ThingCacheSource, WatchRuleStore};
    use tinyiothub_runtime::watch::engine::WatchEngine;
    use tinyiothub_runtime::watch::live_state::LiveState;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::super::wake::WakeRouter;
    use super::{Advance, TaskCoordinator, TaskError};
    use crate::port::events::TurnEvent;
    use crate::port::runtime::AgentLoop;
    use crate::port::task::TaskStore;
    use crate::runtime::events::{AgentEventBus, AgentEventKind};
    use crate::runtime::thing_agent::active_runs::{ActiveRunGuard, ActiveRunRegistry};
    use crate::runtime::thing_agent::manager::AutonomousAgentProvider;
    use crate::runtime::thing_agent::runner::{AgentHandle, RunContext, RunContextInner, Runner};
    use crate::runtime::thing_agent::traits::DirectiveSink;
    use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

    #[derive(Default)]
    struct MemoryState {
        tasks: HashMap<String, AgentTask>,
        wakes: HashMap<(String, String), TaskWake>,
        runs: HashSet<(String, String)>,
        reports: Vec<RunReport>,
        interrupted: usize,
    }

    #[derive(Default)]
    pub(crate) struct MemoryTasks {
        state: Mutex<MemoryState>,
        bind_gate: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    }

    impl MemoryTasks {
        fn insert(&self, task: AgentTask) {
            self.state.lock().expect("tasks").tasks.insert(task.id.clone(), task);
        }

        fn insert_wake(&self, wake: TaskWake) {
            self.state
                .lock()
                .expect("tasks")
                .wakes
                .insert((wake.task_id.clone(), wake.event_id.clone()), wake);
        }

        fn task(&self, id: &str) -> AgentTask {
            self.state.lock().expect("tasks").tasks[id].clone()
        }

        pub(crate) fn wake_count(&self) -> usize {
            self.state.lock().expect("tasks").wakes.len()
        }

        fn wake(&self, task_id: &str, event_id: &str) -> TaskWake {
            self.state.lock().expect("tasks").wakes[&(task_id.into(), event_id.into())].clone()
        }
    }

    #[async_trait]
    impl TaskStore for MemoryTasks {
        async fn create_task_with_wake(
            &self,
            task: &AgentTask,
            wake: &TaskWake,
        ) -> Result<CreateTaskOutcome, TaskStoreError> {
            self.insert(task.clone());
            self.insert_wake(wake.clone());
            Ok(CreateTaskOutcome {
                task: task.clone(),
                deduplicated: false,
            })
        }

        async fn get_task(&self, workspace_id: &str, task_id: &str) -> Result<Option<AgentTask>, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .tasks
                .get(task_id)
                .filter(|task| task.workspace_id == workspace_id)
                .cloned())
        }

        async fn list_tasks(
            &self,
            workspace_id: &str,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<AgentTask>, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .tasks
                .values()
                .filter(|task| task.workspace_id == workspace_id)
                .cloned()
                .collect())
        }

        async fn wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .wakes
                .get(&(task_id.into(), event_id.into()))
                .cloned())
        }

        async fn record_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .wakes
                .insert((wake.task_id.clone(), wake.event_id.clone()), wake.clone())
                .is_none())
        }

        async fn resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError> {
            let mut state = self.state.lock().expect("tasks");
            let mut resolved = 0;
            for wake in state.wakes.values_mut().filter(|wake| wake.task_id == task_id) {
                if wake.resolved_at.is_none() {
                    wake.resolved_at = Some(Utc::now());
                    resolved += 1;
                }
            }
            Ok(resolved)
        }

        async fn interrupt_unfinished_runs(&self) -> Result<usize, TaskStoreError> {
            let mut state = self.state.lock().expect("tasks");
            state.interrupted += 1;
            Ok(1)
        }

        async fn transition_task(
            &self,
            workspace_id: &str,
            task_id: &str,
            expected_version: i64,
            next: TaskStatus,
            wake: Option<&TaskWake>,
        ) -> Result<AgentTask, TaskStoreError> {
            let mut state = self.state.lock().expect("tasks");
            let task = state
                .tasks
                .get_mut(task_id)
                .filter(|task| task.workspace_id == workspace_id)
                .ok_or(TaskStoreError::ConcurrentUpdate)?;
            if task.state_version != expected_version {
                return Err(TaskStoreError::ConcurrentUpdate);
            }
            task.status = next;
            task.state_version += 1;
            task.ended_at = next.is_terminal().then(Utc::now);
            let task = task.clone();
            if let Some(wake) = wake {
                state
                    .wakes
                    .insert((wake.task_id.clone(), wake.event_id.clone()), wake.clone());
            }
            Ok(task)
        }

        async fn bind_current_run(
            &self,
            task_id: &str,
            expected_version: i64,
            run_id: Option<&str>,
        ) -> Result<AgentTask, TaskStoreError> {
            let task = {
                let mut state = self.state.lock().expect("tasks");
                let task = state.tasks.get_mut(task_id).ok_or(TaskStoreError::ConcurrentUpdate)?;
                if task.state_version != expected_version {
                    return Err(TaskStoreError::ConcurrentUpdate);
                }
                task.current_run_id = run_id.map(str::to_string);
                task.state_version += 1;
                task.clone()
            };
            let gate = self.bind_gate.lock().expect("bind gate").clone();
            if let Some((entered, release)) = gate {
                entered.notify_one();
                release.notified().await;
            }
            Ok(task)
        }

        async fn begin_run(
            &self,
            task: &AgentTask,
            wake_event_id: &str,
            _run_id: &str,
        ) -> Result<bool, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .runs
                .insert((task.id.clone(), wake_event_id.into())))
        }

        async fn finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError> {
            self.state.lock().expect("tasks").reports.push(report.clone());
            Ok(())
        }

        async fn latest_task_run(&self, task_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
            Ok(self
                .state
                .lock()
                .expect("tasks")
                .reports
                .iter()
                .rev()
                .find(|report| report.task_id.as_deref() == Some(task_id))
                .cloned())
        }

        async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError> {
            let mut state = self.state.lock().expect("tasks");
            let mut claimed = state
                .wakes
                .values()
                .filter(|wake| {
                    wake.dispatch_state == WakeDispatchState::Pending
                        && wake.dispatched_at.is_none()
                        && wake.resolved_at.is_none()
                })
                .cloned()
                .collect::<Vec<_>>();
            claimed.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.task_id.cmp(&b.task_id))
                    .then_with(|| a.event_id.cmp(&b.event_id))
            });
            claimed.truncate(limit.max(0) as usize);
            for wake in &claimed {
                if let Some(stored) = state.wakes.get_mut(&(wake.task_id.clone(), wake.event_id.clone())) {
                    stored.dispatched_at = Some(Utc::now());
                }
            }
            Ok(claimed)
        }

        async fn recover_claimed_wakes(&self) -> Result<usize, TaskStoreError> {
            let mut state = self.state.lock().expect("tasks");
            let mut recovered = 0;
            for wake in state
                .wakes
                .values_mut()
                .filter(|wake| wake.dispatch_state == WakeDispatchState::Pending && wake.dispatched_at.is_some())
            {
                wake.dispatched_at = None;
                recovered += 1;
            }
            Ok(recovered)
        }

        async fn mark_wake(
            &self,
            task_id: &str,
            event_id: &str,
            next: WakeDispatchState,
        ) -> Result<(), TaskStoreError> {
            if let Some(wake) = self
                .state
                .lock()
                .expect("tasks")
                .wakes
                .get_mut(&(task_id.into(), event_id.into()))
            {
                wake.dispatch_state = next;
                wake.dispatched_at = match next {
                    WakeDispatchState::Dispatched => Some(Utc::now()),
                    WakeDispatchState::Pending => None,
                    WakeDispatchState::Dead => wake.dispatched_at,
                };
            }
            Ok(())
        }

        async fn retry_dead_wake(&self, _workspace_id: &str, _event_id: &str) -> Result<bool, TaskStoreError> {
            Ok(false)
        }

        async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError> {
            let mut tasks = self
                .state
                .lock()
                .expect("tasks")
                .tasks
                .values()
                .filter(|task| !task.status.is_terminal())
                .filter(|task| workspace_id.is_none_or(|workspace| workspace == task.workspace_id))
                .cloned()
                .collect::<Vec<_>>();
            tasks.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(tasks)
        }
    }

    #[derive(Default)]
    struct MemoryWatches {
        rules: Mutex<Vec<WatchRule>>,
        deleted_tasks: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl WatchRuleStore for MemoryWatches {
        async fn list_all(&self) -> Result<Vec<WatchRule>, String> {
            Ok(self.rules.lock().expect("watches").clone())
        }
        async fn insert(&self, rule: &WatchRule) -> Result<(), String> {
            self.rules.lock().expect("watches").push(rule.clone());
            Ok(())
        }
        async fn delete(&self, workspace_id: &str, id: &str) -> Result<bool, String> {
            let mut rules = self.rules.lock().expect("watches");
            let before = rules.len();
            rules.retain(|rule| rule.id != id || rule.workspace_id != workspace_id);
            Ok(rules.len() != before)
        }
        async fn complete_task_watch(&self, workspace_id: &str, id: &str, _wake: &TaskWake) -> Result<bool, String> {
            self.delete(workspace_id, id).await
        }
        async fn delete_task_watches(&self, workspace_id: &str, task_id: &str) -> Result<usize, String> {
            self.deleted_tasks
                .lock()
                .expect("deleted tasks")
                .push((workspace_id.into(), task_id.into()));
            let mut rules = self.rules.lock().expect("watches");
            let before = rules.len();
            rules.retain(|rule| rule.workspace_id != workspace_id || rule.owner_task_id.as_deref() != Some(task_id));
            Ok(before - rules.len())
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        signals: Mutex<Vec<WakeSignal>>,
    }

    impl DirectiveSink for RecordingSink {
        fn enqueue(&self, signal: WakeSignal) -> Result<(), crate::runtime::thing_agent::scheduler::EnqueueError> {
            self.signals.lock().expect("signals").push(signal);
            Ok(())
        }
    }

    struct EmptyCache;

    impl ThingCacheSource for EmptyCache {
        fn all(&self) -> Vec<Thing> {
            vec![]
        }
        fn get(&self, _id: &str) -> Option<Thing> {
            None
        }
        fn get_by_name(&self, _name: &str) -> Option<Thing> {
            None
        }
        fn insert(&self, _device: Thing) {}
        fn update(&self, _device: Thing) {}
        fn remove(&self, _id: &str) {}
    }

    struct NoopLedger;

    #[async_trait]
    impl HubLedger for NoopLedger {
        async fn append(
            &self,
            _workspace_id: &str,
            _kind: &str,
            _thing_id: Option<&str>,
            _payload: serde_json::Value,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct ImmediateAgent {
        prompts: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl AgentLoop for ImmediateAgent {
        async fn turn_streamed(
            &self,
            user_message: &str,
            _event_tx: mpsc::Sender<TurnEvent>,
            _cancel_token: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            self.prompts.lock().expect("prompts").push(user_message.into());
            Ok("done".into())
        }
        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            Ok("done".into())
        }
        async fn clear_history(&self) {}
        async fn seed_history(&self, _messages: &[crate::port::provider::ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct CountingProvider {
        calls: Mutex<usize>,
        prompts: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl AutonomousAgentProvider for CountingProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            *self.calls.lock().expect("calls") += 1;
            Ok(Arc::new(tokio::sync::Mutex::new(ImmediateAgent {
                prompts: self.prompts.clone(),
            })))
        }
        fn invalidate(&self, _workspace_id: &str) {}
    }

    #[derive(Default)]
    struct LateSteeringProvider {
        calls: Mutex<usize>,
        prompts: Arc<Mutex<Vec<String>>>,
        coordinator: Mutex<Option<std::sync::Weak<TaskCoordinator>>>,
        injected: Arc<std::sync::atomic::AtomicBool>,
    }

    struct LateSteeringAgent {
        prompts: Arc<Mutex<Vec<String>>>,
        coordinator: std::sync::Weak<TaskCoordinator>,
        injected: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl AgentLoop for LateSteeringAgent {
        async fn turn_streamed(
            &self,
            user_message: &str,
            _event_tx: mpsc::Sender<TurnEvent>,
            _cancel_token: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            self.prompts.lock().expect("prompts").push(user_message.into());
            if !self.injected.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let coordinator = self.coordinator.upgrade().expect("coordinator");
                assert_eq!(coordinator.advance("task_1", "event_late").await, Ok(Advance::Steered));
                coordinator
                    .store
                    .mark_wake("task_1", "event_late", WakeDispatchState::Dispatched)
                    .await
                    .expect("acknowledge steered wake");
            }
            Ok("done".into())
        }
        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            Ok("done".into())
        }
        async fn clear_history(&self) {}
        async fn seed_history(&self, _messages: &[crate::port::provider::ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl AutonomousAgentProvider for LateSteeringProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            *self.calls.lock().expect("calls") += 1;
            Ok(Arc::new(tokio::sync::Mutex::new(LateSteeringAgent {
                prompts: self.prompts.clone(),
                coordinator: self
                    .coordinator
                    .lock()
                    .expect("coordinator")
                    .clone()
                    .expect("coordinator installed"),
                injected: self.injected.clone(),
            })))
        }
        fn invalidate(&self, _workspace_id: &str) {}
    }

    #[derive(Default)]
    struct MemoryJobs {
        jobs: Mutex<BTreeMap<String, DeviceJob>>,
    }

    #[async_trait]
    impl DeviceJobStore for MemoryJobs {
        async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
            self.jobs.lock().expect("jobs").insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs")
                .get(job_id)
                .filter(|job| job.workspace_id == workspace_id)
                .cloned())
        }

        async fn get_task_job(
            &self,
            workspace_id: &str,
            task_id: &str,
            job_id: &str,
        ) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs")
                .get(job_id)
                .filter(|job| job.workspace_id == workspace_id && job.task_id.as_deref() == Some(task_id))
                .cloned())
        }

        async fn get_job_by_tool_call(&self, tool_call_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs")
                .values()
                .find(|job| job.tool_call_id.as_deref() == Some(tool_call_id))
                .cloned())
        }

        async fn submit_approved_job(&self, job: &DeviceJob) -> Result<(Option<DeviceJob>, bool), DeviceJobError> {
            self.jobs.lock().expect("jobs").insert(job.id.clone(), job.clone());
            Ok((Some(job.clone()), true))
        }

        async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            job.state = DeviceJobState::Running;
            job.driver_call_id = Some(driver_call_id.into());
            Ok(job.clone())
        }

        async fn finish_job(
            &self,
            job_id: &str,
            state: DeviceJobState,
            result: &serde_json::Value,
            _wake: Option<&TaskWake>,
        ) -> Result<bool, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            if job.state.is_terminal() {
                return Ok(false);
            }
            job.state = state;
            job.result_json = Some(result.clone());
            job.ended_at = Some(Utc::now());
            Ok(true)
        }

        async fn update_progress(&self, job_id: &str, progress: &serde_json::Value) -> Result<bool, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            job.progress_json = Some(progress.clone());
            Ok(true)
        }

        async fn request_stop(
            &self,
            workspace_id: &str,
            task_id: &str,
            job_id: &str,
        ) -> Result<DeviceJob, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs");
            let job = jobs
                .get_mut(job_id)
                .filter(|job| job.workspace_id == workspace_id && job.task_id.as_deref() == Some(task_id))
                .ok_or(DeviceJobError::NotFound)?;
            if job.state.is_active() {
                job.stop_requested_at.get_or_insert_with(Utc::now);
            }
            Ok(job.clone())
        }

        async fn active_jobs(
            &self,
            workspace_id: Option<&str>,
            unknown_only: bool,
        ) -> Result<Vec<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs")
                .values()
                .filter(|job| workspace_id.is_none_or(|workspace| workspace == job.workspace_id))
                .filter(|job| {
                    if unknown_only {
                        job.state == DeviceJobState::CancelledUnknown
                    } else {
                        job.state.is_active()
                    }
                })
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct RecordingWorker {
        cancelled: Mutex<Vec<String>>,
    }

    impl JobDeviceWorker for RecordingWorker {
        fn submit(&self, _cmd: ThingCommand, _job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
            Ok(())
        }
        fn cancel_job(&self, job_id: &str) -> bool {
            self.cancelled.lock().expect("cancelled").push(job_id.into());
            true
        }
        fn has_recoverable_call(&self, _job_id: &str) -> bool {
            true
        }
        fn complete_job(&self, _job_id: &str) {}
    }

    struct RecordingWorkers {
        worker: Arc<RecordingWorker>,
    }

    impl JobWorkerLocator for RecordingWorkers {
        fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
            Some(self.worker.clone())
        }
    }

    struct UnusedValidator;

    #[async_trait]
    impl AccessCallValidator for UnusedValidator {
        async fn validate(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _tool: &str,
            _params: Option<&serde_json::Value>,
        ) -> Result<
            tinyiothub_runtime::access::validator::ValidatedCall,
            tinyiothub_runtime::access::validator::CallError,
        > {
            Err(tinyiothub_runtime::access::validator::CallError::Unavailable(
                "unused".into(),
            ))
        }
    }

    struct UnusedResources;

    #[async_trait]
    impl ResourceResolver for UnusedResources {
        async fn resolve_action(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _tool_name: &str,
        ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
            Err("unused".into())
        }
        async fn resolve_properties(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _properties: &[String],
        ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
            Err("unused".into())
        }
    }

    struct Fixture {
        coordinator: Arc<TaskCoordinator>,
        store: Arc<MemoryTasks>,
        sink: Arc<RecordingSink>,
        provider: Arc<CountingProvider>,
        watches: Arc<MemoryWatches>,
        watch_cmds: Arc<dashmap::DashMap<String, mpsc::Sender<crate::runtime::thing_agent::WatchCommand>>>,
        active: ActiveRunRegistry,
        token: Option<CancellationToken>,
        _guard: Option<ActiveRunGuard>,
    }

    impl Fixture {
        fn new(task: AgentTask) -> Self {
            Self::new_with_jobs(task, None)
        }

        fn new_with_jobs(task: AgentTask, jobs: Option<JobService>) -> Self {
            Self::new_with_jobs_and_capacity(task, jobs, 4)
        }

        fn new_with_jobs_and_capacity(task: AgentTask, jobs: Option<JobService>, capacity: usize) -> Self {
            let store = Arc::new(MemoryTasks::default());
            store.insert(task.clone());
            let sink = Arc::new(RecordingSink::default());
            let provider = Arc::new(CountingProvider::default());
            let watches = Arc::new(MemoryWatches::default());
            let watch_cmds = Arc::new(dashmap::DashMap::new());
            let events = Arc::new(AgentEventBus::new(16));
            let active = ActiveRunRegistry::new(capacity);
            let mut guard = None;
            let mut token = None;
            if let Some(run_id) = task.current_run_id.clone() {
                let ctx = RunContext::new(run_id, task.workspace_id.clone(), "task".into());
                let cancel = CancellationToken::new();
                guard = active.register(ctx, cancel.clone());
                token = Some(cancel);
            }
            let coordinator = Arc::new(TaskCoordinator::new(
                store.clone(),
                jobs,
                watches.clone(),
                watch_cmds.clone(),
                events,
                active.clone(),
                Arc::new(Runner::new()),
                provider.clone(),
            ));
            Self {
                coordinator,
                store,
                sink,
                provider,
                watches,
                watch_cmds,
                active,
                token,
                _guard: guard,
            }
        }

        fn add_wake(&self, event_id: &str) {
            self.store.insert_wake(wake(event_id));
        }
    }

    fn task(status: TaskStatus) -> AgentTask {
        AgentTask {
            id: "task_1".into(),
            workspace_id: "ws_1".into(),
            idempotency_key: "idem_1".into(),
            objective: "objective".into(),
            session_key: Some("session_1".into()),
            created_by: "user_1".into(),
            status,
            current_run_id: None,
            state_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ended_at: None,
        }
    }

    fn wake(event_id: &str) -> TaskWake {
        let signal = WakeSignal {
            workspace_id: "ws_1".into(),
            priority: Priority::Normal,
            dedup_key: Some(format!("wake:{event_id}")),
            source: TriggerSource::UserDirective {
                user_id: "user_1".into(),
                text: "continue".into(),
                session_key: Some("session_1".into()),
                source: None,
                problem_key: None,
            },
        };
        TaskWake {
            task_id: "task_1".into(),
            event_id: event_id.into(),
            signal_json: serde_json::to_string(&signal).expect("wake JSON"),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn owned_watch(id: &str) -> WatchRule {
        WatchRule {
            id: id.into(),
            workspace_id: "ws_1".into(),
            condition: tinyiothub_core::watch::WatchCondition::Availability {
                thing_id: "thing_1".into(),
                online: false,
            },
            persistent: false,
            timeout_secs: 600,
            created_at: Utc::now().to_rfc3339(),
            owner_task_id: Some("task_1".into()),
        }
    }

    fn job(id: &str, task_id: &str, state: DeviceJobState) -> DeviceJob {
        DeviceJob {
            id: id.into(),
            workspace_id: "ws_1".into(),
            device_id: format!("device_{id}"),
            tool: "set_speed".into(),
            args_json: json!({"speed": 1}),
            state,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(task_id.into()),
            run_id: Some("run_existing".into()),
            tool_call_id: Some(format!("call_{id}")),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    fn jobs() -> (JobService, Arc<MemoryJobs>, Arc<RecordingWorker>) {
        let store = Arc::new(MemoryJobs::default());
        let worker = Arc::new(RecordingWorker::default());
        let service = JobService::new(
            store.clone(),
            Arc::new(RecordingWorkers { worker: worker.clone() }),
            Arc::new(UnusedValidator),
            Arc::new(UnusedResources),
            Arc::new(ArbitrationRegistry::default()),
        );
        (service, store, worker)
    }

    #[tokio::test]
    async fn approval_decision_wake_defers_while_paused_and_resumes_without_execution_replay() {
        let paused = Fixture::new(task(TaskStatus::Paused));
        paused.store.insert_wake(TaskWake {
            signal_json:
                json!({"kind":"approval_decided","workspace_id":"ws_1","task_id":"task_1","approval_id":"approval_1"})
                    .to_string(),
            ..wake("decision_1")
        });
        assert_eq!(
            paused
                .coordinator
                .advance("task_1", "decision_1")
                .await
                .expect("paused"),
            Advance::Deferred
        );
        assert_eq!(
            paused
                .store
                .get_task("ws_1", "task_1")
                .await
                .expect("task")
                .expect("present")
                .status,
            TaskStatus::Paused
        );

        let decided = Fixture::new(task(TaskStatus::NeedsApproval));
        decided.store.insert_wake(TaskWake {
            signal_json:
                json!({"kind":"approval_decided","workspace_id":"ws_1","task_id":"task_1","approval_id":"approval_1"})
                    .to_string(),
            ..wake("decision_1")
        });
        assert!(matches!(
            decided
                .coordinator
                .advance("task_1", "decision_1")
                .await
                .expect("resume"),
            Advance::Started { .. }
        ));
    }

    #[tokio::test]
    async fn cancel_stops_only_owned_jobs_with_job_scoped_cancellation() {
        let (jobs, store, worker) = jobs();
        store
            .create_job(&job("job_1", "task_1", DeviceJobState::Running))
            .await
            .expect("owned job");
        store
            .create_job(&job("job_2", "task_2", DeviceJobState::Running))
            .await
            .expect("other job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::WaitingJob), Some(jobs.clone()));
        fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        assert_eq!(worker.cancelled.lock().expect("cancelled").as_slice(), &["job_1"]);
        assert!(store.jobs.lock().expect("jobs")["job_1"].stop_requested_at.is_some());
        assert!(store.jobs.lock().expect("jobs")["job_2"].stop_requested_at.is_none());
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Cancelling);
        assert!(
            !fixture
                .coordinator
                .cancel("ws_1", "task_1")
                .await
                .expect("repeat cancel")
                .accepted
        );
        assert_eq!(worker.cancelled.lock().expect("cancelled").as_slice(), &["job_1"]);
        store
            .finish_job("job_1", DeviceJobState::Cancelled, &json!({"cancelled":true}), None)
            .await
            .expect("job terminal");
        fixture.add_wake("job_1_cancelled");
        assert_eq!(
            fixture
                .coordinator
                .advance("task_1", "job_1_cancelled")
                .await
                .expect("finish cancellation"),
            Advance::Recorded
        );
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Cancelled);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn cancel_maps_any_unknown_job_outcome_to_failed() {
        let (jobs, store, worker) = jobs();
        store
            .create_job(&job("job_unknown", "task_1", DeviceJobState::CancelledUnknown))
            .await
            .expect("unknown job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::WaitingJob), Some(jobs));
        fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        assert!(worker.cancelled.lock().expect("cancelled").is_empty());
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn repeated_pause_returns_the_current_task_without_advancing() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        let paused = fixture.coordinator.pause("ws_1", "task_1").await.expect("first pause");
        let replay = fixture
            .coordinator
            .pause("ws_1", "task_1")
            .await
            .expect("idempotent pause");
        assert_eq!(replay, paused);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn result_before_cancel_preserves_the_terminal_task_projection() {
        let mut running = task(TaskStatus::Active);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running.clone());
        fixture
            .coordinator
            .finish_task_run(&running, "run_existing", Outcome::Acted)
            .await;
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Completed);
        let outcome = fixture.coordinator.cancel("ws_1", "task_1").await.expect("late cancel");
        assert!(!outcome.accepted);
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Completed);
    }

    #[tokio::test]
    async fn pause_records_wake_without_starting_or_steering_run() {
        let mut running = task(TaskStatus::Active);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running);
        let token = fixture.token.clone().expect("active token");
        fixture.coordinator.pause("ws_1", "task_1").await.expect("pause");
        fixture.add_wake("event_2");
        assert_eq!(
            fixture.coordinator.advance("task_1", "event_2").await,
            Ok(Advance::Deferred)
        );
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
        assert!(fixture.sink.signals.lock().expect("signals").is_empty());
        assert!(!token.is_cancelled());
        assert_eq!(
            fixture.store.wake("task_1", "event_2").dispatch_state,
            WakeDispatchState::Pending
        );
    }

    #[tokio::test]
    async fn resume_releases_a_pending_wake_at_most_once() {
        let fixture = Fixture::new(task(TaskStatus::Paused));
        fixture.add_wake("event_resume");
        fixture.coordinator.resume("ws_1", "task_1").await.expect("resume");
        let router = WakeRouter::new(fixture.store.clone(), fixture.coordinator.clone());
        router.run(10).await.expect("first dispatch");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while *fixture.provider.calls.lock().expect("calls") == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("run started");
        router.run(10).await.expect("replay dispatch");
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 1);
        assert_eq!(fixture.store.state.lock().expect("tasks").runs.len(), 1);
    }

    #[tokio::test]
    async fn resume_restores_the_persisted_waiting_watch_relationship() {
        let fixture = Fixture::new(task(TaskStatus::Paused));
        fixture.watches.insert(&owned_watch("watch_1")).await.expect("watch");
        let resumed = fixture.coordinator.resume("ws_1", "task_1").await.expect("resume");
        assert_eq!(resumed.status, TaskStatus::WaitingWatch);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn recovery_restores_active_job_relationship_without_a_recovery_run() {
        let (jobs, store, _worker) = jobs();
        store
            .create_job(&job("job_1", "task_1", DeviceJobState::Running))
            .await
            .expect("job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::Active), Some(jobs));
        assert_eq!(fixture.coordinator.recover().await.expect("recover"), 0);
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::WaitingJob);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn recovery_finishes_cancellation_after_owned_jobs_are_terminal() {
        let (jobs, store, _worker) = jobs();
        store
            .create_job(&job("job_1", "task_1", DeviceJobState::Cancelled))
            .await
            .expect("terminal job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::Cancelling), Some(jobs));
        assert_eq!(fixture.coordinator.recover().await.expect("recover"), 0);
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Cancelled);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn task_prompt_uses_the_existing_bounded_workspace_snapshot() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        let (tx, mut rx) = mpsc::channel(1);
        fixture.watch_cmds.insert("ws_1".into(), tx);
        tokio::spawn(async move {
            let Some(crate::runtime::thing_agent::WatchCommand::Snapshot { reply }) = rx.recv().await else {
                panic!("expected snapshot request");
            };
            reply
                .send(Some(tinyiothub_runtime::watch::live_state::WorkspaceSnapshot {
                    v: 1,
                    devices: vec![tinyiothub_runtime::watch::live_state::DeviceSnapshot {
                        thing_id: "thing_1".into(),
                        name: "task snapshot device".into(),
                        health: tinyiothub_runtime::watch::live_state::Health::Ok,
                        online: true,
                        zone: "line-a".into(),
                        values: vec![("temperature".into(), "21".into())],
                        properties: vec!["temperature".into()],
                    }],
                    total: 1,
                }))
                .expect("snapshot reply");
        });
        fixture.add_wake("event_snapshot");
        fixture
            .coordinator
            .advance("task_1", "event_snapshot")
            .await
            .expect("advance");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fixture.provider.prompts.lock().expect("prompts").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("prompt");
        let prompt = fixture.provider.prompts.lock().expect("prompts")[0].clone();
        assert!(prompt.contains("task snapshot device"), "{prompt}");
        assert!(prompt.contains("temperature"), "{prompt}");
    }

    #[tokio::test]
    async fn task_prompt_loads_persistent_stage_result_jobs_and_owned_watches() {
        let (jobs, job_store, _worker) = jobs();
        job_store
            .create_job(&job("job_1", "task_1", DeviceJobState::Running))
            .await
            .expect("job");
        job_store
            .create_job(&job("job_2", "task_2", DeviceJobState::Running))
            .await
            .expect("other job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::Active), Some(jobs));
        fixture
            .watches
            .insert(&owned_watch("watch_1"))
            .await
            .expect("owned watch");
        let mut other_watch = owned_watch("watch_other");
        other_watch.owner_task_id = Some("task_2".into());
        fixture.watches.insert(&other_watch).await.expect("other watch");
        fixture.store.state.lock().expect("tasks").reports.push(RunReport {
            run_id: "run_previous".into(),
            workspace_id: "ws_1".into(),
            trigger: "task:task_1".into(),
            outcome: tinyiothub_core::agent_runs::Outcome::Acted,
            summary: "persisted stage result".into(),
            actions: vec![],
            verified: true,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some("task_1".into()),
            wake_event_id: Some("event_previous".into()),
        });
        fixture.add_wake("event_prompt");
        fixture
            .coordinator
            .advance("task_1", "event_prompt")
            .await
            .expect("advance");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fixture.provider.prompts.lock().expect("prompts").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("prompt");
        let prompt = fixture.provider.prompts.lock().expect("prompts")[0].clone();
        assert!(prompt.contains("persisted stage result"), "{prompt}");
        assert!(prompt.contains("job_1"), "{prompt}");
        assert!(!prompt.contains("job_2"), "{prompt}");
        assert!(prompt.contains("watch_1"), "{prompt}");
        assert!(!prompt.contains("watch_other"), "{prompt}");
    }

    #[tokio::test]
    async fn successful_run_waits_for_owned_job_instead_of_completing() {
        let (jobs, store, _worker) = jobs();
        store
            .create_job(&job("job_1", "task_1", DeviceJobState::Running))
            .await
            .expect("job");
        let fixture = Fixture::new_with_jobs(task(TaskStatus::Active), Some(jobs));
        fixture.add_wake("event_job");
        fixture
            .coordinator
            .advance("task_1", "event_job")
            .await
            .expect("advance");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fixture.store.task("task_1").status == TaskStatus::Active {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("run settled");
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::WaitingJob);
    }

    #[tokio::test]
    async fn successful_run_waits_for_owned_watch_instead_of_completing() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        fixture.watches.insert(&owned_watch("watch_1")).await.expect("watch");
        fixture.add_wake("event_watch");
        fixture
            .coordinator
            .advance("task_1", "event_watch")
            .await
            .expect("advance");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fixture.store.task("task_1").status == TaskStatus::Active {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("run settled");
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::WaitingWatch);
    }

    #[tokio::test]
    async fn recovery_interrupts_stale_model_run_and_starts_one_recovery_run() {
        let mut active = task(TaskStatus::Active);
        active.current_run_id = Some("run_stale".into());
        let mut fixture = Fixture::new(active);
        fixture._guard = None;
        fixture.token = None;
        assert_eq!(fixture.coordinator.recover().await.expect("recover"), 1);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while *fixture.provider.calls.lock().expect("calls") == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("recovery run started");
        assert_eq!(fixture.store.state.lock().expect("tasks").interrupted, 1);
        assert!(
            fixture
                .store
                .state
                .lock()
                .expect("tasks")
                .runs
                .contains(&("task_1".into(), "recovery:task_1".into()))
        );
        assert_eq!(fixture.coordinator.recover().await.expect("repeat recovery"), 0);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 1);
    }

    #[tokio::test]
    async fn recovery_preserves_waiting_and_paused_tasks_without_model_runs() {
        let waiting = Fixture::new(task(TaskStatus::WaitingWatch));
        assert_eq!(waiting.coordinator.recover().await.expect("recover waiting"), 0);
        assert_eq!(*waiting.provider.calls.lock().expect("calls"), 0);
        assert_eq!(waiting.store.task("task_1").status, TaskStatus::WaitingWatch);
        let paused = Fixture::new(task(TaskStatus::Paused));
        assert_eq!(paused.coordinator.recover().await.expect("recover paused"), 0);
        assert_eq!(*paused.provider.calls.lock().expect("calls"), 0);
        assert_eq!(paused.store.task("task_1").status, TaskStatus::Paused);
    }

    #[tokio::test]
    async fn cancel_aborts_the_shared_run_token_before_terminal_recording() {
        let mut running = task(TaskStatus::Active);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running);
        let token = fixture.token.clone().expect("active token");
        let outcome = fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        assert!(outcome.accepted);
        assert!(token.is_cancelled());
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn cancel_resolves_owned_watches_and_pending_outbox() {
        let mut running = task(TaskStatus::Active);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running);
        fixture.add_wake("event_pending");
        fixture.watches.insert(&owned_watch("watch_1")).await.expect("watch");
        assert!(fixture.active.get("run_existing").is_some());
        let outcome = fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        assert!(outcome.accepted);
        assert!(fixture.store.wake("task_1", "event_pending").resolved_at.is_some());
        assert!(fixture.watches.rules.lock().expect("watches").is_empty());
        assert_eq!(
            fixture.watches.deleted_tasks.lock().expect("deleted").as_slice(),
            &[("ws_1".into(), "task_1".into())]
        );
        let router = WakeRouter::new(fixture.store.clone(), fixture.coordinator.clone());
        assert_eq!(router.run(10).await.expect("no cancelled wake"), 0);
    }

    #[tokio::test]
    async fn cancel_removes_owned_rule_from_a_running_watch_engine() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        let rule = owned_watch("watch_live");
        fixture.watches.insert(&rule).await.expect("persisted watch");
        let (hit_tx, mut hit_rx) = mpsc::channel(4);
        let live = LiveState::new(Arc::new(EmptyCache));
        let mut engine = WatchEngine::new(live, Arc::new(NoopLedger), fixture.watches.clone(), hit_tx);
        let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
        fixture.watch_cmds.insert("ws_1".into(), cmd_tx.clone());
        let (change_tx, mut change_rx) = mpsc::channel::<(bool, tokio::sync::oneshot::Sender<()>)>(2);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let engine_task = tokio::spawn(async move {
            engine.upsert_rule(rule).await.expect("live rule");
            ready_tx.send(()).expect("ready");
            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        let Some(cmd) = cmd else { break };
                        match cmd {
                            crate::runtime::thing_agent::WatchCommand::Remove(id) => {
                                engine.remove_rule(&id);
                            }
                            crate::runtime::thing_agent::WatchCommand::Snapshot { reply } => {
                                let _ = reply.send(None);
                            }
                            _ => {}
                        }
                    }
                    change = change_rx.recv() => {
                        let Some((online, done)) = change else { break };
                        engine.on_availability("ws_1", "thing_1", online).await;
                        let _ = done.send(());
                    }
                }
            }
        });
        ready_rx.await.expect("engine ready");
        fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        let (snapshot_tx, snapshot_rx) = tokio::sync::oneshot::channel();
        cmd_tx
            .send(crate::runtime::thing_agent::WatchCommand::Snapshot { reply: snapshot_tx })
            .await
            .expect("snapshot command");
        snapshot_rx.await.expect("snapshot processed");
        let (changed_tx, changed_rx) = tokio::sync::oneshot::channel();
        change_tx.send((false, changed_tx)).await.expect("availability change");
        changed_rx.await.expect("change processed");
        assert!(
            hit_rx.try_recv().is_err(),
            "cancelled live rule must not emit another hit"
        );
        assert!(fixture.watches.rules.lock().expect("watches").is_empty());
        engine_task.abort();
    }

    #[tokio::test]
    async fn task_run_started_and_report_share_wake_identity() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        fixture.add_wake("event_1");
        let mut events = fixture.coordinator.events.subscribe();
        let advance = fixture.coordinator.advance("task_1", "event_1").await.expect("advance");
        let Advance::Started { run_id } = advance else {
            panic!("expected started run");
        };
        let started = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .expect("started timeout")
            .expect("started event");
        match started.kind {
            AgentEventKind::RunStarted {
                run_id: started_run,
                task_id,
                wake_event_id,
                ..
            } => {
                assert_eq!(started_run, run_id);
                assert_eq!(task_id.as_deref(), Some("task_1"));
                assert_eq!(wake_event_id.as_deref(), Some("event_1"));
            }
            _ => panic!("expected RunStarted"),
        }
        let recorded = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .expect("recorded timeout")
            .expect("recorded event");
        match recorded.kind {
            AgentEventKind::RunRecorded { report, .. } => {
                assert_eq!(report.run_id, run_id);
                assert_eq!(report.task_id.as_deref(), Some("task_1"));
                assert_eq!(report.wake_event_id.as_deref(), Some("event_1"));
            }
            _ => panic!("expected RunRecorded"),
        }
    }

    #[tokio::test]
    async fn replayed_run_pair_is_duplicate_without_another_execution() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        fixture.add_wake("event_1");
        fixture
            .store
            .state
            .lock()
            .expect("tasks")
            .runs
            .insert(("task_1".into(), "event_1".into()));
        assert_eq!(
            fixture.coordinator.advance("task_1", "event_1").await,
            Ok(Advance::Duplicate)
        );
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn run_completion_rejects_waiting_watch_to_waiting_job() {
        let (jobs, job_store, _worker) = jobs();
        job_store
            .create_job(&job("job_1", "task_1", DeviceJobState::Running))
            .await
            .expect("active job");
        let mut running = task(TaskStatus::WaitingWatch);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new_with_jobs(running.clone(), Some(jobs));
        fixture
            .coordinator
            .finish_task_run(&running, "run_existing", Outcome::Acted)
            .await;
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::WaitingWatch);
    }

    #[tokio::test]
    async fn run_completion_rejects_waiting_job_to_waiting_watch() {
        let mut running = task(TaskStatus::WaitingJob);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running.clone());
        fixture.watches.insert(&owned_watch("watch_1")).await.expect("watch");
        fixture
            .coordinator
            .finish_task_run(&running, "run_existing", Outcome::Acted)
            .await;
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::WaitingJob);
    }

    #[tokio::test]
    async fn cancel_after_run_binding_before_registration_still_prevents_model_execution() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        fixture.add_wake("event_cancel_window");
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        *fixture.store.bind_gate.lock().expect("bind gate") = Some((entered.clone(), release.clone()));
        let coordinator = fixture.coordinator.clone();
        let advancing = tokio::spawn(async move { coordinator.advance("task_1", "event_cancel_window").await });
        entered.notified().await;
        fixture.coordinator.cancel("ws_1", "task_1").await.expect("cancel");
        fixture.store.bind_gate.lock().expect("bind gate").take();
        release.notify_one();
        advancing.await.expect("advance join").expect("advance result");
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(fixture.provider.prompts.lock().expect("prompts").is_empty());
        assert_eq!(fixture.store.task("task_1").status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn registry_capacity_refusal_never_executes_an_unregistered_task_run() {
        let fixture = Fixture::new_with_jobs_and_capacity(task(TaskStatus::Active), None, 0);
        fixture.add_wake("event_capacity");
        assert!(fixture.coordinator.advance("task_1", "event_capacity").await.is_err());
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(fixture.provider.prompts.lock().expect("prompts").is_empty());
    }

    #[tokio::test]
    async fn active_task_wake_steers_without_starting_another_run() {
        let mut running = task(TaskStatus::Active);
        running.current_run_id = Some("run_existing".into());
        let fixture = Fixture::new(running);
        fixture.add_wake("event_2");
        assert_eq!(
            fixture.coordinator.advance("task_1", "event_2").await,
            Ok(Advance::Steered)
        );
        assert_eq!(fixture.sink.signals.lock().expect("signals").len(), 0);
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
        let active = fixture.active.get("run_existing").expect("registered Task Run");
        let steering = active.steering.lock().expect("steering buffer");
        assert_eq!(steering.len(), 1);
        assert!(steering[0].contains("event_2"));
        assert!(steering[0].contains("continue"));
    }

    #[tokio::test]
    async fn steering_after_final_completion_is_dispatched_with_original_wake_id() {
        let mut fixture = Fixture::new(task(TaskStatus::Active));
        let provider = Arc::new(LateSteeringProvider::default());
        fixture.coordinator = Arc::new(TaskCoordinator::new(
            fixture.store.clone(),
            None,
            fixture.watches.clone(),
            fixture.watch_cmds.clone(),
            Arc::new(AgentEventBus::new(16)),
            fixture.active.clone(),
            Arc::new(Runner::new()),
            provider.clone(),
        ));
        *provider.coordinator.lock().expect("coordinator") = Some(Arc::downgrade(&fixture.coordinator));
        fixture.add_wake("event_initial");
        fixture.add_wake("event_late");

        assert!(matches!(
            fixture.coordinator.advance("task_1", "event_initial").await,
            Ok(Advance::Started { .. })
        ));
        for _ in 0..100 {
            if *provider.calls.lock().expect("calls") == 2
                && fixture.store.task("task_1").status == TaskStatus::Completed
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert_eq!(
            *provider.calls.lock().expect("calls"),
            2,
            "an acknowledged late wake must start a follow-up Run"
        );
        let prompts = provider.prompts.lock().expect("prompts");
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("event_late"));
        assert!(
            fixture
                .store
                .state
                .lock()
                .expect("tasks")
                .runs
                .contains(&("task_1".into(), "event_late".into()))
        );
    }

    #[tokio::test]
    async fn missing_wake_is_an_explicit_error() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        assert_eq!(
            fixture.coordinator.advance("task_1", "missing").await,
            Err(TaskError::WakeNotFound)
        );
    }

    #[tokio::test]
    async fn wake_router_restores_paused_claim_to_pending() {
        let fixture = Fixture::new(task(TaskStatus::Paused));
        fixture.add_wake("event_paused");
        let router = WakeRouter::new(fixture.store.clone(), fixture.coordinator.clone());
        assert_eq!(router.run(10).await.expect("router"), 1);
        assert_eq!(
            fixture.store.wake("task_1", "event_paused").dispatch_state,
            WakeDispatchState::Pending
        );
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn wake_router_marks_duplicate_dispatch_without_another_run() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        fixture.add_wake("event_duplicate");
        fixture
            .store
            .state
            .lock()
            .expect("tasks")
            .runs
            .insert(("task_1".into(), "event_duplicate".into()));
        let router = WakeRouter::new(fixture.store.clone(), fixture.coordinator.clone());
        router.run(10).await.expect("router");
        assert_eq!(
            fixture.store.wake("task_1", "event_duplicate").dispatch_state,
            WakeDispatchState::Dispatched
        );
        assert_eq!(*fixture.provider.calls.lock().expect("calls"), 0);
    }

    #[tokio::test]
    async fn wake_router_dead_letters_after_three_consecutive_failures() {
        let fixture = Fixture::new(task(TaskStatus::Active));
        let mut invalid = wake("event_invalid");
        invalid.signal_json = "not-json".into();
        fixture.store.insert_wake(invalid);
        let router = WakeRouter::new(fixture.store.clone(), fixture.coordinator.clone());
        for expected in [
            WakeDispatchState::Pending,
            WakeDispatchState::Pending,
            WakeDispatchState::Dead,
        ] {
            router.run(10).await.expect("router");
            assert_eq!(fixture.store.wake("task_1", "event_invalid").dispatch_state, expected);
        }
    }
}
