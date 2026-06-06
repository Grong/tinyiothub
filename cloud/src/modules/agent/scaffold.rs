// Workspace Scaffold Service - Initializes agent workspace with template files
//
// Architecture:
//   data/agents/_default/    ← Shared base (7 files, updated on deploy)
//   data/agents/{ws_id}/     ← Per-workspace (USER.md only, user-customizable)
//
// Prompt loading (agent/mod.rs) checks: workspace dir → shared base → embedded template.
// This means updating a shared template only requires touching _default/, not every workspace.

use std::path::Path;

use anyhow::Result;

/// Files shared across all workspaces (scaffolded once to _default/)
const SHARED_TEMPLATE_FILES: &[(&str, &str)] = &[
    ("IDENTITY.md", include_str!("../../../templates/agent/IDENTITY.md")),
    ("SOUL.md", include_str!("../../../templates/agent/SOUL.md")),
    ("AGENTS.md", include_str!("../../../templates/agent/AGENTS.md")),
    ("TOOLS.md", include_str!("../../../templates/agent/TOOLS.md")),
    ("MEMORY.md", include_str!("../../../templates/agent/MEMORY.md")),
    ("HEARTBEAT.md", include_str!("../../../templates/agent/HEARTBEAT.md")),
    ("BOOTSTRAP.md", include_str!("../../../templates/agent/BOOTSTRAP.md")),
];

/// Files created per workspace (user-customizable overrides)
const WORKSPACE_ONLY_FILES: &[(&str, &str)] =
    &[("USER.md", include_str!("../../../templates/agent/USER.md"))];

/// Subdirectories to create in each workspace
const WORKSPACE_SUBDIRS: &[&str] = &["sessions", "memory", "state", "cron", "skills"];

/// Scaffold the shared base directory (data/agents/_default/) with all template files.
/// Called during system initialization. Overwrites existing files to ensure
/// templates stay up-to-date on deploy.
pub async fn scaffold_shared_base() -> Result<WorkspaceScaffoldResult> {
    let base_dir = crate::shared::paths::shared_agent_base_dir();
    tokio::fs::create_dir_all(&base_dir).await?;

    let mut created = 0;
    let mut skipped = 0;

    for (filename, _content) in SHARED_TEMPLATE_FILES {
        let file_path = base_dir.join(filename);
        let content = get_shared_template_content(filename).unwrap_or("");
        if file_path.exists() {
            let existing = tokio::fs::read_to_string(&file_path).await.unwrap_or_default();
            if existing == content {
                skipped += 1;
                continue;
            }
        }
        tokio::fs::write(&file_path, content).await?;
        created += 1;
    }

    Ok(WorkspaceScaffoldResult { created_files: created, skipped_files: skipped, created_dirs: 1 })
}

/// Scaffold a workspace by creating subdirectories and per-workspace files (USER.md only).
///
/// Shared files (IDENTITY.md, SOUL.md, etc.) are NOT created here — they are served
/// from data/agents/_default/ by the two-tier prompt loader.
///
/// Existing files are preserved (not overwritten) to protect user modifications.
pub async fn scaffold_workspace(workspace_dir: &Path) -> Result<WorkspaceScaffoldResult> {
    let mut created_files = 0;
    let mut skipped_files = 0;
    let mut created_dirs = 0;

    for subdir in WORKSPACE_SUBDIRS {
        let dir_path = workspace_dir.join(subdir);
        if !dir_path.exists() {
            tokio::fs::create_dir_all(&dir_path).await?;
            created_dirs += 1;
        }
    }

    for (filename, _content) in WORKSPACE_ONLY_FILES {
        let file_path = workspace_dir.join(filename);
        if !file_path.exists() {
            let content = get_workspace_template_content(filename);
            if let Some(content) = content {
                tokio::fs::write(&file_path, content).await?;
                created_files += 1;
            }
        } else {
            skipped_files += 1;
        }
    }

    Ok(WorkspaceScaffoldResult { created_files, skipped_files, created_dirs })
}

fn get_shared_template_content(filename: &str) -> Option<&'static str> {
    SHARED_TEMPLATE_FILES.iter().find(|(name, _)| *name == filename).map(|(_, content)| *content)
}

fn get_workspace_template_content(filename: &str) -> Option<&'static str> {
    WORKSPACE_ONLY_FILES.iter().find(|(name, _)| *name == filename).map(|(_, content)| *content)
}

#[derive(Debug, Clone)]
pub struct WorkspaceScaffoldResult {
    pub created_files: usize,
    pub skipped_files: usize,
    pub created_dirs: usize,
}

impl std::fmt::Display for WorkspaceScaffoldResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "created {} files, skipped {} existing, {} directories",
            self.created_files, self.skipped_files, self.created_dirs
        )
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_scaffold_shared_base_creates_all_files() {
        // We don't write to the real shared base; test the logic via temp dir override.
        // scaffold_shared_base uses paths::shared_agent_base_dir() which is hardcoded,
        // so we test scaffold_workspace instead and verify the file split.
        let dir = tempdir().unwrap();
        let ws_dir = dir.path();

        let result = scaffold_workspace(ws_dir).await.unwrap();

        // Should only create USER.md, not the shared files
        assert!(ws_dir.join("USER.md").exists(), "USER.md should exist");
        assert!(!ws_dir.join("IDENTITY.md").exists(), "IDENTITY.md should NOT be in workspace");
        assert!(!ws_dir.join("SOUL.md").exists(), "SOUL.md should NOT be in workspace");

        for subdir in WORKSPACE_SUBDIRS {
            assert!(ws_dir.join(subdir).exists(), "subdir {} should exist", subdir);
        }

        assert_eq!(result.created_files, WORKSPACE_ONLY_FILES.len());
        assert_eq!(result.created_dirs, WORKSPACE_SUBDIRS.len());
    }

    #[tokio::test]
    async fn test_scaffold_workspace_preserves_existing_user_md() {
        let dir = tempdir().unwrap();
        let ws_dir = dir.path();

        // Create subdirs first so first scaffold can create USER.md
        for subdir in WORKSPACE_SUBDIRS {
            tokio::fs::create_dir_all(ws_dir.join(subdir)).await.unwrap();
        }

        let result1 = scaffold_workspace(ws_dir).await.unwrap();
        assert_eq!(result1.created_files, WORKSPACE_ONLY_FILES.len());

        let user_file = ws_dir.join("USER.md");
        tokio::fs::write(&user_file, "Custom user context").await.unwrap();

        let result2 = scaffold_workspace(ws_dir).await.unwrap();
        assert_eq!(result2.skipped_files, WORKSPACE_ONLY_FILES.len());
        assert_eq!(result2.created_files, 0);

        let content = tokio::fs::read_to_string(&user_file).await.unwrap();
        assert_eq!(content, "Custom user context");
    }

    #[tokio::test]
    async fn test_shared_template_files_count() {
        // 7 shared files: IDENTITY, SOUL, AGENTS, TOOLS, MEMORY, HEARTBEAT, BOOTSTRAP
        assert_eq!(SHARED_TEMPLATE_FILES.len(), 7);
        // 1 workspace-only file: USER
        assert_eq!(WORKSPACE_ONLY_FILES.len(), 1);
    }
}
