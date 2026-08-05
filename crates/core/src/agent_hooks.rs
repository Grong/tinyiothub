//! Agent hooks — the seam that lets the workspace domain consume agent-owned
//! heartbeat-task knowledge without depending on the agent domain.
//!
//! Workspace needs three agent-owned capabilities: the default heartbeat task
//! set seeded into new workspaces, parsing of the legacy `HEARTBEAT.md` task
//! file, and the one-time file→DB migration of those tasks. This trait is the
//! contract; the agent domain provides the implementation and the composition
//! layer injects it as `Arc<dyn AgentHooks>`.
//!
//! Core guardrail: trait + value types only, no logic here.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A heartbeat task definition crossing the workspace→agent boundary.
/// Mirrors the agent-side task entry (priority/text/paused); server-assigned
/// fields (id, version, timestamps) never cross this seam.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTaskDef {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Agent-provided capabilities consumed by the workspace domain.
#[async_trait::async_trait]
pub trait AgentHooks: Send + Sync {
    /// The default heartbeat task set seeded into a brand-new workspace.
    fn default_heartbeat_tasks(&self) -> Vec<HeartbeatTaskDef>;

    /// Read tasks from the legacy `HEARTBEAT.md` file. Returns the default
    /// set when the file is absent.
    async fn read_legacy_heartbeat_tasks(&self, workspace_dir: &Path) -> Result<Vec<HeartbeatTaskDef>, String>;

    /// One-time import of legacy `HEARTBEAT.md` tasks into the DB. No-op
    /// when the DB already has tasks for the workspace or the file is
    /// absent. Returns true when a migration happened.
    async fn migrate_legacy_heartbeat_tasks(&self, workspace_id: &str, workspace_dir: &Path) -> Result<bool, String>;
}
