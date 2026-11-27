//! SessionRunner — 一次 Assistant Turn 的唯一执行入口。
//!
//! 它只负责：安装 TurnContext、驱动 AgentLoop、把 TurnEvent 写入 session_events、
//! 通知可选 observer（聊天 SSE 适配器）、收尾。消息持久化/反思仍由 chat 组合层
//! 负责，避免 SessionRunner 依赖 chat 的存储历史。

use std::sync::Arc;

use crate::domains::agent::runtime::session_live::SessionLiveFrame;
use serde_json::json;
use tinyiothub_agent::harness::{AgendaItem, AgendaStatus};
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::runtime::AgentLoop;
use tinyiothub_core::session_protocol::{AssistantStreamSegment, StreamKind};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::agenda::load_turn_agenda;
use super::context::{TurnContext, TurnContextRegistry};
use super::events::{SessionEventSink, session_event};
use super::suppression::audit::{WakeAttempt, attempt_event};
use super::todos::TodoStore;
use super::types::{Todo, TodoStatus};

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

fn push_assistant_segment(segments: &mut Vec<AssistantStreamSegment>, kind: StreamKind, text: &str, at_ms: u64) {
    if let Some(last) = segments.last_mut()
        && last.kind == kind
    {
        last.text.push_str(text);
        return;
    }
    segments.push(AssistantStreamSegment {
        kind,
        at_ms,
        text: text.to_string(),
    });
}

async fn emit_assistant_message(
    events: &Arc<dyn SessionEventSink>,
    context: &TurnContext,
    step_id: &str,
    text: &str,
    reasoning: &str,
    stream: &[AssistantStreamSegment],
    usage: Option<&serde_json::Value>,
) -> bool {
    let mut payload = json!({
        "turnId": &context.turn_id,
        "stepId": step_id,
        "messageId": uuid::Uuid::new_v4().to_string(),
        "text": text,
        "reasoning": reasoning,
        "stream": stream,
    });
    if let Some(usage) = usage {
        payload["usage"] = usage.clone();
    }
    events
        .emit(session_event(context, "assistant/message", payload))
        .await
        .is_ok()
}

async fn emit_assistant_attempt(
    events: &Arc<dyn SessionEventSink>,
    context: &TurnContext,
    step_id: &str,
    attempt_id: &str,
    attempt: (u32, u32),
    stream: &[AssistantStreamSegment],
    error: &str,
    delay_ms: u64,
) -> bool {
    let (attempt, max_attempts) = attempt;
    events
        .emit(session_event(
            context,
            "assistant/attempt",
            json!({
                "turnId": &context.turn_id,
                "stepId": step_id,
                "attemptId": attempt_id,
                "attempt": attempt,
                "maxAttempts": max_attempts,
                "stream": stream,
                "error": error,
                "retryDelayMs": delay_ms,
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
/// persisted and the `turn/end` terminal was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnEventHealth {
    pub failed: u32,
    /// The `turn/end` terminal event was persisted.
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
    todos: Option<Arc<TodoStore>>,
}

/// Accumulates per-model-call usage into one `turn/end` payload.
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
        Self {
            events,
            contexts,
            todos: None,
        }
    }

    /// Wire the session todo cache so a completed model answer cannot silently
    /// finalize a turn whose plan still has pending work.
    pub fn with_todos(mut self, todos: Arc<TodoStore>) -> Self {
        self.todos = Some(todos);
        self
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
    /// `turn/start`/`turn/end` share the attempt-derived ids used by
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
        if !self.emit_turn_event(context, attempt, "turn/start", started).await {
            health.failed += 1;
        }
        let agenda = load_turn_agenda(&context.workspace_id).await.unwrap_or_default();
        if !agenda.is_empty() {
            let _ = self
                .emit_turn_event(
                    context,
                    attempt,
                    "agenda/set",
                    json!({
                        "turnId": &context.turn_id,
                        "items": &agenda,
                    }),
                )
                .await;
            if let Some(store) = &self.todos
                && store
                    .get(&context.session_key)
                    .iter()
                    .all(|todo| todo.status == TodoStatus::Completed)
            {
                let seeded = agenda_todos(&agenda);
                store.replace(&context.session_key, seeded.clone());
                let _ = self
                    .emit_turn_event(
                        context,
                        attempt,
                        "todo/write",
                        json!({
                            "turnId": &context.turn_id,
                            "todos": &seeded,
                        }),
                    )
                    .await;
            }
        }

        let (event_tx, mut event_rx) = mpsc::channel::<TurnEvent>(64);
        let forward = tokio::spawn({
            let events = self.events.clone();
            let context = context.clone();
            let observer = observer.clone();
            let events_for_agenda = self.events.clone();
            let agenda_context = context.clone();
            #[allow(unused_assignments)]
            async move {
                let mut agenda = agenda;
                let mut tool_calls = 0u32;
                // One Usage event is emitted per model call; this is the
                // server-side model-call count used by gate evidence.
                let mut model_calls = 0u64;
                let mut usage = UsageTotals::default();
                let mut failed = 0u32;
                let mut step_index = 0u32;
                let mut step_open = false;
                let mut step_usage_seen = false;
                let mut step_started = std::time::Instant::now();
                let mut step_text = String::new();
                let mut step_reasoning = String::new();
                let mut step_stream: Vec<AssistantStreamSegment> = Vec::new();
                let mut step_assistant_written = false;
                let mut step_attempt_id = uuid::Uuid::new_v4().to_string();
                let mut step_usage: Option<serde_json::Value> = None;

                macro_rules! step_id {
                    () => {
                        format!("{}:s{}", context.turn_id, step_index)
                    };
                }

                macro_rules! open_step {
                    () => {{
                        step_index += 1;
                        step_open = true;
                        step_usage_seen = false;
                        step_assistant_written = false;
                        step_attempt_id = uuid::Uuid::new_v4().to_string();
                        step_started = std::time::Instant::now();
                        step_text.clear();
                        step_reasoning.clear();
                        step_stream.clear();
                        step_usage = None;
                        if !emit_step_marker(&events, &context, "step/start", step_index).await {
                            failed += 1;
                        }
                        events
                            .live(
                                &context.session_key,
                                SessionLiveFrame::StreamStart {
                                    stream_id: step_id!(),
                                    turn_id: context.turn_id.clone(),
                                    step_id: step_id!(),
                                    attempt_id: step_attempt_id.clone(),
                                },
                            )
                            .await;
                    }};
                }

                macro_rules! close_step {
                    () => {{
                        if step_open {
                            if !step_assistant_written && (!step_text.is_empty() || !step_reasoning.is_empty()) {
                                if !emit_assistant_message(
                                    &events,
                                    &context,
                                    &step_id!(),
                                    &step_text,
                                    &step_reasoning,
                                    &step_stream,
                                    step_usage.as_ref(),
                                )
                                .await
                                {
                                    failed += 1;
                                }
                                step_assistant_written = true;
                            }
                            events
                                .live(
                                    &context.session_key,
                                    SessionLiveFrame::StreamEnd {
                                        stream_id: step_id!(),
                                        turn_id: context.turn_id.clone(),
                                        step_id: step_id!(),
                                        status: "completed".to_string(),
                                    },
                                )
                                .await;
                            if !emit_step_marker(&events, &context, "step/end", step_index).await {
                                failed += 1;
                            }
                            step_open = false;
                        }
                    }};
                }

                while let Some(event) = event_rx.recv().await {
                    match &event {
                        TurnEvent::Chunk { delta } => {
                            if step_open && step_usage_seen {
                                close_step!();
                            }
                            if !step_open {
                                open_step!();
                            }
                            let at_ms = step_started.elapsed().as_millis() as u64;
                            step_text.push_str(delta);
                            push_assistant_segment(&mut step_stream, StreamKind::Text, delta, at_ms);
                            events
                                .live(
                                    &context.session_key,
                                    SessionLiveFrame::StreamDelta {
                                        stream_id: step_id!(),
                                        turn_id: context.turn_id.clone(),
                                        step_id: step_id!(),
                                        kind: StreamKind::Text,
                                        text: delta.clone(),
                                    },
                                )
                                .await;
                        }
                        TurnEvent::ToolCheckpoint { id, name, ack } => {
                            let persisted = events
                                .emit(session_event(
                                    &context,
                                    "tool/checkpoint",
                                    json!({
                                        "turnId": &context.turn_id,
                                        "toolCallId": &id,
                                        "tool": &name,
                                    }),
                                ))
                                .await
                                .is_ok();
                            ack.complete(persisted);
                        }
                        TurnEvent::ToolCall { id, name, args } => {
                            if !step_open {
                                open_step!();
                            }
                            tool_calls += 1;
                            if let Some(key) = agenda_key(&args) {
                                if let Some(index) = agenda.iter().position(|item| item.key == key) {
                                    let changed = agenda[index].status != AgendaStatus::InProgress;
                                    // Loading the next skill does not prove the previous skill's
                                    // steps ran. Keep prior skills open until Todo completion or
                                    // final settlement; otherwise agenda jumps to N/N at load time.
                                    agenda[index].status = AgendaStatus::InProgress;
                                    if changed {
                                        let _ = events_for_agenda
                                            .emit(session_event(
                                                &agenda_context,
                                                "agenda/update",
                                                json!({
                                                    "turnId": &agenda_context.turn_id,
                                                    "items": &agenda,
                                                }),
                                            ))
                                            .await;
                                    }
                                }
                            }
                            if events
                                .emit(session_event(
                                    &context,
                                    "tool/call",
                                    json!({
                                        "turnId": &context.turn_id,
                                        "stepId": step_id!(),
                                        "toolCallId": id,
                                        "tool": name,
                                        "viewKey": crate::domains::agent::tools::catalog::tool_view_key(name),
                                        "args": args
                                    }),
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
                            let evidence: Vec<tinyiothub_core::media::EvidenceRef> =
                                attachments.iter().map(Into::into).collect();
                            if events
                                .emit(session_event(
                                    &context,
                                    "tool/result",
                                    json!({
                                        "turnId": &context.turn_id,
                                        "stepId": if step_open { Some(step_id!()) } else { None },
                                        "toolCallId": id,
                                        "tool": name,
                                        "viewKey": crate::domains::agent::tools::catalog::tool_view_key(name),
                                        "result": truncate(output, 64_000),
                                        "evidence": evidence
                                    }),
                                ))
                                .await
                                .is_err()
                            {
                                failed += 1;
                            }
                            if name == "todo_write" && todo_result_all_completed(output) {
                                let unresolved = agenda.iter().any(|item| {
                                    !matches!(
                                        item.status,
                                        AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked
                                    )
                                });
                                if unresolved {
                                    settle_agenda(&mut agenda);
                                    let _ = events_for_agenda
                                        .emit(session_event(
                                            &agenda_context,
                                            "agenda/update",
                                            json!({
                                                "turnId": &agenda_context.turn_id,
                                                "items": &agenda,
                                            }),
                                        ))
                                        .await;
                                }
                            }
                        }
                        TurnEvent::Thinking { delta } => {
                            if step_open && step_usage_seen {
                                close_step!();
                            }
                            if !step_open {
                                open_step!();
                            }
                            let at_ms = step_started.elapsed().as_millis() as u64;
                            step_reasoning.push_str(delta);
                            push_assistant_segment(&mut step_stream, StreamKind::Reasoning, delta, at_ms);
                            events
                                .live(
                                    &context.session_key,
                                    SessionLiveFrame::StreamDelta {
                                        stream_id: step_id!(),
                                        turn_id: context.turn_id.clone(),
                                        step_id: step_id!(),
                                        kind: StreamKind::Reasoning,
                                        text: delta.clone(),
                                    },
                                )
                                .await;
                        }
                        TurnEvent::Retry(notice) => {
                            if !step_open {
                                open_step!();
                            }
                            events
                                .live(
                                    &context.session_key,
                                    SessionLiveFrame::StreamEnd {
                                        stream_id: step_id!(),
                                        turn_id: context.turn_id.clone(),
                                        step_id: step_id!(),
                                        status: "retrying".to_string(),
                                    },
                                )
                                .await;
                            if !emit_assistant_attempt(
                                &events,
                                &context,
                                &step_id!(),
                                &step_attempt_id,
                                (notice.attempt, notice.max_retries + 1),
                                &step_stream,
                                &notice.error,
                                notice.delay_ms,
                            )
                            .await
                            {
                                failed += 1;
                            }
                            step_attempt_id = uuid::Uuid::new_v4().to_string();
                            step_usage_seen = false;
                            step_assistant_written = false;
                            step_text.clear();
                            step_reasoning.clear();
                            step_stream.clear();
                            step_usage = None;
                        }
                        TurnEvent::Usage {
                            input_tokens,
                            cached_input_tokens,
                            output_tokens,
                            cost_usd,
                        } => {
                            if !step_open {
                                open_step!();
                            }
                            step_usage_seen = true;
                            step_usage = Some(json!({
                                "inputTokens": input_tokens,
                                "cachedInputTokens": cached_input_tokens,
                                "outputTokens": output_tokens,
                                "costUsd": cost_usd,
                            }));
                            model_calls += 1;
                            usage.add(*input_tokens, *cached_input_tokens, *output_tokens, *cost_usd);
                            if !step_assistant_written {
                                if !emit_assistant_message(
                                    &events,
                                    &context,
                                    &step_id!(),
                                    &step_text,
                                    &step_reasoning,
                                    &step_stream,
                                    step_usage.as_ref(),
                                )
                                .await
                                {
                                    failed += 1;
                                }
                                step_assistant_written = true;
                            }
                        }
                        _ => {}
                    }
                    if let Some(observer) = &observer {
                        observer.on_turn_event(&event).await;
                    }
                }
                close_step!();
                (tool_calls, model_calls, usage.to_json(), failed, agenda)
            }
        });

        let cancel_token = cancel.as_ref().map(|cancellation| cancellation.token.clone());
        let result = agent.turn_streamed(message, event_tx, cancel_token).await;
        let (tool_calls, model_calls, usage, forward_failed, mut agenda) =
            forward.await.unwrap_or((0, 0, serde_json::Value::Null, 1, Vec::new()));
        health.failed += forward_failed;

        let outcome = match result {
            Ok(text) => {
                let pending = self
                    .todos
                    .as_ref()
                    .map(|store| store.get(&context.session_key))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|todo| todo.status != TodoStatus::Completed)
                    .collect::<Vec<_>>();
                // A final answer with no pending Todo means the currently
                // active skill has finished. Settle that skill explicitly so a
                // normal turn does not stay partial merely because it was the
                // last skill loaded. Pending skills remain unresolved.
                if pending.is_empty()
                    && agenda.iter().any(|item| {
                        !matches!(
                            item.status,
                            AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked
                        )
                    })
                {
                    for item in &mut agenda {
                        if !matches!(
                            item.status,
                            AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked
                        ) {
                            item.status = AgendaStatus::Completed;
                        }
                    }
                    let _ = self
                        .emit_turn_event(
                            context,
                            attempt,
                            "agenda/update",
                            json!({
                                "turnId": &context.turn_id,
                                "items": &agenda,
                            }),
                        )
                        .await;
                }
                let pending_agenda = agenda
                    .iter()
                    .filter(|item| {
                        !matches!(
                            item.status,
                            AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked
                        )
                    })
                    .collect::<Vec<_>>();
                if !pending.is_empty() || !pending_agenda.is_empty() {
                    let _ = self
                        .emit_turn_event(
                            context,
                            attempt,
                            "turn/incomplete",
                            json!({
                                "turnId": &context.turn_id,
                                "pending": pending.iter().map(|todo| json!({
                                    "content": &todo.content,
                                    "status": todo.status,
                                })).collect::<Vec<_>>(),
                                "pendingAgenda": pending_agenda.iter().map(|item| json!({
                                    "key": &item.key,
                                    "label": &item.label,
                                    "status": item.status,
                                })).collect::<Vec<_>>(),
                                "reason": "final answer arrived while the task plan still has pending items",
                            }),
                        )
                        .await;
                }
                health.terminal_written = self
                    .emit_turn_event(
                        context,
                        attempt,
                        "turn/end",
                        json!({
                            "turnId": &context.turn_id,
                            "outcome": if pending.is_empty() && agenda.iter().all(|item| matches!(item.status, AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked)) { "completed" } else { "partial" },
                            "text": &text,
                            "toolCalls": tool_calls,
                            "modelCalls": model_calls,
                            "usage": &usage,
                            "incomplete": !pending.is_empty() || !agenda.iter().all(|item| matches!(item.status, AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked)),
                            "pendingTodos": pending.iter().map(|todo| &todo.content).collect::<Vec<_>>(),
                            "pendingAgenda": agenda.iter().filter(|item| !matches!(item.status, AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked)).map(|item| &item.key).collect::<Vec<_>>(),
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
                health.terminal_written = self.emit_turn_event(context, attempt, "turn/end", payload).await;
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
            Some(attempt) if matches!(event_type, "turn/start" | "turn/end") => {
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

fn todo_result_all_completed(output: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return false;
    };
    value.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
        && value
            .get("todos")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|todos| {
                !todos.is_empty()
                    && todos
                        .iter()
                        .all(|todo| todo.get("status").and_then(serde_json::Value::as_str) == Some("completed"))
            })
}

fn settle_agenda(agenda: &mut [AgendaItem]) {
    for item in agenda {
        if !matches!(
            item.status,
            AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Blocked
        ) {
            item.status = AgendaStatus::Completed;
        }
    }
}

fn agenda_todos(agenda: &[AgendaItem]) -> Vec<Todo> {
    agenda
        .iter()
        .enumerate()
        .map(|(index, item)| Todo {
            content: item.label.clone(),
            status: if index == 0 {
                TodoStatus::InProgress
            } else {
                TodoStatus::Pending
            },
            active_form: item.label.clone(),
        })
        .collect()
}

fn agenda_key(args: &serde_json::Value) -> Option<&str> {
    ["skill_name", "skillName", "name"]
        .iter()
        .find_map(|key| args.get(*key).and_then(|value| value.as_str()))
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
    struct CheckpointLoop;

    #[async_trait::async_trait]
    impl AgentLoop for CheckpointLoop {
        async fn turn_streamed(
            &self,
            _message: &str,
            tx: tokio::sync::mpsc::Sender<crate::domains::agent::session::runner::TurnEvent>,
            _cancel: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            let ack = tinyiothub_agent::port::events::ToolCheckpointAck::new();
            tx.send(crate::domains::agent::session::runner::TurnEvent::ToolCheckpoint {
                id: "c1".into(),
                name: "read_device".into(),
                ack: ack.clone(),
            })
            .await
            .unwrap();
            assert!(ack.wait().await, "runner must acknowledge the checkpoint");
            tx.send(crate::domains::agent::session::runner::TurnEvent::Chunk { delta: "ok".into() })
                .await
                .unwrap();
            Ok("ok".into())
        }

        async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
            Ok(())
        }
    }

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
            .find(|event| event.event_type == "turn/end")
            .expect("turn/end emitted");
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
                "turn/start",
                "step/start",
                "tool/call",
                "tool/result",
                "assistant/message",
                "step/end",
                "turn/end"
            ]
        );
        let tool_call = events
            .iter()
            .find(|event| event.event_type == "tool/call")
            .expect("tool call");
        assert_eq!(tool_call.payload_json["viewKey"], "todo.write");
        let tool_result = events
            .iter()
            .find(|event| event.event_type == "tool/result")
            .expect("tool result");
        assert_eq!(tool_result.payload_json["viewKey"], "todo.write");
        let text = events
            .iter()
            .find(|event| event.event_type == "assistant/message")
            .expect("assistant message");
        assert_eq!(text.payload_json["text"], "hi");
        assert!(text.payload_json["turnId"].as_str().is_some_and(|id| !id.is_empty()));
        assert!(text.payload_json["stepId"].as_str().is_some_and(|id| !id.is_empty()));
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
            .find(|event| event.event_type == "turn/end")
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

    #[test]
    fn settling_completes_pending_and_in_progress_skills() {
        let mut agenda = vec![
            AgendaItem {
                key: "inspection".into(),
                label: "巡检".into(),
                status: AgendaStatus::Pending,
            },
            AgendaItem {
                key: "alarm".into(),
                label: "告警".into(),
                status: AgendaStatus::InProgress,
            },
            AgendaItem {
                key: "skip".into(),
                label: "跳过".into(),
                status: AgendaStatus::Skipped,
            },
        ];
        super::settle_agenda(&mut agenda);
        assert_eq!(agenda[0].status, AgendaStatus::Completed);
        assert_eq!(agenda[1].status, AgendaStatus::Completed);
        assert_eq!(agenda[2].status, AgendaStatus::Skipped);
    }

    #[test]
    fn agenda_seeds_one_todo_per_skill() {
        let agenda = vec![
            AgendaItem {
                key: "inspection".into(),
                label: "巡检".into(),
                status: AgendaStatus::Pending,
            },
            AgendaItem {
                key: "energy".into(),
                label: "节能".into(),
                status: AgendaStatus::Pending,
            },
        ];
        let todos = super::agenda_todos(&agenda);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].content, "巡检");
        assert_eq!(todos[0].status, TodoStatus::InProgress);
        assert_eq!(todos[1].status, TodoStatus::Pending);
    }

    #[test]
    fn todo_result_settles_agenda_only_after_a_successful_all_completed_write() {
        assert!(super::todo_result_all_completed(
            r#"{"ok":true,"todos":[{"content":"a","status":"completed"},{"content":"b","status":"completed"}]}"#
        ));
        assert!(!super::todo_result_all_completed(
            r#"{"ok":true,"todos":[{"content":"a","status":"completed"},{"content":"b","status":"pending"}]}"#
        ));
        assert!(!super::todo_result_all_completed(
            r#"{"success":false,"error":"only one todo may move to completed"}"#
        ));
    }

    #[tokio::test]
    async fn runner_persists_and_acknowledges_tool_checkpoint() {
        let (runner, sink) = runner_and_sink();
        let outcome = runner.run_turn(&CheckpointLoop, &ctx(), "go", None, None).await;
        assert!(matches!(outcome, TurnOutcome::Completed { .. }));
        let events = sink.events.lock().await;
        assert!(events.iter().any(|event| event.event_type == "tool/checkpoint"));
    }

    #[tokio::test]
    async fn completed_turn_with_pending_todos_is_marked_incomplete() {
        let (runner, sink) = runner_and_sink();
        let todos = Arc::new(TodoStore::default());
        todos.replace(
            "agent:ws:agent_main/s1",
            vec![super::super::types::Todo {
                content: "检查告警".into(),
                status: TodoStatus::Pending,
                active_form: "检查告警".into(),
            }],
        );
        let runner = runner.with_todos(todos);

        let outcome = runner
            .run_turn(&ScriptedLoop::completed(Vec::new()), &ctx(), "hello", None, None)
            .await;

        assert!(matches!(outcome, TurnOutcome::Completed { .. }));
        let events = sink.events.lock().await;
        let incomplete = events
            .iter()
            .find(|event| event.event_type == "turn/incomplete")
            .expect("incomplete event");
        assert_eq!(incomplete.payload_json["pending"][0]["content"], "检查告警");
        let end = events
            .iter()
            .find(|event| event.event_type == "turn/end")
            .expect("turn end");
        assert_eq!(end.payload_json["incomplete"], true);
        assert_eq!(end.payload_json["pendingTodos"][0], "检查告警");
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
            .find(|event| event.event_type == "assistant/attempt")
            .expect("assistant/attempt emitted");
        assert_eq!(retry.payload_json["attempt"], 2);
        assert_eq!(retry.payload_json["maxAttempts"], 4);
        assert_eq!(retry.payload_json["retryDelayMs"], 2000);
        assert_eq!(retry.payload_json["error"], "HTTP 503: unavailable");
        assert!(
            retry.payload_json["attemptId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
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
        assert!(index("turn/start") < index("tool/call"));
        assert!(index("tool/result") < index("assistant/message"));
        assert!(index("assistant/message") < index("step/end"));
        assert!(index("step/end") < index("turn/end"));
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
