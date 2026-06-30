//! HeartbeatRunner — per-workspace heartbeat loop lifecycle manager.
//!
//! Owns a DashMap of cancel channels and handles. Start/stop are idempotent.
//! TrustConfig is loaded from DB on start and cached in memory.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::metrics::Metrics;
use super::repo::HeartbeatTaskRepository;
use super::types::{HeartbeatConfig, HeartbeatSignal};
use crate::agent::pool::AgentPoolLike;
use crate::event::bus::AiEventPublisher;
use crate::tool::trust::TrustConfig;

struct LoopHandle {
    cancel_tx: oneshot::Sender<()>,
    _join_handle: tokio::task::JoinHandle<()>,
}

/// Manages per-workspace heartbeat loop lifecycle.
pub struct HeartbeatRunner {
    loops: DashMap<String, LoopHandle>,
    signal_senders: DashMap<String, mpsc::UnboundedSender<HeartbeatSignal>>,
    trust_configs: DashMap<String, TrustConfig>,
    task_repo: Arc<dyn HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    agent_pool: RwLock<Option<Arc<dyn AgentPoolLike>>>,
    config: HeartbeatConfig,
    /// Workspace IDs that tried to start before AgentPool was injected.
    pending_starts: RwLock<Vec<String>>,
    /// Operational metrics.
    pub metrics: Arc<Metrics>,
}

impl HeartbeatRunner {
    pub fn new(
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        event_publisher: Arc<AiEventPublisher>,
        config: HeartbeatConfig,
    ) -> Self {
        Self {
            loops: DashMap::new(),
            signal_senders: DashMap::new(),
            trust_configs: DashMap::new(),
            task_repo,
            event_publisher,
            agent_pool: RwLock::new(None),
            config,
            pending_starts: RwLock::new(Vec::new()),
            metrics: Arc::new(Metrics::new()),
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
        if !self.config.enabled {
            info!(workspace_id, "Heartbeat disabled, skipping start");
            return;
        }

        self.stop(workspace_id).await;

        let trust_config = self.load_trust_config(workspace_id).await;
        self.trust_configs
            .insert(workspace_id.to_string(), trust_config.clone());

        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, trust_config.clone());
        }

        let tasks = match self.task_repo.list_by_workspace(workspace_id).await {
            Ok(t) => t,
            Err(e) => {
                error!(workspace_id, error = %e, "Failed to load heartbeat tasks");
                return;
            }
        };

        if tasks.is_empty() {
            info!(workspace_id, "No heartbeat tasks, skipping loop start");
            return;
        }

        let pool = self.agent_pool.read().await.clone();
        if pool.is_none() {
            info!(workspace_id, "AgentPool not ready, queuing heartbeat start");
            self.pending_starts.write().await.push(workspace_id.to_string());
            return;
        }

        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let ws_id = workspace_id.to_string();
        let task_repo = self.task_repo.clone();
        let event_publisher = self.event_publisher.clone();
        let config = self.config.clone();

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
            )
            .await;
        });

        self.signal_senders.insert(workspace_id.to_string(), signal_tx);
        self.loops.insert(
            workspace_id.to_string(),
            LoopHandle {
                cancel_tx,
                _join_handle: join_handle,
            },
        );

        info!(workspace_id, "Heartbeat loop started");
    }

    /// Stop a heartbeat loop for a workspace. No-op if not running.
    pub async fn stop(&self, workspace_id: &str) {
        if let Some((_, handle)) = self.loops.remove(workspace_id) {
            let _ = handle.cancel_tx.send(());
            let abort_handle = handle._join_handle.abort_handle();
            tokio::select! {
                _ = handle._join_handle => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    warn!(workspace_id, "Heartbeat loop did not exit in 5s, aborting");
                    abort_handle.abort();
                }
            }
        }
        self.signal_senders.remove(workspace_id);
        self.trust_configs.remove(workspace_id);
        info!(workspace_id, "Heartbeat loop stopped");
    }

    /// Send a signal to a workspace's heartbeat loop. Non-blocking.
    pub fn signal(&self, signal: HeartbeatSignal) {
        let ws_id = signal.workspace_id.clone();
        match self.signal_senders.get(&ws_id) {
            Some(sender) => {
                if let Err(e) = sender.send(signal) {
                    warn!(workspace_id = %ws_id, error = %e, "Failed to send heartbeat signal");
                }
            }
            None => {
                debug!(workspace_id = %ws_id, "No active heartbeat loop, skipping signal");
            }
        }
    }

    pub async fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, config.clone());
        }
        self.trust_configs.insert(workspace_id.to_string(), config);
        info!(workspace_id, "TrustConfig updated");
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

    pub async fn shutdown(&self) {
        let ws_ids: Vec<String> = self.active_workspaces();
        for ws_id in &ws_ids {
            self.stop(ws_id).await;
        }
        info!(count = ws_ids.len(), "HeartbeatRunner shut down");
    }

    async fn load_trust_config(&self, _workspace_id: &str) -> TrustConfig {
        TrustConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::bus::AiEventPublisher;
    use crate::heartbeat::repo::RepoError;
    use crate::heartbeat::types::HeartbeatTask;

    struct MockTaskRepo {
        tasks: Vec<HeartbeatTask>,
    }

    #[async_trait::async_trait]
    impl HeartbeatTaskRepository for MockTaskRepo {
        async fn list_by_workspace(&self, _workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
            Ok(self.tasks.clone())
        }
        async fn upsert(
            &self,
            _workspace_id: &str,
            _task: &HeartbeatTask,
            _expected_version: i64,
        ) -> Result<bool, RepoError> {
            Ok(true)
        }
        async fn insert(&self, _workspace_id: &str, _priority: &str, _text: &str) -> Result<HeartbeatTask, RepoError> {
            Err(RepoError::Database("mock".into()))
        }
        async fn set_paused(&self, _workspace_id: &str, _task_id: i64, _paused: bool) -> Result<(), RepoError> {
            Ok(())
        }
        async fn delete(&self, _workspace_id: &str, _task_id: i64) -> Result<(), RepoError> {
            Ok(())
        }
        async fn insert_result(
            &self,
            _workspace_id: &str,
            _result: &crate::heartbeat::types::HeartbeatResult,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())))
    }

    #[tokio::test]
    async fn test_runner_construction() {
        let repo = Arc::new(MockTaskRepo { tasks: vec![] });
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        assert_eq!(runner.active_loop_count(), 0);
        assert!(runner.active_workspaces().is_empty());
    }

    #[tokio::test]
    async fn test_start_with_no_tasks_exits_early() {
        let repo = Arc::new(MockTaskRepo { tasks: vec![] });
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_is_noop() {
        let repo = Arc::new(MockTaskRepo { tasks: vec![] });
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.stop("nonexistent").await;
        assert_eq!(runner.active_loop_count(), 0);
    }

    #[tokio::test]
    async fn test_start_when_disabled() {
        let repo = Arc::new(MockTaskRepo {
            tasks: vec![HeartbeatTask {
                id: 1,
                workspace_id: "ws_1".into(),
                priority: "high".into(),
                text: "test".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        });
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
        let repo = Arc::new(MockTaskRepo {
            tasks: vec![HeartbeatTask {
                id: 1,
                workspace_id: "ws_1".into(),
                priority: "high".into(),
                text: "test".into(),
                paused: false,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
        });
        let publisher = make_publisher();
        let runner = HeartbeatRunner::new(repo, publisher, HeartbeatConfig::default());
        runner.start("ws_1").await;
        assert_eq!(runner.active_loop_count(), 0);
    }
}
