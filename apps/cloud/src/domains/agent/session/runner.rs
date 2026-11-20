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
use super::suppression::audit::{WakeAttempt, attempt_event};

/// Emit one durable step boundary marker. The runner owns the step window:
/// a step opens on the first model output/tool call and closes when the next
/// model output arrives after the previous call settled.
async fn emit_step_marker(
    events: &Arc<dyn SessionEventSink>,
    context: &TurnContext,
    event_type: &'static str,
    step_index: u32,
) -> bool {
    let step_id = format!("{}:s{}", context.turn_id, step_index);
    events
        .emit(session_event(
            context,
            event_type,
            json!({
                "turnId": &context.turn_id,
                "stepId": step_id,
                "index": step_index,
            }),
        ))
        .await
        .is_ok()
}

#[async_trait::async_trait]
pub trait TurnObserver: Send + Sync {
    async fn on_turn_event(&self, event: &TurnEvent);
}

#[derive(Debug, Clone)]
pub struct TurnCancellation {
    pub token: CancellationToken,
    pub reason: String,
    pub source: String,
}

impl TurnCancellation {
    pub fn user(token: CancellationToken) -> Self {
        Self {
            token,
            reason: "用户主动停止本轮执行".into(),
            source: "user".into(),
        }
    }

    pub fn schedule(token: CancellationToken) -> Self {
        Self {
            token,
            reason: "调度器停止本轮执行".into(),
            source: "schedule".into(),
        }
    }

    pub fn system(token: CancellationToken, reason: impl Into<String>) -> Self {
        Self {
            token,
            reason: reason.into(),
            source: "system".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed { text: String, tool_calls: u32 },
    Cancelled { reason: String, source: String },
    Failed { error: String },
}

/// Session Event write health for one Turn.
///
/// A full Turn may install a suppression baseline only when every event
/// persisted and the `turn.completed` terminal was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnEventHealth {
    pub failed: u32,
    /// The `turn.completed` terminal event was persisted.
    pub terminal_written: bool,
}

impl TurnEventHealth {
    pub fn is_healthy(&self) -> bool {
        self.failed == 0 && self.terminal_written
    }
}

/// Actual model/tool calls observed for one Turn. Recorded as suppression gate
/// evidence; never operator-typed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnCounts {
    pub model_calls: u64,
    pub tool_calls: u64,
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
        cancel: Option<TurnCancellation>,
    ) -> TurnOutcome {
        self.run_turn_tracked(agent, context, message, observer, cancel, None, None)
            .await
            .0
    }

    /// Run one Turn and report Session Event write health.
    ///
    /// Planned-wake callers pass the durably allocated attempt so
    /// `turn.started`/`turn.completed` share the attempt-derived ids used by
    /// the skip audit. `previous_attempt_id` links a changed-source replay's
    /// new attempt to the committed skip it supersedes. The health evidence
    /// lets those callers refuse to install a baseline when any event (in
    /// particular the terminal) was not persisted.
    pub async fn run_turn_tracked(
        &self,
        agent: &dyn AgentLoop,
        context: &TurnContext,
        message: &str,
        observer: Option<Arc<dyn TurnObserver>>,
        cancel: Option<TurnCancellation>,
        attempt: Option<&WakeAttempt>,
        previous_attempt_id: Option<&str>,
    ) -> (TurnOutcome, TurnEventHealth, TurnCounts) {
        let _guard = self.contexts.set(context.clone());
        let mut health = TurnEventHealth::default();
        let mut started = json!({ "turnId": &context.turn_id });
        if let Some(previous) = previous_attempt_id {
            started["previousAttemptId"] = json!(previous);
            started["supersedes"] = json!(true);
        }
        if !self.emit_turn_event(context, attempt, "turn.started", started).await {
            health.failed += 1;
        }

        let (event_tx, mut event_rx) = mpsc::channel::<TurnEvent>(64);
        let forward = tokio::spawn({
            let events = self.events.clone();
            let context = context.clone();
            let observer = observer.clone();
            async move {
                let mut tool_calls = 0u32;
                // One Usage event is emitted per model call; this is the
                // server-side model-call count used by gate evidence.
                let mut model_calls = 0u64;
                let mut usage = UsageTotals::default();
                let mut failed = 0u32;
                let mut text_buf = String::new();
                let mut last_text_flush = tokio::time::Instant::now();
                let mut step_index = 0u32;
                let mut step_open = false;
                let mut step_usage_seen = false;
                while let Some(event) = event_rx.recv().await {
                    match &event {
                        TurnEvent::Chunk { delta } => {
                            if step_open && step_usage_seen {
                                if !emit_step_marker(&events, &context, "step/end", step_index).await {
                                    failed += 1;
                                }
                                step_open = false;
                            }
                            if !step_open {
                                step_index += 1;
                                step_open = true;
                                step_usage_seen = false;
                                if !emit_step_marker(&events, &context, "step/start", step_index).await {
                                    failed += 1;
                                }
                            }
                            text_buf.push_str(delta);
                            let due = text_buf.chars().count() >= 200
                                || last_text_flush.elapsed() >= std::time::Duration::from_millis(250);
                            if due && !text_buf.is_empty() {
                                let text = std::mem::take(&mut text_buf);
                                if events
                                    .emit(session_event(
                                        &context,
                                        "step.text",
                                        json!({ "turnId": &context.turn_id, "text": text }),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    failed += 1;
                                }
                                last_text_flush = tokio::time::Instant::now();
                            }
                        }
                        TurnEvent::ToolCall { id, name, args } => {
                            if !step_open {
                                step_index += 1;
                                step_open = true;
                                step_usage_seen = false;
                                if !emit_step_marker(&events, &context, "step/start", step_index).await {
                                    failed += 1;
                                }
                            }
                            if !text_buf.is_empty() {
                                let text = std::mem::take(&mut text_buf);
                                if events
                                    .emit(session_event(
                                        &context,
                                        "step.text",
                                        json!({ "turnId": &context.turn_id, "text": text }),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    failed += 1;
                                }
                                last_text_flush = tokio::time::Instant::now();
                            }
                            tool_calls += 1;
                            if events
                                .emit(session_event(
                                    &context,
                                    "tool.called",
                                    json!({ "toolCallId": id, "tool": name, "args": args }),
                                ))
                                .await
                                .is_err()
                            {
                                failed += 1;
                            }
                        }
                        TurnEvent::ToolResult {
                            id,
                            name,
                            output,
                            attachments,
                        } => {
                            if !text_buf.is_empty() {
                                let text = std::mem::take(&mut text_buf);
                                if events
                                    .emit(session_event(
                                        &context,
                                        "step.text",
                                        json!({ "turnId": &context.turn_id, "text": text }),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    failed += 1;
                                }
                                last_text_flush = tokio::time::Instant::now();
                            }
                            let evidence: Vec<tinyiothub_core::media::EvidenceRef> =
                                attachments.iter().map(Into::into).collect();
                            if events
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
                                .await
                                .is_err()
                            {
                                failed += 1;
                            }
                        }
                        TurnEvent::Thinking { delta } => {
                            if step_open && step_usage_seen {
                                if !emit_step_marker(&events, &context, "step/end", step_index).await {
                                    failed += 1;
                                }
                                step_open = false;
                            }
                            if !step_open {
                                step_index += 1;
                                step_open = true;
                                step_usage_seen = false;
                                if !emit_step_marker(&events, &context, "step/start", step_index).await {
                                    failed += 1;
                                }
                            }
                            let written = events
                                .emit(session_event(
                                    &context,
                                    "step.thinking",
                                    json!({ "turnId": &context.turn_id, "thinking": delta }),
                                ))
                                .await
                                .is_ok();
                            if !written {
                                failed += 1;
                            }
                        }
                        TurnEvent::Retry(notice) => {
                            // A retry stays inside the open step: the failed
                            // attempt's usage must not close the step before the
                            // retry produces the authoritative model output.
                            step_usage_seen = false;
                            // Buffered text belongs to the failed model attempt;
                            // persisted chunks are rolled back by turn.retry in the UI.
                            text_buf.clear();
                            if events
                                .emit(session_event(
                                    &context,
                                    "turn.retry",
                                    json!({
                                        "turnId": &context.turn_id,
                                        "attempt": notice.attempt,
                                        "maxRetries": notice.max_retries,
                                        "delayMs": notice.delay_ms,
                                        "error": notice.error
                                    }),
                                ))
                                .await
                                .is_err()
                            {
                                failed += 1;
                            }
                        }
                        TurnEvent::Usage {
                            input_tokens,
                            cached_input_tokens,
                            output_tokens,
                            cost_usd,
                        } => {
                            if !step_open {
                                step_index += 1;
                                step_open = true;
                                if !emit_step_marker(&events, &context, "step/start", step_index).await {
                                    failed += 1;
                                }
                            }
                            step_usage_seen = true;
                            model_calls += 1;
                            usage.add(*input_tokens, *cached_input_tokens, *output_tokens, *cost_usd);
                        }
                        _ => {}
                    }
                    if let Some(observer) = &observer {
                        observer.on_turn_event(&event).await;
                    }
                }
                if step_open && !emit_step_marker(&events, &context, "step/end", step_index).await {
                    failed += 1;
                }
                if !text_buf.is_empty()
                    && events
                        .emit(session_event(
                            &context,
                            "step.text",
                            json!({ "turnId": &context.turn_id, "text": std::mem::take(&mut text_buf) }),
                        ))
                        .await
                        .is_err()
                {
                    failed += 1;
                }
                (tool_calls, model_calls, usage.to_json(), failed)
            }
        });

        let cancel_token = cancel.as_ref().map(|cancellation| cancellation.token.clone());
        let result = agent.turn_streamed(message, event_tx, cancel_token).await;
        let (tool_calls, model_calls, usage, forward_failed) =
            forward.await.unwrap_or((0, 0, serde_json::Value::Null, 1));
        health.failed += forward_failed;

        let outcome = match result {
            Ok(text) => {
                if !self
                    .emit_turn_event(
                        context,
                        attempt,
                        "message.created",
                        json!({
                            "messageId": uuid::Uuid::new_v4().to_string(),
                            "turnId": &context.turn_id,
                            "role": "assistant",
                            "text": &text
                        }),
                    )
                    .await
                {
                    health.failed += 1;
                }
                health.terminal_written = self
                    .emit_turn_event(
                        context,
                        attempt,
                        "turn.completed",
                        json!({
                            "turnId": &context.turn_id,
                            "outcome": "completed",
                            "text": &text,
                            "toolCalls": tool_calls,
                            "modelCalls": model_calls,
                            "usage": &usage
                        }),
                    )
                    .await;
                if !health.terminal_written {
                    health.failed += 1;
                }
                TurnOutcome::Completed { text, tool_calls }
            }
            Err(error) => {
                let cancellation = cancel.as_ref().filter(|cancellation| cancellation.token.is_cancelled());
                let error = error.to_string();
                let mut payload = json!({
                    "turnId": &context.turn_id,
                    "outcome": if cancellation.is_some() { "cancelled" } else { "failed" },
                    "toolCalls": tool_calls,
                    "modelCalls": model_calls,
                    "usage": &usage
                });
                if let Some(cancellation) = cancellation {
                    let reason = super::suppression::error::sanitize_public_text(&cancellation.reason);
                    payload["error"] = json!(&reason);
                    payload["errorMessage"] = json!(&reason);
                    payload["source"] = json!(&cancellation.source);
                } else {
                    let budget_code = if error.contains("context_too_large") {
                        Some(super::suppression::error::ErrorCode::ContextTooLarge)
                    } else if error.contains("context_budget_unavailable") {
                        Some(super::suppression::error::ErrorCode::ContextBudgetUnavailable)
                    } else {
                        None
                    };
                    if let Some(code) = budget_code {
                        let typed = super::suppression::error::SuppressionError::from_code(code);
                        payload = typed.async_failure_payload();
                        payload["source"] = json!("system");
                    } else {
                        // Async failures carry the safe summary the existing
                        // frontend renders, without payloads, paths or credentials.
                        let message = super::suppression::error::sanitize_public_text(&error);
                        payload["error"] = json!(&message);
                        payload["errorMessage"] = json!(&message);
                        payload["source"] = json!("system");
                    }
                }
                health.terminal_written = self.emit_turn_event(context, attempt, "turn.completed", payload).await;
                if !health.terminal_written {
                    health.failed += 1;
                }
                if let Some(cancellation) = cancellation {
                    TurnOutcome::Cancelled {
                        reason: cancellation.reason.clone(),
                        source: cancellation.source.clone(),
                    }
                } else {
                    TurnOutcome::Failed { error }
                }
            }
        };
        (
            outcome,
            health,
            TurnCounts {
                model_calls,
                tool_calls: u64::from(tool_calls),
            },
        )
    }

    /// Emit one Turn event. Planned-wake lifecycle events use the attempt's
    /// stable ids; everything else keeps the existing per-invocation identity.
    async fn emit_turn_event(
        &self,
        context: &TurnContext,
        attempt: Option<&WakeAttempt>,
        event_type: &str,
        payload: serde_json::Value,
    ) -> bool {
        let input = match attempt {
            Some(attempt) if matches!(event_type, "turn.started" | "turn.completed") => {
                attempt_event(context, attempt, event_type, payload)
            }
            _ => session_event(context, event_type, payload),
        };
        if let Err(error) = self.events.emit(input).await {
            tracing::warn!(%error, event_type, "session event write failed");
            false
        } else {
            true
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
                "step/start",
                "step.text",
                "tool.called",
                "tool.returned",
                "step/end",
                "message.created",
                "turn.completed"
            ]
        );
        let text = events
            .iter()
            .find(|event| event.event_type == "step.text")
            .expect("step text");
        assert_eq!(text.payload_json["text"], "hi");
        assert!(text.payload_json["turnId"].as_str().is_some_and(|id| !id.is_empty()));
        let step_start = events
            .iter()
            .find(|event| event.event_type == "step/start")
            .expect("step start");
        assert_eq!(step_start.payload_json["index"], 1);
        assert!(
            step_start.payload_json["stepId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let completed = events
            .iter()
            .find(|event| event.event_type == "turn.completed")
            .expect("turn completed");
        assert_eq!(completed.payload_json["outcome"], "completed");
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
            .run_turn(
                &ScriptedLoop::failed("aborted"),
                &ctx(),
                "hello",
                None,
                Some(TurnCancellation::user(cancel)),
            )
            .await;

        assert_eq!(
            outcome,
            TurnOutcome::Cancelled {
                reason: "用户主动停止本轮执行".into(),
                source: "user".into(),
            }
        );
        let events = sink.events.lock().await;
        let payload = completed_payload(&events);
        assert_eq!(payload["outcome"], "cancelled");
        assert_eq!(payload["error"], "用户主动停止本轮执行");
        assert_eq!(payload["source"], "user");
    }

    #[tokio::test]
    async fn retry_events_are_persisted_with_attempt_and_delay() {
        let (runner, sink) = runner_and_sink();
        let notice = tinyiothub_agent::port::events::RetryNotice {
            attempt: 2,
            max_retries: 3,
            delay_ms: 2000,
            error: "HTTP 503: unavailable".into(),
        };
        let outcome = runner
            .run_turn(
                &ScriptedLoop::completed(vec![TurnEvent::Retry(notice)]),
                &ctx(),
                "hello",
                None,
                None,
            )
            .await;
        assert!(matches!(outcome, TurnOutcome::Completed { .. }));
        let events = sink.events.lock().await;
        let retry = events
            .iter()
            .find(|event| event.event_type == "turn.retry")
            .expect("turn.retry emitted");
        assert_eq!(
            retry.payload_json,
            json!({
                "turnId": retry.payload_json["turnId"],
                "attempt": 2,
                "maxRetries": 3,
                "delayMs": 2000,
                "error": "HTTP 503: unavailable"
            })
        );
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
        assert_eq!(payload["source"], "system");
    }
}
