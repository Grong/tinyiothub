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
                let mut payload = json!({
                    "turnId": &context.turn_id,
                    "outcome": if cancelled { "cancelled" } else { "failed" },
                    "toolCalls": tool_calls,
                    "usage": &usage
                });
                if !cancelled {
                    payload["error"] = json!(&error);
                }
                self.emit(context, "turn.completed", payload).await;
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

    /// Loop that emits a fixed script of events, then returns a fixed result.
    struct ScriptedLoop {
        events: Vec<TurnEvent>,
        result: Result<String, String>,
    }

    impl ScriptedLoop {
        fn completed(events: Vec<TurnEvent>) -> Self {
            Self {
                events,
                result: Ok("done".into()),
            }
        }

        fn failed(error: &str) -> Self {
            Self {
                events: Vec::new(),
                result: Err(error.to_string()),
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentLoop for ScriptedLoop {
        async fn turn_streamed(
            &self,
            _message: &str,
            tx: tokio::sync::mpsc::Sender<TurnEvent>,
            _cancel: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            for event in &self.events {
                tx.send(event.clone()).await.unwrap();
            }
            match &self.result {
                Ok(text) => Ok(text.clone()),
                Err(error) => Err(anyhow::anyhow!(error.clone())),
            }
        }

        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn ctx() -> TurnContext {
        TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        )
    }

    fn runner_and_sink() -> (SessionRunner, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let runner = SessionRunner::new(sink.clone(), Arc::new(TurnContextRegistry::default()));
        (runner, sink)
    }

    fn completed_payload(events: &[tinyiothub_core::harness::HarnessEventInput]) -> &serde_json::Value {
        let event = events
            .iter()
            .find(|event| event.event_type == "turn.completed")
            .expect("turn.completed emitted");
        &event.payload_json
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

    #[tokio::test]
    async fn usage_is_accumulated_into_turn_completed() {
        let (runner, sink) = runner_and_sink();
        let agent = ScriptedLoop::completed(vec![
            TurnEvent::Usage {
                input_tokens: Some(10),
                cached_input_tokens: Some(2),
                output_tokens: Some(5),
                cost_usd: Some(0.001),
            },
            TurnEvent::Usage {
                input_tokens: Some(3),
                cached_input_tokens: None,
                output_tokens: Some(1),
                cost_usd: None,
            },
        ]);

        let outcome = runner.run_turn(&agent, &ctx(), "hello", None, None).await;

        assert_eq!(
            outcome,
            TurnOutcome::Completed {
                text: "done".into(),
                tool_calls: 0
            }
        );
        let events = sink.events.lock().await;
        assert_eq!(
            completed_payload(&events)["usage"],
            json!({
                "inputTokens": 13,
                "cachedInputTokens": 2,
                "outputTokens": 6,
                "costUsd": 0.001
            })
        );
    }

    #[tokio::test]
    async fn usage_is_null_without_usage_events() {
        let (runner, sink) = runner_and_sink();

        let outcome = runner.run_turn(&FakeLoop, &ctx(), "hello", None, None).await;

        assert!(matches!(outcome, TurnOutcome::Completed { .. }));
        let events = sink.events.lock().await;
        assert!(
            completed_payload(&events)["usage"].is_null(),
            "usage is null when the turn reported none"
        );
    }

    #[tokio::test]
    async fn cancelled_turn_reports_cancelled() {
        let (runner, sink) = runner_and_sink();
        let cancel = CancellationToken::new();
        cancel.cancel();

        let outcome = runner
            .run_turn(&ScriptedLoop::failed("aborted"), &ctx(), "hello", None, Some(cancel))
            .await;

        assert_eq!(outcome, TurnOutcome::Cancelled);
        let events = sink.events.lock().await;
        let payload = completed_payload(&events);
        assert_eq!(payload["outcome"], "cancelled");
        assert!(payload.get("error").is_none(), "cancelled turn omits error");
    }

    #[tokio::test]
    async fn lifecycle_order_is_stable() {
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = SessionRunner::new(sink.clone(), contexts);
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        runner.run_turn(&FakeLoop, &ctx, "hello", None, None).await;
        let events = sink.events.lock().await;
        let index = |name: &str| {
            events
                .iter()
                .position(|event| event.event_type == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        assert!(index("turn.started") < index("tool.called"));
        assert!(index("tool.returned") < index("message.created"));
        assert!(index("message.created") < index("turn.completed"));
    }

    #[tokio::test]
    async fn failed_turn_reports_error() {
        let (runner, sink) = runner_and_sink();

        let outcome = runner
            .run_turn(&ScriptedLoop::failed("boom"), &ctx(), "hello", None, None)
            .await;

        assert_eq!(outcome, TurnOutcome::Failed { error: "boom".into() });
        let events = sink.events.lock().await;
        let payload = completed_payload(&events);
        assert_eq!(payload["outcome"], "failed");
        assert!(payload["error"].as_str().is_some_and(|error| error.contains("boom")));
    }
}
