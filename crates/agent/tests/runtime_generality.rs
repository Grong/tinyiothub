//! T8 generality acceptance: two declaration-only fixtures complete through the
//! same Runtime tools, Task coordinator, Job service and projection contracts.
//!
//! The fixtures differ only in `manifest.json` / `task.json` data: device and
//! source identifiers, declared initial values, thresholds, objectives and the
//! expected tool order. No scenario-specific tool, prompt branch or fallback is
//! introduced to make either fixture pass.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::provider::ChatMessage;
use tinyiothub_agent::port::runtime::AgentLoop;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_agent::runtime::events::AgentEventBus;
use tinyiothub_agent::runtime::thing_agent::active_runs::ActiveRunRegistry;
use tinyiothub_agent::runtime::thing_agent::manager::AutonomousAgentProvider;
use tinyiothub_agent::runtime::thing_agent::runner::{AgentHandle, RunContextInner, Runner};
use tinyiothub_agent::runtime::thing_agent::task::coordinator::{Advance, TaskCoordinator};
use tinyiothub_agent::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::watch::WatchRule;
use tinyiothub_runtime::access::resources::ResourceResolver;
use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
use tinyiothub_runtime::job::service::{JobFinal, JobRequest, JobService};
use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobWorkerLocator};
use tinyiothub_runtime::ports::{DeviceJobStore, WatchRuleStore};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ── fixture declarations ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FixtureManifest {
    workspace_id: String,
    device_id: String,
    sources: Vec<String>,
    initial: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
struct FixtureTask {
    task_id: String,
    workspace_id: String,
    device_id: String,
    objective: String,
    steps: Vec<Value>,
    expected_tools: Vec<String>,
}

struct RuntimeFixture {
    manifest: FixtureManifest,
    task: FixtureTask,
}

impl RuntimeFixture {
    fn load(name: &str) -> Self {
        let (manifest_json, task_json) = match name {
            "runtime_a" => (
                include_str!("fixtures/runtime_a/manifest.json"),
                include_str!("fixtures/runtime_a/task.json"),
            ),
            "runtime_b" => (
                include_str!("fixtures/runtime_b/manifest.json"),
                include_str!("fixtures/runtime_b/task.json"),
            ),
            other => panic!("unknown Runtime fixture {other}"),
        };
        let manifest: Value = serde_json::from_str(manifest_json).expect("manifest JSON");
        let task: Value = serde_json::from_str(task_json).expect("task JSON");
        let device = &manifest["devices"][0];
        let mut initial = BTreeMap::new();
        if let Some(values) = device["initial"].as_object() {
            for (key, value) in values {
                initial.insert(key.clone(), value.clone());
            }
        }
        Self {
            manifest: FixtureManifest {
                workspace_id: manifest["workspaceId"].as_str().expect("workspace").into(),
                device_id: device["id"].as_str().expect("device").into(),
                sources: device["sources"]
                    .as_array()
                    .expect("sources")
                    .iter()
                    .map(|source| source.as_str().expect("source").to_string())
                    .collect(),
                initial,
            },
            task: FixtureTask {
                task_id: task["taskId"].as_str().expect("task id").into(),
                workspace_id: task["workspaceId"].as_str().expect("workspace").into(),
                device_id: task["deviceId"].as_str().expect("device").into(),
                objective: task["objective"].as_str().expect("objective").into(),
                steps: task["steps"].as_array().expect("steps").clone(),
                expected_tools: task["expectedTools"]
                    .as_array()
                    .expect("expected tools")
                    .iter()
                    .map(|tool| tool.as_str().expect("tool").to_string())
                    .collect(),
            },
        }
    }

    fn expected_tools(&self) -> Vec<String> {
        self.task.expected_tools.clone()
    }

    async fn run_to_terminal(&self) -> FixtureOutcome {
        assert_eq!(self.manifest.workspace_id, self.task.workspace_id);
        assert_eq!(self.manifest.device_id, self.task.device_id);
        let harness = Arc::new(Harness::new(&self.manifest, &self.task));
        let coordinator = harness.coordinator();
        let now = Utc::now();
        let task = AgentTask {
            id: self.task.task_id.clone(),
            workspace_id: self.manifest.workspace_id.clone(),
            idempotency_key: format!("fixture:{}", self.task.task_id),
            objective: self.task.objective.clone(),
            session_key: None,
            created_by: "fixture-user".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: format!("wake:{}", self.task.task_id),
            signal_json: serde_json::to_string(&WakeSignal {
                workspace_id: self.manifest.workspace_id.clone(),
                priority: Priority::High,
                dedup_key: None,
                source: TriggerSource::UserDirective {
                    user_id: "fixture-user".into(),
                    text: self.task.objective.clone(),
                    session_key: None,
                    source: None,
                    problem_key: None,
                },
            })
            .expect("wake JSON"),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        harness
            .tasks
            .create_task_with_wake(&task, &wake)
            .await
            .expect("create task");
        let advance = coordinator
            .advance(&task.id, &wake.event_id)
            .await
            .expect("advance declaration task");
        assert!(
            matches!(advance, Advance::Started { .. }),
            "declaration Task did not start a Run: {advance:?}"
        );

        let mut terminal = None;
        for _ in 0..500 {
            let current = harness
                .tasks
                .get_task(&task.workspace_id, &task.id)
                .await
                .expect("get task")
                .expect("task present");
            if current.status.is_terminal() {
                terminal = Some(current);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let task = terminal.expect("Task did not reach a terminal state");
        FixtureOutcome {
            task,
            jobs: harness.jobs.snapshot(),
            wake_ids: harness.tasks.wake_ids(),
            used_tools: harness.used_tools.lock().expect("used tools").clone(),
        }
    }
}

struct FixtureOutcome {
    task: AgentTask,
    jobs: Vec<DeviceJob>,
    wake_ids: Vec<String>,
    used_tools: Vec<String>,
}

// ── generic execution harness (no scenario branches) ─────────────────────────

struct Harness {
    manifest: FixtureManifest,
    task: FixtureTask,
    tasks: Arc<MemoryTasks>,
    devices: Arc<Mutex<BTreeMap<String, Value>>>,
    jobs: Arc<MemoryJobs>,
    used_tools: Arc<Mutex<Vec<String>>>,
    service: JobService,
}

impl Harness {
    fn new(manifest: &FixtureManifest, task: &FixtureTask) -> Self {
        let jobs = Arc::new(MemoryJobs::default());
        let worker = Arc::new(RecordingWorker);
        let service = JobService::new(
            jobs.clone(),
            Arc::new(RecordingWorkers { worker }),
            Arc::new(ManifestValidator),
            Arc::new(NoResources),
            Arc::new(ArbitrationRegistry::default()),
        );
        Self {
            manifest: manifest.clone(),
            task: task.clone(),
            tasks: Arc::new(MemoryTasks::default()),
            devices: Arc::new(Mutex::new(manifest.initial.clone())),
            jobs,
            used_tools: Arc::new(Mutex::new(Vec::new())),
            service,
        }
    }

    fn coordinator(&self) -> TaskCoordinator {
        TaskCoordinator::new(
            self.tasks.clone(),
            Some(self.service.clone()),
            Arc::new(EmptyWatches),
            Arc::new(dashmap::DashMap::new()),
            Arc::new(AgentEventBus::new(16)),
            ActiveRunRegistry::new(8),
            Arc::new(Runner::new()),
            Arc::new(PlanProvider {
                manifest: self.manifest.clone(),
                task: self.task.clone(),
                devices: self.devices.clone(),
                used_tools: self.used_tools.clone(),
                jobs: self.service.clone(),
            }),
        )
    }
}

fn comparison_matches(value: &Value, operator: &str, threshold: &Value) -> bool {
    match (value.as_f64(), threshold.as_f64()) {
        (Some(lhs), Some(rhs)) => match operator {
            "gt" => lhs > rhs,
            "gte" => lhs >= rhs,
            "lt" => lhs < rhs,
            "lte" => lhs <= rhs,
            "eq" => lhs == rhs,
            _ => false,
        },
        _ => false,
    }
}

// ── in-memory ports ──────────────────────────────────────────────────────────

#[derive(Default)]
struct MemoryState {
    tasks: HashMap<String, AgentTask>,
    wakes: HashMap<(String, String), TaskWake>,
    runs: HashSet<(String, String)>,
    reports: Vec<RunReport>,
    intents: HashMap<(String, String), TaskFinalizeIntent>,
}

#[derive(Default)]
struct MemoryTasks {
    state: Mutex<MemoryState>,
}

impl MemoryTasks {
    fn wake_ids(&self) -> Vec<String> {
        let mut ids = self
            .state
            .lock()
            .expect("tasks")
            .wakes
            .keys()
            .map(|(_, event_id)| event_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }
}

#[async_trait]
impl TaskStore for MemoryTasks {
    async fn create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError> {
        let mut state = self.state.lock().expect("tasks");
        state.tasks.insert(task.id.clone(), task.clone());
        state
            .wakes
            .insert((wake.task_id.clone(), wake.event_id.clone()), wake.clone());
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
        Ok(0)
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
        let mut state = self.state.lock().expect("tasks");
        let task = state.tasks.get_mut(task_id).ok_or(TaskStoreError::ConcurrentUpdate)?;
        if task.state_version != expected_version {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        task.current_run_id = run_id.map(str::to_string);
        task.state_version += 1;
        Ok(task.clone())
    }

    async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError> {
        if !task.status.can_transition_to(TaskStatus::Active) {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        let mut state = self.state.lock().expect("tasks");
        if !state.runs.insert((task.id.clone(), wake_event_id.into())) {
            return Ok(false);
        }
        let stored = state.tasks.get_mut(&task.id).ok_or(TaskStoreError::ConcurrentUpdate)?;
        if stored.state_version != task.state_version {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        stored.status = TaskStatus::Active;
        stored.current_run_id = Some(run_id.to_string());
        stored.state_version += 1;
        stored.updated_at = Utc::now();
        if let Some(wake) = state.wakes.get_mut(&(task.id.clone(), wake_event_id.to_string())) {
            wake.dispatch_state = WakeDispatchState::Dispatched;
            wake.dispatched_at = Some(Utc::now());
        }
        Ok(true)
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
        Ok(0)
    }

    async fn mark_wake(&self, task_id: &str, event_id: &str, next: WakeDispatchState) -> Result<(), TaskStoreError> {
        if let Some(wake) = self
            .state
            .lock()
            .expect("tasks")
            .wakes
            .get_mut(&(task_id.into(), event_id.into()))
        {
            wake.dispatch_state = next;
        }
        Ok(())
    }

    async fn retry_dead_wake(&self, _workspace_id: &str, _event_id: &str) -> Result<bool, TaskStoreError> {
        Ok(false)
    }

    async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError> {
        Ok(self
            .state
            .lock()
            .expect("tasks")
            .tasks
            .values()
            .filter(|task| !task.status.is_terminal())
            .filter(|task| workspace_id.is_none_or(|workspace| workspace == task.workspace_id))
            .cloned()
            .collect())
    }

    async fn requeue_with_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        let mut state = self.state.lock().expect("tasks");
        let wake = state
            .wakes
            .get_mut(&(intent.task_id.clone(), intent.wake_event_id.clone()))
            .ok_or(TaskStoreError::ConcurrentUpdate)?;
        wake.dispatch_state = WakeDispatchState::Pending;
        wake.dispatched_at = None;
        state
            .intents
            .insert((intent.task_id.clone(), intent.wake_event_id.clone()), intent.clone());
        Ok(())
    }

    async fn finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError> {
        Ok(self
            .state
            .lock()
            .expect("tasks")
            .intents
            .get(&(task_id.into(), event_id.into()))
            .cloned())
    }

    async fn complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError> {
        self.state
            .lock()
            .expect("tasks")
            .intents
            .remove(&(task_id.into(), event_id.into()));
        Ok(())
    }

    async fn put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        self.state
            .lock()
            .expect("tasks")
            .intents
            .insert((intent.task_id.clone(), intent.wake_event_id.clone()), intent.clone());
        Ok(())
    }

    async fn completed_run_report(&self, run_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        Ok(self
            .state
            .lock()
            .expect("tasks")
            .reports
            .iter()
            .find(|report| report.run_id == run_id)
            .cloned())
    }

    async fn count_finalize_intents(&self) -> Result<i64, TaskStoreError> {
        Ok(self.state.lock().expect("tasks").intents.len() as i64)
    }

    async fn prune_finalize_intents(&self, _older_than: chrono::DateTime<Utc>) -> Result<usize, TaskStoreError> {
        let mut state = self.state.lock().expect("tasks");
        let MemoryState {
            tasks, wakes, intents, ..
        } = &mut *state;
        let before = intents.len();
        intents.retain(|(task_id, event_id), _| {
            let wake_live = wakes
                .get(&(task_id.clone(), event_id.clone()))
                .is_some_and(|wake| wake.resolved_at.is_none());
            let task_live = tasks.get(task_id).is_some_and(|task| !task.status.is_terminal());
            wake_live && task_live
        });
        Ok(before - intents.len())
    }
}

#[derive(Default)]
struct MemoryJobs {
    jobs: Mutex<BTreeMap<String, DeviceJob>>,
}

impl MemoryJobs {
    fn snapshot(&self) -> Vec<DeviceJob> {
        let mut jobs = self.jobs.lock().expect("jobs").values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        jobs
    }
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

    async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        Ok(self.jobs.lock().expect("jobs").get(job_id).cloned())
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

    async fn get_job_by_tool_call(
        &self,
        workspace_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        Ok(self
            .jobs
            .lock()
            .expect("jobs")
            .values()
            .find(|job| job.workspace_id == workspace_id && job.tool_call_id.as_deref() == Some(tool_call_id))
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
        result: &Value,
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

    async fn update_progress(&self, job_id: &str, progress: &Value) -> Result<bool, DeviceJobError> {
        let mut jobs = self.jobs.lock().expect("jobs");
        let job = jobs.get_mut(job_id).ok_or(DeviceJobError::NotFound)?;
        job.progress_json = Some(progress.clone());
        Ok(true)
    }

    async fn request_stop(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<DeviceJob, DeviceJobError> {
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

struct EmptyWatches;

#[async_trait]
impl WatchRuleStore for EmptyWatches {
    async fn list_all(&self) -> Result<Vec<WatchRule>, String> {
        Ok(vec![])
    }
    async fn insert(&self, _rule: &WatchRule) -> Result<(), String> {
        Ok(())
    }
    async fn delete(&self, _workspace_id: &str, _id: &str) -> Result<bool, String> {
        Ok(false)
    }
    async fn complete_task_watch(&self, _workspace_id: &str, _id: &str, _wake: &TaskWake) -> Result<bool, String> {
        Ok(false)
    }
    async fn delete_task_watches(&self, _workspace_id: &str, _task_id: &str) -> Result<usize, String> {
        Ok(0)
    }
}

#[derive(Default)]
struct RecordingWorker;

impl JobDeviceWorker for RecordingWorker {
    fn submit(&self, _cmd: ThingCommand, _job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
        Ok(())
    }
    fn cancel_job(&self, _job_id: &str) -> bool {
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

struct ManifestValidator;

#[async_trait]
impl AccessCallValidator for ManifestValidator {
    async fn validate(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        tool: &str,
        params: Option<&Value>,
    ) -> Result<ValidatedCall, CallError> {
        if !matches!(tool, "read_device" | "set_device") {
            return Err(CallError::UndeclaredTool(tool.into()));
        }
        let params = params.ok_or_else(|| CallError::InvalidParams("missing arguments".into()))?;
        if params.get("source").and_then(Value::as_str).is_none() {
            return Err(CallError::InvalidParams("missing source".into()));
        }
        let manifest_tool = serde_json::from_value(serde_json::json!({ "name": tool }))
            .map_err(|error| CallError::InvalidParams(error.to_string()))?;
        Ok(ValidatedCall {
            tool: manifest_tool,
            timeout_ms: 2_000,
            clamped: false,
        })
    }
}

struct NoResources;

#[async_trait]
impl ResourceResolver for NoResources {
    async fn resolve_action(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _tool_name: &str,
    ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
        Ok(vec![])
    }
    async fn resolve_properties(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _properties: &[String],
    ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
        Ok(vec![])
    }
}

// ── declaration-driven provider/agent ────────────────────────────────────────

struct PlanProvider {
    manifest: FixtureManifest,
    task: FixtureTask,
    devices: Arc<Mutex<BTreeMap<String, Value>>>,
    used_tools: Arc<Mutex<Vec<String>>>,
    jobs: JobService,
}

#[async_trait]
impl AutonomousAgentProvider for PlanProvider {
    async fn get_or_create(
        &self,
        _workspace_id: &str,
        ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        let (task_id, run_id) = {
            let inner = ctx.read().await;
            (inner.task_id.clone(), inner.run_id.clone())
        };
        Ok(Arc::new(tokio::sync::Mutex::new(PlanAgent {
            manifest: self.manifest.clone(),
            task: self.task.clone(),
            devices: self.devices.clone(),
            used_tools: self.used_tools.clone(),
            jobs: self.jobs.clone(),
            task_id: task_id.expect("Run context Task id"),
            run_id,
        })))
    }

    fn invalidate(&self, _workspace_id: &str) {}
}

struct PlanAgent {
    manifest: FixtureManifest,
    task: FixtureTask,
    devices: Arc<Mutex<BTreeMap<String, Value>>>,
    used_tools: Arc<Mutex<Vec<String>>>,
    jobs: JobService,
    task_id: String,
    run_id: String,
}

impl PlanAgent {
    async fn execute_plan(&self) -> anyhow::Result<()> {
        for step in &self.task.steps {
            let read = step["read"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("step has no read source"))?;
            let value = self.read(read).await?;
            if let Some(condition) = step.get("if")
                && let Some(operator) = condition["operator"].as_str()
                && comparison_matches(&value, operator, &condition["value"])
            {
                for action in step["then"].as_array().cloned().unwrap_or_default() {
                    let source = action["write"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("action has no write source"))?;
                    self.write(source, action["value"].clone()).await?;
                }
            }
            if let Some(expected) = step.get("expect") {
                anyhow::ensure!(
                    &value == expected,
                    "verification failed for {read}: expected {expected}, got {value}"
                );
            }
        }
        Ok(())
    }

    async fn read(&self, source: &str) -> anyhow::Result<Value> {
        self.declared(source)?;
        self.used_tools.lock().expect("used tools").push("read_device".into());
        let value = self
            .devices
            .lock()
            .expect("devices")
            .get(source)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("source {source} has no value"))?;
        self.run_tool(
            "read_device",
            serde_json::json!({"source": source}),
            serde_json::json!({"source": source, "value": value}),
        )
        .await?;
        Ok(value)
    }

    async fn write(&self, source: &str, value: Value) -> anyhow::Result<()> {
        self.declared(source)?;
        self.used_tools.lock().expect("used tools").push("set_device".into());
        self.devices
            .lock()
            .expect("devices")
            .insert(source.into(), value.clone());
        self.run_tool(
            "set_device",
            serde_json::json!({"source": source, "value": value}),
            serde_json::json!({"source": source, "value": value}),
        )
        .await?;
        Ok(())
    }

    fn declared(&self, source: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.manifest.sources.iter().any(|declared| declared == source),
            "undeclared source {source} for device {}",
            self.manifest.device_id
        );
        Ok(())
    }

    async fn run_tool(&self, tool: &str, args: Value, result: Value) -> anyhow::Result<()> {
        let job = self
            .jobs
            .submit(JobRequest {
                job_id: None,
                workspace_id: self.manifest.workspace_id.clone(),
                device_id: self.manifest.device_id.clone(),
                tool: tool.into(),
                args_json: args,
                task_id: Some(self.task_id.clone()),
                run_id: Some(self.run_id.clone()),
                tool_call_id: Some(format!("call_{}", uuid::Uuid::new_v4().simple())),
            })
            .await?;
        self.jobs.finish(&job.id, JobFinal::Done(result)).await?;
        Ok(())
    }
}

#[async_trait]
impl AgentLoop for PlanAgent {
    async fn turn_streamed(
        &self,
        _user_message: &str,
        _event_tx: mpsc::Sender<TurnEvent>,
        _cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        self.execute_plan().await?;
        Ok("declaration plan complete".into())
    }

    async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
        self.execute_plan().await?;
        Ok("declaration plan complete".into())
    }

    async fn clear_history(&self) {}

    async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── T8 acceptance ────────────────────────────────────────────────────────────

#[tokio::test]
async fn two_fixtures_complete_without_scenario_branches() {
    for fixture in [RuntimeFixture::load("runtime_a"), RuntimeFixture::load("runtime_b")] {
        let outcome = fixture.run_to_terminal().await;
        assert_eq!(outcome.task.status, TaskStatus::Completed);
        assert!(!outcome.jobs.is_empty(), "autonomous run must persist Jobs");
        assert_eq!(
            outcome.jobs.len(),
            outcome.used_tools.len(),
            "every autonomous tool call must submit exactly one Job"
        );
        assert!(outcome.jobs.iter().all(|job| job.state == DeviceJobState::Done));
        assert!(outcome.wake_ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(outcome.used_tools, fixture.expected_tools());
    }
    let source = include_str!("../src/runtime/thing_agent/task/coordinator.rs");
    for forbidden in ["greenhouse", "entrance", "temperature_above", "occupancy"] {
        assert!(!source.contains(forbidden), "scenario branch: {forbidden}");
    }
}
