//! HeartbeatRunner — per-workspace heartbeat loop lifecycle manager.
//!
//! Owns a DashMap of cancel channels and handles. Start/stop are idempotent.
//! Tasks / TrustConfig / interval live in runner memory (Task 5: decoupled
//! from the db facade) — injected via restore/commands; DB writes
//! are the cloud service's job BEFORE calling the command (D11-⑤ 写序).

use super::types::{HeartbeatConfig, HeartbeatSignal, LoopSignal};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::metrics::Metrics;
use crate::runtime::agent::pool::AgentPoolLike;
use crate::runtime::event::bus::AiEventPublisher;
use crate::runtime::snapshot::WorkspaceHeartbeatState;
use tinyiothub_core::heartbeat::{HeartbeatTask, MIN_HEARTBEAT_INTERVAL_MINUTES, TrustConfig};

struct LoopHandle {
    cancel_tx: oneshot::Sender<()>,
    abort_handle: tokio::task::AbortHandle,
    /// Resolves when the supervisor has observed the loop task's exit.
    exit_rx: oneshot::Receiver<()>,
}

/// Bounded signal queue per workspace loop. External wakeups flood in from
/// alarms; an unbounded queue turns an alarm storm into a memory leak.
const SIGNAL_CHANNEL_CAPACITY: usize = 64;

/// Send a wakeup signal, coalescing duplicates: a full channel already has a
/// pending wakeup, so dropping the new one changes nothing. Returns false
/// when dropped.
fn send_wakeup(sender: &mpsc::Sender<LoopSignal>, signal: LoopSignal, workspace_id: &str) -> bool {
    match sender.try_send(signal) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            debug!(workspace_id, "Heartbeat signal channel full, coalescing wakeup");
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            warn!(workspace_id, "Heartbeat signal channel closed");
            false
        }
    }
}

/// Send a control signal (reloads). These carry state changes and must not
/// be dropped; when the channel is momentarily full, deliver asynchronously.
fn send_control(sender: &mpsc::Sender<LoopSignal>, signal: LoopSignal, workspace_id: &str) {
    match sender.try_send(signal) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(signal)) => {
            let sender = sender.clone();
            let ws = workspace_id.to_string();
            tokio::spawn(async move {
                if sender.send(signal).await.is_err() {
                    warn!(
                        workspace_id = ws,
                        "Heartbeat control signal undeliverable: channel closed"
                    );
                }
            });
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            warn!(workspace_id, "Heartbeat signal channel closed");
        }
    }
}

/// Manages per-workspace heartbeat loop lifecycle.
pub struct HeartbeatRunner {
    loops: Arc<DashMap<String, LoopHandle>>,
    signal_senders: Arc<DashMap<String, mpsc::Sender<LoopSignal>>>,
    trust_configs: DashMap<String, TrustConfig>,
    /// 内存任务真源（Task 5）：由 restore/reload 命令注入；运行中 loop 的
    /// ReloadTasks 信号重读此表。Arc 共享给 loop 任务。
    tasks: Arc<DashMap<String, Vec<HeartbeatTask>>>,
    /// 每工作区心跳间隔（分钟）；缺省走 config.interval_minutes。
    intervals: DashMap<String, u32>,
    /// 每工作区最近一次 tick 完成时间（D13 实时读出口；loop 每 tick 后写入）。
    last_ticks: Arc<DashMap<String, chrono::DateTime<chrono::Utc>>>,
    /// 每工作区最近心跳结果窗口（CEO review T22，cap 20）：dump_state
    /// 导出供 Lagged resync/周期对账补回丢失的 agent_actions 行——此前
    /// 心跳结果不在 dump_state 内，丢事件即永久丢失。
    recent_results: Arc<DashMap<String, std::collections::VecDeque<tinyiothub_core::heartbeat::HeartbeatResult>>>,
    /// 运行中 loop 的共享信任配置句柄（T18 修复）：start() 注册、
    /// stop()/remove_workspace 移除；update_trust_config 写穿——此前
    /// loop 持有的是启动快照 Arc，运行中更新永不生效（直到重启）。
    trust_handles: DashMap<String, Arc<RwLock<TrustConfig>>>,
    event_publisher: Arc<AiEventPublisher>,
    agent_pool: RwLock<Option<Arc<dyn AgentPoolLike>>>,
    config: HeartbeatConfig,
    /// Workspace IDs that tried to start before AgentPool was injected.
    pending_starts: RwLock<Vec<String>>,
    /// Operational metrics.
    pub metrics: Arc<Metrics>,
    /// Set to true during shutdown to reject new starts and signals.
    shutting_down: Arc<AtomicBool>,
}

impl HeartbeatRunner {
    pub fn new(event_publisher: Arc<AiEventPublisher>, config: HeartbeatConfig) -> Self {
        Self {
            loops: Arc::new(DashMap::new()),
            signal_senders: Arc::new(DashMap::new()),
            trust_configs: DashMap::new(),
            tasks: Arc::new(DashMap::new()),
            intervals: DashMap::new(),
            last_ticks: Arc::new(DashMap::new()),
            recent_results: Arc::new(DashMap::new()),
            trust_handles: DashMap::new(),
            event_publisher,
            agent_pool: RwLock::new(None),
            config,
            pending_starts: RwLock::new(Vec::new()),
            metrics: Arc::new(Metrics::new()),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn set_agent_pool(&self, pool: Arc<dyn AgentPoolLike>) {
        let mut guard = self.agent_pool.write().await;
        *guard = Some(pool);
        drop(guard);

        let pending = {
            let mut guard = self.pending_starts.write().await;
            std::mem::take(&mut *guard)
        };
        for ws_id in &pending {
            info!(ws_id, "Flushing pending heartbeat start");
            self.start(ws_id).await;
        }
    }

    /// Start a heartbeat loop for a workspace. Idempotent.
    pub async fn start(&self, workspace_id: &str) {
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!(workspace_id, "HeartbeatRunner is shutting down, rejecting start");
            return;
        }

        if !self.config.enabled {
            info!(workspace_id, "Heartbeat disabled, skipping start");
            return;
        }

        self.stop(workspace_id).await;

        // TrustConfig 来自内存（restore/命令注入）；未注入时用缺省。
        let trust_config = Arc::new(RwLock::new(self.get_trust_config(workspace_id).unwrap_or_default()));
        // T18：句柄注册——update_trust_config 写穿此 Arc，运行中 loop
        // 每 tick 读到的即最新值。
        self.trust_handles
            .insert(workspace_id.to_string(), trust_config.clone());
        let trust_config_for_cache = trust_config.clone();

        self.trust_configs
            .insert(workspace_id.to_string(), trust_config_for_cache.read().await.clone());

        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, trust_config_for_cache.read().await.clone());
        }

        // 任务来自内存（restore/reload 命令注入；Task 9 接线启动恢复）。
        // 未注入时跳过启动 —— 空任务集的 loop 没有意义。
        let tasks_vec = self
            .tasks
            .get(workspace_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();
        if tasks_vec.is_empty() {
            info!(workspace_id, "No heartbeat tasks, skipping loop start");
            return;
        }
        let tasks = Arc::new(RwLock::new(tasks_vec));

        let pool = self.agent_pool.read().await.clone();
        if pool.is_none() {
            info!(workspace_id, "AgentPool not ready, queuing heartbeat start");
            let mut pending = self.pending_starts.write().await;
            if !pending.iter().any(|p| p == workspace_id) {
                pending.push(workspace_id.to_string());
            }
            return;
        }

        let (signal_tx, signal_rx) = mpsc::channel::<LoopSignal>(SIGNAL_CHANNEL_CAPACITY);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let ws_id = workspace_id.to_string();
        let tasks_source = self.tasks.clone();
        let last_ticks = self.last_ticks.clone();
        let event_publisher = self.event_publisher.clone();
        let mut config = self.config.clone();
        config.interval_minutes = self.effective_interval_minutes(workspace_id);
        let metrics = self.metrics.clone();

        let join_handle = tokio::spawn(async move {
            super::loop_::heartbeat_loop(
                ws_id,
                tasks,
                trust_config,
                pool,
                tasks_source,
                last_ticks,
                event_publisher,
                config,
                signal_rx,
                cancel_rx,
                metrics,
            )
            .await;
        });

        // Supervisor: reap the loop's map entries when its task exits without
        // stop() being called (panic or unexpected return). stop() removes
        // the entry first, so a still-present entry means an unexpected exit.
        let abort_handle = join_handle.abort_handle();
        let (exit_tx, exit_rx) = oneshot::channel();
        let loops = Arc::clone(&self.loops);
        let senders = Arc::clone(&self.signal_senders);
        let metrics = self.metrics.clone();
        let ws_supervised = workspace_id.to_string();
        tokio::spawn(async move {
            let result = join_handle.await;
            if loops.get(&ws_supervised).is_some() {
                match &result {
                    Ok(()) => info!(ws_supervised, "Heartbeat loop exited unexpectedly, reaping"),
                    Err(e) => error!(ws_supervised, error = %e, "Heartbeat loop crashed, reaping"),
                }
                loops.remove(&ws_supervised);
                senders.remove(&ws_supervised);
                metrics.failed_loops.fetch_add(1, Ordering::Relaxed);
                metrics.active_loops.fetch_sub(1, Ordering::Relaxed);
            }
            let _ = exit_tx.send(());
        });

        self.signal_senders.insert(workspace_id.to_string(), signal_tx);
        self.loops.insert(
            workspace_id.to_string(),
            LoopHandle {
                cancel_tx,
                abort_handle,
                exit_rx,
            },
        );
        self.metrics.active_loops.fetch_add(1, Ordering::Relaxed);

        info!(workspace_id, "Heartbeat loop started");
    }

    /// Stop a heartbeat loop for a workspace. No-op if not running.
    pub async fn stop(&self, workspace_id: &str) {
        self.trust_handles.remove(workspace_id);
        if let Some((_, handle)) = self.loops.remove(workspace_id) {
            let _ = handle.cancel_tx.send(());
            if tokio::time::timeout(std::time::Duration::from_secs(5), handle.exit_rx)
                .await
                .is_err()
            {
                warn!(workspace_id, "Heartbeat loop did not exit in 5s, aborting");
                handle.abort_handle.abort();
            }
            self.metrics.active_loops.fetch_sub(1, Ordering::Relaxed);
            self.metrics.loops_completed.fetch_add(1, Ordering::Relaxed);
        }
        self.signal_senders.remove(workspace_id);
        // trust_configs/tasks/intervals 是内存真源，stop 后保留 —— start 幂等
        // 重启（如改间隔）依赖它们在重启间存活。工作区删除走 remove_workspace。
        info!(workspace_id, "Heartbeat loop stopped");
    }

    /// 工作区删除清理命令：停 loop，清三张内存表（trust/tasks/intervals），
    /// 并出队 pending start。不清理则已删工作区在内存与 dump_state 快照中
    /// 永久残留（Task 5 fix round 1）。
    pub async fn remove_workspace(&self, workspace_id: &str) {
        self.stop(workspace_id).await;
        self.trust_configs.remove(workspace_id);
        self.tasks.remove(workspace_id);
        self.intervals.remove(workspace_id);
        self.last_ticks.remove(workspace_id);
        self.recent_results.remove(workspace_id);
        self.trust_handles.remove(workspace_id);
        self.pending_starts.write().await.retain(|p| p != workspace_id);
        info!(workspace_id, "Workspace heartbeat state removed");
    }

    /// 记录一条心跳结果进窗口（cap 20/工作区，CEO review T22）：
    /// 由 orchestrator 在发射 HeartbeatResultReady 的同一位置调用——
    /// 内存窗口与事件出口同源，dump_state 导出才与"已发射"一致。
    pub fn record_result(&self, workspace_id: &str, result: tinyiothub_core::heartbeat::HeartbeatResult) {
        const RECENT_RESULTS_CAP: usize = 20;
        let mut entry = self.recent_results.entry(workspace_id.to_string()).or_default();
        entry.push_back(result);
        while entry.len() > RECENT_RESULTS_CAP {
            entry.pop_front();
        }
    }

    /// 全部工作区的近期心跳结果（dump_state 导出用；按 id 排序保证确定性）。
    pub fn recent_results(&self) -> Vec<tinyiothub_core::heartbeat::HeartbeatResult> {
        let mut out: Vec<_> = self
            .recent_results
            .iter()
            .flat_map(|entry| entry.value().iter().cloned().collect::<Vec<_>>())
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Send a signal to a workspace's heartbeat loop. Non-blocking.
    pub fn signal(&self, signal: HeartbeatSignal) {
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!("HeartbeatRunner is shutting down, ignoring signal");
            return;
        }

        let ws_id = signal.workspace_id.clone();
        match self.signal_senders.get(&ws_id) {
            Some(sender) => {
                send_wakeup(&sender, LoopSignal::External(signal), &ws_id);
            }
            None => {
                debug!(workspace_id = %ws_id, "No active heartbeat loop, skipping signal");
            }
        }
    }

    /// Notify a running loop that its in-memory tasks were replaced.
    fn notify_tasks_changed(&self, workspace_id: &str) {
        if let Some(sender) = self.signal_senders.get(workspace_id) {
            send_control(&sender, LoopSignal::ReloadTasks, workspace_id);
            info!(workspace_id, "Heartbeat loop notified: tasks changed");
        }
    }

    /// Notify a running loop to re-read TrustConfig.
    fn notify_config_changed(&self, workspace_id: &str) {
        if let Some(sender) = self.signal_senders.get(workspace_id) {
            send_control(&sender, LoopSignal::ReloadConfig, workspace_id);
        }
    }

    /// 全量替换内存任务并通知运行中 loop 重读（ReloadTasks 信号语义：
    /// 内存已被本命令更新，loop 重读内存）。DB 写由调用方先做（D11-⑤）。
    pub fn set_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        self.tasks.insert(workspace_id.to_string(), tasks);
        self.notify_tasks_changed(workspace_id);
    }

    /// 工作区内存任务快照（无注入时为空）。
    pub fn tasks(&self, workspace_id: &str) -> Vec<HeartbeatTask> {
        self.tasks
            .get(workspace_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// 更新内存心跳间隔（分钟）。DB 写与 enabled 归并由调用方先做；
    /// 运行中 loop 的重启由调用方触发（start 幂等）。
    pub fn set_interval_minutes(&self, workspace_id: &str, interval_minutes: u32) {
        self.intervals.insert(workspace_id.to_string(), interval_minutes);
    }

    /// trust config 变更命令：只更新内存 + 热更 pool + 通知 loop。
    /// DB 写由 cloud 侧 service 先行完成（D11-⑤ 写序）；事件由门面发射。
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.trust_configs.insert(workspace_id.to_string(), config.clone());
        // T18：写穿运行中 loop 的共享 Arc——此前 loop 持启动快照，
        // PUT trust 后运行中 loop 永不生效（直到重启）。
        if let Some(handle) = self.trust_handles.get(workspace_id) {
            match handle.value().try_write() {
                Ok(mut guard) => *guard = config.clone(),
                Err(_) => {
                    // tick 读锁在飞（毫秒级）：spawn 等待写入，与
                    // send_control 的"必达"语义一致。
                    let handle = handle.value().clone();
                    let ws = workspace_id.to_string();
                    let config = config.clone();
                    tokio::spawn(async move {
                        *handle.write().await = config;
                        debug!(workspace_id = ws, "trust config hot-update applied after contention");
                    });
                }
            }
        }
        match self.agent_pool.try_read() {
            Ok(guard) => {
                if let Some(pool) = guard.as_ref() {
                    pool.set_trust_config(workspace_id, config);
                }
            }
            Err(_) => warn!(workspace_id, "AgentPool locked, trust config hot-update skipped"),
        }
        self.notify_config_changed(workspace_id);
        info!(workspace_id, "TrustConfig updated");
    }

    /// The interval a workspace's loop should use: per-workspace memory value
    /// when injected (clamped to the minimum), otherwise the runner default.
    pub fn effective_interval_minutes(&self, workspace_id: &str) -> u32 {
        self.intervals
            .get(workspace_id)
            .map(|r| (*r.value()).max(MIN_HEARTBEAT_INTERVAL_MINUTES))
            .unwrap_or(self.config.interval_minutes)
    }

    pub fn get_trust_config(&self, workspace_id: &str) -> Option<TrustConfig> {
        self.trust_configs.get(workspace_id).map(|r| r.value().clone())
    }

    pub fn active_loop_count(&self) -> usize {
        self.loops.len()
    }

    pub fn active_workspaces(&self) -> Vec<String> {
        self.loops.iter().map(|r| r.key().clone()).collect()
    }

    /// 工作区最近一次 tick 完成时间（D13 实时读：内存态，无 tick 过为 None）。
    pub fn last_tick(&self, workspace_id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        self.last_ticks.get(workspace_id).map(|r| *r.value())
    }

    /// 导出内存心跳状态（dump_state 对账出口）：tasks/trust/intervals 三表
    /// key 的并集，缺省字段回退 default。
    pub fn snapshot_states(&self) -> Vec<WorkspaceHeartbeatState> {
        let mut ws_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in self.tasks.iter() {
            ws_ids.insert(r.key().clone());
        }
        for r in self.trust_configs.iter() {
            ws_ids.insert(r.key().clone());
        }
        for r in self.intervals.iter() {
            ws_ids.insert(r.key().clone());
        }
        ws_ids
            .into_iter()
            .map(|ws| WorkspaceHeartbeatState {
                tasks: self.tasks(&ws),
                trust_config: self.get_trust_config(&ws).unwrap_or_default(),
                interval_minutes: self
                    .intervals
                    .get(&ws)
                    .map(|r| *r.value())
                    .unwrap_or(self.config.interval_minutes),
                workspace_id: ws,
            })
            .collect()
    }

    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let ws_ids: Vec<String> = self.active_workspaces();
        for ws_id in &ws_ids {
            self.stop(ws_id).await;
        }
        info!(count = ws_ids.len(), "HeartbeatRunner shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::event::bus::AiEventPublisher;

    /// 内存夹具（Task 5 去 repo 后替代 real_repo SQLite 夹具）：显式构造，
    /// 任务经 `set_tasks` 注入，trust/interval 经命令方法注入。
    fn task_fixture(text: &str) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws_1".into(),
            priority: "high".into(),
            text: text.into(),
            paused: false,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())))
    }

    fn make_runner() -> HeartbeatRunner {
        HeartbeatRunner::new(make_publisher(), HeartbeatConfig::default())
    }

    #[tokio::test]
    async fn test_runner_construction() {
        let runner = make_runner();
        assert_eq!(runner.active_loop_count(), 0);
        assert!(runner.active_workspaces().is_empty());
        assert!(
            runner.last_tick("ws_1").is_none(),
            "no tick recorded before any loop run"
        );
    }

    #[tokio::test]
    async fn test_start_with_no_tasks_exits_early() {
        let runner = make_runner();
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_is_noop() {
        let runner = make_runner();
        runner.stop("nonexistent").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_start_when_disabled() {
        let runner = HeartbeatRunner::new(
            make_publisher(),
            HeartbeatConfig {
                enabled: false,
                ..Default::default()
            },
        );
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_pending_starts_queued_when_pool_not_ready() {
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_pending_starts_deduped() {
        // Repeated start() calls while the pool is down must not pile up
        // duplicate entries — each would trigger a redundant stop+start when
        // the pool arrives.
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.start("ws_1").await;
        runner.start("ws_1").await;
        runner.start("ws_1").await;
        assert_eq!(runner.pending_starts.read().await.len(), 1);
    }

    #[test]
    fn wakeup_signal_drops_when_channel_full() {
        // Wakeups are coalescing: a full channel means a wakeup is already
        // pending, so the new one is redundant. Unbounded growth is not ok.
        let (tx, _rx) = mpsc::channel::<LoopSignal>(1);
        assert!(send_wakeup(&tx, LoopSignal::ReloadConfig, "ws"));
        assert!(
            !send_wakeup(&tx, LoopSignal::ReloadConfig, "ws"),
            "full channel must drop the duplicate wakeup"
        );
    }

    #[tokio::test]
    async fn control_signal_is_not_lost_when_channel_full() {
        // Reload signals carry state (task edits); dropping them loses user
        // changes. When the channel is momentarily full they must still be
        // delivered once space frees up.
        let (tx, mut rx) = mpsc::channel::<LoopSignal>(1);
        tx.try_send(LoopSignal::ReloadConfig).unwrap();
        send_control(&tx, LoopSignal::ReloadTasks, "ws");

        assert!(matches!(rx.recv().await, Some(LoopSignal::ReloadConfig)));
        assert!(
            matches!(rx.recv().await, Some(LoopSignal::ReloadTasks)),
            "control signal must be delivered after space frees"
        );
    }

    struct PanicPool;

    #[async_trait::async_trait]
    impl AgentPoolLike for PanicPool {
        async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
            Ok("agent".into())
        }
        async fn send_message(
            &self,
            _workspace_id: &str,
            _prompt: &str,
        ) -> anyhow::Result<crate::runtime::agent::pool::AgentRunOutput> {
            panic!("simulated loop crash");
        }
        async fn shutdown(&self) {}
        fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
        fn cleanup_idle(&self) -> usize {
            0
        }
    }

    #[tokio::test]
    async fn panicking_loop_is_reaped_by_supervisor() {
        // A crashed loop must not linger in the maps: its entry and signal
        // sender are removed so later starts/signals behave correctly.
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.set_agent_pool(Arc::new(PanicPool)).await;
        runner.start("ws_1").await;

        for _ in 0..100 {
            if runner.active_loop_count() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(runner.active_loop_count(), 0, "panicked loop must be reaped");
        assert!(
            runner.signal_senders.get("ws_1").is_none(),
            "dead loop's sender must be removed"
        );
    }

    #[tokio::test]
    async fn test_start_uses_injected_trust_config() {
        let runner = make_runner();
        runner.update_trust_config(
            "ws_1",
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                ..Default::default()
            },
        );
        runner.start("ws_1").await;

        let loaded = runner.get_trust_config("ws_1").expect("trust config cached on start");
        assert_eq!(loaded.trust_level, tinyiothub_core::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    async fn test_start_falls_back_to_default_trust_config() {
        let runner = make_runner();
        runner.start("ws_1").await;

        let loaded = runner.get_trust_config("ws_1").expect("trust config cached on start");
        assert_eq!(loaded.trust_level, tinyiothub_core::heartbeat::TrustLevel::ReadOnlyAuto);
    }

    #[tokio::test]
    async fn test_update_trust_config_updates_memory() {
        // Task 5：命令只写内存（DB 写由 cloud 侧 service 先行，D11-⑤ 写序）。
        let runner = make_runner();

        let cfg = tinyiothub_core::heartbeat::TrustConfig {
            trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
            ..Default::default()
        };
        runner.update_trust_config("ws_1", cfg);

        let saved = runner.get_trust_config("ws_1").expect("in-memory trust config");
        assert_eq!(saved.trust_level, tinyiothub_core::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    async fn test_trust_config_survives_stop_start_cycle() {
        // start 幂等（先 stop 再起）；内存真源必须在重启间存活，否则改间隔
        // 重启会丢 trust 配置。
        let runner = make_runner();
        runner.update_trust_config(
            "ws_1",
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                ..Default::default()
            },
        );
        runner.stop("ws_1").await;
        runner.start("ws_1").await;

        let loaded = runner.get_trust_config("ws_1").expect("trust config survives restart");
        assert_eq!(loaded.trust_level, tinyiothub_core::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    async fn test_effective_interval_uses_injected_value() {
        let runner = make_runner();
        runner.set_interval_minutes("ws_1", 30);

        assert_eq!(runner.effective_interval_minutes("ws_1"), 30);
        assert_eq!(runner.effective_interval_minutes("ws_other"), 15);
    }

    #[tokio::test]
    async fn test_effective_interval_clamps_to_minimum() {
        let runner = make_runner();
        runner.set_interval_minutes("ws_1", 1);

        assert_eq!(
            runner.effective_interval_minutes("ws_1"),
            MIN_HEARTBEAT_INTERVAL_MINUTES
        );
    }

    #[tokio::test]
    async fn remove_workspace_clears_all_in_memory_state() {
        // 工作区删除后三表必须清空，否则已删工作区在内存与 dump_state
        // 快照中永久残留（stop 只停 loop，不清内存真源）。
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.update_trust_config(
            "ws_1",
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                ..Default::default()
            },
        );
        runner.set_interval_minutes("ws_1", 30);
        runner.pending_starts.write().await.push("ws_1".to_string());

        runner.remove_workspace("ws_1").await;

        assert!(runner.tasks("ws_1").is_empty());
        assert!(runner.get_trust_config("ws_1").is_none());
        assert_eq!(
            runner.effective_interval_minutes("ws_1"),
            HeartbeatConfig::default().interval_minutes,
            "cleared interval must fall back to the runner default"
        );
        assert!(runner.pending_starts.read().await.is_empty());
        assert!(
            runner.snapshot_states().is_empty(),
            "removed workspace must not appear in the exported snapshot"
        );
    }

    /// T18：运行中 loop 的信任配置热更新链路（HTTP handler 先写 DB →
    /// update_trust_config → loop 共享 Arc 即时可见）。修复前：loop 持
    /// 启动快照 Arc，运行中更新永不生效（直到 stop/start）。
    #[tokio::test]
    async fn test_update_trust_config_reaches_running_loop() {
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.set_agent_pool(Arc::new(OkPool)).await;
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 1);

        // loop 每 tick 的读路径就是这个共享 Arc（经 runner 句柄取出）。
        let handle = runner
            .trust_handles
            .get("ws_1")
            .expect("trust handle registered on start")
            .value()
            .clone();
        assert_eq!(
            handle.read().await.trust_level,
            tinyiothub_core::heartbeat::TrustLevel::ReadOnlyAuto
        );

        // 命令更新（cloud handler 已先写 DB，D11-⑤ 写序）。
        runner.update_trust_config(
            "ws_1",
            tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                ..Default::default()
            },
        );

        // 同一 Arc 立即读到新值——运行中 loop 的下一 tick 即生效。
        assert_eq!(
            handle.read().await.trust_level,
            tinyiothub_core::heartbeat::TrustLevel::FullAuto,
            "running loop's shared trust config must reflect hot updates"
        );
        runner.stop("ws_1").await;
        assert!(
            runner.trust_handles.get("ws_1").is_none(),
            "stop must remove the trust handle"
        );
    }

    /// T22 覆盖补钉：record_result 窗口 cap 20 驱逐最老。
    #[tokio::test]
    async fn record_result_window_evicts_oldest_beyond_cap() {
        let runner = make_runner();
        for i in 0..25 {
            runner.record_result(
                "ws_1",
                tinyiothub_core::heartbeat::HeartbeatResult {
                    id: format!("tick-{i:02}"),
                    workspace_id: "ws_1".into(),
                    status: tinyiothub_core::heartbeat::HeartbeatStatus::Complete,
                    summary: "ok".into(),
                    task_count: 1,
                    executed_actions: vec![],
                    proposals: vec![],
                    error: None,
                },
            );
        }
        let recent = runner.recent_results();
        assert_eq!(recent.len(), 20, "window must cap at 20");
        assert!(
            recent.iter().all(|r| r.id != "tick-00"),
            "oldest result must be evicted"
        );
        assert!(recent.iter().any(|r| r.id == "tick-24"));
    }

    struct OkPool;

    #[async_trait::async_trait]
    impl AgentPoolLike for OkPool {
        async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
            Ok("agent".into())
        }
        async fn send_message(
            &self,
            _workspace_id: &str,
            _prompt: &str,
        ) -> anyhow::Result<crate::runtime::agent::pool::AgentRunOutput> {
            Ok(crate::runtime::agent::pool::AgentRunOutput {
                text: r#"{"status":"complete","summary":"ok","proposals":[]}"#.into(),
                tool_calls: vec![],
            })
        }
        async fn shutdown(&self) {}
        fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
        fn cleanup_idle(&self) -> usize {
            0
        }
    }

    #[tokio::test]
    async fn metrics_track_loop_lifecycle() {
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.set_agent_pool(Arc::new(OkPool)).await;

        runner.start("ws_1").await;
        assert_eq!(
            runner.metrics.active_loops.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "start must count the new active loop"
        );

        runner.stop("ws_1").await;
        assert_eq!(
            runner.metrics.active_loops.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            runner
                .metrics
                .loops_completed
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "clean stop must count a completed loop"
        );
    }

    #[tokio::test]
    async fn crashed_loop_counts_as_failed_in_metrics() {
        let runner = make_runner();
        runner.set_tasks("ws_1", vec![task_fixture("test")]);
        runner.set_agent_pool(Arc::new(PanicPool)).await;
        runner.start("ws_1").await;

        for _ in 0..100 {
            if runner.active_loop_count() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(runner.active_loop_count(), 0, "panicked loop must be reaped");
        assert_eq!(
            runner.metrics.failed_loops.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "crashed loop must be counted as failed"
        );
        assert_eq!(
            runner.metrics.active_loops.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
