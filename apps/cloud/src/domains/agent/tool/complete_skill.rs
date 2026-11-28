// 结算当前技能，并把下一个技能的正文直接交给模型。
//
// 一轮里的技能顺序由 TURN_BRIEF 决定（`session/agenda.rs`），状态机在
// `session/skill_sequencer.rs`。这个工具只做三件事：结算、发事件、把下一个技能
// 正文回灌，让模型不需要（也不允许）提前批量加载技能。

use async_trait::async_trait;
use std::sync::Arc;

use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use super::get_skill::read_skill_content;
use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::skill_sequencer::{FinishedSkill, SkillOutcome, SkillSequencer};

pub struct CompleteSkillTool {
    workspace_id: String,
    agent_id: Option<String>,
    contexts: Arc<TurnContextRegistry>,
    events: Option<Arc<dyn SessionEventSink>>,
    skills: Arc<SkillSequencer>,
}

impl CompleteSkillTool {
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

    async fn emit_completed(
        &self,
        session_key: &str,
        finished: &FinishedSkill,
        next: Option<&str>,
    ) -> anyhow::Result<()> {
        let (Some(agent_id), Some(events)) = (&self.agent_id, &self.events) else {
            return Ok(());
        };
        let Some(context) = self.contexts.get(agent_id) else {
            return Ok(());
        };
        if context.session_key != session_key {
            return Ok(());
        }
        events
            .emit(session_event(
                &context,
                "skill/completed",
                serde_json::json!({
                    "skillId": finished.entry.key,
                    "name": finished.entry.key,
                    "title": finished.entry.title,
                    "status": finished.outcome.as_str(),
                    "summary": finished.summary,
                    "nextSkill": next,
                }),
            ))
            .await?;
        Ok(())
    }

    /// 下一个技能是自动接上的，没有 get_skill 调用；补一条 skill/loaded，
    /// 让时间线（story / story-map）能按技能切行。
    async fn emit_loaded(&self, session_key: &str, body: &str, key: &str) -> anyhow::Result<()> {
        let (Some(agent_id), Some(events)) = (&self.agent_id, &self.events) else {
            return Ok(());
        };
        let Some(context) = self.contexts.get(agent_id) else {
            return Ok(());
        };
        if context.session_key != session_key {
            return Ok(());
        }
        let Some(skill) = tinyiothub_skills::loader::parse_skill(key, body) else {
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

impl Default for CompleteSkillTool {
    fn default() -> Self {
        Self {
            workspace_id: crate::domains::agent::prompt::paths::DEFAULT_WORKSPACE_ID.to_string(),
            agent_id: None,
            contexts: Arc::new(TurnContextRegistry::default()),
            events: None,
            skills: Arc::new(SkillSequencer::new()),
        }
    }
}

impl Attributable for CompleteSkillTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for CompleteSkillTool {
    fn name(&self) -> &str {
        "complete_skill"
    }

    fn description(&self) -> &str {
        "Settle the currently active skill and receive the next one. Call this once you have \
         finished the skill's steps, or when it is blocked/skipped. Do NOT call get_skill for the \
         next skill — its full instructions are returned here. Only one skill may run at a time."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["completed", "skipped", "blocked"],
                    "description": "completed = 做完了；skipped = 不需要做；blocked = 做不了（缺权限/设备无响应等）。"
                },
                "summary": {
                    "type": "string",
                    "description": "一句话说明这个技能做了什么，或为什么跳过/阻塞。"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let outcome = args
            .get("status")
            .and_then(|value| value.as_str())
            .map(SkillOutcome::parse)
            .unwrap_or(SkillOutcome::Completed);
        let summary = args
            .get("summary")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        let Some(session_key) = self.session_key() else {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("没有进行中的对话轮次，无法结算技能。".into()),
                attachments: vec![],
            });
        };

        let result = self.skills.finish(&session_key, outcome, &summary);
        let next = result.next.clone();
        if let Some(finished) = &result.finished {
            self.emit_completed(&session_key, finished, next.as_ref().map(|entry| entry.key.as_str()))
                .await?;
        }

        let mut output = String::new();
        match &result.finished {
            Some(finished) => {
                output.push_str(&format!(
                    "已结算技能：{}（{}）· {}{}\n",
                    finished.entry.title,
                    finished.entry.key,
                    finished.outcome.as_str(),
                    if finished.summary.is_empty() {
                        String::new()
                    } else {
                        format!("：{}", finished.summary)
                    }
                ));
            }
            None => {
                output.push_str(
                    "当前没有正在执行的技能。新轮次开始时技能状态已重置；\
                     如果要执行技能，请先用 get_skill 加载，不要在新轮次开头调用 complete_skill。",
                );
                return Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                    attachments: vec![],
                });
            }
        }

        match next {
            Some(entry) => {
                output.push_str(&format!(
                    "\n下一个技能：{}（{}）。以下是它的完整正文，直接按步骤执行，不需要再调用 get_skill。\n\n",
                    entry.title, entry.key
                ));
                match read_skill_content(&self.workspace_id, &entry.key) {
                    Some(body) => {
                        self.emit_loaded(&session_key, &body, &entry.key).await?;
                        output.push_str(&body)
                    }
                    None => output.push_str(&format!(
                        "（技能 {} 的正文未找到，请检查 workspace/global skills 目录。）",
                        entry.key
                    )),
                }
            }
            None => {
                output.push_str("\n本轮技能已全部结算，可以给出最终结论。");
            }
        }

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::{TurnContext, TurnContextRegistry};
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::skill_sequencer::SkillEntry;

    const SESSION: &str = "agent:ws-default-001:agent_main/s1";

    #[tokio::test]
    async fn settles_and_returns_next_skill_body() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let sink = Arc::new(RecordingSink::default());
        let skills = Arc::new(SkillSequencer::new());
        let tool = CompleteSkillTool::new(
            "ws-default-001".to_string(),
            Some("agent_main".to_string()),
            contexts.clone(),
            sink.clone(),
            skills.clone(),
        );
        let _guard = contexts.set(TurnContext::new(
            "ws-default-001".into(),
            "agent_main".into(),
            SESSION.into(),
            "run1".into(),
            0,
        ));
        skills.begin(
            SESSION,
            vec![
                SkillEntry::new("energy-saving", "节能分析"),
                SkillEntry::new("inspection", "巡检"),
            ],
        );
        skills.request(SESSION, "energy-saving", "节能分析").unwrap();

        let result = tool
            .execute(serde_json::json!({"status": "completed", "summary": "节电 8%"}))
            .await
            .unwrap();

        assert!(result.success, "{:?}", result.error);
        assert!(result.output.contains("energy-saving"));
        assert!(result.output.contains("inspection"), "{}", result.output);
        assert!(result.output.contains("巡检"));
        let events = sink.events.lock().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "skill/completed");
        assert_eq!(events[0].payload_json["status"], "completed");
        assert_eq!(events[0].payload_json["nextSkill"], "inspection");
        // 自动接上的下一个技能补一条 skill/loaded，前端才能按技能切行
        assert_eq!(events[1].event_type, "skill/loaded");
        assert_eq!(events[1].payload_json["name"], "inspection");
    }
}
