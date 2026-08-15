//! HeartbeatRunner — per-workspace heartbeat loop lifecycle manager.
//!
//! Owns a DashMap of cancel channels and handles. Start/stop are idempotent.
//! TrustConfig is loaded from DB on start and cached in memory.

use super::types::{HeartbeatConfig, HeartbeatSignal, LoopSignal};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::metrics::Metrics;
use super::repo::HeartbeatTaskRepository;
use crate::domains::agent::loop_::agent::pool::AgentPoolLike;
use crate::domains::agent::loop_::event::bus::AiEventPublisher;
use tinyiothub_core::heartbeat::TrustConfig;

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
    task_repo: Arc<HeartbeatTaskRepository>,
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
    pub fn new(
        task_repo: Arc<HeartbeatTaskRepository>,
        event_publisher: Arc<AiEventPublisher>,
        config: HeartbeatConfig,
    ) -> Self {
        Self {
            loops: Arc::new(DashMap::new()),
            signal_senders: Arc::new(DashMap::new()),
            trust_configs: DashMap::new(),
            task_repo,
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

        let trust_config = Arc::new(RwLock::new(self.load_trust_config(workspace_id).await));
        let trust_config_for_cache = trust_config.clone();

        self.trust_configs
            .insert(workspace_id.to_string(), trust_config_for_cache.read().await.clone());

        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, trust_config_for_cache.read().await.clone());
        }

        let tasks = Arc::new(RwLock::new(
            match self.task_repo.list_by_workspace(workspace_id).await {
                Ok(t) => t,
                Err(e) => {
                    error!(workspace_id, error = %e, "Failed to load heartbeat tasks");
                    return;
                }
            },
        ));

        if tasks.read().await.is_empty() {
            info!(workspace_id, "No heartbeat tasks, skipping loop start");
            return;
        }

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
        let task_repo = self.task_repo.clone();
        let event_publisher = self.event_publisher.clone();
        let mut config = self.config.clone();
        config.interval_minutes = self.effective_interval_minutes(workspace_id).await;
        let metrics = self.metrics.clone();

        let join_handle = tokio::spawn(async move {
            super::loop_::heartbeat_loop(
                ws_id,
                tasks,
                trust_config,
                pool,
                task_repo,
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
        self.trust_configs.remove(workspace_id);
        info!(workspace_id, "Heartbeat loop stopped");
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

    /// Notify a running loop to reload tasks from the repository.
    pub fn notify_tasks_changed(&self, workspace_id: &str) {
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

    pub async fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        if let Err(e) = self.task_repo.save_trust_config(workspace_id, &config).await {
            warn!(workspace_id, error = %e, "Failed to persist TrustConfig");
        }
        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, config.clone());
        }
        self.trust_configs.insert(workspace_id.to_string(), config);
        self.notify_config_changed(workspace_id);
        info!(workspace_id, "TrustConfig updated");
    }

    /// The interval a workspace's loop should use: per-workspace config when
    /// persisted (clamped to the minimum), otherwise the runner default.
    pub async fn effective_interval_minutes(&self, workspace_id: &str) -> u32 {
        match self.task_repo.load_heartbeat_config(workspace_id).await {
            Ok(Some(cfg)) => cfg
                .interval_minutes
                .max(tinyiothub_core::heartbeat::MIN_HEARTBEAT_INTERVAL_MINUTES),
            Ok(None) => self.config.interval_minutes,
            Err(e) => {
                warn!(workspace_id, error = %e, "Failed to load heartbeat config, using default interval");
                self.config.interval_minutes
            }
        }
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

    /// Access the task repository (used by API handlers for task CRUD).
    pub fn task_repo(&self) -> Arc<HeartbeatTaskRepository> {
        self.task_repo.clone()
    }

    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let ws_ids: Vec<String> = self.active_workspaces();
        for ws_id in &ws_ids {
            self.stop(ws_id).await;
        }
        info!(count = ws_ids.len(), "HeartbeatRunner shut down");
    }

    async fn load_trust_config(&self, workspace_id: &str) -> TrustConfig {
        match self.task_repo.load_trust_config(workspace_id).await {
            Ok(Some(config)) => config,
            Ok(None) => TrustConfig::default(),
            Err(e) => {
                warn!(workspace_id, error = %e, "Failed to load TrustConfig, using default");
                TrustConfig::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::loop_::event::bus::AiEventPublisher;

    /// 真实 SQLite 版 heartbeat repo（E6b 去 trait 后替代 MockTaskRepo）。
    async fn real_repo() -> Arc<HeartbeatTaskRepository> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        // workspaces 行是 trust/hb config UPDATE 的目标（无行则静默 no-op）
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ('t1', 'T', 't')")
            .execute(&pool)
            .await
            .expect("seed tenant");
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws_1', 'WS', 't1', '2026-01-01', '2026-01-01')")
            .execute(&pool)
            .await
            .expect("seed workspace");
        Arc::new(HeartbeatTaskRepository::new(pool))
    }

    /// 预置一条 high/test 任务（新库 rowid=1，与原 mock 的 id:1 一致）。
    async fn real_repo_with_task() -> Arc<HeartbeatTaskRepository> {
        let repo = real_repo().await;
        repo.insert("ws_1", "high", "test").await.expect("seed task");
        repo
    }

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())))
    }

    #[tokio::test]
    async fn test_runner_construction() {
        let repo = real_repo().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        assert_eq!(runner.active_loop_count(), 0);
        assert!(runner.active_workspaces().is_empty());
    }

    #[tokio::test]
    async fn test_start_with_no_tasks_exits_early() {
        let repo = real_repo().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_is_noop() {
        let repo = real_repo().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.stop("nonexistent").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_start_when_disabled() {
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let config = HeartbeatConfig {
            enabled: false,
            ..Default::default()
        };
        let runner = HeartbeatRunner::new(repo, publisher, config);
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_pending_starts_queued_when_pool_not_ready() {
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_pending_starts_deduped() {
        // Repeated start() calls while the pool is down must not pile up
        // duplicate entries — each would trigger a redundant stop+start when
        // the pool arrives.
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
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
        ) -> anyhow::Result<crate::domains::agent::loop_::agent::pool::AgentRunOutput> {
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
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
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
    async fn test_start_loads_trust_config_from_repo() {
        let repo = real_repo().await;
        repo.save_trust_config(
            "ws_1",
            &tinyiothub_core::heartbeat::TrustConfig {
                trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
                ..Default::default()
            },
        )
        .await
        .expect("seed trust");
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;

        let loaded = runner.get_trust_config("ws_1").expect("trust config cached on start");
        assert_eq!(loaded.trust_level, tinyiothub_core::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    async fn test_start_falls_back_to_default_trust_config() {
        let repo = real_repo().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;

        let loaded = runner.get_trust_config("ws_1").expect("trust config cached on start");
        assert_eq!(loaded.trust_level, tinyiothub_core::heartbeat::TrustLevel::ReadOnlyAuto);
    }

    #[tokio::test]
    async fn test_update_trust_config_persists_via_repo() {
        let repo = real_repo().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo.clone(), publisher, HeartbeatConfig::default());

        let cfg = tinyiothub_core::heartbeat::TrustConfig {
            trust_level: tinyiothub_core::heartbeat::TrustLevel::FullAuto,
            ..Default::default()
        };
        runner.update_trust_config("ws_1", cfg).await;

        let saved = repo.load_trust_config("ws_1").await.expect("load").expect("persisted");
        assert_eq!(saved.trust_level, tinyiothub_core::heartbeat::TrustLevel::FullAuto);
    }

    #[tokio::test]
    async fn test_effective_interval_uses_workspace_config() {
        let repo = real_repo().await;
        repo.save_heartbeat_config(
            "ws_1",
            &crate::domains::agent::loop_::heartbeat::types::WorkspaceHeartbeatConfig {
                enabled: true,
                interval_minutes: 30,
            },
        )
        .await
        .expect("seed hb config");
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());

        assert_eq!(runner.effective_interval_minutes("ws_1").await, 30);
        assert_eq!(runner.effective_interval_minutes("ws_other").await, 15);
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
        ) -> anyhow::Result<crate::domains::agent::loop_::agent::pool::AgentRunOutput> {
            Ok(crate::domains::agent::loop_::agent::pool::AgentRunOutput {
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
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
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
        let repo = real_repo_with_task().await;
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
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
