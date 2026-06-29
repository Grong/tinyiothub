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
    /// Dedup key fields: signals with same (device_id, alarm_type) are merged.
    /// `None` means the signal has no per-device dedup key (always kept).
    pub device_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
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

/// Per-tool trust level — controls what AI can do autonomously
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrustLevel {
    /// AI can execute without asking
    #[serde(rename = "full_auto")]
    FullAuto,
    /// Execute but record for audit
    #[serde(rename = "auto_with_log")]
    AutoWithLog,
    /// AI must propose as pending — human approval required
    #[serde(rename = "approval_required")]
    ApprovalRequired,
    /// Tool completely disabled
    #[serde(rename = "disabled")]
    Disabled,
}

impl Default for TrustLevel {
    fn default() -> Self {
        TrustLevel::ApprovalRequired
    }
}

impl TrustLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustLevel::FullAuto => "full_auto",
            TrustLevel::AutoWithLog => "auto_with_log",
            TrustLevel::ApprovalRequired => "approval_required",
            TrustLevel::Disabled => "disabled",
        }
    }
}

/// Per-workspace trust configuration: (tool_name → device_id → trust_level)
/// `device_id = "*"` is the wildcard default for all devices.
pub type TrustConfig =
    std::collections::HashMap<String, std::collections::HashMap<String, TrustLevel>>;

/// Look up the effective trust level for a (tool, device_id) pair.
/// Priority: device-specific > "*" wildcard > ApprovalRequired default.
pub fn resolve_trust(config: &TrustConfig, tool_name: &str, device_id: &str) -> TrustLevel {
    if let Some(devices) = config.get(tool_name) {
        // device-specific override
        if let Some(level) = devices.get(device_id) {
            return *level;
        }
        // wildcard fallback
        if let Some(level) = devices.get("*") {
            return *level;
        }
    }
    TrustLevel::default()
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
    /// Per-workspace last tick instant — prevents overlapping or too-frequent ticks
    /// even if duplicate loops are accidentally spawned.
    pub last_ticks: Arc<DashMap<String, tokio::time::Instant>>,
    /// Per-workspace trust configs
    pub trust_configs: DashMap<String, TrustConfig>,
    /// Per-workspace heartbeat config overrides (interval, enabled, etc.)
    pub workspace_configs: DashMap<String, HeartbeatConfig>,
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
            last_ticks: Arc::new(DashMap::new()),
            trust_configs: DashMap::new(),
            workspace_configs: DashMap::new(),
        }
    }

    /// Set per-workspace config override (interval, enabled, etc.)
    pub fn set_workspace_config(&self, workspace_id: &str, config: HeartbeatConfig) {
        self.workspace_configs.insert(workspace_id.to_string(), config);
    }

    /// Get effective config for a workspace: per-workspace override → global → built-in default
    fn effective_config(&self, workspace_id: &str) -> HeartbeatConfig {
        self.workspace_configs
            .get(workspace_id)
            .map(|entry| entry.value().clone())
            .or_else(|| self.config.try_read().map(|c| c.clone()).ok())
            .unwrap_or_default()
    }

    /// Get the trust config for a workspace (empty default if not configured)
    pub fn get_trust_config(&self, workspace_id: &str) -> TrustConfig {
        self.trust_configs.get(workspace_id).map(|entry| entry.value().clone()).unwrap_or_default()
    }

    /// Update trust config for a workspace
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.agent_pool.set_trust_config(workspace_id, config.clone());
        self.trust_configs.insert(workspace_id.to_string(), config);
    }

    /// Start a heartbeat loop for a workspace. Idempotent: cleans up any existing entry first.
    pub async fn start(&self, workspace_id: &str) {
        let mut config = self.effective_config(workspace_id);

        if !config.enabled {
            tracing::debug!(%workspace_id, "Heartbeat disabled, skipping start");
            return;
        }

        // Reject zero-interval to prevent infinite hot loop hammering the LLM
        if config.interval_minutes == 0 {
            tracing::error!(%workspace_id, "interval_minutes=0 rejected — clamping to 1 to prevent busy loop");
            config.interval_minutes = 1;
        }

        tracing::info!(%workspace_id, interval_minutes = config.interval_minutes, "Heartbeat loop starting with effective config");

        // Sync trust config to AgentPool so tools get wrapped with TrustAwareTool
        if let Some(tc) = self.trust_configs.get(workspace_id) {
            self.agent_pool.set_trust_config(workspace_id, tc.value().clone());
        }

        // Clean up any existing entry (idempotent)
        self.stop(workspace_id).await;

        let (wake_tx, wake_rx) = mpsc::channel::<WakeSignal>(config.channel_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        self.channels.insert(workspace_id.to_string(), (wake_tx, shutdown_tx));

        let ws_id = workspace_id.to_string();
        let agent_pool = Arc::clone(&self.agent_pool);
        let action_repo = Arc::clone(&self.action_repo);
        let heartbeat_file = crate::shared::paths::heartbeat_file(&ws_id);
        let last_ticks = Arc::clone(&self.last_ticks);

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
                last_ticks,
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

        // Clean up rate-limit state
        self.last_ticks.remove(workspace_id);

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

    /// Start periodic cleanup (runs every 6 hours):
    ///   - Deletes agent_actions older than 90 days
    ///   - Evicts idle agents (30min+ unused) from AgentPool
    pub fn start_retention_task(self: &Arc<Self>) {
        let hm = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(6 * 3600));
            loop {
                interval.tick().await;
                let cutoff = chrono::Utc::now() - chrono::Duration::days(90);
                match hm.action_repo.delete_old(cutoff).await {
                    Ok(count) if count > 0 => {
                        tracing::info!(count, "Retention: deleted old agent_actions");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("Retention cleanup failed: {}", e);
                    }
                }
                // Evict idle agents from pool (30min+ inactivity)
                let removed = hm.agent_pool.cleanup_idle();
                if removed > 0 {
                    tracing::info!(removed, "Evicted idle agents from pool");
                }
            }
        });
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
