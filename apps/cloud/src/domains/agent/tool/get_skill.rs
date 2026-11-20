// 组合层工具实现 —— 经 ToolRegistry provider 注入（Task 14）
// GetSkillTool — on-demand skill loading for AI agents
//
// Instead of injecting all ~10KB of skill content into every system prompt,
// this tool lets the LLM load specific skill files only when needed.
// The system prompt carries a compact skill index (name + one-line description);
// the full content is fetched from workspace skills, then global data/skills.

use async_trait::async_trait;
use std::sync::Arc;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_skills::loader::parse_skill;

use crate::domains::agent::prompt::paths;
use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};

pub struct GetSkillTool {
    workspace_id: String,
    agent_id: Option<String>,
    contexts: Arc<TurnContextRegistry>,
    events: Option<Arc<dyn SessionEventSink>>,
}

impl GetSkillTool {
    pub fn new(
        workspace_id: String,
        agent_id: Option<String>,
        contexts: Arc<TurnContextRegistry>,
        events: Arc<dyn SessionEventSink>,
    ) -> Self {
        Self {
            workspace_id,
            agent_id,
            contexts,
            events: Some(events),
        }
    }

    fn skill_dirs(&self) -> [std::path::PathBuf; 2] {
        [
            paths::workspace_skills_dir(&self.workspace_id),
            paths::global_skills_dir(),
        ]
    }

    fn read_skill(&self, skill_name: &str) -> Option<String> {
        self.skill_dirs()
            .into_iter()
            .map(|dir| dir.join(format!("{skill_name}.md")))
            .find_map(|path| std::fs::read_to_string(path).ok())
    }

    async fn emit_loaded(&self, skill_name: &str, content: &str) -> anyhow::Result<()> {
        let (Some(agent_id), Some(events)) = (&self.agent_id, &self.events) else {
            return Ok(());
        };
        let Some(context) = self.contexts.get(agent_id) else {
            return Ok(());
        };
        let Some(skill) = parse_skill(skill_name, content) else {
            return Ok(());
        };
        events
            .emit(session_event(
                &context,
                "skill/loaded",
                serde_json::json!({
                    "skillId": skill.name,
                    "name": skill.name,
                    "version": skill.version
                }),
            ))
            .await?;
        Ok(())
    }
}

impl Default for GetSkillTool {
    fn default() -> Self {
        Self {
            workspace_id: paths::DEFAULT_WORKSPACE_ID.to_string(),
            agent_id: None,
            contexts: Arc::new(TurnContextRegistry::default()),
            events: None,
        }
    }
}

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
                    available_skills(&self)
                )),
                attachments: vec![],
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
                attachments: vec![],
            });
        }

        // Prevent path traversal
        if skill_name.contains("..") || skill_name.contains('/') || skill_name.contains('\\') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Invalid skill_name: '{}'", skill_name)),
                attachments: vec![],
            });
        }

        let content = self.read_skill(skill_name).or_else(|| {
            (skill_name == "visualization")
                .then(|| crate::domains::agent::prompt::templates::VISUALIZATION_SKILL_MD.to_string())
        });
        match content {
            Some(content) if content.trim().is_empty() => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{}' is empty", skill_name)),
                attachments: vec![],
            }),
            Some(content) => {
                self.emit_loaded(skill_name, &content).await?;
                Ok(ToolResult {
                    success: true,
                    output: content,
                    error: None,
                    attachments: vec![],
                })
            }
            None => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Skill '{}' not found. Available skills: {}",
                    skill_name,
                    available_skills(self)
                )),
                attachments: vec![],
            }),
        }
    }
}

/// List workspace and global skill file stems, comma-separated.
fn available_skills(tool: &GetSkillTool) -> String {
    let mut names = std::collections::BTreeSet::new();
    for dir in tool.skill_dirs() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Some(name) = path.file_stem().and_then(|name| name.to_str())
            {
                names.insert(name.to_string());
            }
        }
    }
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.into_iter().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn visualization_skill_loads_from_embedded_fallback() {
        let result = GetSkillTool::default()
            .execute(serde_json::json!({"skill_name":"visualization"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("a2ui_push"));
        assert!(result.output.contains("ThingCard"));
    }

    #[tokio::test]
    async fn workspace_skill_loads_and_emits_skill_loaded() {
        let contexts = std::sync::Arc::new(crate::domains::agent::session::context::TurnContextRegistry::default());
        let sink = std::sync::Arc::new(crate::domains::agent::session::events::test_support::RecordingSink::default());
        let tool = GetSkillTool::new(
            "ws-default-001".to_string(),
            Some("agent_main".to_string()),
            contexts.clone(),
            sink.clone(),
        );
        let context = crate::domains::agent::session::context::TurnContext::new(
            "ws-default-001".into(),
            "agent_main".into(),
            "agent:ws-default-001:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(context);

        let result = tool
            .execute(serde_json::json!({"skill_name":"energy-saving"}))
            .await
            .unwrap();

        assert!(result.success, "{:?}", result.error);
        assert!(result.output.contains("节能分析"));
        let events = sink.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "skill/loaded");
        assert_eq!(events[0].payload_json["name"], "energy-saving");
        assert_eq!(events[0].payload_json["version"], "2");
    }

    #[tokio::test]
    async fn missing_skill_does_not_emit_skill_loaded() {
        let contexts = std::sync::Arc::new(crate::domains::agent::session::context::TurnContextRegistry::default());
        let sink = std::sync::Arc::new(crate::domains::agent::session::events::test_support::RecordingSink::default());
        let tool = GetSkillTool::new(
            "ws-default-001".to_string(),
            Some("agent_main".to_string()),
            contexts.clone(),
            sink.clone(),
        );
        let context = crate::domains::agent::session::context::TurnContext::new(
            "ws-default-001".into(),
            "agent_main".into(),
            "agent:ws-default-001:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(context);

        let result = tool
            .execute(serde_json::json!({"skill_name":"missing-skill"}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(sink.events.lock().await.is_empty());
    }
}
