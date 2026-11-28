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
use crate::domains::agent::session::skill_sequencer::SkillSequencer;

pub struct GetSkillTool {
    workspace_id: String,
    agent_id: Option<String>,
    contexts: Arc<TurnContextRegistry>,
    events: Option<Arc<dyn SessionEventSink>>,
    skills: Arc<SkillSequencer>,
}

impl GetSkillTool {
    pub fn new(
        workspace_id: String,
        agent_id: Option<String>,
        contexts: Arc<TurnContextRegistry>,
        events: Arc<dyn SessionEventSink>,
        skills: Arc<SkillSequencer>,
    ) -> Self {
        Self {
            workspace_id,
            agent_id,
            contexts,
            events: Some(events),
            skills,
        }
    }

    fn session_key(&self) -> Option<String> {
        let agent_id = self.agent_id.as_ref()?;
        self.contexts.get(agent_id).map(|context| context.session_key)
    }

    fn skill_dirs(&self) -> [std::path::PathBuf; 2] {
        [
            paths::workspace_skills_dir(&self.workspace_id),
            paths::global_skills_dir(),
        ]
    }

    fn read_skill(&self, skill_name: &str) -> Option<String> {
        read_skill_content(&self.workspace_id, skill_name)
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
                    "title": skill.title,
                    "description": skill.description,
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
            skills: Arc::new(SkillSequencer::new()),
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
        "Load a skill's full instructions on demand. When a skill matches the current task, \
         calling this is a BLOCKING REQUIREMENT — you MUST invoke it BEFORE taking any other action. \
         Only ONE skill may be active at a time: execute the loaded skill to completion, then call \
         `complete_skill` to settle it. Loading another skill before settling the current one is \
         rejected, and the next skill's instructions are delivered by `complete_skill` automatically."
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

        let session_key = self.session_key();
        // 已经是当前技能时只是“重读正文”，不要再发一条 skill/loaded，
        // 否则自动接续 + 显式 get_skill 会让时间线上出现两条同名技能行。
        let reload = session_key
            .as_ref()
            .and_then(|session| self.skills.active(session))
            .is_some_and(|active| active.key == skill_name);
        if let Some(session) = &session_key
            && let Err(active) = self.skills.request(session, skill_name, "")
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "技能 '{}' 尚未结算，不能同时加载 '{}'。先完成当前技能并调用 complete_skill（它会自动把下一个技能的正文交给你）。",
                    active.key, skill_name
                )),
                attachments: vec![],
            });
        }

        let content = self.read_skill(skill_name).or_else(|| {
            (skill_name == "visualization")
                .then(|| crate::domains::agent::prompt::templates::VISUALIZATION_SKILL_MD.to_string())
        });
        if content.as_deref().is_none_or(|text| text.trim().is_empty())
            && let Some(session) = &session_key
        {
            self.skills.cancel(session, skill_name);
        }
        match content {
            Some(content) if content.trim().is_empty() => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{}' is empty", skill_name)),
                attachments: vec![],
            }),
            Some(content) => {
                if !reload {
                    self.emit_loaded(skill_name, &content).await?;
                }
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

/// 按 workspace → global 的顺序读取技能正文；`visualization` 有内置兜底。
pub(crate) fn read_skill_content(workspace_id: &str, skill_name: &str) -> Option<String> {
    let dirs = [paths::workspace_skills_dir(workspace_id), paths::global_skills_dir()];
    dirs.into_iter()
        .map(|dir| dir.join(format!("{skill_name}.md")))
        .find_map(|path| std::fs::read_to_string(path).ok())
        .or_else(|| {
            (skill_name == "visualization")
                .then(|| crate::domains::agent::prompt::templates::VISUALIZATION_SKILL_MD.to_string())
        })
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
            std::sync::Arc::new(crate::domains::agent::session::skill_sequencer::SkillSequencer::new()),
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
    async fn second_skill_is_rejected_until_the_first_is_settled() {
        let contexts = std::sync::Arc::new(crate::domains::agent::session::context::TurnContextRegistry::default());
        let sink = std::sync::Arc::new(crate::domains::agent::session::events::test_support::RecordingSink::default());
        let sequencer = std::sync::Arc::new(crate::domains::agent::session::skill_sequencer::SkillSequencer::new());
        let tool = GetSkillTool::new(
            "ws-default-001".to_string(),
            Some("agent_main".to_string()),
            contexts.clone(),
            sink.clone(),
            sequencer.clone(),
        );
        let context = crate::domains::agent::session::context::TurnContext::new(
            "ws-default-001".into(),
            "agent_main".into(),
            "agent:ws-default-001:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(context);
        sequencer.begin(
            "agent:ws-default-001:agent_main/s1",
            vec![
                crate::domains::agent::session::skill_sequencer::SkillEntry::new("energy-saving", "节能分析"),
                crate::domains::agent::session::skill_sequencer::SkillEntry::new("inspection", "巡检"),
            ],
        );

        let first = tool
            .execute(serde_json::json!({"skill_name":"energy-saving"}))
            .await
            .unwrap();
        assert!(first.success, "{:?}", first.error);

        let second = tool
            .execute(serde_json::json!({"skill_name":"inspection"}))
            .await
            .unwrap();
        assert!(!second.success);
        assert!(second.error.unwrap_or_default().contains("尚未结算"));
    }

    #[tokio::test]
    async fn reloading_the_same_skill_does_not_emit_twice() {
        let contexts = std::sync::Arc::new(crate::domains::agent::session::context::TurnContextRegistry::default());
        let sink = std::sync::Arc::new(crate::domains::agent::session::events::test_support::RecordingSink::default());
        let sequencer = std::sync::Arc::new(crate::domains::agent::session::skill_sequencer::SkillSequencer::new());
        let tool = GetSkillTool::new(
            "ws-default-001".to_string(),
            Some("agent_main".to_string()),
            contexts.clone(),
            sink.clone(),
            sequencer,
        );
        let context = crate::domains::agent::session::context::TurnContext::new(
            "ws-default-001".into(),
            "agent_main".into(),
            "agent:ws-default-001:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(context);

        tool.execute(serde_json::json!({"skill_name":"energy-saving"}))
            .await
            .unwrap();
        tool.execute(serde_json::json!({"skill_name":"energy-saving"}))
            .await
            .unwrap();

        let events = sink.events.lock().await;
        assert_eq!(events.len(), 1, "同一技能重复加载只应记录一次");
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
            std::sync::Arc::new(crate::domains::agent::session::skill_sequencer::SkillSequencer::new()),
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
