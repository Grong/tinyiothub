use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tinyiothub_core::agent_task::{TaskWake, WakeDispatchState};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::task_approval::ApprovedToolCall;
use tokio::sync::watch;

use super::worker_adapter::{JobResultSink, JobWorkerLocator};
use crate::access::resources::ResourceResolver;
use crate::access::validator::AccessCallValidator;
use crate::arbitration::{ArbitrationRegistry, ResourceMode, ResourceSpec};
use crate::ports::DeviceJobStore;

#[derive(Debug)]
pub struct JobRequest {
    pub job_id: Option<String>,
    pub workspace_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug)]
pub enum JobFinal {
    Done(Value),
    Failed(String),
    Rejected(String),
    Cancelled { within_grace: bool },
    Interrupted(String),
}

#[allow(clippy::large_enum_variant)] // Public wait contract keeps the terminal Job inline.
#[derive(Debug)]
pub enum JobWait {
    Final(DeviceJob),
    Running { job_id: String },
}

#[derive(Debug, Default)]
pub struct FinishOutcome {
    pub wake_created: bool,
    pub transitioned: bool,
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub stop_reasserted: usize,
    pub interrupted: usize,
}

struct Inner {
    store: Arc<dyn DeviceJobStore>,
    workers: Arc<dyn JobWorkerLocator>,
    validator: Arc<dyn AccessCallValidator>,
    resources: Arc<dyn ResourceResolver>,
    arbitration: Arc<ArbitrationRegistry>,
    deadlines: DashMap<String, tokio::time::Instant>,
    notifications: DashMap<String, watch::Sender<u64>>,
    workspaces: DashMap<String, String>,
    terminal_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Clone)]
pub struct JobService {
    inner: Arc<Inner>,
}

impl JobService {
    pub fn new(
        store: Arc<dyn DeviceJobStore>,
        workers: Arc<dyn JobWorkerLocator>,
        validator: Arc<dyn AccessCallValidator>,
        resources: Arc<dyn ResourceResolver>,
        arbitration: Arc<ArbitrationRegistry>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                workers,
                validator,
                resources,
                arbitration,
                deadlines: DashMap::new(),
                notifications: DashMap::new(),
                workspaces: DashMap::new(),
                terminal_locks: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn terminal_lock(&self, job_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.inner
            .terminal_locks
            .lock()
            .entry(job_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    fn notification(&self, job_id: &str) -> watch::Receiver<u64> {
        self.inner
            .notifications
            .entry(job_id.to_string())
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }

    fn notify(&self, job_id: &str) {
        if let Some(sender) = self.inner.notifications.get(job_id) {
            sender.send_modify(|version| *version += 1);
        }
    }

    fn prune_terminal(&self, job_id: &str) {
        self.inner.deadlines.remove(job_id);
        self.inner.workspaces.remove(job_id);
        self.inner.notifications.remove(job_id);
        self.inner.terminal_locks.lock().remove(job_id);
    }

    fn restore_deadline(&self, job: &DeviceJob) {
        let timeout = Duration::from_millis(job.timeout_ms);
        let elapsed = Utc::now()
            .signed_duration_since(job.started_at)
            .to_std()
            .unwrap_or(Duration::ZERO);
        self.inner.deadlines.insert(
            job.id.clone(),
            tokio::time::Instant::now() + timeout.saturating_sub(elapsed),
        );
    }

    pub async fn submit(&self, request: JobRequest) -> Result<DeviceJob, DeviceJobError> {
        let validated = self
            .inner
            .validator
            .validate(
                &request.workspace_id,
                &request.device_id,
                &request.tool,
                Some(&request.args_json),
            )
            .await
            .map_err(|error| DeviceJobError::Validation(error.to_string()))?;
        let claims = self
            .inner
            .resources
            .resolve_action(&request.workspace_id, &request.device_id, &request.tool)
            .await
            .map_err(DeviceJobError::Validation)?;
        let specs = claims
            .into_iter()
            .map(|claim| ResourceSpec {
                id: claim.id,
                mode: if claim.exclusive {
                    ResourceMode::Exclusive
                } else {
                    ResourceMode::Shared
                },
                capacity: claim.capacity,
            })
            .collect::<Vec<_>>();
        let job_id = request.job_id.unwrap_or_else(tinyiothub_core::generate_id);
        let timeout = Duration::from_millis(validated.timeout_ms);
        let lease = self
            .inner
            .arbitration
            .try_acquire_resources(&request.device_id, format!("device_job:{job_id}"), &specs, timeout)
            .map_err(|_| DeviceJobError::ExecutionConflict)?;
        let now = Utc::now();
        let job = DeviceJob {
            id: job_id.clone(),
            workspace_id: request.workspace_id,
            device_id: request.device_id,
            tool: request.tool,
            args_json: request.args_json,
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: request.task_id,
            run_id: request.run_id,
            tool_call_id: request.tool_call_id,
            driver_call_id: None,
            timeout_ms: validated.timeout_ms,
            started_at: now,
            ended_at: None,
        };
        self.inner.store.create_job(&job).await?;
        tracing::info!(metric="runtime_job_accepted", workspace_id=%job.workspace_id,
            task_id=?job.task_id, run_id=?job.run_id, job_id=%job.id,
            device_id=%job.device_id, "device job accepted");
        self.inner.workspaces.insert(job.id.clone(), job.workspace_id.clone());
        self.notification(&job.id);
        self.restore_deadline(&job);
        let cmd = ThingCommand {
            id: job.id.clone(),
            thing_id: job.device_id.clone(),
            name: job.tool.clone(),
            display_name: None,
            description: None,
            parameters: Some(job.args_json.to_string()),
            created_at: job.started_at.to_rfc3339(),
        };
        let submission = self
            .inner
            .workers
            .worker_for(&job.device_id)
            .ok_or_else(|| Error::Internal("device worker unavailable".into()))
            .and_then(|worker| worker.submit(cmd, &job.id, Some(lease)));
        if let Err(error) = submission {
            let result = json!({"error": error.to_string()});
            self.inner
                .store
                .finish_job(&job.id, DeviceJobState::Rejected, &result, self.wake_for(&job).as_ref())
                .await?;
            return Err(DeviceJobError::Validation(error.to_string()));
        }
        Ok(job)
    }

    pub async fn submit_approved(&self, call: ApprovedToolCall) -> Result<Option<DeviceJob>, DeviceJobError> {
        let validated = self
            .inner
            .validator
            .validate(&call.workspace_id, &call.device_id, &call.tool, Some(&call.args_json))
            .await
            .map_err(|error| DeviceJobError::Validation(error.to_string()))?;
        let matches = |job: &DeviceJob| {
            job.workspace_id == call.workspace_id
                && job.task_id.as_deref() == Some(call.task_id.as_str())
                && job.run_id.as_deref() == Some(call.run_id.as_str())
                && job.tool_call_id.as_deref() == Some(call.tool_call_id.as_str())
                && job.device_id == call.device_id
                && job.tool == call.tool
                && job.args_json == call.args_json
        };
        if let Some(existing) = self
            .inner
            .store
            .get_job_by_tool_call(&call.workspace_id, &call.tool_call_id)
            .await?
        {
            if !matches(&existing) {
                return Err(DeviceJobError::ExecutionConflict);
            }
            self.inner
                .workspaces
                .insert(existing.id.clone(), existing.workspace_id.clone());
            self.notification(&existing.id);
            if existing.state.is_active() {
                self.restore_deadline(&existing);
            }
            return Ok(Some(existing));
        }
        let claims = self
            .inner
            .resources
            .resolve_action(&call.workspace_id, &call.device_id, &call.tool)
            .await
            .map_err(DeviceJobError::Validation)?;
        let specs = claims
            .into_iter()
            .map(|claim| ResourceSpec {
                id: claim.id,
                mode: if claim.exclusive {
                    ResourceMode::Exclusive
                } else {
                    ResourceMode::Shared
                },
                capacity: claim.capacity,
            })
            .collect::<Vec<_>>();
        let job_id = tinyiothub_core::generate_id();
        let timeout = Duration::from_millis(validated.timeout_ms);
        let lease = self
            .inner
            .arbitration
            .try_acquire_resources(&call.device_id, format!("device_job:{job_id}"), &specs, timeout)
            .map_err(|_| DeviceJobError::ExecutionConflict)?;
        let job = DeviceJob {
            id: job_id,
            workspace_id: call.workspace_id.clone(),
            device_id: call.device_id.clone(),
            tool: call.tool.clone(),
            args_json: call.args_json.clone(),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(call.task_id.clone()),
            run_id: Some(call.run_id.clone()),
            tool_call_id: Some(call.tool_call_id.clone()),
            driver_call_id: None,
            timeout_ms: validated.timeout_ms,
            started_at: Utc::now(),
            ended_at: None,
        };
        let (job, inserted) = self.inner.store.submit_approved_job(&job).await?;
        let Some(job) = job else {
            return Ok(None);
        };
        if !matches(&job) {
            return Err(DeviceJobError::ExecutionConflict);
        }
        self.inner.workspaces.insert(job.id.clone(), job.workspace_id.clone());
        self.notification(&job.id);
        self.restore_deadline(&job);
        if !inserted {
            return Ok(Some(job));
        }
        tracing::info!(metric="runtime_job_accepted", workspace_id=%job.workspace_id,
            task_id=?job.task_id, run_id=?job.run_id, job_id=%job.id,
            device_id=%job.device_id, approved=true, "device job accepted");
        let cmd = ThingCommand {
            id: job.id.clone(),
            thing_id: job.device_id.clone(),
            name: job.tool.clone(),
            display_name: None,
            description: None,
            parameters: Some(job.args_json.to_string()),
            created_at: job.started_at.to_rfc3339(),
        };
        let submission = self
            .inner
            .workers
            .worker_for(&job.device_id)
            .ok_or_else(|| Error::Internal("device worker unavailable".into()))
            .and_then(|worker| worker.submit(cmd, &job.id, Some(lease)));
        if let Err(error) = submission {
            let result = json!({"error": error.to_string()});
            self.inner
                .store
                .finish_job(&job.id, DeviceJobState::Rejected, &result, self.wake_for(&job).as_ref())
                .await?;
            return Err(DeviceJobError::Validation(error.to_string()));
        }
        Ok(Some(job))
    }

    fn wake_for(&self, job: &DeviceJob) -> Option<TaskWake> {
        let task_id = job.task_id.clone()?;
        Some(TaskWake {
            task_id,
            event_id: format!("device_job:{}:{}", job.id, job.state.as_str()),
            signal_json: json!({
                "kind":"device_job_terminal",
                "job_id":job.id,
                "workspace_id":job.workspace_id
            })
            .to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        })
    }

    pub async fn finish(&self, job_id: &str, final_result: JobFinal) -> Result<FinishOutcome, DeviceJobError> {
        let terminal_lock = self.terminal_lock(job_id);
        let _guard = terminal_lock.lock().await;
        // Always resolve by id: the in-memory map is a hint, never a
        // substitute for the store's duplicate-id / unreadable-workspace
        // policy (Task 19C3 H1).
        let job = self
            .inner
            .store
            .get_job_by_id(job_id)
            .await?
            .ok_or(DeviceJobError::NotFound)?;
        self.finish_known(&job, final_result).await
    }

    async fn finish_known(&self, job: &DeviceJob, final_result: JobFinal) -> Result<FinishOutcome, DeviceJobError> {
        let (state, result) = match final_result {
            JobFinal::Done(value) => (DeviceJobState::Done, value),
            JobFinal::Failed(error) => (DeviceJobState::Failed, json!({"error":error})),
            JobFinal::Rejected(error) => (DeviceJobState::Rejected, json!({"error":error})),
            JobFinal::Cancelled { within_grace: true } => (DeviceJobState::Cancelled, json!({"cancelled":true})),
            JobFinal::Cancelled { within_grace: false } => (
                DeviceJobState::CancelledUnknown,
                json!({"cancelled":true,"outcome":"unknown"}),
            ),
            JobFinal::Interrupted(error) => (DeviceJobState::Interrupted, json!({"error":error})),
        };
        let mut terminal = job.clone();
        terminal.state = state;
        let wake = self.wake_for(&terminal);
        let wake_created = wake.is_some();
        let transitioned = self
            .inner
            .store
            .finish_job(&job.id, state, &result, wake.as_ref())
            .await?;
        if transitioned {
            if let Some(worker) = self.inner.workers.worker_for(&job.device_id) {
                worker.complete_job(&job.id);
            }
            self.notify(&job.id);
            tracing::info!(metric="runtime_job_final", workspace_id=%job.workspace_id,
                task_id=?job.task_id, run_id=?job.run_id, job_id=%job.id,
                device_id=%job.device_id, state=?terminal.state, "runtime job final");
            tracing::info!(metric="runtime_job_duration", job_id=%job.id,
                state=?terminal.state, duration_ms=(chrono::Utc::now() - job.started_at).num_milliseconds().max(0),
                "runtime job duration");
            self.prune_terminal(&job.id);
        }
        Ok(FinishOutcome {
            wake_created: transitioned && wake_created,
            transitioned,
        })
    }

    pub async fn stop(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<DeviceJob, DeviceJobError> {
        let previous = self
            .inner
            .store
            .get_task_job(workspace_id, task_id, job_id)
            .await?
            .ok_or(DeviceJobError::NotFound)?;
        let job = self.inner.store.request_stop(workspace_id, task_id, job_id).await?;
        if previous.state.is_active()
            && previous.stop_requested_at.is_none()
            && let Some(worker) = self.inner.workers.worker_for(&job.device_id)
        {
            worker.cancel_job(job_id);
        }
        Ok(job)
    }

    pub async fn progress(&self, job_id: &str, progress: &Value) -> Result<bool, DeviceJobError> {
        self.inner.store.update_progress(job_id, progress).await
    }

    pub async fn wait(&self, job_id: &str, run_remaining: Duration) -> JobWait {
        // The in-memory maps are pruned once a Job is terminal; a store read by
        // id keeps `wait` authoritative for a completed Job and always applies
        // the duplicate-id / unreadable-workspace policy (Task 19C3 H1).
        let initial = self.inner.store.get_job_by_id(job_id).await;
        let Ok(Some(initial)) = initial else {
            return JobWait::Running {
                job_id: job_id.to_string(),
            };
        };
        if initial.state.is_terminal() {
            return JobWait::Final(initial);
        }
        let workspace_id = initial.workspace_id;
        let deadline = self
            .inner
            .deadlines
            .get(job_id)
            .map(|value| *value)
            .unwrap_or_else(tokio::time::Instant::now);
        let wait_for = Duration::from_secs(50)
            .min(run_remaining)
            .min(deadline.saturating_duration_since(tokio::time::Instant::now()));
        let mut notification = self.notification(job_id);
        let notified = async {
            loop {
                // Read before waiting: the terminal path prunes the notification
                // sender, so a waiter that subscribed after that point would
                // otherwise wait for a signal that can never arrive.
                if let Ok(Some(job)) = self.inner.store.get_job(&workspace_id, job_id).await
                    && job.state.is_terminal()
                {
                    return job;
                }
                if notification.changed().await.is_err() {
                    // The sender is pruned on the terminal path. Give the
                    // durable Job one last read before parking forever.
                    if let Ok(Some(job)) = self.inner.store.get_job(&workspace_id, job_id).await
                        && job.state.is_terminal()
                    {
                        return job;
                    }
                    std::future::pending::<()>().await;
                }
            }
        };
        match tokio::time::timeout(wait_for, notified).await {
            Ok(job) => JobWait::Final(job),
            Err(_) if tokio::time::Instant::now() >= deadline => {
                let job = match self.inner.store.get_job(&workspace_id, job_id).await {
                    Ok(Some(job)) if job.state.is_active() => {
                        if let Some(worker) = self.inner.workers.worker_for(&job.device_id) {
                            worker.cancel_job(&job.id);
                        }
                        match self.finish_known(&job, JobFinal::Failed("timeout".into())).await {
                            Ok(_) => match self.inner.store.get_job(&workspace_id, job_id).await {
                                Ok(Some(job)) => job,
                                _ => {
                                    return JobWait::Running {
                                        job_id: job_id.to_string(),
                                    };
                                }
                            },
                            Err(_) => {
                                return JobWait::Running {
                                    job_id: job_id.to_string(),
                                };
                            }
                        }
                    }
                    Ok(Some(job)) => job,
                    _ => {
                        return JobWait::Running {
                            job_id: job_id.to_string(),
                        };
                    }
                };
                JobWait::Final(job)
            }
            Err(_) => JobWait::Running {
                job_id: job_id.to_string(),
            },
        }
    }

    /// Read one Job for idempotent submission/diagnostics.
    pub async fn job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        self.inner.store.get_job(workspace_id, job_id).await
    }

    pub async fn task_jobs(
        &self,
        workspace_id: &str,
        task_id: &str,
        unknown_only: bool,
    ) -> Result<Vec<DeviceJob>, DeviceJobError> {
        let mut jobs = self
            .inner
            .store
            .active_jobs(Some(workspace_id), unknown_only)
            .await?
            .into_iter()
            .filter(|job| job.task_id.as_deref() == Some(task_id))
            .collect::<Vec<_>>();
        jobs.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(jobs)
    }

    pub async fn recover(&self) -> Result<RecoveryReport, DeviceJobError> {
        let mut report = RecoveryReport::default();
        for job in self.inner.store.active_jobs(None, false).await? {
            self.inner.workspaces.insert(job.id.clone(), job.workspace_id.clone());
            self.notification(&job.id);
            self.restore_deadline(&job);
            let worker = self.inner.workers.worker_for(&job.device_id);
            let recoverable = worker
                .as_ref()
                .is_some_and(|worker| worker.has_recoverable_call(&job.id));
            if job.stop_requested_at.is_some() && recoverable {
                if let Some(worker) = worker {
                    worker.cancel_job(&job.id);
                }
                report.stop_reasserted += 1;
            } else if !recoverable {
                let terminal_lock = self.terminal_lock(&job.id);
                let _guard = terminal_lock.lock().await;
                let outcome = self
                    .finish_known(&job, JobFinal::Interrupted("worker call lost".into()))
                    .await?;
                if outcome.transitioned {
                    report.interrupted += 1;
                }
            }
        }
        Ok(report)
    }

    async fn command_finished(&self, driver_call_id: &str, success: bool, error: Option<&str>) {
        // Device Jobs use the Job ID as ThingCommand ID, which is also the
        // driver call ID on the existing worker path.
        match self.inner.store.mark_running(driver_call_id, driver_call_id).await {
            Ok(_) => {}
            Err(DeviceJobError::Conflict(_)) if self.inner.workspaces.contains_key(driver_call_id) => {}
            Err(_) => return,
        }
        let final_result = if success {
            JobFinal::Done(json!({"ok":true}))
        } else {
            JobFinal::Failed(error.unwrap_or("device command failed").to_string())
        };
        if let Err(error) = self.finish(driver_call_id, final_result).await {
            tracing::warn!(job_id = driver_call_id, %error, "device job terminal handoff failed");
        }
    }
}

#[async_trait::async_trait]
impl JobResultSink for JobService {
    async fn command_finished(&self, driver_call_id: &str, success: bool, error: Option<&str>) {
        JobService::command_finished(self, driver_call_id, success, error).await;
    }

    async fn command_cancelled(&self, job_id: &str, within_grace: bool) {
        if let Err(error) = self.finish(job_id, JobFinal::Cancelled { within_grace }).await {
            tracing::warn!(job_id, %error, "device job cancellation handoff failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::TaskWake;
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
    use tinyiothub_core::driver::manifest::{ManifestTool, ToolKindDecl, ToolSafetyDecl, ToolTimeout};
    use tinyiothub_core::error::Error;
    use tinyiothub_core::models::thing_command::ThingCommand;

    use super::super::worker_adapter::{JobDeviceWorker, JobWorkerLocator};
    use super::{JobFinal, JobRequest, JobService, JobWait};
    use crate::access::resources::{ResourceClaim, ResourceResolver, device_claim};
    use crate::access::validator::{AccessCallValidator, CallError, ValidatedCall};
    use crate::arbitration::{ArbitrationRegistry, ResourceLease};
    use crate::ports::DeviceJobStore;

    #[derive(Default)]
    struct MemoryStore {
        jobs: Mutex<BTreeMap<String, DeviceJob>>,
        wakes: Mutex<Vec<TaskWake>>,
    }

    impl MemoryStore {
        fn job(&self, job_id: &str) -> DeviceJob {
            self.jobs
                .lock()
                .expect("jobs lock")
                .get(job_id)
                .cloned()
                .expect("job exists")
        }

        fn wake_count(&self, task_id: &str) -> usize {
            self.wakes
                .lock()
                .expect("wakes lock")
                .iter()
                .filter(|wake| wake.task_id == task_id)
                .count()
        }
    }

    #[async_trait]
    impl DeviceJobStore for MemoryStore {
        async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
            if jobs.contains_key(&job.id) {
                return Err(DeviceJobError::Conflict("duplicate job".into()));
            }
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs lock")
                .get(job_id)
                .filter(|job| job.workspace_id == workspace_id)
                .cloned())
        }

        async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self.jobs.lock().expect("jobs lock").get(job_id).cloned())
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
                .expect("jobs lock")
                .get(job_id)
                .filter(|job| job.workspace_id == workspace_id && job.task_id.as_deref() == Some(task_id))
                .cloned())
        }

        async fn get_job_by_tool_call(
            &self,
            workspace_id: &str,
            tool_call_id: &str,
        ) -> Result<Option<DeviceJob>, DeviceJobError> {
            Ok(self
                .jobs
                .lock()
                .expect("jobs lock")
                .values()
                .find(|job| job.workspace_id == workspace_id && job.tool_call_id.as_deref() == Some(tool_call_id))
                .cloned())
        }

        async fn submit_approved_job(&self, job: &DeviceJob) -> Result<(Option<DeviceJob>, bool), DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
            if let Some(existing) = jobs
                .values()
                .find(|existing| existing.tool_call_id == job.tool_call_id)
                .cloned()
            {
                return Ok((Some(existing), false));
            }
            jobs.insert(job.id.clone(), job.clone());
            Ok((Some(job.clone()), true))
        }

        async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            if job.state != DeviceJobState::Accepted {
                return Err(DeviceJobError::Conflict("job is not accepted".into()));
            }
            job.state = DeviceJobState::Running;
            job.driver_call_id = Some(driver_call_id.to_string());
            Ok(job.clone())
        }

        async fn finish_job(
            &self,
            job_id: &str,
            state: DeviceJobState,
            result: &Value,
            wake: Option<&TaskWake>,
        ) -> Result<bool, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            if !job.state.is_active() {
                return Ok(false);
            }
            job.state = state;
            job.result_json = Some(result.clone());
            job.ended_at = Some(Utc::now());
            if let Some(wake) = wake {
                self.wakes.lock().expect("wakes lock").push(wake.clone());
            }
            Ok(true)
        }

        async fn update_progress(&self, job_id: &str, progress: &Value) -> Result<bool, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
            let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
            if !job.state.is_active() {
                return Ok(false);
            }
            job.progress_json = Some(progress.clone());
            Ok(true)
        }

        async fn request_stop(
            &self,
            workspace_id: &str,
            task_id: &str,
            job_id: &str,
        ) -> Result<DeviceJob, DeviceJobError> {
            let mut jobs = self.jobs.lock().expect("jobs lock");
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
                .expect("jobs lock")
                .values()
                .filter(|job| workspace_id.is_none_or(|id| job.workspace_id == id))
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

    struct FakeValidator {
        timeout_ms: u64,
    }

    #[async_trait]
    impl AccessCallValidator for FakeValidator {
        async fn validate(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            tool: &str,
            _params: Option<&Value>,
        ) -> Result<ValidatedCall, CallError> {
            Ok(ValidatedCall {
                tool: ManifestTool {
                    name: tool.to_string(),
                    kind: ToolKindDecl::Action,
                    target_field: None,
                    parameters: json!({"type":"object"}),
                    timeout: ToolTimeout {
                        default_ms: self.timeout_ms,
                        max_ms: self.timeout_ms,
                    },
                    safety: ToolSafetyDecl::Control,
                    resources: vec![],
                    cancellable: true,
                },
                timeout_ms: self.timeout_ms,
                clamped: false,
            })
        }
    }

    struct FakeResources;

    #[async_trait]
    impl ResourceResolver for FakeResources {
        async fn resolve_action(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _tool_name: &str,
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }

        async fn resolve_properties(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _properties: &[String],
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }
    }

    #[derive(Default)]
    struct FakeWorker {
        submitted: Mutex<Vec<String>>,
        cancelled: Mutex<Vec<String>>,
        recoverable: Mutex<Vec<String>>,
    }

    impl JobDeviceWorker for FakeWorker {
        fn submit(&self, cmd: ThingCommand, job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
            self.submitted.lock().expect("submitted lock").push(job_id.to_string());
            self.recoverable
                .lock()
                .expect("recoverable lock")
                .push(job_id.to_string());
            assert_eq!(cmd.thing_id, "dev_1");
            Ok(())
        }

        fn cancel_job(&self, job_id: &str) -> bool {
            self.cancelled.lock().expect("cancelled lock").push(job_id.to_string());
            false
        }

        fn has_recoverable_call(&self, job_id: &str) -> bool {
            self.recoverable
                .lock()
                .expect("recoverable lock")
                .iter()
                .any(|id| id == job_id)
        }

        fn complete_job(&self, job_id: &str) {
            self.recoverable
                .lock()
                .expect("recoverable lock")
                .retain(|id| id != job_id);
        }
    }

    struct FakeWorkers {
        worker: Arc<FakeWorker>,
        available: bool,
    }

    impl JobWorkerLocator for FakeWorkers {
        fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
            self.available.then(|| self.worker.clone() as Arc<dyn JobDeviceWorker>)
        }
    }

    struct JobFixture {
        service: JobService,
        store: Arc<MemoryStore>,
        worker: Arc<FakeWorker>,
        workers: Arc<FakeWorkers>,
        arbitration: Arc<ArbitrationRegistry>,
        timeout_ms: u64,
    }

    impl JobFixture {
        async fn new() -> Self {
            Self::with_timeout(5_000).await
        }

        async fn with_timeout(timeout_ms: u64) -> Self {
            let store = Arc::new(MemoryStore::default());
            let worker = Arc::new(FakeWorker::default());
            let workers = Arc::new(FakeWorkers {
                worker: worker.clone(),
                available: true,
            });
            let arbitration = Arc::new(ArbitrationRegistry::default());
            let service = JobService::new(
                store.clone(),
                workers.clone(),
                Arc::new(FakeValidator { timeout_ms }),
                Arc::new(FakeResources),
                arbitration.clone(),
            );
            Self {
                service,
                store,
                worker,
                workers,
                arbitration,
                timeout_ms,
            }
        }

        async fn accepted_job(&self, job_id: &str, task_id: &str) -> DeviceJob {
            self.service
                .submit(JobRequest {
                    job_id: Some(job_id.to_string()),
                    workspace_id: "ws_1".to_string(),
                    device_id: "dev_1".to_string(),
                    tool: "set_speed".to_string(),
                    args_json: json!({"speed": 2}),
                    task_id: Some(task_id.to_string()),
                    run_id: Some(format!("run_{task_id}")),
                    tool_call_id: Some(format!("call_{job_id}")),
                })
                .await
                .expect("submit job")
        }

        async fn job(&self, job_id: &str) -> DeviceJob {
            self.store.job(job_id)
        }

        async fn wake_count(&self, task_id: &str) -> usize {
            self.store.wake_count(task_id)
        }

        fn restart(&mut self) {
            self.service = JobService::new(
                self.store.clone(),
                self.workers.clone(),
                Arc::new(FakeValidator {
                    timeout_ms: self.timeout_ms,
                }),
                Arc::new(FakeResources),
                self.arbitration.clone(),
            );
        }
    }

    #[tokio::test]
    async fn task_jobs_filters_workspace_task_and_unknown_state() {
        let fixture = JobFixture::new().await;
        fixture.accepted_job("job_1", "task_1").await;
        fixture.accepted_job("job_2", "task_2").await;
        let active = fixture
            .service
            .task_jobs("ws_1", "task_1", false)
            .await
            .expect("active");
        assert_eq!(active.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(), ["job_1"]);
        fixture
            .service
            .finish("job_1", JobFinal::Cancelled { within_grace: false })
            .await
            .expect("cancel unknown");
        assert!(
            fixture
                .service
                .task_jobs("ws_1", "task_1", false)
                .await
                .expect("active")
                .is_empty()
        );
        let unknown = fixture
            .service
            .task_jobs("ws_1", "task_1", true)
            .await
            .expect("unknown");
        assert_eq!(unknown.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(), ["job_1"]);
    }

    #[tokio::test]
    async fn duplicate_terminal_result_does_not_create_second_wake() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_1", "task_1").await;
        assert!(
            fixture
                .service
                .finish(&job.id, JobFinal::Done(json!({"ok":true})))
                .await
                .expect("first final")
                .wake_created
        );
        assert!(
            !fixture
                .service
                .finish(&job.id, JobFinal::Done(json!({"ok":true})))
                .await
                .expect("duplicate final")
                .wake_created
        );
        assert_eq!(fixture.wake_count("task_1").await, 1);
    }

    #[tokio::test]
    async fn terminal_jobs_are_pruned_from_service_maps_and_wait_falls_back_to_the_store() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_pruned", "task_pruned").await;
        assert!(fixture.service.inner.workspaces.contains_key(&job.id));
        fixture
            .service
            .finish(&job.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("finish");
        assert!(!fixture.service.inner.workspaces.contains_key(&job.id));
        assert!(!fixture.service.inner.deadlines.contains_key(&job.id));
        assert!(!fixture.service.inner.notifications.contains_key(&job.id));
        assert!(!fixture.service.inner.terminal_locks.lock().contains_key(&job.id));
        let waited = fixture.service.wait(&job.id, Duration::from_secs(1)).await;
        assert!(
            matches!(waited, JobWait::Final(ref final_job) if final_job.state == DeviceJobState::Done),
            "wait must read the durable Job after in-memory pruning"
        );
    }

    #[tokio::test]
    async fn stop_is_scoped_to_task_and_survives_repeated_requests() {
        let fixture = JobFixture::new().await;
        fixture.accepted_job("job_a", "task_a").await;
        fixture.accepted_job("job_b", "task_b").await;
        let first = fixture.service.stop("ws_1", "task_a", "job_a").await.expect("stop");
        let second = fixture.service.stop("ws_1", "task_a", "job_a").await.expect("repeat");
        assert_eq!(first.stop_requested_at, second.stop_requested_at);
        assert!(fixture.service.stop("ws_1", "task_b", "job_a").await.is_err());
        assert_eq!(fixture.job("job_b").await.stop_requested_at, None);
        assert_eq!(*fixture.worker.cancelled.lock().expect("cancelled lock"), vec!["job_a"]);
        fixture
            .arbitration
            .try_acquire_default("dev_1", "later-claim".to_string())
            .expect("later claim executes normally");
    }

    #[tokio::test]
    async fn deadline_covers_queue_time_and_finishes_failed_timeout() {
        let fixture = JobFixture::with_timeout(30).await;
        let job = fixture.accepted_job("job_timeout", "task_timeout").await;
        assert_eq!(job.state, DeviceJobState::Accepted);
        let final_state = fixture.service.wait(&job.id, Duration::from_secs(1)).await;
        let JobWait::Final(final_job) = final_state else {
            panic!("deadline must produce a final result");
        };
        assert_eq!(final_job.state, DeviceJobState::Failed);
        assert_eq!(final_job.result_json, Some(json!({"error":"timeout"})));
        assert_eq!(*fixture.worker.cancelled.lock().expect("cancelled lock"), vec![job.id]);
        assert_eq!(fixture.wake_count("task_timeout").await, 1);
    }

    #[tokio::test]
    async fn wait_returns_running_when_only_model_window_expires() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_running", "task_running").await;
        let waited = fixture.service.wait(&job.id, Duration::from_millis(10)).await;
        assert!(matches!(waited, JobWait::Running { ref job_id } if job_id == &job.id));
        assert_eq!(fixture.job(&job.id).await.state, DeviceJobState::Accepted);
    }

    #[tokio::test]
    async fn long_running_job_continuation_reuses_one_job_and_one_side_effect() {
        let mut fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_long", "task_long").await;
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);

        // The model wait window expires while the physical Job is still active.
        let waited = fixture.service.wait(&job.id, Duration::from_millis(10)).await;
        assert!(matches!(waited, JobWait::Running { ref job_id } if job_id == &job.id));
        assert_eq!(fixture.job(&job.id).await.state, DeviceJobState::Accepted);

        // A restarted service rediscovers the same active Job instead of
        // submitting the side effect again.
        fixture.restart();
        let recovery = fixture.service.recover().await.expect("recover");
        assert_eq!(recovery.interrupted, 0);
        let active = fixture
            .service
            .task_jobs("ws_1", "task_long", false)
            .await
            .expect("active jobs");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, job.id);
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);

        let terminal = fixture
            .service
            .finish(&job.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("terminal");
        assert!(terminal.wake_created);
        assert_eq!(fixture.wake_count("task_long").await, 1);
        let duplicate = fixture
            .service
            .finish(&job.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("duplicate terminal");
        assert!(!duplicate.wake_created);
        assert_eq!(fixture.wake_count("task_long").await, 1);
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);
    }

    #[tokio::test]
    async fn progress_updates_projection_without_waking_task() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_progress", "task_progress").await;
        assert!(
            fixture
                .service
                .progress(&job.id, &json!({"message":"calibrating"}))
                .await
                .expect("progress")
        );
        assert_eq!(
            fixture.job(&job.id).await.progress_json,
            Some(json!({"message":"calibrating"}))
        );
        assert_eq!(fixture.wake_count("task_progress").await, 0);
    }

    #[tokio::test]
    async fn worker_command_result_marks_running_then_finishes_job() {
        use super::super::worker_adapter::JobResultSink;

        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_result", "task_result").await;
        JobResultSink::command_finished(&fixture.service, &job.id, true, None).await;
        assert_eq!(fixture.job(&job.id).await.state, DeviceJobState::Done);
        assert_eq!(fixture.wake_count("task_result").await, 1);
    }

    #[tokio::test]
    async fn terminal_projection_acknowledges_worker_handoff() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_ack", "task_ack").await;
        assert!(fixture.worker.has_recoverable_call(&job.id));
        fixture
            .service
            .finish(&job.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("terminal projection");
        assert!(!fixture.worker.has_recoverable_call(&job.id));
    }

    #[tokio::test]
    async fn recover_then_wait_preserves_original_deadline() {
        let mut fixture = JobFixture::with_timeout(80).await;
        let job = fixture
            .accepted_job("job_recovered_deadline", "task_recovered_deadline")
            .await;
        fixture.restart();
        fixture.service.recover().await.expect("recover");
        let before = fixture.service.wait(&job.id, Duration::from_millis(5)).await;
        assert!(
            matches!(before, JobWait::Running { .. }),
            "recovery must not expire the original deadline early"
        );
        tokio::time::sleep(Duration::from_millis(90)).await;
        let after = fixture.service.wait(&job.id, Duration::from_secs(1)).await;
        assert!(matches!(after, JobWait::Final(ref final_job) if final_job.state == DeviceJobState::Failed));
    }

    #[tokio::test]
    async fn restart_reasserts_persistent_stop_without_replay() {
        let mut fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_restart", "task_restart").await;
        fixture
            .service
            .stop("ws_1", "task_restart", &job.id)
            .await
            .expect("stop");
        fixture.worker.cancelled.lock().expect("cancelled lock").clear();
        fixture.restart();
        let report = fixture.service.recover().await.expect("recover");
        assert_eq!(report.stop_reasserted, 1);
        assert_eq!(report.interrupted, 0);
        assert_eq!(*fixture.worker.cancelled.lock().expect("cancelled lock"), vec![job.id]);
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);
    }

    #[tokio::test]
    async fn restart_marks_unrecoverable_job_interrupted_once() {
        let mut fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_lost", "task_lost").await;
        fixture
            .worker
            .recoverable
            .lock()
            .expect("recoverable lock")
            .retain(|id| id != &job.id);
        fixture.restart();
        let report = fixture.service.recover().await.expect("recover");
        assert_eq!(report.interrupted, 1);
        assert_eq!(fixture.job(&job.id).await.state, DeviceJobState::Interrupted);
        assert_eq!(fixture.wake_count("task_lost").await, 1);
        let second = fixture.service.recover().await.expect("repeat recovery");
        assert_eq!(second.interrupted, 0);
        assert_eq!(fixture.wake_count("task_lost").await, 1);
    }

    #[tokio::test]
    async fn approved_submission_is_idempotent_and_never_replays_terminal_or_unknown_jobs() {
        for terminal in [
            DeviceJobState::Done,
            DeviceJobState::Interrupted,
            DeviceJobState::CancelledUnknown,
        ] {
            let mut fixture = JobFixture::new().await;
            let call = tinyiothub_core::task_approval::ApprovedToolCall {
                workspace_id: "ws_1".into(),
                task_id: "task_approved".into(),
                run_id: "run_approved".into(),
                tool_call_id: format!("call_{terminal:?}"),
                device_id: "dev_1".into(),
                tool: "set_speed".into(),
                args_json: json!({"speed": 9}),
            };
            let first = fixture
                .service
                .submit_approved(call.clone())
                .await
                .expect("approved")
                .expect("job");
            assert_eq!(first.args_json, call.args_json);
            let duplicate = fixture
                .service
                .submit_approved(call.clone())
                .await
                .expect("duplicate")
                .expect("existing");
            assert_eq!(duplicate.id, first.id);
            fixture
                .service
                .finish(
                    &first.id,
                    match terminal {
                        DeviceJobState::Done => JobFinal::Done(json!({"ok":true})),
                        DeviceJobState::Interrupted => JobFinal::Interrupted("lost".into()),
                        DeviceJobState::CancelledUnknown => JobFinal::Cancelled { within_grace: false },
                        _ => unreachable!(),
                    },
                )
                .await
                .expect("terminal");
            fixture.restart();
            let recovered = fixture
                .service
                .submit_approved(call)
                .await
                .expect("recover")
                .expect("terminal fact");
            assert_eq!(recovered.id, first.id);
            assert_eq!(recovered.state, terminal);
            assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);
        }
    }

    #[tokio::test]
    async fn approved_submission_rejects_frozen_execution_conflicts() {
        let fixture = JobFixture::new().await;
        fixture.accepted_job("job_conflict", "task_conflict").await;
        let call = tinyiothub_core::task_approval::ApprovedToolCall {
            workspace_id: "ws_1".into(),
            task_id: "task_conflict".into(),
            run_id: "run_task_conflict".into(),
            tool_call_id: "call_job_conflict".into(),
            device_id: "dev_1".into(),
            tool: "set_speed".into(),
            args_json: json!({"speed": 999}),
        };
        assert!(matches!(
            fixture.service.submit_approved(call).await,
            Err(DeviceJobError::ExecutionConflict)
        ));
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);
    }

    #[tokio::test]
    async fn late_cancellation_is_unknown_and_never_replayed() {
        let fixture = JobFixture::new().await;
        let job = fixture.accepted_job("job_unknown", "task_unknown").await;
        fixture
            .service
            .finish(&job.id, JobFinal::Cancelled { within_grace: false })
            .await
            .expect("unknown terminal");
        assert_eq!(fixture.job(&job.id).await.state, DeviceJobState::CancelledUnknown);
        assert_eq!(fixture.worker.submitted.lock().expect("submitted lock").len(), 1);
    }
}
