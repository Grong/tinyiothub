// HeartbeatManager — per-workspace heartbeat loop lifecycle management
//
// Manages independent heartbeat_loop tasks for each workspace via DashMap channels.
// Replaces the old single-global HeartbeatService + AutonomousAgentRunner split.

use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_storage::cache::DeviceCache;
use tokio::sync::{mpsc, oneshot};

use super::{action_repo::AgentActionRepository, agent::AgentPool};

/// Priority level for a WakeSignal
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WakePriority {
    Normal = 0,
    High = 1,
    Critical = 2,
}

/// Signal sent to wake a specific workspace's heartbeat loop
#[derive(Debug, Clone)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub reason: String,
    pub context: String,
    pub priority: WakePriority,
}

impl WakeSignal {
    pub fn priority_label(&self) -> &str {
        match self.priority {
            WakePriority::Critical => "CRITICAL",
            WakePriority::High => "HIGH",
            WakePriority::Normal => "NORMAL",
        }
    }
}

/// Heartbeat configuration
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub max_recent_actions: usize,
    pub channel_size: usize,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self { enabled: true, interval_minutes: 15, max_recent_actions: 10, channel_size: 64 }
    }
}

/// Manages per-workspace heartbeat loops
pub struct HeartbeatManager {
    channels: DashMap<String, (mpsc::Sender<WakeSignal>, oneshot::Sender<()>)>,
    handles: DashMap<String, tokio::task::JoinHandle<()>>,
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    #[allow(dead_code)]
    device_cache: Arc<DeviceCache>,
    config: tokio::sync::RwLock<HeartbeatConfig>,
}

impl HeartbeatManager {
    pub fn new(
        agent_pool: Arc<AgentPool>,
        action_repo: Arc<dyn AgentActionRepository>,
        device_cache: Arc<DeviceCache>,
        config: HeartbeatConfig,
    ) -> Self {
        Self {
            channels: DashMap::new(),
            handles: DashMap::new(),
            agent_pool,
            action_repo,
            device_cache,
            config: tokio::sync::RwLock::new(config),
        }
    }

    /// Start a heartbeat loop for a workspace. Idempotent: cleans up any existing entry first.
    pub async fn start(&self, workspace_id: &str) {
        // Extract config values before any .await to avoid holding the read lock across await points
        let (enabled, channel_size, interval_minutes, max_recent_actions) = {
            let config = self.config.read().await;
            (
                config.enabled,
                config.channel_size,
                config.interval_minutes,
                config.max_recent_actions,
            )
        };

        if !enabled {
            tracing::debug!(%workspace_id, "Heartbeat disabled globally, skipping start");
            return;
        }

        // Reject zero-interval to prevent infinite hot loop hammering the LLM
        let interval_minutes = if interval_minutes == 0 {
            tracing::error!(%workspace_id, "interval_minutes=0 rejected — clamping to 1 to prevent busy loop");
            1u32
        } else {
            interval_minutes
        };

        // Clean up any existing entry (idempotent)
        self.stop(workspace_id).await;

        let (wake_tx, wake_rx) = mpsc::channel::<WakeSignal>(channel_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        self.channels.insert(workspace_id.to_string(), (wake_tx, shutdown_tx));

        let ws_id = workspace_id.to_string();
        let agent_pool = Arc::clone(&self.agent_pool);
        let action_repo = Arc::clone(&self.action_repo);
        let heartbeat_file = crate::shared::paths::heartbeat_file(&ws_id);

        let config =
            HeartbeatConfig { enabled, interval_minutes, max_recent_actions, channel_size };

        // Ensure HEARTBEAT.md exists
        if let Err(e) = ensure_heartbeat_file(&heartbeat_file).await {
            tracing::warn!(%workspace_id, "Failed to create HEARTBEAT.md: {}", e);
        }

        let handle = tokio::spawn(async move {
            super::heartbeat::heartbeat_loop(
                ws_id,
                config,
                agent_pool,
                action_repo,
                heartbeat_file,
                wake_rx,
                shutdown_rx,
            )
            .await;
        });

        self.handles.insert(workspace_id.to_string(), handle);

        tracing::info!(%workspace_id, "Heartbeat loop started");
    }

    /// Stop a workspace's heartbeat loop via oneshot signal
    pub async fn stop(&self, workspace_id: &str) {
        // Send shutdown signal
        if let Some((_, (_, shutdown_tx))) = self.channels.remove(workspace_id) {
            let _ = shutdown_tx.send(());
        }

        // Wait for the handle to finish
        if let Some((_, handle)) = self.handles.remove(workspace_id) {
            let abort_handle = handle.abort_handle();
            // Give the loop a moment to exit gracefully, then abort if stuck
            tokio::select! {
                _ = handle => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    tracing::warn!(%workspace_id, "Heartbeat loop did not exit in 5s, aborting");
                    abort_handle.abort();
                }
            }
        }

        tracing::info!(%workspace_id, "Heartbeat loop stopped");
    }

    /// Wake a workspace's heartbeat loop. Non-blocking: dropped if channel is full
    /// or the workspace has no active loop.
    pub fn wake(&self, workspace_id: &str, signal: WakeSignal) {
        match self.channels.get(workspace_id) {
            Some(entry) => {
                let (tx, _) = entry.value();
                match tx.try_send(signal) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::warn!(
                            %workspace_id,
                            "WakeSignal channel full, dropping signal"
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        tracing::debug!(
                            %workspace_id,
                            "WakeSignal channel closed, workspace may have been stopped"
                        );
                    }
                }
            }
            None => {
                tracing::debug!(
                    %workspace_id,
                    "No active heartbeat loop for workspace, skipping wake"
                );
            }
        }
    }

    /// List all workspace IDs with active heartbeat loops
    pub fn list_active(&self) -> Vec<String> {
        self.channels.iter().map(|e| e.key().clone()).collect()
    }

    /// Return a copy of the current heartbeat config
    pub async fn config(&self) -> HeartbeatConfig {
        self.config.read().await.clone()
    }

    /// Update the global heartbeat config and optionally restart all loops
    pub async fn update_config(
        &self,
        enabled: Option<bool>,
        interval_minutes: Option<u32>,
    ) -> HeartbeatConfig {
        let mut c = self.config.write().await;
        if let Some(e) = enabled {
            c.enabled = e;
        }
        match interval_minutes {
            Some(0) => {
                tracing::error!("Rejected interval_minutes=0 — would create infinite hot loop");
            }
            Some(i) => {
                c.interval_minutes = i;
            }
            None => {}
        }
        c.clone()
    }

    /// Restart a workspace's heartbeat loop with the current config
    pub async fn restart(&self, workspace_id: &str) {
        self.stop(workspace_id).await;
        self.start(workspace_id).await;
    }

    /// Shut down all heartbeat loops
    pub async fn shutdown(&self) {
        let ws_ids: Vec<String> = self.list_active();
        for ws_id in &ws_ids {
            self.stop(ws_id).await;
        }
        tracing::info!("HeartbeatManager shut down ({} loops)", ws_ids.len());
    }
}

async fn ensure_heartbeat_file(path: &std::path::Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = super::heartbeat::build_heartbeat_md(&super::heartbeat::get_default_tasks());
    tokio::fs::write(path, content).await?;
    Ok(())
}
