// Workspace Scaffold Service - Initializes agent workspace with template files
//
// When a new workspace is created, this service copies template files from
// templates/agent/ to the workspace directory. Existing files are preserved
// to maintain user modifications.

use std::path::Path;
use anyhow::Result;

/// Template files to be copied to workspace
const WORKSPACE_TEMPLATE_FILES: &[(&str, &str)] = &[
    ("IDENTITY.md", include_str!("../../../templates/agent/IDENTITY.md")),
    ("SOUL.md", include_str!("../../../templates/agent/SOUL.md")),
    ("AGENTS.md", include_str!("../../../templates/agent/AGENTS.md")),
    ("USER.md", include_str!("../../../templates/agent/USER.md")),
    ("TOOLS.md", include_str!("../../../templates/agent/TOOLS.md")),
    ("MEMORY.md", include_str!("../../../templates/agent/MEMORY.md")),
    ("HEARTBEAT.md", include_str!("../../../templates/agent/HEARTBEAT.md")),
    ("BOOTSTRAP.md", include_str!("../../../templates/agent/BOOTSTRAP.md")),
];

/// Subdirectories to create in workspace
const WORKSPACE_SUBDIRS: &[&str] = &["sessions", "memory", "state", "cron", "skills"];

/// Scaffold a workspace by creating subdirectories and copying template files.
///
/// Files already existing in the workspace are preserved (not overwritten).
/// This ensures user modifications are never lost during upgrades.
pub async fn scaffold_workspace(workspace_dir: &Path) -> Result<WorkspaceScaffoldResult> {
    let mut created_files = 0;
    let mut skipped_files = 0;
    let mut created_dirs = 0;

    // Create subdirectories
    for subdir in WORKSPACE_SUBDIRS {
        let dir_path = workspace_dir.join(subdir);
        if !dir_path.exists() {
            tokio::fs::create_dir_all(&dir_path).await?;
            created_dirs += 1;
        }
    }

    // Copy template files (skip if exists)
    for (filename, _content) in WORKSPACE_TEMPLATE_FILES {
        let file_path = workspace_dir.join(filename);
        if !file_path.exists() {
            // Get content from embedded template
            let content = get_template_content(filename);
            if let Some(content) = content {
                tokio::fs::write(&file_path, content).await?;
                created_files += 1;
            }
        } else {
            skipped_files += 1;
        }
    }

    Ok(WorkspaceScaffoldResult {
        created_files,
        skipped_files,
        created_dirs,
    })
}

/// Get template content by filename
fn get_template_content(filename: &str) -> Option<&'static str> {
    WORKSPACE_TEMPLATE_FILES
        .iter()
        .find(|(name, _)| *name == filename)
        .map(|(_, content)| *content)
}

/// Result of workspace scaffolding
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
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scaffold_creates_files_and_dirs() {
        let dir = tempdir().unwrap();
        let workspace_dir = dir.path();

        let result = scaffold_workspace(workspace_dir).await.unwrap();

        // Check subdirectories were created
        for subdir in WORKSPACE_SUBDIRS {
            assert!(workspace_dir.join(subdir).exists(), "subdir {} should exist", subdir);
        }

        // Check template files were created
        for (filename, _) in WORKSPACE_TEMPLATE_FILES {
            assert!(
                workspace_dir.join(filename).exists(),
                "template file {} should exist",
                filename
            );
        }

        assert_eq!(result.created_files, WORKSPACE_TEMPLATE_FILES.len());
        assert_eq!(result.created_dirs, WORKSPACE_SUBDIRS.len());
    }

    #[tokio::test]
    async fn test_scaffold_preserves_existing_files() {
        let dir = tempdir().unwrap();
        let workspace_dir = dir.path();

        // Create workspace first time
        let result1 = scaffold_workspace(workspace_dir).await.unwrap();
        assert_eq!(result1.created_files, WORKSPACE_TEMPLATE_FILES.len());

        // Modify a file
        let target_file = workspace_dir.join("IDENTITY.md");
        tokio::fs::write(&target_file, "Custom content").await.unwrap();

        // Scaffold again
        let result2 = scaffold_workspace(workspace_dir).await.unwrap();
        assert_eq!(result2.skipped_files, WORKSPACE_TEMPLATE_FILES.len());
        assert_eq!(result2.created_files, 0);

        // Verify content is preserved
        let content = tokio::fs::read_to_string(&target_file).await.unwrap();
        assert_eq!(content, "Custom content");
    }
}
