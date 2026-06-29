//! PatrolManager — per-workspace patrol loop lifecycle manager.
//!
//! Owns a DashMap of cancel channels and handles. Start/stop are idempotent.
//! TrustConfig is loaded from DB on start and cached in memory.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use super::repo::HeartbeatTaskRepository;
use super::types::{HeartbeatConfig, TrustConfig, WakeSignal};
use crate::agent::pool::AgentPoolLike;
use crate::event::bus::AiEventPublisher;

struct PatrolLoopHandle {
    cancel_tx: oneshot::Sender<()>,
    _join_handle: tokio::task::JoinHandle<()>,
}

/// Manages per-workspace patrol loop lifecycle.
pub struct PatrolManager {
    loops: DashMap<String, PatrolLoopHandle>,
    wake_senders: DashMap<String, mpsc::UnboundedSender<WakeSignal>>,
    trust_configs: DashMap<String, TrustConfig>,
    task_repo: Arc<dyn HeartbeatTaskRepository>,
    _event_publisher: Arc<AiEventPublisher>,
    agent_pool: RwLock<Option<Arc<dyn AgentPoolLike>>>,
    config: HeartbeatConfig,
}

impl PatrolManager {
    pub fn new(
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        _event_publisher: Arc<AiEventPublisher>,
        config: HeartbeatConfig,
    ) -> Self {
        Self {
            loops: DashMap::new(),
            wake_senders: DashMap::new(),
            trust_configs: DashMap::new(),
            task_repo,
            _event_publisher,
            agent_pool: RwLock::new(None),
            config,
        }
    }

    /// Set the agent pool — called once during AiSystem assembly (late binding).
    pub fn set_agent_pool(&self, pool: Arc<dyn AgentPoolLike>) {
        let mut guard = self.agent_pool.blocking_write();
        *guard = Some(pool);
    }

    /// Start a patrol loop for a workspace. Idempotent — stops existing loop first.
    pub async fn start(&self, workspace_id: &str) {
        if !self.config.enabled {
            info!(workspace_id, "Patrol disabled, skipping start");
            return;
        }

        self.stop(workspace_id).await;

        // Load TrustConfig from DB (returns default if not configured)
        let trust_config = self.load_trust_config(workspace_id).await;
        self.trust_configs
            .insert(workspace_id.to_string(), trust_config.clone());

        // Sync trust config to agent pool so tools are wrapped appropriately
        if let Some(pool) = self.agent_pool.read().await.as_ref() {
            pool.set_trust_config(workspace_id, trust_config.clone());
        }

        // Load heartbeat tasks from DB
        let tasks = match self.task_repo.list_by_workspace(workspace_id).await {
            Ok(t) => t,
            Err(e) => {
                error!(workspace_id, error = %e, "Failed to load heartbeat tasks");
                return;
            }
        };

        if tasks.is_empty() {
            info!(workspace_id, "No heartbeat tasks, skipping patrol loop start");
            return;
        }

        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let ws_id = workspace_id.to_string();
        let pool = self.agent_pool.read().await.clone();
        let task_repo = self.task_repo.clone();
        let _event_publisher = self._event_publisher.clone();
        let config = self.config.clone();

        let join_handle = tokio::spawn(async move {
            // NOTE: super::loop_::patrol_loop doesn't exist yet (Task 5).
            // For now, log and exit. Task 5 will wire the real loop.
            let _ = (
                ws_id,
                tasks,
                trust_config,
                pool,
                task_repo,
                _event_publisher,
                config,
                wake_rx,
                cancel_rx,
            );
            tracing::debug!("Patrol loop task spawned (stub — real logic in Task 5)");
        });

        self.wake_senders
            .insert(workspace_id.to_string(), wake_tx);
        self.loops.insert(
            workspace_id.to_string(),
            PatrolLoopHandle {
                cancel_tx,
                _join_handle: join_handle,
            },
        );

        info!(workspace_id, "Patrol loop started");
    }

    /// Stop a patrol loop for a workspace. No-op if not running.
    pub async fn stop(&self, workspace_id: &str) {
        if let Some((_, handle)) = self.loops.remove(workspace_id) {
            let _ = handle.cancel_tx.send(());
            let abort_handle = handle._join_handle.abort_handle();
            tokio::select! {
                _ = handle._join_handle => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    warn!(workspace_id, "Patrol loop did not exit in 5s, aborting");
                    abort_handle.abort();
                }
            }
        }
        self.wake_senders.remove(workspace_id);
        self.trust_configs.remove(workspace_id);
        info!(workspace_id, "Patrol loop stopped");
    }

    /// Wake a workspace's patrol loop with a deduplicated signal.
    /// Non-blocking: dropped if channel is full or workspace has no active loop.
    pub fn wake(&self, signal: WakeSignal) {
        let ws_id = signal.workspace_id.clone();
        match self.wake_senders.get(&ws_id) {
            Some(sender) => {
                if let Err(e) = sender.send(signal) {
                    warn!(
                        workspace_id = %ws_id,
                        error = %e,
                        "Failed to send wake signal"
                    );
                }
            }
            None => {
                debug!(
                    workspace_id = %ws_id,
                    "No active patrol loop, skipping wake"
                );
            }
        }
    }

    /// Update TrustConfig for a workspace (cached in memory).
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        // Sync to agent pool so tools get the new config
        if let Some(pool) = self.agent_pool.blocking_read().as_ref() {
            pool.set_trust_config(workspace_id, config.clone());
        }
        self.trust_configs.insert(workspace_id.to_string(), config);
        info!(workspace_id, "TrustConfig updated");
    }

    /// Get cached TrustConfig for a workspace.
    pub fn get_trust_config(&self, workspace_id: &str) -> Option<TrustConfig> {
        self.trust_configs
            .get(workspace_id)
            .map(|r| r.value().clone())
    }

    /// Number of active patrol loops.
    pub fn active_loop_count(&self) -> usize {
        self.loops.len()
    }

    /// List workspace IDs with active loops.
    pub fn active_workspaces(&self) -> Vec<String> {
        self.loops.iter().map(|r| r.key().clone()).collect()
    }

    /// Shut down all patrol loops.
    pub async fn shutdown(&self) {
        let ws_ids: Vec<String> = self.active_workspaces();
        for ws_id in &ws_ids {
            self.stop(ws_id).await;
        }
        info!(count = ws_ids.len(), "PatrolManager shut down");
    }

    async fn load_trust_config(&self, _workspace_id: &str) -> TrustConfig {
        // Query workspace_settings for the heartbeat_trust_config JSON column.
        // For now, return default. Full DB query wired in cloud integration task (Task 12).
        TrustConfig::default()
    }
}
