//! Thing-agent manager — the persistent Task runtime plus the per-workspace
//! watch engine wiring point.
//!
//! ```text
//! WorkspaceCreated → start_workspace_watch(): watch_engine_loop ─► WatchHit
//!                                                                   │
//!                                            session forward loop ──┴─► TriggerRouter::submit_event
//!                                                       (task-owned hits are settled inside the engine)
//! WorkspaceDeleted → stop_workspace_watch(): abort engine/forwarder, drop command channel
//! ```
//!
//! Scheduled wakes, alarm wakes and (since Plan 4A/4B) watch hits all enter
//! the Session loop through [`crate::domains::agent::session::TriggerRouter`].
//! The legacy per-workspace scheduler + wake triggers were retired with this
//! revision; the Task runtime (`start_task_runtime`) remains the only run
//! executor owned here.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::domains::agent::events::AgentEventBus;
use crate::domains::agent::thing_agent::active_runs::ActiveRunRegistry;
use crate::domains::agent::thing_agent::governor::HeartbeatGovernor;
use crate::domains::agent::thing_agent::runner::{AgentHandle, RunContextInner, Runner};
use crate::domains::agent::thing_agent::task::coordinator::{CancelOutcome, TaskCoordinator, TaskError};
use crate::domains::agent::thing_agent::task::wake::WakeRouter;
use crate::domains::agent::thing_agent::traits::SnapshotDemandSink;
use crate::domains::agent::thing_agent::watch_loop::{WatchCommand, WatchDeps, watch_engine_loop};
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};

/// Cadence of the live Task runtime maintenance sweep (retained finalize
/// intents). Approval expiry is owned by the Harness Effect dispatcher.
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(30);

/// Capacity of the per-workspace watch hit / command channels.
const TRIGGER_CHANNEL_CAPACITY: usize = 64;

/// In-flight run registry capacity (T3).
pub(crate) const ACTIVE_RUNS_CAPACITY: usize = TRIGGER_CHANNEL_CAPACITY;

/// Factory abstraction over the T11 autonomous agent factory (implemented by
/// `AutonomousAgentFactory` in the cloud crate; stubbed in tests). One call
/// per run: binds the run context and returns the per-workspace agent.
#[async_trait::async_trait]
pub trait AutonomousAgentProvider: Send + Sync {
    async fn get_or_create(
        &self,
        workspace_id: &str,
        ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle>;

    /// 失效该工作区的缓存 agent（loop 停止时调用，避免 WorkspaceDeleted
    /// 后缓存实例泄漏）。
    fn invalidate(&self, workspace_id: &str);
}

pub struct TaskRuntimeDeps {
    pub store: Arc<dyn tinyiothub_agent::port::task::TaskStore>,
    /// Read-only workspace Decision query backing `waiting_status`.
    pub pending_tool_approvals: Option<Arc<dyn crate::domains::agent::thing_agent::approval_port::PendingToolApproval>>,
    pub watches: Arc<dyn tinyiothub_runtime::ports::WatchRuleStore>,
    pub jobs: Option<tinyiothub_runtime::job::service::JobService>,
    /// True when `store` is the workspace Harness adapter. Cancellation then
    /// moves Task state through the port instead of the legacy platform
    /// approval transaction.
    pub harness_task_store: bool,
    /// Production Harness Effect dispatcher (Task 19C1). None in unit tests.
    pub effect_dispatch: Option<Arc<crate::domains::agent::runtime::dispatch::EffectDispatch>>,
}

/// Heartbeat 源的共享内存依赖：任务表、间隔/开关覆盖、信任配置与
/// last_ticks 读面。restore 自快照预热，handler 写穿共享同一张表
///（D11-⑤ 写序不变）。默认空表 = 无任务。
#[derive(Clone, Default)]
pub struct HeartbeatDeps {
    /// 每工作区巡检任务表。
    pub tasks: Arc<DashMap<String, Vec<HeartbeatTask>>>,
    /// 每工作区间隔（分钟）覆盖表——update_config 写穿。
    pub intervals: Arc<DashMap<String, u32>>,
    /// 每工作区心跳开关表（终审 I-1）。
    pub enabled: Arc<DashMap<String, bool>>,
    /// 每工作区信任配置表（handler 写穿 / dump_state 导出）。
    pub trust_configs: Arc<DashMap<String, TrustConfig>>,
    /// 连败治理（T4）。
    pub governor: HeartbeatGovernor,
    /// 每工作区最近 tick 完成时间表（Task 12，D13 实时读）：handler
    /// get_config 的 last_tick 读源。
    pub last_ticks: Arc<DashMap<String, chrono::DateTime<chrono::Utc>>>,
}

/// Task runtime + watch wiring dependencies.
#[derive(Clone)]
struct PipelineDeps {
    /// Run 事件出口（Task runtime 持久化投影由订阅者落库）。
    events: Arc<AgentEventBus>,
    agent_provider: Arc<dyn AutonomousAgentProvider>,
    runner: Arc<Runner>,
    /// 进行中 run 的内存注册表（T3/T4）。
    active_runs: ActiveRunRegistry,
}

/// One running workspace's watch engine + the loop that forwards non-Task
/// hits to the Session `TriggerRouter`.
struct WorkspaceWatch {
    tasks: Vec<JoinHandle<()>>,
}

/// Per-workspace thing-agent manager. Owns the Task runtime and each
/// workspace's watch engine.
pub struct ThingAgentManager {
    deps: PipelineDeps,
    workspaces: DashMap<String, WorkspaceWatch>,
    /// T13：handler 写穿与 bootstrap 注入用的 heartbeat 共享表句柄。
    heartbeat: HeartbeatDeps,
    /// T13：每工作区心跳热更新信号（handler 写穿后 notify；暂无消费者）。
    heartbeat_reload_notifies: DashMap<String, Arc<tokio::sync::Notify>>,
    /// E1 同键限速共享表（handler 写穿/读面兼容）。
    rate_limits: Arc<DashMap<String, u32>>,
    /// E1 watch 值守构建依赖（Task 8）：bootstrap 注入（`set_watch_deps`）；
    /// None = watch 未接线（测试/边缘形态），loop 不含 engine/forwarder。
    watch_deps: std::sync::Mutex<Option<WatchDeps>>,
    /// E1 watch API 写穿命令通道（每工作区）：start 时建立，stop 时丢弃。
    watch_cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>>,
    /// Session 唤醒适配器：watch 命中经此进入 TriggerRouter。启动时由
    /// service_manager 注入。
    session_router: parking_lot::Mutex<Option<Arc<crate::domains::agent::session::TriggerRouter>>>,
    task_runtime: std::sync::Mutex<Option<Arc<TaskCoordinator>>>,
    task_router: std::sync::Mutex<Option<JoinHandle<()>>>,
    /// Task 19C1 recovery/dispatch loop. It shares the Task runtime lifetime
    /// and is aborted on manager drop, like the wake router.
    task_dispatch: std::sync::Mutex<Option<Arc<crate::domains::agent::runtime::dispatch::EffectDispatch>>>,
    task_dispatch_task: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl ThingAgentManager {
    pub fn new(
        agent_provider: Arc<dyn AutonomousAgentProvider>,
        events: Arc<AgentEventBus>,
        runner: Arc<Runner>,
        heartbeat: HeartbeatDeps,
    ) -> Self {
        let watch_cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>> = Arc::new(DashMap::new());
        Self {
            deps: PipelineDeps {
                events,
                agent_provider,
                runner,
                active_runs: ActiveRunRegistry::with_default_capacity(),
            },
            workspaces: DashMap::new(),
            heartbeat,
            heartbeat_reload_notifies: DashMap::new(),
            rate_limits: Arc::new(DashMap::new()),
            watch_deps: std::sync::Mutex::new(None),
            watch_cmds,
            session_router: parking_lot::Mutex::new(None),
            task_runtime: std::sync::Mutex::new(None),
            task_router: std::sync::Mutex::new(None),
            task_dispatch: std::sync::Mutex::new(None),
            task_dispatch_task: std::sync::Mutex::new(None),
        }
    }

    /// Inject the Session wake router used by the watch forwarding loop.
    pub fn set_session_router(&self, router: Arc<crate::domains::agent::session::TriggerRouter>) {
        *self.session_router.lock() = Some(router);
    }

    fn session_router(&self) -> Option<Arc<crate::domains::agent::session::TriggerRouter>> {
        self.session_router.lock().clone()
    }

    pub async fn start_task_runtime(self: &Arc<Self>, deps: TaskRuntimeDeps) -> Result<(), TaskError> {
        if self
            .task_runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
        {
            return Ok(());
        }
        let jobs = deps.jobs.clone();
        let mut coordinator = TaskCoordinator::new(
            deps.store.clone(),
            jobs.clone(),
            deps.watches,
            self.watch_cmds.clone(),
            self.deps.events.clone(),
            self.deps.active_runs.clone(),
            self.deps.runner.clone(),
            self.deps.agent_provider.clone(),
        );
        if let Some(approvals) = &deps.pending_tool_approvals {
            coordinator = coordinator.with_pending_tool_approvals(approvals.clone());
        }
        if deps.harness_task_store {
            coordinator = coordinator.with_harness_task_store();
        }
        let coordinator = Arc::new(coordinator);
        // N1: fence indeterminate Effects (and pause their Tasks) before Task
        // recovery can start a Run. Job recovery first turns orphan
        // accepted/running Jobs into `interrupted`; the Effect fence then
        // settles them to `unknown` plus a reconciliation Decision. The wake
        // router is spawned only after this block.
        if let Some(dispatch) = deps.effect_dispatch.as_ref() {
            if let Some(jobs) = deps.jobs.as_ref() {
                jobs.recover().await.map_err(|error| {
                    TaskError::Store(tinyiothub_core::agent_task::TaskStoreError::Unavailable(format!(
                        "job recovery failed: {error}"
                    )))
                })?;
            }
            dispatch.reconcile_startup().await;
        }
        coordinator.recover().await?;
        let router = Arc::new(WakeRouter::new(deps.store, coordinator.clone()));
        let router_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(25));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut maintenance = tokio::time::interval(MAINTENANCE_INTERVAL);
            maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(error) = router.run(64).await {
                            tracing::warn!(%error, "Task wake routing failed");
                        }
                    }
                    _ = maintenance.tick() => {
                        if let Err(error) = router.sweep_finalize_intents(chrono::Utc::now()).await {
                            tracing::warn!(%error, "Finalize intent sweep failed");
                        }
                    }
                }
            }
        });
        *self.task_router.lock().unwrap_or_else(|error| error.into_inner()) = Some(router_task);
        // Task 19C1: the same composition that starts the Task runtime starts
        // the bounded per-workspace Effect dispatch/recovery pass.
        if let Some(dispatch) = deps.effect_dispatch {
            let task_dispatch = tokio::spawn(dispatch.clone().run());
            *self.task_dispatch.lock().unwrap_or_else(|error| error.into_inner()) = Some(dispatch);
            *self
                .task_dispatch_task
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(task_dispatch);
        }
        *self.task_runtime.lock().unwrap_or_else(|error| error.into_inner()) = Some(coordinator);
        Ok(())
    }

    /// The production Effect dispatcher, when this manager is fully wired.
    pub fn effect_dispatch(&self) -> Option<Arc<crate::domains::agent::runtime::dispatch::EffectDispatch>> {
        self.task_dispatch
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn task_coordinator(&self) -> Result<Arc<TaskCoordinator>, TaskError> {
        self.task_runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or_else(|| {
                tinyiothub_core::agent_task::TaskStoreError::Unavailable("Task runtime is not started".into()).into()
            })
    }

    pub async fn pause_task(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<tinyiothub_core::agent_task::AgentTask, TaskError> {
        self.task_coordinator()?.pause(workspace_id, task_id).await
    }

    pub async fn resume_task(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<tinyiothub_core::agent_task::AgentTask, TaskError> {
        self.task_coordinator()?.resume(workspace_id, task_id).await
    }

    pub async fn cancel_task(&self, workspace_id: &str, task_id: &str) -> Result<CancelOutcome, TaskError> {
        self.task_coordinator()?.cancel(workspace_id, task_id).await
    }

    pub async fn stop_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<tinyiothub_core::device_job::DeviceJob, tinyiothub_core::device_job::DeviceJobError> {
        self.task_coordinator()
            .map_err(|error| tinyiothub_core::device_job::DeviceJobError::Storage(error.to_string()))?
            .stop_task_job(workspace_id, task_id, job_id)
            .await
    }

    /// Start the watch engine for a workspace plus the forwarding loop that
    /// feeds non-Task hits into the Session `TriggerRouter`. Idempotent — a
    /// running workspace is left untouched. Wired only when watch deps were
    /// injected (tests / edge shapes without watch stay a no-op).
    pub fn start_workspace_watch(&self, workspace_id: &str) {
        use dashmap::mapref::entry::Entry;
        let Some(watch) = self.watch_deps.lock().expect("watch_deps poisoned").clone() else {
            tracing::debug!(workspace_id, "watch deps not wired — start ignored");
            return;
        };
        let Entry::Vacant(vacant) = self.workspaces.entry(workspace_id.to_string()) else {
            tracing::debug!(workspace_id, "workspace watch already running — start ignored");
            return;
        };

        let ws = workspace_id.to_string();
        let (hit_tx, hit_rx) = mpsc::channel(TRIGGER_CHANNEL_CAPACITY);
        let (cmd_tx, cmd_rx) = mpsc::channel(TRIGGER_CHANNEL_CAPACITY);
        self.watch_cmds.insert(ws.clone(), cmd_tx);
        let engine_ws = ws.clone();
        let engine_task = tokio::spawn(async move {
            watch_engine_loop(engine_ws, watch, hit_tx, cmd_rx).await;
        });

        // 命中转发：非 Task watch 进 Session Loop；Task-owned 命中已在
        // engine 内部经 `complete_task_watch` 落 TaskWake（`watch_wake`
        // 对 owner_task_id 为 Some 的命中返回 None）。
        let mut hit_rx = hit_rx;
        let router = self.session_router();
        let hit_ws = ws.clone();
        let session_hit_task = tokio::spawn(async move {
            while let Some(hit) = hit_rx.recv().await {
                if hit.workspace_id != hit_ws {
                    continue;
                }
                let Some(router) = router.clone() else {
                    tracing::warn!(
                        workspace_id = %hit.workspace_id,
                        rule_id = %hit.rule_id,
                        "watch hit dropped: session router not wired"
                    );
                    continue;
                };
                if let Some(wake) = crate::domains::agent::session::trigger::watch_wake(
                    &hit.workspace_id,
                    &hit.rule_id,
                    hit.owner_task_id.as_deref(),
                    &hit.current_value,
                    &format!("{:?}", hit.health),
                    &hit.zone,
                ) && !router.submit_event(wake)
                {
                    tracing::warn!(workspace_id = %hit.workspace_id, rule_id = %hit.rule_id, "watch wake not admitted");
                }
            }
        });

        vacant.insert(WorkspaceWatch {
            tasks: vec![engine_task, session_hit_task],
        });
        tracing::info!(workspace_id, "workspace watch started");
    }

    /// Stop the watch engine and forwarder, drop the command channel and the
    /// cached per-workspace agent. Unknown workspace is a no-op.
    pub async fn stop_workspace_watch(&self, workspace_id: &str) {
        let Some((_, watch)) = self.workspaces.remove(workspace_id) else {
            return;
        };
        for task in &watch.tasks {
            task.abort();
        }
        // T13：notify 条目随 loop 退役——重启后重建，handler 不会唤醒旧
        // trigger 持有的 Notify。
        self.heartbeat_reload_notifies.remove(workspace_id);
        // 心跳共享表随工作区清(RT-6)：workspace churn 下表无限增长。
        self.heartbeat.tasks.remove(workspace_id);
        self.heartbeat.intervals.remove(workspace_id);
        self.heartbeat.enabled.remove(workspace_id);
        self.heartbeat.last_ticks.remove(workspace_id);
        self.heartbeat.governor.record_success(workspace_id);
        self.deps.agent_provider.invalidate(workspace_id);
        // E1 watch：命令通道条目随 loop 退役——engine 任务已被 abort，
        // 残留 sender 只会让 handler 写穿投进死通道。
        self.watch_cmds.remove(workspace_id);
        // M-4：同键限速表随 loop 退役（RT-6 同款）。
        self.rate_limits.remove(workspace_id);
        tracing::info!(workspace_id, "workspace watch stopped");
    }

    pub fn is_running(&self, workspace_id: &str) -> bool {
        self.workspaces.contains_key(workspace_id)
    }

    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    #[cfg(test)]
    fn workspace_task_count_for_test(&self, workspace_id: &str) -> usize {
        self.workspaces
            .get(workspace_id)
            .map(|entry| entry.value().tasks.len())
            .unwrap_or(0)
    }

    /// 进行中 run 注册表（T3）的共享句柄——克隆零成本（内部 Arc 共享同一
    /// DashMap）。路由层（trace 端点实时分支）经此读进度。
    pub fn active_run_registry(&self) -> ActiveRunRegistry {
        self.deps.active_runs.clone()
    }

    // ── T13 heartbeat 依赖接管：bootstrap 注入 + handler 写穿 ────────────

    /// 每工作区心跳热更新信号（T13）。
    pub fn heartbeat_reload_notify(&self, workspace_id: &str) -> Arc<tokio::sync::Notify> {
        self.heartbeat_reload_notifies
            .entry(workspace_id.to_string())
            .or_default()
            .clone()
    }

    /// heartbeat 最近 tick 完成时间共享表（T12 透出）。
    pub fn heartbeat_last_ticks(&self) -> Arc<DashMap<String, chrono::DateTime<chrono::Utc>>> {
        Arc::clone(&self.heartbeat.last_ticks)
    }

    /// handler 写穿（D11-⑤：调用方已写 DB 成功）：任务表全量替换。
    pub fn set_heartbeat_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        self.heartbeat.tasks.insert(workspace_id.to_string(), tasks);
    }

    /// handler 写穿：每工作区间隔（分钟）覆盖。
    pub fn set_heartbeat_interval_minutes(&self, workspace_id: &str, interval_minutes: u32) {
        self.heartbeat
            .intervals
            .insert(workspace_id.to_string(), interval_minutes);
    }

    /// handler 写穿（终审 I-1）：心跳开关。
    pub fn set_heartbeat_enabled(&self, workspace_id: &str, enabled: bool) {
        self.heartbeat.enabled.insert(workspace_id.to_string(), enabled);
    }

    /// handler 写穿：信任配置。
    pub fn set_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.heartbeat.trust_configs.insert(workspace_id.to_string(), config);
    }

    /// 读出口（启动接线断言/调试）：任务表快照。
    pub fn heartbeat_tasks(&self, workspace_id: &str) -> Vec<HeartbeatTask> {
        self.heartbeat
            .tasks
            .get(workspace_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// 读出口：每工作区间隔（分钟）覆盖值。
    pub fn heartbeat_interval_minutes(&self, workspace_id: &str) -> Option<u32> {
        self.heartbeat.intervals.get(workspace_id).map(|r| *r)
    }

    /// 读出口（终审 I-1）：心跳开关——缺省（表无条目）= true。
    pub fn heartbeat_enabled(&self, workspace_id: &str) -> bool {
        self.heartbeat.enabled.get(workspace_id).map(|r| *r).unwrap_or(true)
    }

    /// 读出口：信任配置快照。
    pub fn heartbeat_trust_config(&self, workspace_id: &str) -> Option<TrustConfig> {
        self.heartbeat.trust_configs.get(workspace_id).map(|r| r.clone())
    }

    // ── E1 同键限速（D3/D4）：共享表（handler 写穿/读面兼容）───────────

    /// 同键限速共享表句柄。
    pub fn wake_rate_limits(&self) -> Arc<DashMap<String, u32>> {
        Arc::clone(&self.rate_limits)
    }

    /// handler 写穿（D11-⑤ 同款）：每工作区同键限速（N/分钟）。
    pub fn set_wake_rate_limit_per_min(&self, workspace_id: &str, per_min: u32) {
        self.rate_limits.insert(workspace_id.to_string(), per_min);
    }

    /// 读出口（启动接线断言/调试）：每工作区同键限速覆盖值。
    pub fn wake_rate_limit_per_min(&self, workspace_id: &str) -> Option<u32> {
        self.rate_limits.get(workspace_id).map(|r| *r)
    }

    // ── E1 watch 值守（Task 8）：构建依赖注入 + API 写穿命令通道 ──────────

    /// watch 构建依赖接线（bootstrap：注入后 start 的工作区含 engine +
    /// Session 唤醒转发两个任务）。
    pub fn set_watch_deps(&self, deps: WatchDeps) {
        *self.watch_deps.lock().expect("watch_deps poisoned") = Some(deps);
    }

    /// watch API 写穿出口（D11-⑤：handler 已写 DB 成功）：目标工作区的
    /// engine 命令通道。None = loop 未运行或 watch 未接线。
    pub fn watch_command_sink(&self, workspace_id: &str) -> Option<mpsc::Sender<WatchCommand>> {
        self.watch_cmds.get(workspace_id).map(|r| r.clone())
    }

    /// E3：工作区实时快照（SSE 首帧）。engine 未运行时返回 None。
    pub async fn workspace_snapshot(
        &self,
        workspace_id: &str,
    ) -> Option<tinyiothub_runtime::watch::live_state::WorkspaceSnapshot> {
        let sink = self.watch_command_sink(workspace_id)?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        sink.send(WatchCommand::Snapshot { reply }).await.ok()?;
        tokio::time::timeout(std::time::Duration::from_millis(120), rx)
            .await
            .ok()?
            .ok()?
    }
}

impl Drop for ThingAgentManager {
    fn drop(&mut self) {
        if let Some(task) = self
            .task_router
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
        if let Some(task) = self
            .task_dispatch_task
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

/// E2：向 engine actor 要本工作区实时快照（120ms 超时，fail-soft）。
pub(crate) async fn fetch_workspace_snapshot(
    cmds: &Arc<DashMap<String, mpsc::Sender<WatchCommand>>>,
    workspace_id: &str,
    demand: Option<&dyn SnapshotDemandSink>,
) -> (Option<String>, Option<tinyiothub_core::agent_runs::SnapshotMetrics>) {
    let Some(sink) = cmds.get(workspace_id).map(|s| s.clone()) else {
        return (None, None);
    };
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if sink.send(WatchCommand::Snapshot { reply: reply_tx }).await.is_err() {
        return (None, None);
    }
    match tokio::time::timeout(Duration::from_millis(120), reply_rx).await {
        Ok(Ok(Some(snapshot))) => {
            // E4b §3.2：快照装配属于一次读取——按设备声明属性记需求
            // （只记声明项，不把 LiveState 里的未声明键当作需求）。
            if let Some(sink) = demand {
                for device in &snapshot.devices {
                    for property in &device.properties {
                        sink.record_read(workspace_id, &device.thing_id, property);
                    }
                }
            }
            let (body, metrics) = crate::domains::agent::prompt::snapshot::render_snapshot(&snapshot);
            if body.is_empty() {
                (None, None)
            } else {
                (Some(body), Some(metrics))
            }
        }
        _ => {
            tracing::warn!(workspace_id, "workspace snapshot 装配超时/失败——空快照，run 照常");
            (None, None)
        }
    }
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) mod tests {
    use super::*;
    use crate::domains::agent::events::AgentEventKind;
    use crate::domains::agent::thing_agent::types::WakeSignal;
    use std::sync::Mutex;

    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_agent::port::runtime::AgentLoopConfig;

    const WS: &str = "ws_01";

    // ── scripted LLM provider (no network) ─────────────────────

    /// Replies a fixed text, never calls tools; records every prompt.
    #[derive(Clone, Default)]
    pub(crate) struct ScriptedProvider {
        pub(crate) prompts: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            self.prompts
                .lock()
                .unwrap()
                .push(request.messages.iter().map(|m| m.content.as_str()).collect::<String>());
            Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }
        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    /// Stub agent provider: one shared loop driven by the scripted provider.
    pub(crate) struct StubAgentProvider {
        handle: AgentHandle,
        pub(crate) llm: Arc<ScriptedProvider>,
        pub(crate) invalidated: Mutex<Vec<String>>,
        _dir: tempfile::TempDir,
    }

    impl StubAgentProvider {
        pub(crate) fn new() -> Self {
            let llm = Arc::new(ScriptedProvider::default());
            let dir = tempfile::tempdir().expect("tempdir");
            let llm_for_factory = Arc::clone(&llm);
            let cfg = AgentLoopConfig {
                model_name: "stub-model".to_string(),
                prompt_builder: tinyiothub_agent::port::prompt::SystemPromptBuilder::with_defaults(),
                tools: vec![],
                memory: Arc::new(tinyiothub_agent::port::memory::NoopMemory),
                observer: Arc::new(tinyiothub_agent::port::observer::NoopObserver),
                workspace_dir: dir.path().to_path_buf(),
                security_summary: None,
                require_context_budget: false,
                provider_factory: Arc::new(move || Ok(Box::new(llm_for_factory.as_ref().clone()))),
                conversation_memory: true,
                available_capabilities: vec![],
                tool_call_ids: None,
                checkpoint_before_tool: true,
            };
            let handle = tinyiothub_agent::adapters::rig::loop_::rig_loop_factory(cfg).expect("build stub loop");
            Self {
                handle,
                llm,
                invalidated: Mutex::new(vec![]),
                _dir: dir,
            }
        }
    }

    #[async_trait::async_trait]
    impl AutonomousAgentProvider for StubAgentProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            Ok(Arc::clone(&self.handle))
        }

        fn invalidate(&self, workspace_id: &str) {
            self.invalidated.lock().unwrap().push(workspace_id.to_string());
        }
    }

    // ── run event probe ────────────────────────────────────────

    /// Run 探针：惰性排空 RunRecorded 事件流（Task runtime 的持久化投影
    /// 订阅者消费的同一出口）。try_recv，无后台任务，读取确定。
    pub(crate) struct RunsProbe {
        rx: Mutex<tokio::sync::broadcast::Receiver<crate::domains::agent::events::AgentEvent>>,
        seen: Mutex<Vec<Option<String>>>,
    }

    impl RunsProbe {
        pub(crate) fn has_dedup(&self, key: &str) -> bool {
            self.drain_events();
            self.seen
                .lock()
                .unwrap()
                .iter()
                .any(|dedup| dedup.as_deref() == Some(key))
        }

        fn drain_events(&self) {
            let mut rx = self.rx.lock().unwrap();
            while let Ok(event) = rx.try_recv() {
                if let AgentEventKind::RunRecorded { dedup_key, .. } = event.kind {
                    self.seen.lock().unwrap().push(dedup_key);
                }
            }
        }
    }

    pub(crate) struct StubManagerParts {
        pub(crate) manager: Arc<ThingAgentManager>,
        pub(crate) agents: Arc<StubAgentProvider>,
        /// 测试可播种 tasks/trust 后再 start，governor 克隆读连败计数。
        pub(crate) heartbeat: HeartbeatDeps,
    }

    pub(crate) async fn stub_manager() -> StubManagerParts {
        let events = Arc::new(AgentEventBus::new(64));
        let agents = Arc::new(StubAgentProvider::new());
        let heartbeat = HeartbeatDeps::default();
        let manager = Arc::new(ThingAgentManager::new(
            agents.clone(),
            events,
            Arc::new(Runner::new()),
            heartbeat.clone(),
        ));
        StubManagerParts {
            manager,
            agents,
            heartbeat,
        }
    }

    // ── helpers ────────────────────────────────────────────────

    /// Yield until `cond` holds; panics after a bounded number of yields so a
    /// broken wiring fails loudly instead of hanging (paused-time safe).
    async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        for _ in 0..20_000 {
            if cond() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("timed out waiting for {what}");
    }

    /// Yield pump so the engine→forwarder→router chain picks up freshly
    /// published events.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    // ── T13 bootstrap 装配 ─────────────────────────────────────

    /// T13：reload_notify 每工作区独立且稳定。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_reload_notify_is_per_workspace_and_stable() {
        let parts = stub_manager().await;
        let a1 = parts.manager.heartbeat_reload_notify("ws_a");
        let a2 = parts.manager.heartbeat_reload_notify("ws_a");
        let b = parts.manager.heartbeat_reload_notify("ws_b");
        assert!(Arc::ptr_eq(&a1, &a2), "同一工作区须为同一 Notify");
        assert!(!Arc::ptr_eq(&a1, &b), "不同工作区不得共享 Notify");
    }

    /// T13：stop 丢弃 notify 条目——重启后重建。
    #[tokio::test(start_paused = true)]
    async fn stop_drops_reload_notify_entry() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        parts.manager.set_watch_deps(watch_deps(vec![], ledger));
        parts.manager.start_workspace_watch(WS);
        let before = parts.manager.heartbeat_reload_notify(WS);
        parts.manager.stop_workspace_watch(WS).await;
        let after = parts.manager.heartbeat_reload_notify(WS);
        assert!(!Arc::ptr_eq(&before, &after), "stop 后须重建 Notify");
    }

    /// T13：handler 写穿访问器更新共享表（与读侧同一 DashMap 真源）。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_write_through_updates_shared_tables() {
        let parts = stub_manager().await;
        parts.manager.set_heartbeat_tasks(
            WS,
            vec![HeartbeatTask {
                id: 1,
                workspace_id: WS.into(),
                priority: "P1".into(),
                text: "巡检设备在线率".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        );
        assert_eq!(parts.manager.heartbeat_tasks(WS).len(), 1);
        assert_eq!(parts.heartbeat.tasks.get(WS).map(|t| t.len()), Some(1));

        parts.manager.set_heartbeat_interval_minutes(WS, 45);
        assert_eq!(parts.manager.heartbeat_interval_minutes(WS), Some(45));
        assert_eq!(parts.heartbeat.intervals.get(WS).map(|v| *v), Some(45));

        parts.manager.set_trust_config(
            WS,
            TrustConfig {
                max_auto_actions_per_tick: 3,
                ..Default::default()
            },
        );
        assert_eq!(
            parts
                .manager
                .heartbeat_trust_config(WS)
                .map(|t| t.max_auto_actions_per_tick),
            Some(3)
        );

        // last_ticks 共享表句柄（T12 透出）经 manager 暴露给 bootstrap 注入。
        assert!(parts.manager.heartbeat_last_ticks().is_empty());
    }

    // ── watch lifecycle ────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn stop_halts_watch_and_is_idempotent() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        parts.manager.set_watch_deps(watch_deps(vec![], ledger));
        parts.manager.start_workspace_watch(WS);
        assert!(parts.manager.is_running(WS));
        assert_eq!(parts.manager.workspace_count(), 1);
        assert!(parts.manager.watch_command_sink(WS).is_some());

        parts.manager.stop_workspace_watch(WS).await;
        assert!(!parts.manager.is_running(WS));
        assert_eq!(parts.manager.workspace_count(), 0);
        assert!(parts.manager.watch_command_sink(WS).is_none(), "stop 丢弃命令通道");

        // Stop is idempotent.
        parts.manager.stop_workspace_watch(WS).await;
    }

    #[tokio::test(start_paused = true)]
    async fn stop_invalidates_cached_agent() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        parts.manager.set_watch_deps(watch_deps(vec![], ledger));
        parts.manager.start_workspace_watch(WS);

        parts.manager.stop_workspace_watch(WS).await;
        assert_eq!(
            parts.agents.invalidated.lock().unwrap().as_slice(),
            &[WS.to_string()],
            "stop must invalidate the cached agent for the workspace"
        );

        // Unknown workspace stop → no invalidate call.
        parts.manager.stop_workspace_watch("ws_unknown").await;
        assert_eq!(parts.agents.invalidated.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn start_without_watch_deps_is_a_noop() {
        let parts = stub_manager().await;
        parts.manager.start_workspace_watch(WS);
        assert_eq!(parts.manager.workspace_task_count_for_test(WS), 0);
        assert!(parts.manager.watch_command_sink(WS).is_none());
        assert!(!parts.manager.is_running(WS));
    }

    // ── Task runtime ───────────────────────────────────────────

    #[tokio::test]
    async fn production_task_runtime_dispatches_a_persisted_wake_through_manager() {
        use chrono::Utc;
        use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};

        let parts = stub_manager().await;
        let store = Arc::new(crate::domains::agent::thing_agent::task::coordinator::tests::MemoryTasks::default());
        let task = AgentTask {
            id: "task_production".into(),
            workspace_id: WS.into(),
            idempotency_key: "production-key".into(),
            objective: "inspect the device".into(),
            session_key: None,
            created_by: "user_1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "event_production".into(),
            signal_json: serde_json::to_string(&WakeSignal {
                workspace_id: WS.into(),
                priority: crate::domains::agent::thing_agent::types::Priority::High,
                source: crate::domains::agent::thing_agent::types::TriggerSource::UserDirective {
                    user_id: "user_1".into(),
                    text: task.objective.clone(),
                    session_key: None,
                    source: None,
                    problem_key: None,
                },
                dedup_key: None,
            })
            .expect("wake json"),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        };
        tinyiothub_agent::port::task::TaskStore::create_task_with_wake(store.as_ref(), &task, &wake)
            .await
            .expect("persist task wake");
        parts
            .manager
            .start_task_runtime(crate::domains::agent::thing_agent::manager::TaskRuntimeDeps {
                store,
                pending_tool_approvals: None,
                watches: Arc::new(StubWatchRuleStore { rules: vec![] }),
                jobs: None,
                harness_task_store: false,
                effect_dispatch: None,
            })
            .await
            .expect("start production task runtime");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if parts.agents.llm.prompts.lock().expect("prompts").len() == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("persisted wake dispatched by production router");
        assert_eq!(parts.agents.llm.prompts.lock().expect("prompts").len(), 1);
    }

    #[tokio::test]
    async fn dropping_manager_terminates_task_router() {
        let parts = stub_manager().await;
        let store = Arc::new(crate::domains::agent::thing_agent::task::coordinator::tests::MemoryTasks::default());
        parts
            .manager
            .start_task_runtime(TaskRuntimeDeps {
                store,
                pending_tool_approvals: None,
                watches: Arc::new(StubWatchRuleStore { rules: vec![] }),
                jobs: None,
                harness_task_store: false,
                effect_dispatch: None,
            })
            .await
            .expect("start task runtime");
        let router = parts
            .manager
            .task_router
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .expect("router task")
            .abort_handle();
        let weak = Arc::downgrade(&parts.manager);

        drop(parts.manager);

        assert!(weak.upgrade().is_none(), "Task runtime must not retain the manager");
        tokio::time::timeout(Duration::from_secs(1), async {
            while !router.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("router terminates when manager drops");
    }

    // ── E1 Task 8：watch 值守装配（engine 重织 + 事件接线 + 命令写穿）────

    use tinyiothub_core::models::thing::{Thing, ThingStatus};
    use tinyiothub_core::watch::{CompareOp, WatchCondition, WatchRule};
    use tinyiothub_runtime::ports::{HubLedger, ThingCacheSource, WatchRuleStore};

    /// 内存桩：最小可读 ThingCacheSource（engine LiveState 真相源）。
    struct StubThingCache {
        things: Vec<Thing>,
    }

    impl ThingCacheSource for StubThingCache {
        fn all(&self) -> Vec<Thing> {
            self.things.clone()
        }
        fn get(&self, id: &str) -> Option<Thing> {
            self.things.iter().find(|t| t.id == id).cloned()
        }
        fn get_by_name(&self, name: &str) -> Option<Thing> {
            self.things.iter().find(|t| t.name == name).cloned()
        }
        fn insert(&self, _device: Thing) {}
        fn update(&self, _device: Thing) {}
        fn remove(&self, _id: &str) {}
    }

    /// 内存桩：固定规则集的 WatchRuleStore（启动重织读面）。
    struct StubWatchRuleStore {
        rules: Vec<WatchRule>,
    }

    #[async_trait::async_trait]
    impl WatchRuleStore for StubWatchRuleStore {
        async fn list_all(&self) -> Result<Vec<WatchRule>, String> {
            Ok(self.rules.clone())
        }
        async fn insert(&self, _rule: &WatchRule) -> Result<(), String> {
            Ok(())
        }
        async fn delete(&self, _workspace_id: &str, _id: &str) -> Result<bool, String> {
            Ok(true)
        }
        async fn complete_task_watch(
            &self,
            _workspace_id: &str,
            _id: &str,
            _wake: &tinyiothub_core::agent_task::TaskWake,
        ) -> Result<bool, String> {
            Ok(true)
        }
        async fn delete_task_watches(&self, _workspace_id: &str, _task_id: &str) -> Result<usize, String> {
            Ok(0)
        }
    }

    /// 内存桩：记录全部 append 的 HubLedger。
    #[derive(Default)]
    pub(crate) struct RecordingLedger {
        entries: Mutex<Vec<(String, String, Option<String>)>>,
    }

    #[async_trait::async_trait]
    impl HubLedger for RecordingLedger {
        async fn append(
            &self,
            workspace_id: &str,
            kind: &str,
            thing_id: Option<&str>,
            _payload: serde_json::Value,
        ) -> Result<(), String> {
            self.entries.lock().unwrap().push((
                workspace_id.to_string(),
                kind.to_string(),
                thing_id.map(str::to_string),
            ));
            Ok(())
        }
    }

    fn watch_thing(id: &str) -> Thing {
        Thing {
            id: id.into(),
            name: id.into(),
            workspace_id: Some(WS.to_string()),
            status: ThingStatus::Online,
            ..Thing::default()
        }
    }

    fn watch_rule(id: &str, thing_id: &str) -> WatchRule {
        WatchRule {
            id: id.into(),
            workspace_id: WS.into(),
            condition: WatchCondition::Property {
                thing_id: thing_id.into(),
                property: "温度".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30.0),
            },
            persistent: false,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
            owner_task_id: None,
        }
    }

    pub(crate) fn watch_deps(rules: Vec<WatchRule>, ledger: Arc<RecordingLedger>) -> WatchDeps {
        WatchDeps {
            cache: Arc::new(StubThingCache {
                things: vec![watch_thing("t1")],
            }),
            ledger,
            rules: Arc::new(StubWatchRuleStore { rules }),
            events: Arc::new(tinyiothub_runtime::EventBus::new()),
            zones: None, // E4d：测试保持 hierarchy
        }
    }

    struct TaskWatchStore {
        tasks: Arc<crate::domains::agent::thing_agent::task::coordinator::tests::MemoryTasks>,
        rules: Mutex<Vec<WatchRule>>,
    }

    #[async_trait::async_trait]
    impl WatchRuleStore for TaskWatchStore {
        async fn list_all(&self) -> Result<Vec<WatchRule>, String> {
            Ok(self.rules.lock().expect("rules").clone())
        }
        async fn insert(&self, rule: &WatchRule) -> Result<(), String> {
            self.rules.lock().expect("rules").push(rule.clone());
            Ok(())
        }
        async fn delete(&self, workspace_id: &str, id: &str) -> Result<bool, String> {
            let mut rules = self.rules.lock().expect("rules");
            let before = rules.len();
            rules.retain(|rule| rule.workspace_id != workspace_id || rule.id != id);
            Ok(rules.len() != before)
        }
        async fn complete_task_watch(
            &self,
            workspace_id: &str,
            id: &str,
            wake: &tinyiothub_core::agent_task::TaskWake,
        ) -> Result<bool, String> {
            let deleted = self.delete(workspace_id, id).await?;
            if deleted {
                tinyiothub_agent::port::task::TaskStore::record_wake(self.tasks.as_ref(), wake)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Ok(deleted)
        }
        async fn delete_task_watches(&self, workspace_id: &str, task_id: &str) -> Result<usize, String> {
            let mut rules = self.rules.lock().expect("rules");
            let before = rules.len();
            rules.retain(|rule| rule.workspace_id != workspace_id || rule.owner_task_id.as_deref() != Some(task_id));
            Ok(before - rules.len())
        }
    }

    struct BlockingLoop {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl tinyiothub_agent::port::runtime::AgentLoop for BlockingLoop {
        async fn turn_streamed(
            &self,
            _user_message: &str,
            _event_tx: mpsc::Sender<tinyiothub_agent::port::events::TurnEvent>,
            _cancel_token: Option<tokio_util::sync::CancellationToken>,
        ) -> anyhow::Result<String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.entered.notify_one();
            self.release.notified().await;
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

    struct BlockingProvider {
        handle: AgentHandle,
        calls: Arc<std::sync::atomic::AtomicUsize>,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    impl BlockingProvider {
        fn new() -> Self {
            let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let handle = Arc::new(tokio::sync::Mutex::new(BlockingLoop {
                calls: calls.clone(),
                entered: entered.clone(),
                release: release.clone(),
            }));
            Self {
                handle,
                calls,
                entered,
                release,
            }
        }
    }

    #[async_trait::async_trait]
    impl AutonomousAgentProvider for BlockingProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            Ok(self.handle.clone())
        }
        fn invalidate(&self, _workspace_id: &str) {}
    }

    /// 发布一条 PropertyChange 领域事件（data_server cache_and_publish 同款
    /// 载荷：metadata property_name/value + device_property source）。
    async fn publish_property_change(bus: &tinyiothub_runtime::EventBus, thing_id: &str, value: &str) {
        let content = tinyiothub_core::models::event::RichContent::new_text("t".into(), "c".into())
            .with_metadata("property_name".to_string(), serde_json::json!("温度"))
            .with_metadata("value".to_string(), serde_json::json!(value));
        let event = tinyiothub_core::models::event::Event::new_device_event(
            tinyiothub_core::models::event::ThingEventType::PropertyChange,
            tinyiothub_core::models::event::EventLevel::Debug,
            tinyiothub_core::models::event::EventSource::device_property(
                thing_id.to_string(),
                "p1".to_string(),
                "data_collector".to_string(),
            ),
            content,
        )
        .expect("event");
        bus.publish(event).await.expect("publish");
    }

    // ── watch → Session TriggerRouter forwarding ─────────────────

    /// 记录每条 event wake 的 WakeExecutor（TriggerRouter 的第二出口）。
    #[derive(Default)]
    struct RecordingExecutor {
        wakes: Mutex<Vec<crate::domains::agent::session::trigger::EventWake>>,
    }

    impl RecordingExecutor {
        fn wakes(&self) -> Vec<crate::domains::agent::session::trigger::EventWake> {
            self.wakes.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl crate::domains::agent::session::wake::WakeExecutor for RecordingExecutor {
        async fn execute(
            &self,
            _schedule: tinyiothub_storage::assistant_schedule::AssistantSchedule,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> crate::domains::agent::session::wake::WakeReport {
            crate::domains::agent::session::wake::WakeReport {
                session_key: String::new(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Skipped,
            }
        }

        async fn execute_event(
            &self,
            wake: crate::domains::agent::session::trigger::EventWake,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> crate::domains::agent::session::wake::WakeReport {
            self.wakes.lock().unwrap().push(wake);
            crate::domains::agent::session::wake::WakeReport {
                session_key: String::new(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
            }
        }
    }

    async fn recording_router() -> (
        Arc<crate::domains::agent::session::TriggerRouter>,
        Arc<RecordingExecutor>,
    ) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.expect("sqlite");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(crate::domains::agent::session::TriggerRouter::new(db, executor.clone()));
        (router, executor)
    }

    #[tokio::test]
    async fn watch_hit_feeds_session_router_end_to_end() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        let deps = watch_deps(vec![watch_rule("r1", "t1")], ledger.clone());
        let bus = deps.events.clone();
        parts.manager.set_watch_deps(deps);
        let (router, executor) = recording_router().await;
        parts.manager.set_session_router(router.clone());
        let _router_handle = router.clone().start();
        parts.manager.start_workspace_watch(WS);

        // engine + session forwarder two tasks.
        assert_eq!(parts.manager.workspace_task_count_for_test(WS), 2, "engine + forward");
        assert!(parts.manager.watch_command_sink(WS).is_some());

        // 未越限：不命中。
        publish_property_change(&bus, "t1", "25").await;
        settle().await;
        assert!(executor.wakes().is_empty(), "under-threshold value must not wake");

        // 越限：命中 → EventWake(watch) 进 TriggerRouter → executor 收到。
        publish_property_change(&bus, "t1", "31").await;
        wait_until(|| executor.wakes().len() == 1, "watch session wake").await;
        let wake = executor.wakes().remove(0);
        assert_eq!(wake.workspace_id, WS);
        assert_eq!(wake.trigger, "watch");
        assert_eq!(wake.dedup_key.as_deref(), Some("watch:r1"));
        wait_until(
            || ledger.entries.lock().unwrap().iter().any(|e| e.1 == "watch_hit"),
            "ledger watch_hit",
        )
        .await;
    }

    /// API 写穿（D11-⑤ 内存步）：Upsert 命令进 engine → 事件命中；Remove
    /// 后同事物越限不再命中。stop 丢弃命令通道。
    #[tokio::test]
    async fn watch_command_upsert_then_remove_controls_engine() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        let deps = watch_deps(vec![], ledger); // 空规则起步——不经重织
        let bus = deps.events.clone();
        parts.manager.set_watch_deps(deps);
        let (router, executor) = recording_router().await;
        parts.manager.set_session_router(router.clone());
        let _router_handle = router.clone().start();
        parts.manager.start_workspace_watch(WS);
        let sink = parts.manager.watch_command_sink(WS).expect("command sink");

        sink.send(WatchCommand::Upsert(watch_rule("r2", "t1")))
            .await
            .expect("upsert sent");
        settle().await; // 命令先于事件进 engine（单循环串行消费）
        publish_property_change(&bus, "t1", "31").await;
        wait_until(|| executor.wakes().len() == 1, "upserted rule hits").await;
        assert_eq!(executor.wakes()[0].dedup_key.as_deref(), Some("watch:r2"));

        // one-shot 命中后已自动移除；显式 Remove 幂等不炸，再越限不命中。
        sink.send(WatchCommand::Remove("r2".into())).await.expect("remove sent");
        settle().await;
        publish_property_change(&bus, "t1", "32").await;
        settle().await;
        assert_eq!(executor.wakes().len(), 1, "移除后不得再命中");

        parts.manager.stop_workspace_watch(WS).await;
        assert!(parts.manager.watch_command_sink(WS).is_none(), "stop 丢弃命令通道");
    }

    /// Task 10：Summary 命令经命令通道回传单物实时摘要（get_thing_profile
    /// 工具读面的生产路径）——无实时态 → None；事件进场后 → Some 且值一致。
    #[tokio::test(start_paused = true)]
    async fn watch_command_summary_returns_live_state_snapshot() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        let deps = watch_deps(vec![], ledger); // 空规则起步——Summary 不依赖规则
        let bus = deps.events.clone();
        parts.manager.set_watch_deps(deps);
        parts.manager.start_workspace_watch(WS);
        let sink = parts.manager.watch_command_sink(WS).expect("command sink");

        // 无实时态（未见过该物事件）→ None。
        let (tx, rx) = tokio::sync::oneshot::channel();
        sink.send(WatchCommand::Summary {
            thing_id: "t1".into(),
            reply: tx,
        })
        .await
        .expect("summary query sent");
        assert!(rx.await.expect("reply").is_none());

        // 属性事件进场 → LiveState 有该物 → Some 且值一致。
        publish_property_change(&bus, "t1", "31").await;
        settle().await; // 单循环串行消费：事件先于其后的 Summary 命令
        let (tx, rx) = tokio::sync::oneshot::channel();
        sink.send(WatchCommand::Summary {
            thing_id: "t1".into(),
            reply: tx,
        })
        .await
        .expect("summary query sent");
        let s = rx.await.expect("reply").expect("summary after event");
        assert_eq!(s.values.get("温度").map(String::as_str), Some("31"));
        assert!(s.online);
    }

    /// Task-owned watch hits are settled inside the engine (TaskWake), never
    /// forwarded into the Session loop.
    #[tokio::test]
    async fn task_watch_hit_produces_one_task_execution_and_no_session_wake() {
        use chrono::Utc;
        use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};

        let events = Arc::new(AgentEventBus::new(64));
        let runs = RunsProbe {
            rx: Mutex::new(events.subscribe()),
            seen: Mutex::new(vec![]),
        };
        let provider = Arc::new(BlockingProvider::new());
        let manager = Arc::new(ThingAgentManager::new(
            provider.clone(),
            events,
            Arc::new(Runner::new()),
            HeartbeatDeps::default(),
        ));
        let store = Arc::new(crate::domains::agent::thing_agent::task::coordinator::tests::MemoryTasks::default());
        let mut rule = watch_rule("task_watch", "t1");
        rule.owner_task_id = Some("task_watch_owner".into());
        let watches = Arc::new(TaskWatchStore {
            tasks: store.clone(),
            rules: Mutex::new(vec![rule]),
        });
        let ledger = Arc::new(RecordingLedger::default());
        let mut deps = watch_deps(vec![], ledger);
        deps.rules = watches.clone();
        let bus = deps.events.clone();
        manager.set_watch_deps(deps);
        let now = Utc::now();
        let task = AgentTask {
            id: "task_watch_owner".into(),
            workspace_id: WS.into(),
            idempotency_key: "watch-e2e".into(),
            objective: "inspect".into(),
            session_key: None,
            created_by: "user_1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "initial_watch_e2e".into(),
            signal_json: serde_json::to_string(&WakeSignal {
                workspace_id: WS.into(),
                priority: crate::domains::agent::thing_agent::types::Priority::High,
                source: crate::domains::agent::thing_agent::types::TriggerSource::UserDirective {
                    user_id: "user_1".into(),
                    text: "inspect".into(),
                    session_key: None,
                    source: None,
                    problem_key: None,
                },
                dedup_key: None,
            })
            .expect("wake"),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        tinyiothub_agent::port::task::TaskStore::create_task_with_wake(store.as_ref(), &task, &wake)
            .await
            .expect("task");
        manager
            .start_task_runtime(TaskRuntimeDeps {
                store: store.clone(),
                pending_tool_approvals: None,
                watches,
                jobs: None,
                harness_task_store: false,
                effect_dispatch: None,
            })
            .await
            .expect("task runtime");
        manager.start_workspace_watch(WS);
        tokio::time::timeout(Duration::from_secs(2), provider.entered.notified())
            .await
            .expect("initial task run");

        publish_property_change(&bus, "t1", "31").await;
        tokio::time::timeout(Duration::from_secs(2), async {
            while store.wake_count() != 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("task watch wake persisted");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            !runs.has_dedup("watch:task_watch"),
            "task-owned watch must not produce a Session-side run"
        );
        provider.release.notify_one();
    }

    /// E4b §3.2：turn 快照装配把每台设备声明的属性（含未采样项）写入需求出口。
    #[tokio::test]
    async fn workspace_snapshot_records_declared_properties_as_demand() {
        use crate::domains::agent::thing_agent::traits::SnapshotDemandSink;
        use tinyiothub_runtime::watch::live_state::{DeviceSnapshot, Health, WorkspaceSnapshot};

        #[derive(Default)]
        struct RecordingSink(Mutex<Vec<(String, String, String)>>);

        impl SnapshotDemandSink for RecordingSink {
            fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
                self.0
                    .lock()
                    .unwrap()
                    .push((workspace_id.into(), thing_id.into(), property.into()));
            }
        }

        let cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>> = Arc::new(DashMap::new());
        let (tx, mut rx) = mpsc::channel(4);
        cmds.insert(WS.to_string(), tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let WatchCommand::Snapshot { reply } = command {
                    let _ = reply.send(Some(WorkspaceSnapshot {
                        v: 1,
                        total: 1,
                        devices: vec![DeviceSnapshot {
                            thing_id: "t1".into(),
                            name: "T1".into(),
                            health: Health::Ok,
                            online: true,
                            zone: "z".into(),
                            // 湿度只声明、未上报：快照读取仍要为它建立需求。
                            values: vec![("温度".into(), "31".into())],
                            properties: vec!["温度".into(), "湿度".into()],
                        }],
                    }));
                }
            }
        });

        let sink = RecordingSink::default();
        let (body, metrics) = fetch_workspace_snapshot(&cmds, WS, Some(&sink)).await;
        assert!(body.is_some());
        assert_eq!(metrics.expect("metrics").devices, 1);

        let mut reads = sink.0.lock().unwrap().clone();
        reads.sort();
        assert_eq!(
            reads,
            vec![
                (WS.to_string(), "t1".to_string(), "温度".to_string()),
                (WS.to_string(), "t1".to_string(), "湿度".to_string()),
            ],
            "declared properties (including unsampled) must be recorded"
        );
    }
}
