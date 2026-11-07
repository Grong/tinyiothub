//! SessionRunner — 一次 Assistant Turn 的唯一执行入口。
//!
//! 它只负责：安装 TurnContext、驱动 AgentLoop、把 TurnEvent 写入 session_events、
//! 通知可选 observer（聊天 SSE 适配器）、收尾。消息持久化/反思仍由 chat 组合层
//! 负责，避免 SessionRunner 依赖 chat 的存储历史。

use std::sync::Arc;

use serde_json::json;
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::runtime::AgentLoop;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::context::{TurnContext, TurnContextRegistry};
use super::events::{SessionEventSink, session_event};

#[async_trait::async_trait]
pub trait TurnObserver: Send + Sync {
    async fn on_turn_event(&self, event: &TurnEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed { text: String, tool_calls: u32 },
    Cancelled,
    Failed { error: String },
}

pub struct SessionRunner {
    events: Arc<dyn SessionEventSink>,
    contexts: Arc<TurnContextRegistry>,
}

/// Accumulates per-model-call usage into one `turn.completed` payload.
#[derive(Default)]
struct UsageTotals {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    seen: bool,
}

impl UsageTotals {
    fn add(
        &mut self,
        input_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_usd: Option<f64>,
    ) {
        self.seen = true;
        self.input_tokens += input_tokens.unwrap_or(0);
        self.cached_input_tokens += cached_input_tokens.unwrap_or(0);
        self.output_tokens += output_tokens.unwrap_or(0);
        self.cost_usd += cost_usd.unwrap_or(0.0);
    }

    fn to_json(&self) -> serde_json::Value {
        if !self.seen {
            return serde_json::Value::Null;
        }
        json!({
            "inputTokens": self.input_tokens,
            "cachedInputTokens": self.cached_input_tokens,
            "outputTokens": self.output_tokens,
            "costUsd": self.cost_usd
        })
    }
}

impl SessionRunner {
    pub fn new(events: Arc<dyn SessionEventSink>, contexts: Arc<TurnContextRegistry>) -> Self {
        Self { events, contexts }
    }

    pub async fn run_turn(
        &self,
        agent: &dyn AgentLoop,
        context: &TurnContext,
        message: &str,
        observer: Option<Arc<dyn TurnObserver>>,
        cancel: Option<CancellationToken>,
    ) -> TurnOutcome {
        let _guard = self.contexts.set(context.clone());
        self.emit(context, "turn.started", json!({ "turnId": &context.turn_id }))
            .await;

        let (event_tx, mut event_rx) = mpsc::channel::<TurnEvent>(64);
        let forward = tokio::spawn({
            let events = self.events.clone();
            let context = context.clone();
            let observer = observer.clone();
            async move {
                let mut tool_calls = 0u32;
                let mut usage = UsageTotals::default();
                while let Some(event) = event_rx.recv().await {
                    match &event {
                        TurnEvent::ToolCall { id, name, args } => {
                            tool_calls += 1;
                            let _ = events
                                .emit(session_event(
                                    &context,
                                    "tool.called",
                                    json!({ "toolCallId": id, "tool": name, "args": args }),
                                ))
                                .await;
                        }
                        TurnEvent::ToolResult {
                            id,
                            name,
                            output,
                            attachments,
                        } => {
                            let evidence: Vec<tinyiothub_core::media::EvidenceRef> =
                                attachments.iter().map(Into::into).collect();
                            let _ = events
                                .emit(session_event(
                                    &context,
                                    "tool.returned",
                                    json!({
                                        "toolCallId": id,
                                        "tool": name,
                                        "result": truncate(output, 2000),
                                        "evidence": evidence
                                    }),
                                ))
                                .await;
                        }
                        TurnEvent::Usage {
                            input_tokens,
                            cached_input_tokens,
                            output_tokens,
                            cost_usd,
                        } => usage.add(*input_tokens, *cached_input_tokens, *output_tokens, *cost_usd),
                        _ => {}
                    }
                    if let Some(observer) = &observer {
                        observer.on_turn_event(&event).await;
                    }
                }
                (tool_calls, usage.to_json())
            }
        });

        let result = agent.turn_streamed(message, event_tx, cancel.clone()).await;
        let (tool_calls, usage) = forward.await.unwrap_or((0, serde_json::Value::Null));

        match result {
            Ok(text) => {
                self.emit(
                    context,
                    "message.created",
                    json!({
                        "messageId": uuid::Uuid::new_v4().to_string(),
                        "role": "assistant",
                        "text": &text
                    }),
                )
                .await;
                self.emit(
                    context,
                    "turn.completed",
                    json!({
                        "turnId": &context.turn_id,
                        "outcome": "completed",
                        "text": &text,
                        "toolCalls": tool_calls,
                        "usage": &usage
                    }),
                )
                .await;
                TurnOutcome::Completed { text, tool_calls }
            }
            Err(error) => {
                let cancelled = cancel.as_ref().is_some_and(|token| token.is_cancelled());
                let error = error.to_string();
                self.emit(
                    context,
                    "turn.completed",
                    json!({
                        "turnId": &context.turn_id,
                        "outcome": if cancelled { "cancelled" } else { "failed" },
                        "error": &error,
                        "toolCalls": tool_calls,
                        "usage": &usage
                    }),
                )
                .await;
                if cancelled {
                    TurnOutcome::Cancelled
                } else {
                    TurnOutcome::Failed { error }
                }
            }
        }
    }

    async fn emit(&self, context: &TurnContext, event_type: &str, payload: serde_json::Value) {
        if let Err(error) = self.events.emit(session_event(context, event_type, payload)).await {
            tracing::warn!(%error, event_type, "session event write failed");
        }
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use serde_json::json;
    use std::sync::Arc;
    use tinyiothub_agent::port::provider::ChatMessage;
    use tokio::sync::Mutex;

    struct FakeLoop;

    #[async_trait::async_trait]
    impl AgentLoop for FakeLoop {
        async fn turn_streamed(
            &self,
            _message: &str,
            tx: tokio::sync::mpsc::Sender<TurnEvent>,
            _cancel: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            tx.send(TurnEvent::Chunk { delta: "hi".into() }).await.unwrap();
            tx.send(TurnEvent::ToolCall {
                id: "call1".into(),
                name: "todo_write".into(),
                args: json!({"todos": []}),
            })
            .await
            .unwrap();
            tx.send(TurnEvent::ToolResult {
                id: "call1".into(),
                name: "todo_write".into(),
                output: "ok".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
            Ok("done".into())
        }

        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingObserver {
        events: Mutex<Vec<TurnEvent>>,
    }

    #[async_trait::async_trait]
    impl TurnObserver for RecordingObserver {
        async fn on_turn_event(&self, event: &TurnEvent) {
            self.events.lock().await.push(event.clone());
        }
    }

    #[tokio::test]
    async fn run_turn_persists_lifecycle_and_tools() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = SessionRunner::new(sink.clone(), contexts.clone());
        let observer = Arc::new(RecordingObserver::default());
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );

        let outcome = runner
            .run_turn(&FakeLoop, &ctx, "hello", Some(observer.clone()), None)
            .await;

        assert_eq!(
            outcome,
            TurnOutcome::Completed {
                text: "done".into(),
                tool_calls: 1
            }
        );
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "turn.started",
                "tool.called",
                "tool.returned",
                "message.created",
                "turn.completed"
            ]
        );
        assert_eq!(events[4].payload_json["outcome"], "completed");
        assert_eq!(observer.events.lock().await.len(), 3);
        assert!(contexts.get("agent_main").is_none(), "guard clears turn context");
    }
}
