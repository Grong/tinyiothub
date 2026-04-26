// Shared Path Constants for TinyIoTHub
//
// All filesystem paths used across the application are centralized here.
// This ensures consistency and makes path changes easy to maintain.
//
// Paths are relative to the project root (CARGO_MANIFEST_DIR/..)
// unless otherwise specified.

use std::path::PathBuf;

/// Project root: the tinyiothub/ directory (parent of api/)
pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// API data directory: <project_root>/api/data/
pub fn api_data_dir() -> PathBuf {
    project_root().join("api/data")
}

/// Agent workspaces directory: <api_data>/agents/
pub fn agents_base_dir() -> PathBuf {
    api_data_dir().join("agents")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_dir_construction() {
        let ws_dir = workspace_dir("my-workspace");
        assert!(ws_dir.to_str().unwrap().ends_with("agents/my-workspace"));
    }

    #[test]
    fn test_global_skills_dir() {
        let dir = global_skills_dir();
        assert!(dir.to_str().unwrap().ends_with("api/data/skills"));
    }

    #[test]
    fn test_workspace_skills_dir() {
        let dir = workspace_skills_dir("ws1");
        assert!(dir.to_str().unwrap().ends_with("agents/ws1/skills"));
    }

    #[test]
    fn test_heartbeat_file() {
        let file = heartbeat_file("ws1");
        assert!(file.to_str().unwrap().ends_with("agents/ws1/HEARTBEAT.md"));
    }
}
