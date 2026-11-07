use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::subagent::{SubagentRequest, SubagentSlot, SubagentStatus};

pub struct SpawnSubagentTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    subagents: Arc<SubagentSlot>,
    events: Arc<dyn SessionEventSink>,
}

impl SpawnSubagentTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        subagents: Arc<SubagentSlot>,
        events: Arc<dyn SessionEventSink>,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            subagents,
            events,
        }
    }

    async fn run(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return Ok(failed(format!(
                "turn context missing for assistant {}",
                self.assistant_id
            )));
        };
        if context.depth >= 1 {
            return Ok(failed("subagent depth exceeded: only one level is allowed"));
        }
        let description = args
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or("subagent task")
            .to_string();
        let subagent_type = args
            .get("subagent_type")
            .and_then(|value| value.as_str())
            .unwrap_or("general-purpose")
            .to_string();
        let Some(prompt) = args.get("prompt").and_then(|value| value.as_str()) else {
            return Ok(failed("missing prompt"));
        };
        let prompt = prompt.to_string();
        let Some(runner) = self.subagents.get() else {
            return Ok(failed("subagent runner not configured"));
        };

        let subagent_id = uuid::Uuid::new_v4().to_string();
        self.events
            .emit(session_event(
                &context,
                "subagent.started",
                json!({
                    "subagentId": subagent_id.clone(),
                    "description": description.clone(),
                    "subagentType": subagent_type.clone()
                }),
            ))
            .await?;

        let report = match runner
            .run(SubagentRequest {
                workspace_id: context.workspace_id.clone(),
                parent_agent_id: context.assistant_id.clone(),
                parent_session_key: context.session_key.clone(),
                subagent_id: subagent_id.clone(),
                subagent_type,
                description,
                prompt,
                depth: context.depth + 1,
            })
            .await
        {
            Ok(report) => report,
            Err(error) => {
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "subagent.completed",
                        json!({
                            "subagentId": subagent_id,
                            "status": "failed",
                            "summary": "",
                            "toolCalls": 0,
                            "error": &error
                        }),
                    ))
                    .await;
                return Ok(failed(format!("subagent failed: {error}")));
            }
        };

        self.events
            .emit(session_event(
                &context,
                "subagent.completed",
                json!({
                    "subagentId": report.subagent_id.clone(),
                    "status": report.status,
                    "summary": report.summary.clone(),
                    "toolCalls": report.tool_calls,
                    "evidenceRefs": report.evidence_refs.clone(),
                    "recommendedActions": report.recommended_actions.clone()
                }),
            ))
            .await?;

        Ok(ToolResult {
            success: report.status == SubagentStatus::Completed,
            output: serde_json::to_string(&report)?,
            error: (report.status != SubagentStatus::Completed)
                .then(|| format!("subagent status: {:?}", report.status)),
            attachments: vec![],
        })
    }
}

fn failed(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        attachments: vec![],
    }
}

impl Attributable for SpawnSubagentTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::SpawnSubagent)
    }

    fn alias(&self) -> &str {
        "spawn_subagent"
    }
}

#[async_trait::async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Run a synchronous subagent with isolated context and return its final report. Use for focused, parallelizable investigation. One level only."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "description": {"type": "string", "description": "Short task label"},
                "prompt": {"type": "string", "description": "Self-contained task for the subagent"},
                "subagent_type": {
                    "type": "string",
                    "description": "Focus label, e.g. general-purpose or Explore",
                    "default": "general-purpose"
                }
            },
            "required": ["description", "prompt"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::{TurnContext, TurnGuard};
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::subagent::{
        SubagentReport, SubagentRequest, SubagentRunner, SubagentSlot, SubagentStatus,
    };
    use serde_json::json;
    use std::sync::Arc;

    struct StubRunner;

    #[async_trait::async_trait]
    impl SubagentRunner for StubRunner {
        async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
            assert_eq!(request.prompt, "检查全部设备");
            assert_eq!(request.subagent_type, "general-purpose");
            Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: "检查完成：无异常".to_string(),
                tool_calls: 3,
                evidence_refs: vec!["event:42".to_string()],
                recommended_actions: vec![],
            })
        }
    }

    fn make_tool(
        sink: Arc<RecordingSink>,
        contexts: Arc<TurnContextRegistry>,
        depth: u8,
    ) -> (SpawnSubagentTool, TurnGuard) {
        let slot = Arc::new(SubagentSlot::default());
        slot.set(Arc::new(StubRunner));
        let tool = SpawnSubagentTool::new("agent_main".to_string(), contexts.clone(), slot, sink);
        let guard = contexts.set(TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            depth,
        ));
        (tool, guard)
    }

    #[tokio::test]
    async fn spawn_returns_report_and_emits_lifecycle() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let (tool, _guard) = make_tool(sink.clone(), contexts, 0);
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("检查完成"));
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, vec!["subagent.started", "subagent.completed"]);
        assert_eq!(events[1].payload_json["toolCalls"], 3);
    }

    #[tokio::test]
    async fn spawn_rejects_second_level() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let (tool, _guard) = make_tool(sink.clone(), contexts, 1);
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("depth"));
        assert!(sink.events.lock().await.is_empty());
    }

    #[tokio::test]
    async fn spawn_fails_when_runner_not_wired() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let tool = SpawnSubagentTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            Arc::new(SubagentSlot::default()),
            Arc::new(RecordingSink::default()),
        );
        let _guard = contexts.set(TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        ));
        let result = tool
            .execute(json!({"description": "巡检", "prompt": "检查全部设备"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not configured"));
    }
}
