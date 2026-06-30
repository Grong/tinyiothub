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
        "Load detailed skill/guide content on demand. \
         Use this when you need step-by-step instructions for a specific workflow \
         (alarm diagnosis, device troubleshooting, heartbeat patrol, driver testing, \
         job scheduling). The system prompt only carries a skill index — call this \
         to get the full workflow details for a given skill."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "Skill file name without extension, e.g. 'alarm-management', 'troubleshooting', 'heartbeat-monitor', 'device-management', 'driver-management', 'job-management'"
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
                error: Some("skill_name is required. Use one of: alarm-management, device-management, driver-management, heartbeat-monitor, job-management, troubleshooting".into()),
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

        let path = std::path::PathBuf::from("data/skills").join(format!("{}.md", skill_name));

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Skill '{}' is empty", skill_name)),
                    });
                }
                Ok(ToolResult { success: true, output: content, error: None })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{}' not found: {}", skill_name, e)),
            }),
        }
    }
}
