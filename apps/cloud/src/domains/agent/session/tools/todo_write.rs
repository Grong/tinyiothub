use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::{TodoList, TodoStatus};

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
        let before = self.store.get(&context.session_key);
        let newly_completed = if before.len() == list.todos.len() {
            before
                .iter()
                .zip(&list.todos)
                .filter(|(old, new)| old.status != TodoStatus::Completed && new.status == TodoStatus::Completed)
                .count()
        } else if before.is_empty() {
            0
        } else {
            list.todos
                .iter()
                .filter(|todo| todo.status == TodoStatus::Completed)
                .count()
        };
        if newly_completed > 1 {
            return Ok(failed(
                "only one todo may move to completed per todo_write call; mark the finished item completed, mark the next item in_progress, and submit again",
            ));
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
        "Create or replace the session todo list. For a turn with multiple skills, you may start with one item per skill as a placeholder. Do not guess steps before loading a skill: after loading a skill, replace its placeholder with one item per concrete step from its instructions and keep the other skill items unchanged. Track one active skill at a time. Mark finished items completed and the next item in_progress; keep at most one item in_progress. Only one item may move to completed per call: update progress as each step finishes instead of marking several pending items completed at once. Do not write a final conclusion while any item is pending or in_progress: finish it, or replace the list without obsolete items first. If an item is genuinely blocked, keep its real status and state the blocker explicitly; never mark it completed."
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
    use crate::domains::agent::session::types::Todo;
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

        assert!(tool.description().contains("one item per concrete step"));
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
    async fn todo_write_rejects_batch_completion_without_mutating_store() {
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
        store.replace(
            "agent:ws:agent_main/s1",
            vec![
                Todo {
                    content: "a".into(),
                    status: TodoStatus::Completed,
                    active_form: String::new(),
                },
                Todo {
                    content: "b".into(),
                    status: TodoStatus::InProgress,
                    active_form: String::new(),
                },
                Todo {
                    content: "c".into(),
                    status: TodoStatus::Pending,
                    active_form: String::new(),
                },
            ],
        );

        let result = tool
            .execute(json!({"todos": [
                {"content": "a", "status": "completed"},
                {"content": "b", "status": "completed"},
                {"content": "c", "status": "completed"}
            ]}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("only one todo"));
        let stored = store.get("agent:ws:agent_main/s1");
        assert_eq!(stored[1].status, TodoStatus::InProgress);
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
