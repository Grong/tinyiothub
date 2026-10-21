//! Thing-agent manager (T15) — per-workspace loop registry, the closed-loop
//! wiring point between triggers (T5/T7), scheduler (T8) and the run
//! pipeline (T9 runner + T10 prompt + T11 factory + T12 persistence + T13
//! pushback).
//!
//! ```text
//! WorkspaceCreated → start(): ThingEventTrigger ─┐
//!                       TimerTrigger ────────────┤
//!                       HeartbeatTrigger (T5) ───┼─► forward ─► SchedulerHandle ─► run_pipeline
//!                       DirectiveSink (T14) ─────┘                    │
//!                                                                     ▼
//!                                              build_prompt (T10, memory/history from RunRegistry;
//!                                              Heartbeat 源走 heartbeat 专用 prompt,Task 6)
//!                                                                     │
//!                                              factory.get_or_create (T11) → runner.execute (T9)
//!                                                                     │
//!                                              registry.record + RunRecorded 事件 (Task 4) → deliver (T13)
//!                                              Heartbeat 源另记 governor 连败 (Task 6,行为保持清单第 1 行)
//! WorkspaceDeleted → stop(): abort triggers, drain (O26), drop handle
//! ```
//!
//! Mode gating (off/diagnose/act) is NOT re-implemented here: all triggers
//! (T5 timer / T5 heartbeat / T7 thing-event) already suppress signals when
//! the policy mode is off, and the autonomous `invoke_action` tool (T11)
//! fail-closes on the policy gate. The manager is deliberately a dumb wiring
//! layer.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::prompt::runtime::build_prompt_with_snapshot;
use crate::runtime::events::{AgentEventBus, AgentEventKind};
use crate::runtime::thing_agent::active_runs::ActiveRunRegistry;
use crate::runtime::thing_agent::governor::HeartbeatGovernor;
use crate::runtime::thing_agent::pushback::deliver;
use crate::runtime::thing_agent::registry::RunRegistry;
use crate::runtime::thing_agent::runner::{AgentHandle, RunContext, RunContextInner, Runner};
use crate::runtime::thing_agent::scheduler::{EnqueueError, Scheduler, SchedulerHandle};
use crate::runtime::thing_agent::traits::{
    AutonomyPolicyReader, DirectiveSink, HeartbeatTaskReader, ThingAgentHost, TrustConfigReader,
};
use crate::runtime::thing_agent::trigger::{HeartbeatTrigger, ThingEventTrigger, TimerTrigger, Trigger, WatchTrigger};
use crate::runtime::thing_agent::types::{HeartbeatAlarmContext, TriggerSource, WakeSignal};
use crate::runtime::thing_agent::watch_loop::{WatchCommand, WatchDeps, watch_engine_loop};
use tinyiothub_core::agent_runs::format_summary;
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};
use tinyiothub_policy::autonomy::AutonomyMode;

/// Capacity of the per-workspace trigger→scheduler channel. Backpressure
/// beyond this parks the thing-event trigger, which then lags the broadcast
/// and recovers through cursor replay (O27).
const TRIGGER_CHANNEL_CAPACITY: usize = 64;

/// In-flight run registry capacity (T3) — same source as scheduler
/// concurrency capacity so a full trigger channel can't out-run the registry.
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

/// Tunables for [`ThingAgentManager`].
#[derive(Debug, Clone)]
pub struct ThingAgentManagerConfig {
    /// 定时巡检间隔（spec 默认 15min）。
    pub timer_interval: Duration,
    /// 物事件唤醒最低级别（spec 默认 3 = warning）。
    pub min_wake_level: i32,
    /// 心跳巡检默认间隔（T6；bootstrap 期自 HeartbeatConfig.interval_minutes
    /// 来，T13 接线；每工作区覆盖走 heartbeat intervals 共享表）。
    pub heartbeat_default_interval: Duration,
}

impl Default for ThingAgentManagerConfig {
    fn default() -> Self {
        Self {
            timer_interval: Duration::from_secs(15 * 60),
            min_wake_level: 3,
            heartbeat_default_interval: Duration::from_secs(15 * 60),
        }
    }
}

/// Heartbeat 源的共享内存依赖（Task 6）：trigger 的 tasks/intervals 读表、
/// prompt 装配的任务/信任注入源、governor 连败治理、last_ticks 实时读透出。
/// cloud bootstrap（T13）经 restore 自快照预热，handler 写穿共享同一张表
///（D11-⑤ 写序不变）。默认空表 = trigger 空任务抑制不 tick（行为保持
/// loop_.rs:63）。
#[derive(Clone, Default)]
pub struct HeartbeatDeps {
    /// 每工作区巡检任务表（trigger 读 + prompt 读共享同一张）。
    pub tasks: Arc<DashMap<String, Vec<HeartbeatTask>>>,
    /// 每工作区间隔（分钟）覆盖表——update_config 写穿。
    pub intervals: Arc<DashMap<String, u32>>,
    /// 每工作区心跳开关表（终审 I-1）：trigger 判定集新增维度，false 抑制
    /// 定时 tick 与 notify 立即 tick；缺省（表无条目）= true（对齐
    /// HeartbeatConfig::default enabled:true）。restore 预热自 DB
    /// heartbeat_config，update_config 写穿。
    pub enabled: Arc<DashMap<String, bool>>,
    /// 每工作区信任配置表——prompt 信任段注入源。
    pub trust_configs: Arc<DashMap<String, TrustConfig>>,
    /// 连败治理（T4）：run_pipeline 记账，trigger 读暂停态。
    pub governor: HeartbeatGovernor,
    /// 每工作区最近 tick 完成时间表（Task 12，D13 实时读）：run_pipeline 在
    /// heartbeat 源 run 完成时写入（成败均计，对齐旧 loop_.rs:74-75）；handler
    /// get_config 的 last_tick 读源（T13 bootstrap 注入同一张表）。
    pub last_ticks: Arc<DashMap<String, chrono::DateTime<chrono::Utc>>>,
}

/// Clonable dependency bundle for the run pipeline.
#[derive(Clone)]
struct PipelineDeps {
    host: Arc<dyn ThingAgentHost>,
    policy_repo: Arc<dyn AutonomyPolicyReader>,
    /// run 记录内存真相源（Task 4；原 T12 runs_repo 的运行时读路径全部由它承接）。
    registry: RunRegistry,
    /// 事件出口：RunRecorded 持久化投影由订阅者落库（Task 8）。
    events: Arc<AgentEventBus>,
    agent_provider: Arc<dyn AutonomousAgentProvider>,
    runner: Arc<Runner>,
    /// 工单人工解法注入源（T7；NoopTicketResolutions = 未接线）。
    ticket_resolutions: Arc<dyn crate::runtime::thing_agent::traits::TicketResolutionProvider>,
    /// 进行中 run 的内存注册表（T3/T4）：run_pipeline 每个 run 先注册再发
    /// RunStarted，实时进度轮询的数据源。RAII guard 随 run 结束注销。
    active_runs: ActiveRunRegistry,
    /// heartbeat 源 prompt 任务注入源（Task 6；与 heartbeat_tasks_map 同一张表）。
    heartbeat_tasks: Arc<dyn HeartbeatTaskReader>,
    /// heartbeat 源 prompt 信任段注入源（Task 6）。
    trust_configs: Arc<dyn TrustConfigReader>,
    /// heartbeat trigger 的任务读表（空任务/全暂停不 tick，loop_.rs:63）。
    heartbeat_tasks_map: Arc<DashMap<String, Vec<HeartbeatTask>>>,
    /// heartbeat trigger 的每工作区间隔（分钟）覆盖表。
    heartbeat_intervals: Arc<DashMap<String, u32>>,
    /// heartbeat trigger 的每工作区开关表（终审 I-1）：false 不 tick，
    /// 缺省 true。
    heartbeat_enabled: Arc<DashMap<String, bool>>,
    /// E2 快照读面：每工作区 watch 命令通道（Snapshot 查询在 engine actor 内）。
    watch_cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>>,
    /// heartbeat 连败治理（T4）：run_pipeline 记账，trigger 读暂停态。
    governor: HeartbeatGovernor,
    /// heartbeat 源最近 tick 完成时间表（Task 12，D13 实时读透出）——成败均计，
    /// 与 governor 记账同一落点（record_heartbeat_outcome 两条路径共用）。
    last_ticks: Arc<DashMap<String, chrono::DateTime<chrono::Utc>>>,
}

/// One running workspace loop: the scheduler handle plus the trigger/forward
/// tasks feeding it. Dropping the handle after drain lets the scheduler's
/// consumer task exit on the closed channel.
struct WorkspaceLoop {
    handle: SchedulerHandle,
    tasks: Vec<JoinHandle<()>>,
}

/// Per-workspace thing-agent loop registry. Also the T14 [`DirectiveSink`]
/// routing point: user directives are forwarded to the target workspace's
/// scheduler.
pub struct ThingAgentManager {
    deps: PipelineDeps,
    config: ThingAgentManagerConfig,
    workspaces: DashMap<String, WorkspaceLoop>,
    /// T13：handler 写穿与 bootstrap 注入用的 heartbeat 共享表句柄——与
    /// PipelineDeps 内读侧共享同一 DashMap 真源。
    heartbeat: HeartbeatDeps,
    /// T13：每工作区心跳热更新信号。独立实例而非单一全局 Notify——
    /// `Notify::notify_one` 只唤醒一个等待者，多工作区共享会叫错/叫漏。
    /// get-or-insert：handler 先于 start 调用也安全；stop 时丢弃条目，
    /// 旧 trigger 持有的 Notify 不再被唤醒（避免 stale permit 补 tick）。
    heartbeat_reload_notifies: DashMap<String, Arc<tokio::sync::Notify>>,
    /// E1 同键限速共享表（D3/D4，照 heartbeat intervals 先例）：键
    /// workspace_id，值 N/分钟；start 时注入 scheduler，handler 写穿热改生效。
    rate_limits: Arc<DashMap<String, u32>>,
    /// E1 限速事件账本（spec §3）：None 时 scheduler 只留 metric（测试/
    /// 边缘形态）；bootstrap 接线后 start 的工作区落账 kind=rate_limited。
    wake_ledger: std::sync::Mutex<Option<Arc<dyn tinyiothub_runtime::ports::HubLedger>>>,
    /// E1 watch 值守构建依赖（Task 8）：bootstrap 注入（`set_watch_deps`，
    /// 与 set_wake_ledger 同序约束——只影响其后 start 的工作区）；None =
    /// watch 未接线（测试/边缘形态），loop 不含 engine/trigger 任务。
    watch_deps: std::sync::Mutex<Option<WatchDeps>>,
    /// E1 watch API 写穿命令通道（每工作区）：start 时建立，stop 时丢弃；
    /// handler 经 `watch_command_sink` 取 sender 投递 Upsert/Remove。
    watch_cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>>,
}

impl ThingAgentManager {
    // 9 参（超 workspace 阈值 8）：Task 6 起 heartbeat 依赖以 HeartbeatDeps
    // 聚合一束进场，余下均为不可再合的核心管线依赖（db crate 同款 allow 先例）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        host: Arc<dyn ThingAgentHost>,
        policy_repo: Arc<dyn AutonomyPolicyReader>,
        agent_provider: Arc<dyn AutonomousAgentProvider>,
        registry: RunRegistry,
        events: Arc<AgentEventBus>,
        runner: Arc<Runner>,
        config: ThingAgentManagerConfig,
        ticket_resolutions: Arc<dyn crate::runtime::thing_agent::traits::TicketResolutionProvider>,
        heartbeat: HeartbeatDeps,
    ) -> Self {
        let watch_cmds: Arc<DashMap<String, mpsc::Sender<WatchCommand>>> = Arc::new(DashMap::new());
        Self {
            deps: PipelineDeps {
                watch_cmds: Arc::clone(&watch_cmds),
                host,
                policy_repo,
                registry,
                events,
                agent_provider,
                runner,
                ticket_resolutions,
                active_runs: ActiveRunRegistry::with_default_capacity(),
                heartbeat_tasks: heartbeat.tasks.clone(),
                trust_configs: heartbeat.trust_configs.clone(),
                heartbeat_tasks_map: heartbeat.tasks.clone(),
                heartbeat_intervals: heartbeat.intervals.clone(),
                heartbeat_enabled: heartbeat.enabled.clone(),
                governor: heartbeat.governor.clone(),
                last_ticks: heartbeat.last_ticks.clone(),
            },
            config,
            workspaces: DashMap::new(),
            heartbeat,
            heartbeat_reload_notifies: DashMap::new(),
            rate_limits: Arc::new(DashMap::new()),
            wake_ledger: std::sync::Mutex::new(None),
            watch_deps: std::sync::Mutex::new(None),
            watch_cmds,
        }
    }

    /// Start the loop for a workspace: scheduler + thing-event trigger +
    /// timer trigger + forward task. Idempotent — a running workspace is
    /// left untouched (Orchestrator duplicate-start precedent).
    pub fn start(&self, workspace_id: &str) {
        use dashmap::mapref::entry::Entry;
        let Entry::Vacant(vacant) = self.workspaces.entry(workspace_id.to_string()) else {
            tracing::debug!(workspace_id, "thing-agent loop already running — start ignored");
            return;
        };

        let ws = workspace_id.to_string();
        let deps = self.deps.clone();
        let handle = Scheduler::spawn(
            ws.clone(),
            move |signal| {
                let deps = deps.clone();
                Box::pin(async move { run_pipeline(deps, signal).await })
            },
            Arc::clone(&self.rate_limits),
        );
        // E1 限速落账（spec §3）：bootstrap 已接线账本则注入，None 只留 metric。
        if let Some(ledger) = self.wake_ledger.lock().expect("wake_ledger poisoned").clone() {
            handle.set_ledger(ledger);
        }

        let (tx, rx) = mpsc::channel::<WakeSignal>(TRIGGER_CHANNEL_CAPACITY);
        let forward = tokio::spawn(forward_signals(rx, handle.clone()));

        let event_trigger = ThingEventTrigger::new(
            Arc::clone(&self.deps.host),
            Arc::clone(&self.deps.policy_repo),
            ws.clone(),
            self.config.min_wake_level,
        );
        let event_tx = tx.clone();
        let event_ws = ws.clone();
        let event_task = tokio::spawn(async move {
            if let Err(e) = event_trigger.run(event_tx).await {
                tracing::warn!(workspace_id = %event_ws, error = %e, "thing event trigger exited with error");
            }
        });

        let timer = TimerTrigger {
            workspace_id: ws.clone(),
            interval: self.config.timer_interval,
            policy_repo: Arc::clone(&self.deps.policy_repo),
        };
        let timer_tx = tx.clone();
        let timer_ws = ws.clone();
        let timer_task = tokio::spawn(async move {
            if let Err(e) = timer.run(timer_tx).await {
                tracing::warn!(workspace_id = %timer_ws, error = %e, "timer trigger exited with error");
            }
        });

        // Task 6：心跳触发器挂载（T5）。空任务/全暂停不 tick；governor 暂停
        // 抑制；reload_notify 到达立即 tick（行为保持清单第 1/3 行）。
        // T13：reload_notify 为每工作区独立实例（见 heartbeat_reload_notify）。
        let heartbeat = HeartbeatTrigger {
            workspace_id: workspace_id.to_string(),
            default_interval: self.config.heartbeat_default_interval,
            intervals: Arc::clone(&self.deps.heartbeat_intervals),
            enabled: Arc::clone(&self.deps.heartbeat_enabled),
            tasks_source: Arc::clone(&self.deps.heartbeat_tasks_map),
            policy_repo: Arc::clone(&self.deps.policy_repo),
            governor: self.deps.governor.clone(),
            reload_notify: self.heartbeat_reload_notify(workspace_id),
        };
        let heartbeat_ws = workspace_id.to_string();
        let heartbeat_tx = tx.clone();
        let heartbeat_task = tokio::spawn(async move {
            if let Err(e) = heartbeat.run(heartbeat_tx).await {
                tracing::warn!(workspace_id = %heartbeat_ws, error = %e, "heartbeat trigger exited with error");
            }
        });

        let mut tasks = vec![forward, event_task, timer_task, heartbeat_task];

        // E1 watch 值守（Task 8）：deps 已接线则构建本区 engine 事件循环
        // （启动重织 + EventBus 订阅 + 超时清扫）并注册 WatchTrigger 进
        // trigger 集合（照 heartbeat trigger 模式——命中即 WakeSignal，
        // 无 policy 抑制，spec §1：注册即显式订阅）。
        if let Some(watch) = self.watch_deps.lock().expect("watch_deps poisoned").clone() {
            let (hit_tx, hit_rx) = mpsc::channel(TRIGGER_CHANNEL_CAPACITY);
            let (cmd_tx, cmd_rx) = mpsc::channel(TRIGGER_CHANNEL_CAPACITY);
            self.watch_cmds.insert(ws.clone(), cmd_tx);
            let engine_ws = ws.clone();
            let engine_task = tokio::spawn(async move {
                watch_engine_loop(engine_ws, watch, hit_tx, cmd_rx).await;
            });
            let watch_trigger = WatchTrigger {
                workspace_id: ws.clone(),
                hit_rx: Arc::new(tokio::sync::Mutex::new(hit_rx)),
            };
            let watch_tx = tx.clone();
            let watch_ws = ws.clone();
            let watch_task = tokio::spawn(async move {
                if let Err(e) = watch_trigger.run(watch_tx).await {
                    tracing::warn!(workspace_id = %watch_ws, error = %e, "watch trigger exited with error");
                }
            });
            tasks.push(engine_task);
            tasks.push(watch_task);
        }

        vacant.insert(WorkspaceLoop { handle, tasks });
        tracing::info!(workspace_id, "thing-agent loop started");
    }

    /// Stop the loop: remove from the registry, abort the trigger/forward
    /// tasks (no new signals), then drain queued signals (O26). An in-flight
    /// run is not cancelled; `drain()` returns once it finishes. The cached
    /// per-workspace agent is invalidated last, so a deleted workspace leaves
    /// no factory entry behind.
    pub async fn stop(&self, workspace_id: &str) {
        let Some((_, lp)) = self.workspaces.remove(workspace_id) else {
            return;
        };
        for task in &lp.tasks {
            task.abort();
        }
        lp.handle.drain().await;
        drop(lp.handle);
        // T13：notify 条目随 loop 退役——重启后重建，handler 不会唤醒旧
        // trigger 持有的 Notify（notify_one 无等待者时存的 stale permit
        // 会在新 trigger 首轮等待时触发一次意外立即 tick）。
        self.heartbeat_reload_notifies.remove(workspace_id);
        // 心跳共享表随工作区清(RT-6):旧 HeartbeatRunner.remove_workspace 清
        // last_ticks;不清则工作区churn 下表无限增长,且 id 复用时残留
        // enabled=false/旧间隔/旧暂停态复活。governor 以 record_success 清态。
        self.deps.heartbeat_tasks_map.remove(workspace_id);
        self.deps.heartbeat_intervals.remove(workspace_id);
        self.deps.heartbeat_enabled.remove(workspace_id);
        self.deps.last_ticks.remove(workspace_id);
        self.deps.governor.record_success(workspace_id);
        self.deps.agent_provider.invalidate(workspace_id);
        // E1 watch：命令通道条目随 loop 退役——engine 任务已被 abort，
        // 残留 sender 只会让 handler 写穿投进死通道。
        self.watch_cmds.remove(workspace_id);
        // M-4：同键限速表随 loop 退役（RT-6 同款）——不清则 workspace id
        // 复用时旧限速值复活。
        self.rate_limits.remove(workspace_id);
        tracing::info!(workspace_id, "thing-agent loop stopped");
    }

    pub fn is_running(&self, workspace_id: &str) -> bool {
        self.workspaces.contains_key(workspace_id)
    }

    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    /// 进行中 run 注册表（T3）的共享句柄——克隆零成本（内部 Arc 共享同一
    /// DashMap）。路由层（trace 端点实时分支）经此读进度；注册/注销仍由
    /// run_pipeline 的 RAII guard 管理。
    pub fn active_run_registry(&self) -> ActiveRunRegistry {
        self.deps.active_runs.clone()
    }

    // ── T13 heartbeat 依赖接管：bootstrap 注入 + handler 写穿 ────────────

    /// 每工作区心跳热更新信号（T13，行为保持清单第 3 行接线点）：
    /// update_tasks/update_config handler 写穿 DB+内存表后
    /// `manager.heartbeat_reload_notify(ws).notify_one()`，目标工作区的
    /// heartbeat trigger 立即醒来 tick。get-or-insert：handler 先于 start
    /// 调用也安全（trigger start 时取同一实例）。
    pub fn heartbeat_reload_notify(&self, workspace_id: &str) -> Arc<tokio::sync::Notify> {
        self.heartbeat_reload_notifies
            .entry(workspace_id.to_string())
            .or_default()
            .clone()
    }

    /// heartbeat 最近 tick 完成时间共享表（T12 透出）——T13 bootstrap 注入
    /// `AppState.heartbeat_last_ticks` 的句柄来源（克隆共享同一 DashMap）。
    pub fn heartbeat_last_ticks(&self) -> Arc<DashMap<String, chrono::DateTime<chrono::Utc>>> {
        Arc::clone(&self.heartbeat.last_ticks)
    }

    /// handler 写穿（D11-⑤：调用方已写 DB 成功）：任务表全量替换，trigger
    /// 与 prompt 装配读侧同一张表。
    pub fn set_heartbeat_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        self.heartbeat.tasks.insert(workspace_id.to_string(), tasks);
    }

    /// handler 写穿：每工作区间隔（分钟）覆盖——trigger 每轮重读生效。
    pub fn set_heartbeat_interval_minutes(&self, workspace_id: &str, interval_minutes: u32) {
        self.heartbeat
            .intervals
            .insert(workspace_id.to_string(), interval_minutes);
    }

    /// handler 写穿（终审 I-1）：心跳开关——trigger 判定集新增维度，false
    /// 抑制定时 tick 与 notify 立即 tick（抑制在判定集，handler 侧 notify
    /// 语义不变）。缺省 true（对齐 HeartbeatConfig::default enabled:true）。
    pub fn set_heartbeat_enabled(&self, workspace_id: &str, enabled: bool) {
        self.heartbeat.enabled.insert(workspace_id.to_string(), enabled);
    }

    /// handler 写穿：信任配置（prompt 信任段注入源，下一 run 生效）。
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

    // ── E1 同键限速（D3/D4）：共享表 + 账本接线 ─────────────────────────

    /// 同键限速共享表句柄（E1 Task 8 bootstrap 注入/handler 写穿用——克隆
    /// 共享同一 DashMap 真源，照 heartbeat_last_ticks 先例）。
    pub fn wake_rate_limits(&self) -> Arc<DashMap<String, u32>> {
        Arc::clone(&self.rate_limits)
    }

    /// handler 写穿（D11-⑤ 同款：调用方已写 DB 成功）：每工作区同键限速
    /// （N/分钟）——scheduler enqueue 每信号重读生效，热改无需重启 loop。
    pub fn set_wake_rate_limit_per_min(&self, workspace_id: &str, per_min: u32) {
        self.rate_limits.insert(workspace_id.to_string(), per_min);
    }

    /// 读出口（启动接线断言/调试）：每工作区同键限速覆盖值。
    pub fn wake_rate_limit_per_min(&self, workspace_id: &str) -> Option<u32> {
        self.rate_limits.get(workspace_id).map(|r| *r)
    }

    /// E1 限速事件账本接线（spec §3）：bootstrap 注入 HubLedger 适配器；
    /// 之后 start 的工作区 scheduler 落账 kind=rate_limited。None（默认）
    /// 只留 metric（测试/边缘形态）。
    pub fn set_wake_ledger(&self, ledger: Arc<dyn tinyiothub_runtime::ports::HubLedger>) {
        *self.wake_ledger.lock().expect("wake_ledger poisoned") = Some(ledger);
    }

    // ── E1 watch 值守（Task 8）：构建依赖注入 + API 写穿命令通道 ──────────

    /// watch 构建依赖接线（bootstrap，与 set_wake_ledger 同序约束——必须在
    /// start 之前，只影响其后 start 的工作区）：注入后 start 的 loop 含
    /// engine 事件循环 + WatchTrigger 两个任务。
    pub fn set_watch_deps(&self, deps: WatchDeps) {
        *self.watch_deps.lock().expect("watch_deps poisoned") = Some(deps);
    }

    /// watch API 写穿出口（D11-⑤：handler 已写 DB 成功）：目标工作区的
    /// engine 命令通道。None = loop 未运行或 watch 未接线——跳过即可，
    /// 下次 start 经启动重织自愈。
    pub fn watch_command_sink(&self, workspace_id: &str) -> Option<mpsc::Sender<WatchCommand>> {
        self.watch_cmds.get(workspace_id).map(|r| r.clone())
    }
}

#[async_trait::async_trait]
impl DirectiveSink for ThingAgentManager {
    /// Route a directive to the target workspace's scheduler. Unknown
    /// workspace (loop not started / already stopped) maps to
    /// [`EnqueueError::Closed`].
    fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
        let Some(entry) = self.workspaces.get(&signal.workspace_id) else {
            return Err(EnqueueError::Closed);
        };
        entry.handle.enqueue(signal)
    }

    /// O26 kill switch: forward the drain to the target workspace's
    /// scheduler; unknown workspace is a no-op. The handle is cloned out
    /// before `.await` — never hold a DashMap guard across it.
    async fn drain(&self, workspace_id: &str) {
        let handle = self.workspaces.get(workspace_id).map(|entry| entry.handle.clone());
        if let Some(handle) = handle {
            handle.drain().await;
        }
    }
}

/// Forward trigger output into the scheduler. Scheduler-side errors already
/// log their own metrics (throttled/dropped/rejected); `Closed` means the
/// loop is gone — exit.
async fn forward_signals(mut rx: mpsc::Receiver<WakeSignal>, handle: SchedulerHandle) {
    while let Some(signal) = rx.recv().await {
        if handle.enqueue(signal) == Err(EnqueueError::Closed) {
            break;
        }
    }
}

/// Human-readable trigger label persisted as `agent_runs.trigger_context`;
/// the `xxx:` prefix doubles as `trigger_type` (T12 `trigger_type_of`).
fn trigger_label(signal: &WakeSignal) -> String {
    match &signal.source {
        TriggerSource::ThingEvent {
            thing_id, event_name, ..
        } => format!("thing:{thing_id}:event:{event_name}"),
        TriggerSource::Timer => format!("timer:{}", signal.workspace_id),
        TriggerSource::Heartbeat { .. } => signal
            .dedup_key
            .clone()
            .unwrap_or_else(|| format!("heartbeat:{}", signal.workspace_id)),
        TriggerSource::UserDirective { user_id, .. } => format!("user:{user_id}"),
        TriggerSource::Watch { rule_id, .. } => signal.dedup_key.clone().unwrap_or_else(|| format!("watch:{rule_id}")),
        TriggerSource::Merged { .. } => {
            format!("merged:{}", signal.dedup_key.as_deref().unwrap_or("signals"))
        }
    }
}

/// RunStarted 事件的 problem_keys 提取（T4）：递归 `Merged.signals`，收集
/// `UserDirective.problem_key` 的 Some 值去重（保持首次出现序）；
/// ThingEvent/Timer 无 problem_key。
fn collect_problem_keys(source: &TriggerSource) -> Vec<String> {
    let mut out = Vec::new();
    collect_problem_keys_into(source, &mut out);
    out
}

fn collect_problem_keys_into(source: &TriggerSource, out: &mut Vec<String>) {
    match source {
        TriggerSource::UserDirective {
            problem_key: Some(pk), ..
        } if !out.contains(pk) => out.push(pk.clone()),
        TriggerSource::Merged { signals } => {
            for s in signals {
                collect_problem_keys_into(&s.source, out);
            }
        }
        // 心跳无 problem_key;X6 桥指令走 UserDirective 不变。
        TriggerSource::Heartbeat { .. } => {}
        _ => {}
    }
}

/// heartbeat 成员判定（T6 修复轮 1）：Merged 变体保留（存储/回放兼容，
/// E1 Task 7 起 scheduler 不再产生）——Merged 含 Heartbeat 成员则必然全为
/// heartbeat 成员（合并窗时代只按同 dedup_key 合并，混合不可能遗留）。
/// 防御性递归（合并信号不嵌套 Merged，散兵不丢信号）。
fn has_heartbeat_member(source: &TriggerSource) -> bool {
    match source {
        TriggerSource::Heartbeat { .. } => true,
        TriggerSource::Merged { signals } => signals.iter().any(|s| has_heartbeat_member(&s.source)),
        _ => false,
    }
}

/// 收集信号（含 Merged 成员）里的心跳告警上下文——告警驱动 run 的 prompt
/// 附段数据源，多个告警各一段。
fn heartbeat_alarms(source: &TriggerSource) -> Vec<&HeartbeatAlarmContext> {
    let mut out = Vec::new();
    collect_heartbeat_alarms(source, &mut out);
    out
}

fn collect_heartbeat_alarms<'a>(source: &'a TriggerSource, out: &mut Vec<&'a HeartbeatAlarmContext>) {
    match source {
        TriggerSource::Heartbeat { alarm: Some(a) } => out.push(a),
        TriggerSource::Merged { signals } => {
            for s in signals {
                collect_heartbeat_alarms(&s.source, out);
            }
        }
        _ => {}
    }
}

/// Task 6 心跳连败治理（行为保持清单第 1 行）：heartbeat 源 run 结果记账到
/// governor——连败达阈由 trigger 侧暂停定时 tick，告警入口（alarm=Some）
/// 同样计入，与旧 loop 语义一致。修复轮 1：① Merged 穿透（has_heartbeat_member）；
/// ② agent-unavailable 早退路径同样经此记账（旧 loop 的 send_message 错误
/// 路径含 get_or_create 失败——agent 不可用计连败）。
/// Task 12（D13 实时读透出）：tick 完成（成败均计）同时写共享 last_ticks 表
/// ——对齐旧 loop_.rs:74-75；本函数由正常完成与 agent-unavailable 早退两条
/// 路径共用，写一次即两路覆盖。
fn record_heartbeat_outcome(
    deps: &PipelineDeps,
    source: &TriggerSource,
    workspace_id: &str,
    outcome: tinyiothub_core::agent_runs::Outcome,
) {
    if !has_heartbeat_member(source) {
        return;
    }
    // D13 实时读：tick 完成（成败均计）即记录内存态 last_tick。
    deps.last_ticks.insert(workspace_id.to_string(), chrono::Utc::now());
    match outcome {
        tinyiothub_core::agent_runs::Outcome::Failed => deps.governor.record_failure(workspace_id),
        _ => deps.governor.record_success(workspace_id),
    }
}

/// E2：向 engine actor 要本工作区实时快照（120ms 超时，fail-soft）。
async fn fetch_workspace_snapshot(
    cmds: &Arc<DashMap<String, mpsc::Sender<WatchCommand>>>,
    workspace_id: &str,
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
            let (body, metrics) = crate::prompt::snapshot::render_snapshot(&snapshot);
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

/// The full run chain (T9–T13) driven by the scheduler for every admitted
/// wake signal.
async fn run_pipeline(deps: PipelineDeps, signal: WakeSignal) {
    let ws = signal.workspace_id.clone();
    let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
    let ctx = RunContext {
        active_runs: Some(deps.active_runs.clone()),
        ..RunContext::new(run_id.clone(), ws.clone(), trigger_label(&signal))
    };

    // T4 时序约定：**先注册 active registry 再发 RunStarted**——订阅者
    // （judgment_subscriber）回填 judgment.run_id 后前端轮询必命中注册表，
    // 消除"回填快于注册→轮询 404"窗。guard 生命周期横跨整个 execute，
    // run 结束（含 agent 不可用的早退）自动注销。
    let _guard = ctx.active_runs.as_ref().and_then(|r| r.register(ctx.clone()));
    deps.events.emit(AgentEventKind::RunStarted {
        run_id: run_id.clone(),
        workspace_id: ws.clone(),
        problem_keys: collect_problem_keys(&signal.source),
    });

    // T10 injection sources: 内存 registry（Task 4 替代 T12 repo 读取）。
    // 内存读无 I/O 失败路径，不再有 fail-soft 降级分支。
    let memory: Vec<String> = deps
        .registry
        .recent(&ws, 5)
        .iter()
        .map(|r| format_summary(r.outcome.as_str(), &r.summary))
        .collect();
    let history: Vec<String> = match &signal.dedup_key {
        Some(key) => deps
            .registry
            .recent_by_dedup(&ws, key, 3)
            .iter()
            .map(|r| format_summary(r.outcome.as_str(), &r.summary))
            .collect(),
        None => vec![],
    };

    // Boundary segment: the action names the policy gate would allow. Only
    // Act mode has usable actions (Diagnose/Off deny at the gate, T4).
    let allowed = match deps.policy_repo.load_autonomy(&ws).await {
        Ok(Some(policy)) if policy.mode == AutonomyMode::Act => policy.allowed_actions,
        Ok(_) => vec![],
        Err(e) => {
            tracing::warn!(workspace_id = %ws, error = %e, "policy read failed — prompt with empty action list");
            vec![]
        }
    };

    // T7 工单人工解法注入（fail-soft：读失败则空段，不阻断 run）。
    // 同 thing 优先（ThingEvent / 告警心跳信号带 thing_id；其余源按 workspace 取最近）。
    let signal_thing_id = match &signal.source {
        TriggerSource::ThingEvent { thing_id, .. } => Some(thing_id.as_str()),
        TriggerSource::Heartbeat { alarm: Some(alarm) } => alarm.thing_id.as_deref(),
        _ => None,
    };
    let resolutions = match deps
        .ticket_resolutions
        .recent_resolutions(&ws, signal_thing_id, 5)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(workspace_id = %ws, error = %e, "ticket resolutions read failed — prompt without them");
            vec![]
        }
    };

    // E2：turn 开头工作区快照（fail-soft：超时/无 engine → 空段，run 照常）。
    let (snapshot_body, snapshot_metrics) = fetch_workspace_snapshot(&deps.watch_cmds, &ws).await;
    if let Some(metrics) = snapshot_metrics {
        ctx.inner.write().await.snapshot = Some(metrics);
    }

    let principles = crate::prompt::workspace::load_principles(&ws);
    // Task 6：heartbeat 源走 heartbeat 专用 prompt（任务列表+信任段+JSON
    // 报告模板，行为保持 loop_.rs build_heartbeat_prompt）。修复轮 1：Merged
    // 穿透（has_heartbeat_member——scheduler 只按同 dedup_key 合并，含
    // Heartbeat 成员的 Merged 必然全为 heartbeat 成员），成员告警上下文逐个
    // 渲染（渲染复用 prompt::runtime 的既有 arm，T3 落点，不重复造）。
    // 其余源走四段式 build_prompt 不变。
    let prompt = if has_heartbeat_member(&signal.source) {
        let tasks = deps.heartbeat_tasks.active_tasks(&ws);
        let trust = deps.trust_configs.get(&ws);
        let task_refs: Vec<&HeartbeatTask> = tasks.iter().collect();
        let mut p = crate::prompt::heartbeat::build_heartbeat_prompt(&ws, &principles, &task_refs, &trust);
        for a in heartbeat_alarms(&signal.source) {
            // 告警驱动 run：附上触发上下文（审计链 trigger_label 已含告警身份）。
            p.push_str("\n\n");
            crate::prompt::runtime::render_heartbeat_alarm(a, &mut p);
        }
        if let Some(body) = snapshot_body.as_deref() {
            p.push_str(&crate::prompt::snapshot::section(body));
        }
        p
    } else {
        build_prompt_with_snapshot(
            &signal,
            &principles,
            &memory,
            &history,
            &allowed,
            &resolutions,
            snapshot_body.as_deref(),
        )
    };

    let agent = match deps.agent_provider.get_or_create(&ws, Arc::clone(&ctx.inner)).await {
        Ok(agent) => agent,
        Err(e) => {
            tracing::error!(workspace_id = %ws, run_id = %run_id, error = %e, "autonomous agent unavailable — run aborted");
            // 工单 T2：Agent/LLM 供应商不可用是最需要人工介入的自治失败，
            // 必须经 RunRecorded 走到工单订阅者——否则订阅者永远看不到这类
            // 失败（"零静默失败"承诺在此断裂）。与正常路径同构：record + emit。
            let thing_id = match &signal.source {
                TriggerSource::ThingEvent { thing_id, .. } => Some(thing_id.clone()),
                // T3 评审 Minor 3 结转（T6 裁决 1）：告警心跳与 ThingEvent 一致取 thing_id。
                TriggerSource::Heartbeat { alarm: Some(alarm) } => alarm.thing_id.clone(),
                _ => None,
            };
            let problem_key = match &signal.source {
                TriggerSource::UserDirective { problem_key, .. } => problem_key.as_deref(),
                _ => None,
            };
            let report = tinyiothub_core::agent_runs::RunReport {
                run_id: run_id.clone(),
                workspace_id: ws.clone(),
                trigger: trigger_label(&signal),
                outcome: tinyiothub_core::agent_runs::Outcome::Failed,
                summary: format!("自治 Agent 不可用（{e}），run 未执行即中止"),
                actions: vec![],
                verified: false,
                duration_ms: 0,
                tool_calls: 0,
                tokens: 0,
                end_reason: Some(tinyiothub_core::agent_runs::EndReason::AgentUnavailable),
                thing_id,
                trace: vec![],
                // 供应商不可用 run 未执行即中止，无任务表消费——None（适配器映射 0，
                // 与旧 error 锚行 taskCount=0 同显）。
                task_count: None,
                snapshot: None,
            };
            deps.registry.record_with_keys(
                report.clone(),
                crate::runtime::snapshot::RunDedupKeys {
                    problem_key: problem_key.map(str::to_owned),
                    dedup_key: signal.dedup_key.clone(),
                },
            );
            // 修复轮 1：agent 不可用早退对 heartbeat 源同样计连败（旧 loop 语义：
            // get_or_create 失败在 send_message 错误路径内，计连败）——否则 provider
            // 永久不可用时永远到不了暂停阈值。
            record_heartbeat_outcome(&deps, &signal.source, &ws, tinyiothub_core::agent_runs::Outcome::Failed);
            deps.events.emit(AgentEventKind::RunRecorded {
                report: Box::new(report),
                problem_key: problem_key.map(str::to_owned),
                dedup_key: signal.dedup_key.clone(),
            });
            return;
        }
    };

    let outcome = deps.runner.execute(agent, prompt, ctx).await;
    let mut report = outcome.report;
    // 读侧 parity（Task 10 修复轮 1）：heartbeat 源 run 把任务表大小写进
    // report.task_count——get_logs ExecutionRecord.taskCount 的数据源，对齐旧
    // loop_.rs `result.task_count = tasks.len()` 语义（prompt 同一张 active 表）。
    if has_heartbeat_member(&signal.source) {
        report.task_count = Some(deps.heartbeat_tasks.active_tasks(&ws).len() as u32);
    }

    // 内存记录（真相源）+ 事件出口（持久化投影）。落库失败不阻断回推语义
    // 不变（T12）：持久化已移出调用路径，由 Task 8 的 RunRecorded 订阅者落库；
    // record/emit 均不可失败，回推（T13）永远能读到含当前 run 的窗口。
    // X6：原心跳桥投递的指令携带 problem_key（O11 dedup 判定依据）；其余触发
    // 源为 None。X6 自动派发已退役（2026-10-16 裁定：提案走审批队列）——
    // 生产路径不再有 source=Some("heartbeat") 的 UserDirective，problem_key
    // 恒为 None；字段与写入路径保留待 O11 machinery 整体清理。
    let problem_key = match &signal.source {
        TriggerSource::UserDirective { problem_key, .. } => problem_key.as_deref(),
        _ => None,
    };
    // CEO review T3 + 对抗性 F7：record 与发射期 dedup 键一步写入——
    // 键与窗口条目同生，无两步形态的 TOCTOU 孤儿键窗口。
    deps.registry.record_with_keys(
        report.clone(),
        crate::runtime::snapshot::RunDedupKeys {
            problem_key: problem_key.map(str::to_owned),
            dedup_key: signal.dedup_key.clone(),
        },
    );
    // O11 dedup 元数据（Task 6）：problem run 结果写入内存 dedup 真源。
    // X6 退役（2026-10-16）后写-读失衡：读侧（HeartbeatBridge::should_dispatch）
    // 已删除，ack 回写（mark_problem_acked）仍在；machinery 保留待后续清理。
    if let Some(pk) = problem_key {
        deps.registry.record_problem_run(
            &report.workspace_id,
            pk,
            &report.run_id,
            report.outcome,
            report.verified,
            chrono::Utc::now(),
        );
    }
    deps.events.emit(AgentEventKind::RunRecorded {
        report: Box::new(report.clone()),
        problem_key: problem_key.map(str::to_owned),
        dedup_key: signal.dedup_key.clone(),
    });
    // Task 6 心跳连败治理（行为保持清单第 1 行）：heartbeat 源 run 结果记账
    // 到 governor——连败达阈由 trigger 侧暂停定时 tick；告警入口不受影响。
    // 修复轮 1：记账与早退路径共用 record_heartbeat_outcome（Merged 穿透）。
    record_heartbeat_outcome(&deps, &signal.source, &ws, report.outcome);
    deliver(&report, &signal, &deps.registry, deps.host.as_ref()).await;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::runtime::thing_agent::types::{HeartbeatAlarmContext, Outcome, Priority};
    use std::sync::Mutex;

    use crate::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use crate::port::provider::{ChatRequest, ChatResponse, ModelProvider};
    use crate::port::runtime::AgentLoopConfig;

    use crate::runtime::thing_agent::traits::ThingEventSignal;
    use tinyiothub_policy::autonomy::AutonomyPolicy;

    const WS: &str = "ws_01";

    // ── scripted LLM provider (no network) ─────────────────────

    /// Replies a fixed text, never calls tools; records every prompt.
    /// Clones share the prompt log (the agent holds one, the test another).
    /// `fail` 为故障注入开关（T6 governor 连败路径）：true 时 chat 报错。
    #[derive(Clone, Default)]
    pub(crate) struct ScriptedProvider {
        pub(crate) prompts: Arc<Mutex<Vec<String>>>,
        pub(crate) fail: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
                anyhow::bail!("scripted provider failure (test injection)");
            }
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

    /// Stub agent provider: one shared loop (port AgentLoop via the
    /// zeroclaw adapter) driven by the scripted provider (serial scheduler ⇒
    /// no concurrent turns).
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
                prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
                tools: vec![],
                memory: Arc::new(crate::port::memory::NoopMemory),
                observer: Arc::new(crate::port::observer::NoopObserver),
                workspace_dir: dir.path().to_path_buf(),
                security_summary: None,
                provider_factory: Arc::new(move || Ok(Box::new(llm_for_factory.as_ref().clone()))),
                // 自治语义的测试夹具：对齐真实 autonomous_factory 的 true。
                conversation_memory: true,
            };
            let handle = crate::adapters::rig::loop_::rig_loop_factory(cfg).expect("build stub loop");
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

    /// 永远失败的 agent provider（工单 T2 早退路径 + T6 修复轮 1 早退连败
    /// 记账测试共用）。
    pub(crate) struct FailingProvider;

    #[async_trait::async_trait]
    impl AutonomousAgentProvider for FailingProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            anyhow::bail!("llm provider down")
        }
        fn invalidate(&self, _workspace_id: &str) {}
    }

    // ── host / policy / runs stubs ─────────────────────────────

    pub(crate) struct StubHost {
        pub(crate) tx: tokio::sync::broadcast::Sender<ThingEventSignal>,
        pub(crate) pushes: Mutex<Vec<(String, String, String)>>,
        pub(crate) alerts: Mutex<Vec<(String, serde_json::Value)>>,
        pub(crate) admin_session: Option<String>,
    }

    impl StubHost {
        pub(crate) fn new() -> Self {
            let (tx, _) = tokio::sync::broadcast::channel(64);
            Self {
                tx,
                pushes: Mutex::new(vec![]),
                alerts: Mutex::new(vec![]),
                admin_session: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl ThingAgentHost for StubHost {
        fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
            self.tx.subscribe()
        }

        async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            Ok(vec![])
        }

        async fn push_chat_message(&self, session_key: &str, content: &str, run_id: &str) -> anyhow::Result<()> {
            self.pushes
                .lock()
                .unwrap()
                .push((session_key.to_string(), content.to_string(), run_id.to_string()));
            Ok(())
        }

        async fn notify_alert(&self, workspace_id: &str, payload: serde_json::Value) -> anyhow::Result<()> {
            self.alerts.lock().unwrap().push((workspace_id.to_string(), payload));
            Ok(())
        }

        async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
            Ok(self.admin_session.clone())
        }
    }

    /// 内存策略桩（Task 13 起替代真实 SQLite repo 夹具）：Act 模式，
    /// 配额沿用原 db 播种值（3/run、30/hour）。
    pub(crate) fn stub_policy_reader(ws: &str) -> Arc<dyn AutonomyPolicyReader> {
        let reader = Arc::new(crate::runtime::thing_agent::traits::test_stubs::StubAutonomyPolicyReader::new());
        reader.save(
            ws,
            AutonomyPolicy {
                mode: AutonomyMode::Act,
                allowed_actions: vec!["*".to_string()],
                denied_actions: vec![],
                max_actions_per_run: 3,
                max_actions_per_hour: 30,
            },
        );
        reader
    }

    #[derive(Debug, Clone)]
    pub(crate) struct RecordedRun {
        pub(crate) run_id: String,
        pub(crate) trigger: String,
        pub(crate) outcome: Outcome,
        /// 修复轮 1：task_count 装配断言的观测口（持久化投影同一事件流）。
        pub(crate) task_count: Option<u32>,
        pub(crate) problem_key: Option<String>,
        pub(crate) dedup_key: Option<String>,
    }

    /// Run 探针（Task 4 替代原 RunsProbe SQLite 直查，显式 wiring 不用
    /// Default）：dedup 计数走内存 registry（真相源）；problem_key/dedup_key
    /// 等持久化投影字段走 RunRecorded 事件流（Task 8 订阅者消费的同一出口）。
    /// try_recv 惰性排空，无后台任务，读取确定性。
    pub(crate) struct RunsProbe {
        registry: RunRegistry,
        rx: Mutex<tokio::sync::broadcast::Receiver<crate::runtime::events::AgentEvent>>,
        seen: Mutex<Vec<RecordedRun>>,
    }

    impl RunsProbe {
        pub(crate) fn runs_with_dedup(&self, key: &str) -> usize {
            self.registry.count_by_dedup(key)
        }

        pub(crate) fn all(&self) -> Vec<RecordedRun> {
            self.drain_events();
            self.seen.lock().unwrap().clone()
        }

        pub(crate) fn is_empty(&self) -> bool {
            self.all().is_empty()
        }

        pub(crate) fn len(&self) -> usize {
            self.all().len()
        }

        fn drain_events(&self) {
            let mut rx = self.rx.lock().unwrap();
            while let Ok(event) = rx.try_recv() {
                if let AgentEventKind::RunRecorded {
                    report,
                    problem_key,
                    dedup_key,
                } = event.kind
                {
                    self.seen.lock().unwrap().push(RecordedRun {
                        run_id: report.run_id.clone(),
                        trigger: report.trigger.clone(),
                        outcome: report.outcome,
                        task_count: report.task_count,
                        problem_key,
                        dedup_key,
                    });
                }
            }
        }
    }

    pub(crate) struct StubManagerParts {
        pub(crate) manager: Arc<ThingAgentManager>,
        pub(crate) host: Arc<StubHost>,
        pub(crate) runs: RunsProbe,
        pub(crate) agents: Arc<StubAgentProvider>,
        /// T6 heartbeat 依赖（空表默认）：测试可播种 tasks/trust 后再 start，
        /// governor 克隆读连败计数。
        pub(crate) heartbeat: HeartbeatDeps,
    }

    /// Manager with all-stub deps and a 24h timer interval. Task 4：run 记录
    /// 走内存 registry + 事件总线，探针订阅同一总线（先 subscribe 再 start，
    /// 不丢事件）。E1 Task 7（合并窗已废）：timer 首 tick 即醒即跑成一条
    /// `timer:{ws}` run——断言一律按 dedup_key 过滤，不吃总数。
    pub(crate) async fn stub_manager() -> StubManagerParts {
        let host = Arc::new(StubHost::new());
        let registry = RunRegistry::new();
        let events = Arc::new(AgentEventBus::new(64));
        let runs = RunsProbe {
            registry: registry.clone(),
            rx: Mutex::new(events.subscribe()),
            seen: Mutex::new(vec![]),
        };
        let agents = Arc::new(StubAgentProvider::new());
        let heartbeat = HeartbeatDeps::default();
        let manager = Arc::new(ThingAgentManager::new(
            host.clone(),
            stub_policy_reader(WS),
            agents.clone(),
            registry,
            events,
            Arc::new(Runner::new()),
            ThingAgentManagerConfig {
                timer_interval: Duration::from_secs(24 * 3600),
                min_wake_level: 3,
                heartbeat_default_interval: Duration::from_secs(24 * 3600),
            },
            Arc::new(crate::runtime::thing_agent::traits::NoopTicketResolutions),
            heartbeat.clone(),
        ));
        StubManagerParts {
            manager,
            host,
            runs,
            agents,
            heartbeat,
        }
    }

    // ── helpers ────────────────────────────────────────────────

    fn event(event_id: i64, level: i32, actor: &str) -> ThingEventSignal {
        ThingEventSignal {
            workspace_id: WS.to_string(),
            thing_id: "t1".to_string(),
            event_name: "temp_high".to_string(),
            event_id,
            level,
            data: serde_json::json!({"value": 42}),
            is_unknown: false,
            actor: actor.to_string(),
        }
    }

    const EVENT_KEY: &str = "thing:t1:event:temp_high";

    async fn wait_subscribed(host: &StubHost) {
        for _ in 0..10_000 {
            if host.tx.receiver_count() > 0 {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("thing-event trigger did not subscribe");
    }

    /// Yield until `cond` holds; panics after a bounded number of yields so a
    /// broken pipeline fails loudly instead of hanging (paused-time safe).
    async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        for _ in 0..20_000 {
            if cond() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("timed out waiting for {what}");
    }

    /// Yield pump so the trigger→forward→consumer chain picks up freshly
    /// sent events.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    // ── tests ──────────────────────────────────────────────────

    // Critical thing event → immediate wake (no merge window) → full pipeline:
    // run persisted with the event dedup key, pushback falls back to alert.
    #[tokio::test(start_paused = true)]
    async fn critical_event_runs_full_pipeline() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        assert!(parts.manager.is_running(WS));
        wait_subscribed(&parts.host).await;

        parts.host.tx.send(event(1, 5, "device")).expect("send critical event");

        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "run persisted").await;

        let runs = parts.runs.all();
        let run = runs.iter().find(|r| r.dedup_key.as_deref() == Some(EVENT_KEY)).unwrap();
        assert_eq!(run.trigger, EVENT_KEY, "trigger label must be the event dedup key");
        // Scripted LLM replies plain text without tool calls → no actions.
        assert_eq!(run.outcome, Outcome::NoActionNeeded);
        drop(runs);

        // T13 pushback: no user session, no admin session → alert fallback.
        wait_until(|| !parts.host.alerts.lock().unwrap().is_empty(), "pushback alert").await;
        let alerts = parts.host.alerts.lock().unwrap();
        assert_eq!(alerts[0].1["reason"], "no_active_session");

        // The run really went through prompt assembly (T10).
        let prompts = parts.agents.llm.prompts.lock().unwrap();
        assert!(
            prompts
                .iter()
                .any(|p| p.contains("自治运维 Agent") && p.contains("temp_high")),
            "LLM must receive the assembled four-segment prompt"
        );
    }

    // 5 same-key warning events → 同键限速（E1 D3/D4，合并窗已废）：前 3 条
    // 即来即醒各自成 run，第 4/5 条被限速拒绝（默认 3/min）。
    #[tokio::test(start_paused = true)]
    async fn same_key_events_rate_limited_after_third() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        for id in 1..=5 {
            parts.host.tx.send(event(id, 3, "device")).expect("send event");
        }
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 3, "3 admitted runs").await;

        // 限速拒绝是 enqueue 的同步决策——3 条落库即管道排空到第 5 条
        // （trigger/forward 串行 FIFO），无需墙钟等待即可阴性断言。
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            3,
            "第 4/5 条同键事件必须被限速（agent_wake_rate_limited），不得成 run"
        );
    }

    // Resonance guard (O21): agent-produced events never wake the loop, even
    // at critical level.
    #[tokio::test(start_paused = true)]
    async fn agent_actor_event_does_not_wake() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts
            .host
            .tx
            .send(event(1, 5, "agent"))
            .expect("send agent-actor event");
        settle().await;
        // No wake for the agent-actor event (合并窗已废，任何唤醒都会立即
        // 落 run；timer 首 tick 的 run 落在另一条 dedup_key 上，按键过滤)。
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            0,
            "agent-actor event must not wake"
        );

        // A device-actor event at the same level DOES wake — the block above
        // was the actor filter, not a dead loop.
        parts.host.tx.send(event(2, 5, "device")).expect("send device event");
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "device event wakes").await;
    }

    // Duplicate start() must not spawn a second loop (two triggers would
    // double every wake).
    #[tokio::test(start_paused = true)]
    async fn start_is_idempotent() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        parts.manager.start(WS);
        assert_eq!(parts.manager.workspace_count(), 1);
        wait_subscribed(&parts.host).await;

        parts.host.tx.send(event(1, 5, "device")).expect("send event");
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "single run").await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            1,
            "duplicate start must not double runs"
        );
    }

    // ── T13 bootstrap 装配（T15 起唯一心跳路径）───────────────

    /// T13：start 挂载 4 个任务——forward + thing-event trigger + timer
    /// trigger + heartbeat trigger（统一循环接管心跳依赖的结构断言）。
    #[tokio::test(start_paused = true)]
    async fn start_mounts_heartbeat_trigger_alongside_timer_and_event() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        let lp = parts.manager.workspaces.get(WS).expect("loop running");
        assert_eq!(
            lp.tasks.len(),
            4,
            "forward + event + timer + heartbeat trigger 四个任务"
        );
    }

    /// T13：reload_notify 每工作区独立且稳定——handler 写穿后 notify_one
    /// 只唤醒目标工作区的 trigger（单全局 Notify 的 notify_one 只唤醒一个
    /// 等待者，多工作区下会叫错/叫漏）。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_reload_notify_is_per_workspace_and_stable() {
        let parts = stub_manager().await;
        let a1 = parts.manager.heartbeat_reload_notify("ws_a");
        let a2 = parts.manager.heartbeat_reload_notify("ws_a");
        let b = parts.manager.heartbeat_reload_notify("ws_b");
        assert!(Arc::ptr_eq(&a1, &a2), "同一工作区须为同一 Notify");
        assert!(!Arc::ptr_eq(&a1, &b), "不同工作区不得共享 Notify");
    }

    /// T13：stop 丢弃 notify 条目——重启后重建，旧 trigger 持有的 Notify
    /// 不再被 handler 唤醒（避免 stale permit 触发立即补 tick）。
    #[tokio::test(start_paused = true)]
    async fn stop_drops_reload_notify_entry() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        let before = parts.manager.heartbeat_reload_notify(WS);
        parts.manager.stop(WS).await;
        let after = parts.manager.heartbeat_reload_notify(WS);
        assert!(!Arc::ptr_eq(&before, &after), "stop 后须重建 Notify");
    }

    /// T13：handler 写穿访问器更新共享表（与 trigger/prompt 读侧同一
    /// DashMap 真源——经 stub 夹具保留的 HeartbeatDeps 句柄双向验证）。
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

    /// 终审 I-1：enabled 开关接入统一循环——写穿 false 后启动心跳 trigger
    /// 不 tick（含 notify 立即 tick），翻回 true 恢复；缺省（未写）= true
    /// （对齐 HeartbeatConfig::default enabled:true）。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_enabled_write_through_gates_unified_tick() {
        let parts = stub_manager().await;
        // 缺省 true（表无条目）。
        assert!(parts.manager.heartbeat_enabled(WS), "缺省须为 true");

        parts.heartbeat.tasks.insert(
            WS.to_string(),
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
        // update_config(enabled=false) 写穿（handler D11-⑤ DB 先写后的内存步）。
        parts.manager.set_heartbeat_enabled(WS, false);
        assert!(!parts.manager.heartbeat_enabled(WS));
        assert_eq!(
            parts.heartbeat.enabled.get(WS).map(|v| *v),
            Some(false),
            "写穿与 trigger 读侧共享同一 DashMap 真源"
        );
        parts.manager.start(WS);

        // 启动首 tick 被抑制（enabled=false）：不得落 run。
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup("heartbeat:ws_01"),
            0,
            "enabled=false 的启动 tick 不得落 run"
        );

        // notify 立即 tick 同样被抑制（update_config(false) 后 handler 仍 notify）。
        parts.manager.heartbeat_reload_notify(WS).notify_one();
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup("heartbeat:ws_01"),
            0,
            "enabled=false 时 notify 不得补 tick"
        );

        // 翻回 true：notify 立即 tick 恢复，run 落库。
        parts.manager.set_heartbeat_enabled(WS, true);
        parts.manager.heartbeat_reload_notify(WS).notify_one();
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 1,
            "enabled=true 后 notify 立即 tick 落 run",
        )
        .await;
    }

    // stop(): triggers aborted, queued signals drained, later events ignored.
    #[tokio::test(start_paused = true)]
    async fn stop_halts_loop() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        // 排空屏障（E1 Task 7：timer 首 tick 即醒即跑）——等其落定后 stop，
        // 之后的 before/after 计数不受在途 run 干扰。
        wait_until(
            || parts.runs.runs_with_dedup("timer:ws_01") == 1,
            "timer first-tick run",
        )
        .await;

        parts.manager.stop(WS).await;
        assert!(!parts.manager.is_running(WS));
        assert_eq!(parts.manager.workspace_count(), 0);

        let before = parts.runs.len();
        // Broadcast with zero receivers errors — the loop is gone, as intended.
        let _ = parts.host.tx.send(event(1, 5, "device"));
        settle().await;
        assert_eq!(parts.runs.len(), before, "stopped loop must not run");

        // Stop is idempotent.
        parts.manager.stop(WS).await;
    }

    // stop() 必须失效工厂里缓存的 per-workspace agent（WorkspaceDeleted 不
    // 泄漏缓存实例）。
    #[tokio::test(start_paused = true)]
    async fn stop_invalidates_cached_agent() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts.manager.stop(WS).await;
        assert_eq!(
            parts.agents.invalidated.lock().unwrap().as_slice(),
            &[WS.to_string()],
            "stop must invalidate the cached agent for the workspace"
        );

        // Unknown workspace stop → no invalidate call.
        parts.manager.stop("ws_unknown").await;
        assert_eq!(parts.agents.invalidated.lock().unwrap().len(), 1);
    }

    // O26 kill switch 接线：DirectiveSink::drain 经 manager 路由到目标工作区
    // 调度器；未知工作区 no-op；drain 后 loop 仍存活接活（合并窗已废——
    // drain 清 ready 队列的语义锁在 scheduler 单测，本用例锁路由与存活）。
    #[tokio::test(start_paused = true)]
    async fn directive_sink_drain_routes_and_loop_survives() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.drain("ws_unknown").await; // no-op，不得 panic
        sink.drain(WS).await; // 在途 run（timer 首 tick）不取消，drain 等其完成

        // drain 后调度器仍接活：指令照常全管线执行落库。
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "drain 后仍接活".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        })
        .expect("directive accepted");
        wait_until(
            || parts.runs.all().iter().any(|r| r.trigger == "user:u1"),
            "post-drain directive run",
        )
        .await;
    }

    // T14 DirectiveSink: directives route to the workspace scheduler and run
    // immediately (no merge window); the T13 pushback hits the user session.
    #[tokio::test(start_paused = true)]
    async fn directive_sink_routes_and_pushes_to_session() {
        let parts = stub_manager().await;
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "把车间温度降到 26 度".to_string(),
                session_key: Some("agent:ws_01:a/s1".to_string()),
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        })
        .expect("directive accepted");

        wait_until(|| !parts.host.pushes.lock().unwrap().is_empty(), "session push").await;
        let pushes = parts.host.pushes.lock().unwrap().clone();
        assert_eq!(pushes[0].0, "agent:ws_01:a/s1");
        assert!(pushes[0].1.contains("done"), "assistant message carries the summary");

        let runs = parts.runs.all();
        let run = runs
            .iter()
            .find(|r| r.trigger == "user:u1")
            .expect("user run persisted");
        assert_eq!(run.dedup_key, None);
        assert_eq!(pushes[0].2, run.run_id, "push carries the persisted run id");
        drop(runs);

        // Unknown workspace → Closed (loop never started).
        let err = sink.enqueue(WakeSignal {
            workspace_id: "ws_unknown".to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "hi".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        });
        assert_eq!(err, Err(EnqueueError::Closed));
    }

    // X6 (T18): 心跳桥投递的 directive（source=Some("heartbeat")）携带
    // problem_key —— run 落库必须带上，否则 O11 dedup 永远查不到历史。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_directive_run_records_problem_key() {
        let parts = stub_manager().await;
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::UserDirective {
                user_id: "heartbeat".to_string(),
                text: "心跳巡检发现待处置问题 set_hvac:dev-1，请诊断并处置。".to_string(),
                session_key: None,
                source: Some("heartbeat".to_string()),
                problem_key: Some("set_hvac:dev-1".to_string()),
            },
            dedup_key: None,
        })
        .expect("heartbeat directive accepted");

        wait_until(|| !parts.runs.is_empty(), "run persisted").await;
        let runs = parts.runs.all();
        let run = runs
            .iter()
            .find(|r| r.problem_key.as_deref() == Some("set_hvac:dev-1"))
            .expect("run must record the heartbeat problem_key");
        assert_eq!(run.trigger, "user:heartbeat");
        assert_eq!(run.dedup_key, None, "心跳 directive 不参与合并，dedup_key 为空");
    }

    // T6：heartbeat 信号走统一循环全管线——run 落库 trigger=heartbeat:{ws},
    // governor 按结果记账(成功清零/失败+1)。空任务表下 trigger 不 tick,
    // 信号由 DirectiveSink 手工投递。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_signal_produces_run_report_and_governor_records_outcome() {
        let parts = stub_manager().await;
        let governor = parts.heartbeat.governor.clone();
        // 预置一败:成功 run 必须清零(证明 record_success 真的跑了)。
        governor.record_failure(WS);
        assert_eq!(governor.consecutive_failures(WS), 1);
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        let heartbeat_signal = || WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        };
        sink.enqueue(heartbeat_signal()).expect("heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 1,
            "heartbeat run persisted",
        )
        .await;

        let runs = parts.runs.all();
        let run = runs
            .iter()
            .find(|r| r.dedup_key.as_deref() == Some("heartbeat:ws_01"))
            .expect("heartbeat run recorded");
        assert_eq!(run.trigger, "heartbeat:ws_01");
        drop(runs);
        wait_until(|| governor.consecutive_failures(WS) == 0, "success resets failures").await;

        // provider 失败一次 → run outcome=Failed → governor 记一败。
        parts.agents.llm.fail.store(true, std::sync::atomic::Ordering::Relaxed);
        sink.enqueue(heartbeat_signal())
            .expect("second heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 2,
            "failed run persisted",
        )
        .await;
        wait_until(|| governor.consecutive_failures(WS) == 1, "failure recorded").await;
    }

    // T6:heartbeat 源 prompt 装配——任务文本与信任段(仿 loop_.rs
    // build_heartbeat_prompt 的内容)必须进 LLM 实际收到的 prompt。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_prompt_contains_tasks_and_trust() {
        let parts = stub_manager().await;
        parts.heartbeat.tasks.insert(
            WS.to_string(),
            vec![tinyiothub_core::heartbeat::HeartbeatTask {
                id: 1,
                workspace_id: WS.into(),
                priority: "high".into(),
                text: "检查车间温度传感器".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        );
        parts.heartbeat.trust_configs.insert(
            WS.to_string(),
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                max_auto_actions_per_tick: 7,
                ..Default::default()
            },
        );
        parts.manager.start(WS);

        // heartbeat trigger 启动即首 tick(有活动任务);合并窗已废,即来即醒。
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 1,
            "heartbeat tick run",
        )
        .await;

        let prompts = parts.agents.llm.prompts.lock().unwrap();
        assert!(
            prompts.iter().any(|p| p.contains("检查车间温度传感器")),
            "任务文本必须进 heartbeat prompt: {prompts:?}"
        );
        assert!(
            prompts
                .iter()
                .any(|p| p.contains("FullAuto") && p.contains("每次 tick 最多自动执行动作数：7")),
            "信任段必须进 heartbeat prompt: {prompts:?}"
        );
        drop(prompts);

        // 修复轮 1：heartbeat 装配点把任务表大小写进 report.task_count
        // （读侧 ExecutionRecord.taskCount 数据源，对齐旧 loop task_count 语义）。
        let runs = parts.runs.all();
        let hb = runs
            .iter()
            .find(|r| r.trigger == "heartbeat:ws_01")
            .expect("heartbeat run recorded");
        assert_eq!(hb.task_count, Some(1), "任务表 1 条 → report.task_count=Some(1)");
    }

    // T6 修复轮 1①：agent 不可用早退对 heartbeat 源同样计连败（旧 loop 语义:
    // get_or_create 失败在 send_message 错误路径内,计连败)——否则 provider
    // 永久不可用时永远到不了暂停阈值。
    #[tokio::test(start_paused = true)]
    async fn agent_unavailable_heartbeat_run_records_governor_failure() {
        let host = Arc::new(StubHost::new());
        let registry = RunRegistry::new();
        let events = Arc::new(AgentEventBus::new(64));
        let heartbeat = HeartbeatDeps::default();
        let governor = heartbeat.governor.clone();
        let manager = Arc::new(ThingAgentManager::new(
            host.clone(),
            stub_policy_reader(WS),
            Arc::new(FailingProvider),
            registry,
            events,
            Arc::new(Runner::new()),
            ThingAgentManagerConfig {
                timer_interval: Duration::from_secs(24 * 3600),
                min_wake_level: 3,
                heartbeat_default_interval: Duration::from_secs(24 * 3600),
            },
            Arc::new(crate::runtime::thing_agent::traits::NoopTicketResolutions),
            heartbeat,
        ));
        manager.start(WS);

        let sink: &dyn DirectiveSink = manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        })
        .expect("heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || governor.consecutive_failures(WS) == 1,
            "agent-unavailable heartbeat run counts as governor failure",
        )
        .await;
    }

    // T6 修复轮 1②（E1 Task 7 改写）：Merged 穿透——合并窗已废，scheduler
    // 不再产生 Merged；变体保留（存储/回放兼容），直接经 DirectiveSink 注入
    // Merged 信号锁穿透语义：run 走 heartbeat 专用 prompt（任务+信任段）且
    // governor 记账。trigger 标签 merged:heartbeat:ws_01 证明 Merged 路径
    // 真的被走过（单信号不包 Merged）。enabled=false 抑制 trigger 首 tick，
    // 只留注入一路，run 计数确定。
    #[tokio::test(start_paused = true)]
    async fn merged_heartbeat_signal_uses_heartbeat_prompt_and_records_governor() {
        let parts = stub_manager().await;
        parts.heartbeat.tasks.insert(
            WS.to_string(),
            vec![tinyiothub_core::heartbeat::HeartbeatTask {
                id: 1,
                workspace_id: WS.into(),
                priority: "normal".into(),
                text: "巡检固件版本".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        );
        parts.heartbeat.trust_configs.insert(
            WS.to_string(),
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                max_auto_actions_per_tick: 9,
                ..Default::default()
            },
        );
        let governor = parts.heartbeat.governor.clone();
        governor.record_failure(WS); // 预置一败:成功 run 清零才证明记账穿透 Merged
        parts.manager.set_heartbeat_enabled(WS, false);
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        let hb = || WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        };
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![hb(), hb()],
            },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        })
        .expect("merged heartbeat signal accepted");
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 1,
            "merged heartbeat run",
        )
        .await;

        let runs = parts.runs.all();
        let run = runs
            .iter()
            .find(|r| r.dedup_key.as_deref() == Some("heartbeat:ws_01"))
            .expect("merged heartbeat run recorded");
        assert_eq!(
            run.trigger, "merged:heartbeat:ws_01",
            "Merged 信号必须保持 Merged 标签（单信号无 merged: 前缀）"
        );
        drop(runs);

        // 锁内克隆快照,守卫立即释放(clippy: guard 不得跨 await)。
        let prompts = parts.agents.llm.prompts.lock().unwrap().clone();
        assert!(
            prompts.iter().any(|p| p.contains("巡检固件版本")),
            "Merged heartbeat run 必须走 heartbeat 专用 prompt(任务段): {prompts:?}"
        );
        assert!(
            prompts
                .iter()
                .any(|p| p.contains("FullAuto") && p.contains("每次 tick 最多自动执行动作数：9")),
            "Merged heartbeat run 必须走 heartbeat 专用 prompt(信任段): {prompts:?}"
        );
        wait_until(|| governor.consecutive_failures(WS) == 0, "governor 记账穿透 Merged").await;
    }

    // Task 12（D13 实时读透出）：统一循环 heartbeat 源 run 完成（成败均计）写
    // 共享 last_ticks 表——对齐旧 loop_.rs:74-75 语义，handler 读源切换后的数据源。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_run_records_last_tick_in_shared_map() {
        let parts = stub_manager().await;
        let last_ticks = parts.heartbeat.last_ticks.clone();
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        let heartbeat_signal = || WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        };
        sink.enqueue(heartbeat_signal()).expect("heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 1,
            "heartbeat run persisted",
        )
        .await;
        wait_until(|| last_ticks.contains_key(WS), "success tick records last_tick").await;

        // 失败 run 同样计（成败均计）：清表后注入 provider 失败再跑一轮，
        // 表重新有值才证明失败路径也写。
        last_ticks.remove(WS);
        parts.agents.llm.fail.store(true, std::sync::atomic::Ordering::Relaxed);
        sink.enqueue(heartbeat_signal())
            .expect("second heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01") == 2,
            "failed run persisted",
        )
        .await;
        wait_until(|| last_ticks.contains_key(WS), "failed tick also records last_tick").await;
    }

    // Task 12：agent 不可用早退路径同样写 last_ticks（旧 loop 语义：tick 完成
    // 成败均计，get_or_create 失败在 send_message 错误路径内）——与 governor
    // 连败记账同一落点，两条路径共用。
    #[tokio::test(start_paused = true)]
    async fn agent_unavailable_heartbeat_run_records_last_tick() {
        let host = Arc::new(StubHost::new());
        let registry = RunRegistry::new();
        let events = Arc::new(AgentEventBus::new(64));
        let heartbeat = HeartbeatDeps::default();
        let last_ticks = heartbeat.last_ticks.clone();
        let manager = Arc::new(ThingAgentManager::new(
            host.clone(),
            stub_policy_reader(WS),
            Arc::new(FailingProvider),
            registry,
            events,
            Arc::new(Runner::new()),
            ThingAgentManagerConfig {
                timer_interval: Duration::from_secs(24 * 3600),
                min_wake_level: 3,
                heartbeat_default_interval: Duration::from_secs(24 * 3600),
            },
            Arc::new(crate::runtime::thing_agent::traits::NoopTicketResolutions),
            heartbeat,
        ));
        manager.start(WS);

        let sink: &dyn DirectiveSink = manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws_01".to_string()),
        })
        .expect("heartbeat signal accepted");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(
            || last_ticks.contains_key(WS),
            "agent-unavailable heartbeat run records last_tick",
        )
        .await;
    }

    // Task 12 行为锁：非 heartbeat 源 run 不写 last_ticks（表是 heartbeat 支路
    // 专属——thing 事件 run 完成后表必须仍为空）。
    #[tokio::test(start_paused = true)]
    async fn non_heartbeat_run_does_not_write_last_tick() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts.host.tx.send(event(1, 5, "device")).expect("send critical event");
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "run persisted").await;
        assert!(
            parts.heartbeat.last_ticks.is_empty(),
            "thing-event run must not write heartbeat last_ticks"
        );
    }

    /// T4：RunStarted 的 problem_keys 提取——递归 Merged.signals，收集
    /// UserDirective.problem_key 的 Some 值并去重（ThingEvent/Timer 无）。
    #[test]
    fn collect_problem_keys_recurses_merged_and_dedups() {
        let directive = |problem_key: Option<&str>| WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::UserDirective {
                user_id: "heartbeat".to_string(),
                text: "x".to_string(),
                session_key: None,
                source: Some("heartbeat".to_string()),
                problem_key: problem_key.map(str::to_owned),
            },
            dedup_key: None,
        };
        let thing = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id: 1,
                level: 3,
                data: serde_json::Value::Null,
            },
            dedup_key: None,
        };
        let merged = TriggerSource::Merged {
            signals: vec![thing, directive(Some("k1")), directive(Some("k1")), directive(None)],
        };
        assert_eq!(collect_problem_keys(&merged), vec!["k1".to_string()]);

        // 非合并源：直取；无 problem_key 的源为空。
        assert_eq!(
            collect_problem_keys(&directive(Some("k2")).source),
            vec!["k2".to_string()]
        );
        assert!(collect_problem_keys(&TriggerSource::Timer).is_empty());
    }

    #[test]
    fn trigger_labels_carry_type_prefixes() {
        let event_signal = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id: 7,
                level: 3,
                data: serde_json::Value::Null,
            },
            dedup_key: Some(EVENT_KEY.to_string()),
        };
        assert_eq!(trigger_label(&event_signal), EVENT_KEY);

        let timer = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Timer,
            dedup_key: Some("timer:ws_01".to_string()),
        };
        assert_eq!(trigger_label(&timer), "timer:ws_01");

        let directive = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "x".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        };
        assert_eq!(trigger_label(&directive), "user:u1");

        let merged = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![event_signal],
            },
            dedup_key: Some(EVENT_KEY.to_string()),
        };
        assert_eq!(trigger_label(&merged), format!("merged:{EVENT_KEY}"));
    }

    #[test]
    fn heartbeat_variant_trigger_label_uses_dedup_key() {
        // 定时 tick:dedup heartbeat:{ws};告警:heartbeat:{ws}:{thing}:{alarm_type}
        let tick = WakeSignal {
            workspace_id: "ws1".into(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat { alarm: None },
            dedup_key: Some("heartbeat:ws1".into()),
        };
        assert_eq!(trigger_label(&tick), "heartbeat:ws1");
        // trigger_type_of 取前缀 → "heartbeat"(读侧/统计按类型过滤依赖此前缀)
        assert_eq!(
            tick.dedup_key.as_deref().unwrap().split(':').next().unwrap(),
            "heartbeat"
        );
    }

    #[test]
    fn heartbeat_alarm_variant_carries_context_and_distinct_dedup() {
        let alarm = WakeSignal {
            workspace_id: "ws1".into(),
            priority: Priority::Critical,
            source: TriggerSource::Heartbeat {
                alarm: Some(HeartbeatAlarmContext {
                    thing_id: Some("d1".into()),
                    alarm_type: Some("offline".into()),
                    rule_id: Some("r1".into()),
                    reason: "设备离线".into(),
                    context: "{}".into(),
                }),
            },
            dedup_key: Some("heartbeat:ws1:d1:offline".into()),
        };
        assert_eq!(trigger_label(&alarm), "heartbeat:ws1:d1:offline");
    }

    // Value card
    // - 覆盖路径：告警驱动心跳 run 全管线（Merged 信号经 scheduler 普通路径
    //   → run_pipeline 的 heartbeat prompt + heartbeat_alarms 递归抽取 +
    //   render_heartbeat_alarm 附段 + task_count 回写 + governor 记账）。
    //   E1 Task 7 改写：合并窗已废，scheduler 不再产生 Merged；变体保留
    //   （存储/回放兼容），直接经 DirectiveSink 注入 Merged 信号锁穿透。
    // - 真实回归场景：有人删掉 run_pipeline 里的告警附段循环或
    //   collect_heartbeat_alarms 不递归 Merged——LLM 收到的告警 run 丢失
    //   原因/设备/类型上下文，自治处置质量静默退化，无任何测试报警。
    // - 失败信号：prompt 缺「设备离线」/「dev-1」/「offline」即断口；
    //   trigger 标签缺 merged: 前缀说明 Merged 路径未走（散兵信号不包 Merged）。
    // - 覆盖理由：callbacks 单测只到信号构造（to_wake_signal/优先级映射），
    //   manager 既有用例只覆盖 alarm=None 的定时 tick——告警上下文进 prompt
    //   的唯一管线在此。
    #[tokio::test(start_paused = true)]
    async fn merged_alarm_heartbeat_run_renders_alarm_context_in_prompt() {
        let parts = stub_manager().await;
        parts.heartbeat.tasks.insert(
            WS.to_string(),
            vec![tinyiothub_core::heartbeat::HeartbeatTask {
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
        let governor = parts.heartbeat.governor.clone();
        governor.record_failure(WS); // 预置一败：成功 run 清零才证明记账穿透
        // 抑制 trigger 首 tick（enabled=false）——只留注入的 Merged 一路，
        // run 计数断言确定。
        parts.manager.set_heartbeat_enabled(WS, false);
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        let alarm_signal = || WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Heartbeat {
                alarm: Some(HeartbeatAlarmContext {
                    thing_id: Some("dev-1".into()),
                    alarm_type: Some("offline".into()),
                    rule_id: Some("r1".into()),
                    reason: "设备离线".into(),
                    context: "{}".into(),
                }),
            },
            dedup_key: Some("heartbeat:ws_01:dev-1:offline".to_string()),
        };
        // 两条同键告警成员包进 Merged 直接注入——collect_heartbeat_alarms
        // 必须递归 Merged.signals 抽取告警上下文（顶层不再是 Heartbeat）。
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![alarm_signal(), alarm_signal()],
            },
            dedup_key: Some("heartbeat:ws_01:dev-1:offline".to_string()),
        })
        .expect("merged alarm signal accepted");
        wait_until(
            || parts.runs.runs_with_dedup("heartbeat:ws_01:dev-1:offline") == 1,
            "merged alarm run",
        )
        .await;

        let runs = parts.runs.all();
        let run = runs
            .iter()
            .find(|r| r.dedup_key.as_deref() == Some("heartbeat:ws_01:dev-1:offline"))
            .expect("alarm run recorded");
        assert_eq!(
            run.trigger, "merged:heartbeat:ws_01:dev-1:offline",
            "Merged 信号必须保持 Merged 标签（单信号无 merged: 前缀）"
        );
        assert_eq!(run.task_count, Some(1), "告警 run 同样回写 task_count");
        drop(runs);

        let prompts = parts.agents.llm.prompts.lock().unwrap().clone();
        assert!(
            prompts.iter().any(|p| p.contains("巡检设备在线率")
                && p.contains("设备离线")
                && p.contains("dev-1")
                && p.contains("offline")),
            "告警 run 必须走 heartbeat prompt 且附告警上下文（原因/设备/类型）: {prompts:?}"
        );
        wait_until(
            || governor.consecutive_failures(WS) == 0,
            "governor 记账穿透告警 Merged",
        )
        .await;
    }

    /// 工单 T2：agent_provider 失败的 early-return 必须发 RunRecorded
    ///（outcome=Failed, end_reason=AgentUnavailable）——否则工单订阅者永远
    /// 看不到这类最需要人工介入的失败，"零静默失败"承诺在此断裂。
    #[tokio::test(start_paused = true)]
    async fn agent_unavailable_emits_run_recorded() {
        let host = Arc::new(StubHost::new());
        let registry = RunRegistry::new();
        let events = Arc::new(AgentEventBus::new(64));
        let mut rx = events.subscribe();
        let manager = Arc::new(ThingAgentManager::new(
            host.clone(),
            stub_policy_reader(WS),
            Arc::new(FailingProvider),
            registry,
            events,
            Arc::new(Runner::new()),
            ThingAgentManagerConfig {
                timer_interval: Duration::from_secs(24 * 3600),
                min_wake_level: 3,
                heartbeat_default_interval: Duration::from_secs(24 * 3600),
            },
            Arc::new(crate::runtime::thing_agent::traits::NoopTicketResolutions),
            HeartbeatDeps::default(),
        ));
        manager.start(WS);
        wait_subscribed(&host).await;

        host.tx.send(event(1, 3, "device")).expect("send event");
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(30)).await;

        // RunsProbe 同款排空模式（try_recv + yield 泵）：暂停时钟下 rx.recv()
        // 挂起会让运行时误判空闲、自动快进到 timeout，管线可能还没被 poll 到
        // ——泵 yield 直到事件到达，有界防挂死。
        // 过滤 thing_id：timer 首 tick 即醒即跑（合并窗已废），同样以
        // AgentUnavailable 中止但 thing_id=None；事件触发的 run 才是断言对象。
        let report = 'outer: {
            for _ in 0..10_000 {
                match rx.try_recv() {
                    Ok(ev) => {
                        if let AgentEventKind::RunRecorded { report, .. } = ev.kind
                            && report.thing_id.is_some()
                        {
                            break 'outer report;
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => {}
                }
                tokio::task::yield_now().await;
            }
            panic!("RunRecorded not emitted for agent-unavailable abort");
        };

        assert_eq!(report.outcome, Outcome::Failed);
        assert_eq!(
            report.end_reason,
            Some(tinyiothub_core::agent_runs::EndReason::AgentUnavailable)
        );
        assert_eq!(report.thing_id.as_deref(), Some("t1"));
        assert!(report.summary.contains("不可用"));
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
    }

    /// 内存桩：记录全部 append 的 HubLedger。
    #[derive(Default)]
    struct RecordingLedger {
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
        }
    }

    fn watch_deps(rules: Vec<WatchRule>, ledger: Arc<RecordingLedger>) -> WatchDeps {
        WatchDeps {
            cache: Arc::new(StubThingCache {
                things: vec![watch_thing("t1")],
            }),
            ledger,
            rules: Arc::new(StubWatchRuleStore { rules }),
            events: Arc::new(tinyiothub_runtime::EventBus::new()),
        }
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

    // Value card
    // - 覆盖路径：set_watch_deps → start 挂载 engine+trigger → 启动重织
    //   （store 里的规则 upsert 进 engine）→ EventBus PropertyChange 越限 →
    //   WatchHit → WatchTrigger → WakeSignal(watch:r1) → 全管线 run 落库 +
    //   账本 watch_hit 落行。
    // - 真实回归场景：接线漏一环（重织没跑 / 事件没分发 / trigger 没注册）
    //   ——值守规则静默不命中，用户以为有值守实际没有，无任何告警。
    // - 失败信号：run 不落 dedup watch:r1 或账本缺 watch_hit 即断口。
    // - 覆盖理由：engine/trigger 单测都不经 manager 装配；API 测试不经
    //   事件链路。本用例是唯一端到端锁。
    #[tokio::test(start_paused = true)]
    async fn watch_hit_wakes_workspace_run_end_to_end() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        let deps = watch_deps(vec![watch_rule("r1", "t1")], ledger.clone());
        let bus = deps.events.clone();
        parts.manager.set_watch_deps(deps);
        parts.manager.start(WS);

        // engine + trigger 两个任务随 deps 挂载（forward+event+timer+heartbeat 之上）。
        assert_eq!(parts.manager.workspaces.get(WS).unwrap().tasks.len(), 6);
        assert!(parts.manager.watch_command_sink(WS).is_some());

        // 未越限：不命中（only-new-data 语义之外的一般路径）。
        publish_property_change(&bus, "t1", "25").await;
        settle().await;
        assert_eq!(parts.runs.runs_with_dedup("watch:r1"), 0);

        // 越限：命中 → run 落库 + 账本 watch_hit。
        publish_property_change(&bus, "t1", "31").await;
        wait_until(|| parts.runs.runs_with_dedup("watch:r1") == 1, "watch run persisted").await;
        wait_until(
            || ledger.entries.lock().unwrap().iter().any(|e| e.1 == "watch_hit"),
            "ledger watch_hit",
        )
        .await;
    }

    /// API 写穿（D11-⑤ 内存步）：Upsert 命令进 engine → 事件命中；Remove
    /// 后同事物越限不再命中。stop 丢弃命令通道。
    #[tokio::test(start_paused = true)]
    async fn watch_command_upsert_then_remove_controls_engine() {
        let parts = stub_manager().await;
        let ledger = Arc::new(RecordingLedger::default());
        let deps = watch_deps(vec![], ledger); // 空规则起步——不经重织
        let bus = deps.events.clone();
        parts.manager.set_watch_deps(deps);
        parts.manager.start(WS);
        let sink = parts.manager.watch_command_sink(WS).expect("command sink");

        sink.send(WatchCommand::Upsert(watch_rule("r2", "t1")))
            .await
            .expect("upsert sent");
        settle().await; // 命令先于事件进 engine（单循环串行消费）
        publish_property_change(&bus, "t1", "31").await;
        wait_until(|| parts.runs.runs_with_dedup("watch:r2") == 1, "upserted rule hits").await;

        // one-shot 命中后已自动移除；显式 Remove 幂等不炸，再越限不命中。
        sink.send(WatchCommand::Remove("r2".into())).await.expect("remove sent");
        settle().await;
        publish_property_change(&bus, "t1", "32").await;
        settle().await;
        assert_eq!(parts.runs.runs_with_dedup("watch:r2"), 1, "移除后不得再命中");

        parts.manager.stop(WS).await;
        assert!(parts.manager.watch_command_sink(WS).is_none(), "stop 丢弃命令通道");
    }

    /// 未接线（默认 None）：loop 不含 watch 任务，无命令通道。
    #[tokio::test(start_paused = true)]
    async fn start_without_watch_deps_mounts_no_watch_tasks() {
        let parts = stub_manager().await;
        parts.manager.start(WS);
        assert_eq!(parts.manager.workspaces.get(WS).unwrap().tasks.len(), 4);
        assert!(parts.manager.watch_command_sink(WS).is_none());
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
        parts.manager.start(WS);
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
}
