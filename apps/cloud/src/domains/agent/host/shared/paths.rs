//! Agent-domain filesystem paths (workspace dirs, skills dirs, prompt files).
//!
//! Re-homed from `cloud::shared::paths` in P4-Task22. The project root used to
//! resolve via cloud's `CARGO_MANIFEST_DIR`; inside this crate the default is
//! derived from this crate's manifest dir (crates/agent → crates → repo root),
//! and the composition layer / tests may override it via [`set_project_root`]
//! (or the `TINYIOTHUB__PROJECT_ROOT` env var, as before).

use std::path::PathBuf;
use std::sync::RwLock;

static PROJECT_ROOT_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Override the project root (composition layer at startup, tests with a
/// tempdir). Takes precedence over the env var and manifest-derived default.
pub fn set_project_root(root: PathBuf) {
    *PROJECT_ROOT_OVERRIDE.write().expect("project root lock poisoned") = Some(root);
}

/// Project root: the tinyiothub/ directory.
/// 可通过环境变量 TINYIOTHUB__PROJECT_ROOT 覆盖（Docker 等场景）
pub fn project_root() -> PathBuf {
    if let Some(root) = PROJECT_ROOT_OVERRIDE
        .read()
        .expect("project root lock poisoned")
        .as_ref()
    {
        return root.clone();
    }
    if let Ok(root) = std::env::var("TINYIOTHUB__PROJECT_ROOT") {
        return PathBuf::from(root);
    }
    // crates/agent → crates → repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// Runtime data directory: <project_root>/data/
pub fn api_data_dir() -> PathBuf {
    project_root().join("data")
}

/// Agent workspaces directory: <api_data>/agents/
pub fn agents_base_dir() -> PathBuf {
    api_data_dir().join("agents")
}

/// Shared agent base directory: <agents_base>/_default/
/// Contains default prompt files shared across all workspaces.
pub fn shared_agent_base_dir() -> PathBuf {
    agents_base_dir().join("_default")
}

/// Single workspace directory: <agents_base>/{workspace_id}/
pub fn workspace_dir(workspace_id: &str) -> PathBuf {
    agents_base_dir().join(workspace_id)
}

/// Global skills directory (shared across all workspaces): <api_data>/skills/
pub fn global_skills_dir() -> PathBuf {
    api_data_dir().join("skills")
}

/// Workspace-specific skills directory: <workspace_dir>/skills/
pub fn workspace_skills_dir(workspace_id: &str) -> PathBuf {
    workspace_dir(workspace_id).join("skills")
}

/// Workspace agent-specific skills directory: <workspace_dir>/{agent_id}/skills/
pub fn agent_skills_dir(workspace_id: &str, agent_id: &str) -> PathBuf {
    workspace_dir(workspace_id).join(agent_id).join("skills")
}

/// Heartbeat file within a workspace: <workspace_dir>/HEARTBEAT.md
pub fn heartbeat_file(workspace_id: &str) -> PathBuf {
    workspace_dir(workspace_id).join("HEARTBEAT.md")
}

/// Default workspace ID used when none is specified.
/// Must match the ID created by initialization.rs (`ws-default-001`).
pub const DEFAULT_WORKSPACE_ID: &str = "ws-default-001";

/// Default workspace directory (for config defaults)
pub fn default_workspace_dir() -> PathBuf {
    workspace_dir(DEFAULT_WORKSPACE_ID)
}
