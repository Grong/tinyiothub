// 留 cloud（计划裁定：组合层工具实现；经 ToolRegistry provider 注入，Task 14）
// GetSkillTool — on-demand skill loading for AI agents
//
// Instead of injecting all ~10KB of skill content into every system prompt,
// this tool lets the LLM load specific skill files only when needed.
// The system prompt carries a compact skill index (name + one-line description);
// the full content is fetched here from data/skills/<name>.md.

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

pub struct GetSkillTool;

impl Attributable for GetSkillTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetSkillTool {
    fn name(&self) -> &str {
        "get_skill"
    }

    fn description(&self) -> &str {
        "Load a skill's full instructions on demand. \
         When a skill matches the current task, calling this is a BLOCKING REQUIREMENT — \
         you MUST invoke it BEFORE taking any other action. \
         The system prompt carries a skill index (name + one-line description); \
         call this with a skill name to get the complete step-by-step workflow."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "Skill file name without extension, as listed in the system prompt skill index (e.g. 'workspace')."
                }
            },
            "required": ["skill_name"],
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let skill_name = args.get("skill_name").and_then(|v| v.as_str()).unwrap_or("");

        if skill_name.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "skill_name is required. Available skills: {}",
                    available_skills()
                )),
            });
        }

        // Sanitize: only allow alphanumeric, hyphens, underscores
        if !skill_name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Invalid skill_name: '{}'. Use only letters, digits, hyphens, underscores.",
                    skill_name
                )),
            });
        }

        // Prevent path traversal
        if skill_name.contains("..") || skill_name.contains('/') || skill_name.contains('\\') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Invalid skill_name: '{}'", skill_name)),
            });
        }

        let path = skill_dir().join(format!("{}.md", skill_name));

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Skill '{}' is empty", skill_name)),
                    });
                }
                Ok(ToolResult {
                    success: true,
                    output: content,
                    error: None,
                })
            }
            Err(_) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Skill '{}' not found. Available skills: {}",
                    skill_name,
                    available_skills()
                )),
            }),
        }
    }
}

/// List skill file stems present in `data/skills/`, comma-separated.
fn available_skills() -> String {
    let mut names: Vec<String> = std::fs::read_dir(skill_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().is_some_and(|ext| ext == "md") {
                p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

/// Resolve the `data/skills` directory robustly across dev/test/production layouts.
///
/// Tries, in order:
/// 1. `./data/skills` relative to the current working directory (dev default).
/// 2. The project-root `data/skills` derived from this crate's compile-time manifest dir.
/// 3. `../../data/skills` relative to the running executable (release binaries under `target/`).
fn skill_dir() -> std::path::PathBuf {
    let candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("data/skills"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/skills"),
        std::env::current_exe()
            .map(|exe| exe.parent().map(|p| p.join("../../data/skills")).unwrap_or_default())
            .unwrap_or_default(),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| std::path::PathBuf::from("data/skills"))
}
