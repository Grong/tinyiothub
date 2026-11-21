use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::TodoList;

pub struct TodoWriteTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    store: Arc<TodoStore>,
    events: Arc<dyn SessionEventSink>,
}

impl TodoWriteTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        store: Arc<TodoStore>,
        events: Arc<dyn SessionEventSink>,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            store,
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
        let list: TodoList =
            serde_json::from_value(args).map_err(|error| anyhow::anyhow!("invalid todo list: {error}"))?;
        if let Err(message) = list.validate() {
            return Ok(failed(message));
        }
        let todos = self.store.replace(&context.session_key, list.todos.clone());
        self.events
            .emit(session_event(&context, "todo/write", json!({ "todos": &todos })))
            .await?;
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string(&json!({ "ok": true, "todos": todos }))?,
            error: None,
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

impl Attributable for TodoWriteTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }

    fn alias(&self) -> &str {
        "todo_write"
    }
}

#[async_trait::async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Create or replace the session todo list. For multi-step work, call it before starting and after every step: mark finished items completed and the next item in_progress. Keep at most one item in_progress. Do not write a final conclusion while any item is pending or in_progress: finish it, or replace the list without obsolete items first."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 50,
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
                            "activeForm": {"type": "string"}
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn description_defines_progress_updates_and_a_completed_final_state() {
        let tool = TodoWriteTool::new(
            "agent_main".to_string(),
            Arc::new(TurnContextRegistry::default()),
            Arc::new(TodoStore::default()),
            Arc::new(RecordingSink::default()),
        );

        assert!(tool.description().contains("after every step"));
        assert!(tool.description().contains("at most one item in_progress"));
        assert!(tool.description().contains("Do not write a final conclusion"));
    }

    #[tokio::test]
    async fn todo_write_persists_and_emits() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let store = Arc::new(TodoStore::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = TodoWriteTool::new("agent_main".to_string(), contexts.clone(), store.clone(), sink.clone());
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool
            .execute(json!({
                "todos": [{
                    "content": "检查离线事件",
                    "status": "in_progress",
                    "activeForm": "检查离线事件"
                }]
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(store.get("agent:ws:agent_main/s1").len(), 1);
        let events = sink.events.lock().await;
        assert_eq!(events.last().unwrap().event_type, "todo/write");
        assert_eq!(
            events.last().unwrap().payload_json["todos"][0]["content"],
            "检查离线事件"
        );
    }

    #[tokio::test]
    async fn todo_write_rejects_two_in_progress_without_mutating_store() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let store = Arc::new(TodoStore::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = TodoWriteTool::new("agent_main".to_string(), contexts.clone(), store.clone(), sink);
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool
            .execute(json!({
                "todos": [
                    {"content": "a", "status": "in_progress", "activeForm": "a"},
                    {"content": "b", "status": "in_progress", "activeForm": "b"}
                ]
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(store.get("agent:ws:agent_main/s1").is_empty());
    }

    #[tokio::test]
    async fn todo_write_fails_without_turn_context() {
        let tool = TodoWriteTool::new(
            "agent_main".to_string(),
            Arc::new(TurnContextRegistry::default()),
            Arc::new(TodoStore::default()),
            Arc::new(RecordingSink::default()),
        );
        let result = tool
            .execute(json!({"todos": [{"content": "a", "status": "pending", "activeForm": "a"}]}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("turn context"));
    }
}
