//! Streaming runner — consumes `turn_streamed` events into a tool trajectory,
//! enforces hard budgets (25 tool calls / 5 min) via best-effort abort
//! (CancellationToken, T1 A 方案）, judges `verified` objectively from the
//! trajectory, and synthesizes the summary from the trajectory when the LLM
//! produced no usable text (spec v3 O1/O9).
//!
//! Layout (pure logic thick, zeroclaw wiring thin):
//! - [`RunContextInner::record_event`], [`build_actions`], [`judge_verified`],
//!   [`synthesize_summary`], [`assemble_report`] are pure functions over the trajectory —
//!   unit-tested directly with replayed `TurnEvent` sequences.
//! - [`Runner::execute`] is the thin wiring: channel + forward task + cancel token +
//!   `turn_streamed` under a timeout, then report assembly.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;
use zeroclaw::agent::TurnEvent;
use zeroclaw::agent::loop_::is_tool_loop_cancelled;

use crate::domains::agent::loop_::thing_agent::types::{ActionRecord, ActionResult, Outcome, RunReport};

/// Hard per-run tool-call budget (spec O9). The (N+1)-th ToolCall event
/// triggers cancel; the N already dispatched may complete.
pub const MAX_TOOL_CALLS_PER_RUN: u32 = 25;
/// Hard per-run wall-clock budget. The forward task cancels at the deadline.
pub const MAX_RUN_DURATION: Duration = Duration::from_secs(300);
/// Backstop grace on top of [`MAX_RUN_DURATION`] for the outer
/// `tokio::time::timeout` — normally the forward task's deadline cancel lands
/// first; this only fires if the turn fails to honour cancellation.
const TURN_TIMEOUT_GRACE: Duration = Duration::from_secs(30);

/// Tool names of the cloud thing tools
/// (cloud/src/modules/agent/tools/thing.rs). Duplicated here because the
/// trajectory judgment is name-based and tinyiothub-ai must not depend on
/// cloud.
const TOOL_INVOKE_ACTION: &str = "invoke_action";
const TOOL_READ_PROPERTY: &str = "read_property";
const TOOL_QUERY_EVENTS: &str = "query_events";

/// Shared agent handle (T11 factory injects one per workspace).
pub type AgentHandle = Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>;

/// Why a run was truncated by a hard budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationReason {
    /// More than [`MAX_TOOL_CALLS_PER_RUN`] ToolCall events observed.
    ToolCallBudget,
    /// Run exceeded [`MAX_RUN_DURATION`].
    DurationBudget,
}

/// One recorded tool interaction, in event arrival order. `output` is filled
/// in when the matching `ToolResult` arrives (paired by stable `id`).
#[derive(Debug, Clone)]
pub struct ToolTraceEntry {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    /// `None` = no paired ToolResult (cancelled in flight / stream ended).
    pub output: Option<String>,
}

/// Mutable per-run state shared with the thing tools (O8): tools read the
/// per-thing action counts for their in-tool policy gates (T11); the runner's
/// forward task records the trajectory.
#[derive(Debug, Default)]
pub struct RunContextInner {
    /// Human-readable trigger description (built by T10 prompt assembly).
    pub trigger: String,
    /// Tool call/result trajectory in arrival order.
    pub trace: Vec<ToolTraceEntry>,
    /// Total ToolCall events observed this run.
    pub tool_calls: u32,
    /// thing_id → invoke_action dispatch count (T11 per-thing caps).
    pub action_counts: HashMap<String, u32>,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    /// Sticky: first budget violation that triggered cancel.
    pub truncated: Option<TruncationReason>,
    /// The tool-call budget in force when [`TruncationReason::ToolCallBudget`]
    /// first fired. Parallel dispatch batches can push `tool_calls` several
    /// past the budget in one shot, so the summary must report this, not
    /// `tool_calls - 1`.
    pub tool_call_budget: Option<u32>,
}

pub struct RunContext {
    pub run_id: String,
    pub workspace_id: String,
    pub inner: Arc<RwLock<RunContextInner>>,
}

impl RunContext {
    pub fn new(run_id: String, workspace_id: String, trigger: String) -> Self {
        Self {
            run_id,
            workspace_id,
            inner: Arc::new(RwLock::new(RunContextInner {
                trigger,
                ..RunContextInner::default()
            })),
        }
    }
}

pub struct RunOutcome {
    pub report: RunReport,
    /// The LLM's own final text, if it produced a non-empty one. When `None`
    /// the report summary was synthesized from the trajectory.
    pub llm_text: Option<String>,
}

pub struct Runner {
    max_tool_calls: u32,
    max_duration: Duration,
}

impl Default for Runner {
    fn default() -> Self {
        Self {
            max_tool_calls: MAX_TOOL_CALLS_PER_RUN,
            max_duration: MAX_RUN_DURATION,
        }
    }
}

impl Runner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Custom budgets (tests; production uses [`Runner::new`]).
    pub fn with_budget(max_tool_calls: u32, max_duration: Duration) -> Self {
        Self {
            max_tool_calls,
            max_duration,
        }
    }

    pub async fn execute(&self, agent: AgentHandle, prompt: String, ctx: RunContext) -> RunOutcome {
        let start = Instant::now();
        let (event_tx, event_rx) = mpsc::channel::<TurnEvent>(32);
        let cancel = CancellationToken::new();
        let consume = tokio::spawn(consume_events(
            event_rx,
            Arc::clone(&ctx.inner),
            cancel.clone(),
            self.max_tool_calls,
            self.max_duration,
        ));

        // One lock across the whole turn (chat/service.rs precedent). The
        // backstop timeout only fires if the turn ignores the cancel token;
        // normally the forward task's budget cancel lands first.
        let turn = {
            let mut ag = agent.lock().await;
            tokio::time::timeout(
                self.max_duration + TURN_TIMEOUT_GRACE,
                ag.turn_streamed(&prompt, event_tx, Some(cancel)),
            )
            .await
        };
        // event_tx dropped with the turn future → channel closes → consume ends.
        if let Err(e) = consume.await {
            tracing::warn!(error = %e, run_id = %ctx.run_id, "runner event consumer join failed");
        }

        let end = match turn {
            Ok(Ok((text, _))) if text.trim().is_empty() => TurnEnd::Empty,
            Ok(Ok((text, _))) => TurnEnd::Text(text),
            Ok(Err(e)) if is_tool_loop_cancelled(&e) => TurnEnd::Cancelled,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, run_id = %ctx.run_id, "thing-agent turn failed");
                TurnEnd::Failed
            }
            Err(_) => TurnEnd::TimedOut,
        };

        let inner = ctx.inner.read().await;
        assemble_report(&ctx, end, start.elapsed().as_millis() as u64, &inner)
    }
}

/// How the turn ended, distilled from the `turn_streamed` result. Pure input
/// to [`assemble_report`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum TurnEnd {
    /// Non-empty LLM final text.
    Text(String),
    /// Empty/whitespace LLM text.
    Empty,
    /// `Err(ToolLoopCancelled)` — our budget abort (or external cancel).
    Cancelled,
    /// Any other turn error — LLM/tool-loop failure.
    Failed,
    /// Outer backstop timeout fired (turn ignored cancellation / hung).
    TimedOut,
}

fn thing_id_of(args: &serde_json::Value) -> Option<String> {
    args.get("thingId").and_then(|v| v.as_str()).map(str::to_string)
}

impl RunContextInner {
    /// Record one streamed event. Returns the (sticky) truncation state; the
    /// caller must cancel the turn when this becomes `Some`.
    fn record_event(
        &mut self,
        evt: &TurnEvent,
        elapsed: Duration,
        max_tool_calls: u32,
        max_duration: Duration,
    ) -> Option<TruncationReason> {
        match evt {
            TurnEvent::ToolCall { id, name, args } => {
                self.tool_calls += 1;
                if name == TOOL_INVOKE_ACTION
                    && let Some(thing_id) = thing_id_of(args)
                {
                    *self.action_counts.entry(thing_id).or_insert(0) += 1;
                }
                self.trace.push(ToolTraceEntry {
                    id: id.clone(),
                    name: name.clone(),
                    args: args.clone(),
                    output: None,
                });
                // Fire-and-forget (O1): the tool is already dispatched when we
                // see the event, so the budget stops the NEXT call, not this
                // one. The excess call stays in the trace (unpaired →
                // UnknownCancelled once the abort drops it).
                if self.truncated.is_none() && self.tool_calls > max_tool_calls {
                    self.truncated = Some(TruncationReason::ToolCallBudget);
                    self.tool_call_budget = Some(max_tool_calls);
                }
            }
            TurnEvent::ToolResult { id, output, .. } => {
                // Pair by stable id; latest unpaired wins (ids are unique per
                // call, so this is exact matching, not heuristic).
                if let Some(entry) = self.trace.iter_mut().rev().find(|e| e.id == *id && e.output.is_none()) {
                    entry.output = Some(output.clone());
                }
            }
            TurnEvent::Usage {
                input_tokens,
                cached_input_tokens,
                output_tokens,
                ..
            } => {
                self.input_tokens += input_tokens.unwrap_or(0);
                self.cached_input_tokens += cached_input_tokens.unwrap_or(0);
                self.output_tokens += output_tokens.unwrap_or(0);
            }
            _ => {}
        }
        if self.truncated.is_none() && elapsed >= max_duration {
            self.truncated = Some(TruncationReason::DurationBudget);
        }
        self.truncated
    }
}

/// Forward task: consume the event stream into the shared run context,
/// cancelling the turn on the first budget violation, then keep draining
/// until the sender drops (turn ended) so zeroclaw never blocks on a full
/// channel.
async fn consume_events(
    mut rx: mpsc::Receiver<TurnEvent>,
    inner: Arc<RwLock<RunContextInner>>,
    cancel: CancellationToken,
    max_tool_calls: u32,
    max_duration: Duration,
) {
    let start = tokio::time::Instant::now();
    let deadline = start + max_duration;
    let mut deadline_fired = false;
    loop {
        tokio::select! {
            evt = rx.recv() => {
                let Some(evt) = evt else { break };
                let truncated = inner
                    .write()
                    .await
                    .record_event(&evt, start.elapsed(), max_tool_calls, max_duration);
                if truncated.is_some() {
                    cancel.cancel();
                }
            }
            () = tokio::time::sleep_until(deadline), if !deadline_fired => {
                deadline_fired = true;
                let mut guard = inner.write().await;
                if guard.truncated.is_none() {
                    guard.truncated = Some(TruncationReason::DurationBudget);
                }
                cancel.cancel();
                // Keep draining: the turn honours cancel within ms (T1 spike),
                // but draining also guarantees zeroclaw never blocks on a full
                // channel while it winds down.
            }
        }
    }
}

/// Project the trajectory into action records: every `invoke_action` entry
/// becomes one [`ActionRecord`]. A paired result parses as JSON →
/// [`ActionResult::Success`] (cloud `tool_ok` always serializes payloads to
/// JSON); a non-JSON output → [`ActionResult::Failed`] (cloud `tool_err`
/// returns a plain message); no paired result →
/// [`ActionResult::UnknownCancelled`].
pub fn build_actions(trace: &[ToolTraceEntry]) -> Vec<ActionRecord> {
    trace
        .iter()
        .filter(|e| e.name == TOOL_INVOKE_ACTION)
        .map(|e| ActionRecord {
            thing_id: thing_id_of(&e.args).unwrap_or_default(),
            action_name: e
                .args
                .get("actionName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            params: e.args.get("params").cloned().unwrap_or(serde_json::Value::Null),
            result: match &e.output {
                None => ActionResult::UnknownCancelled,
                Some(out) => match serde_json::from_str::<serde_json::Value>(out) {
                    Ok(v) => ActionResult::Success(v),
                    Err(_) => ActionResult::Failed(out.clone()),
                },
            },
            verified: false,
        })
        .collect()
}

/// Objective verified judgment (R1, no trust in LLM self-report): an action
/// is verified iff a LATER trace entry is a completed (`output` present)
/// `read_property`/`query_events` for the same thing_id. Sets per-action
/// flags; returns the run-level verdict = all actions verified (vacuously
/// true with no actions).
pub fn judge_verified(trace: &[ToolTraceEntry], actions: &mut [ActionRecord]) -> bool {
    let mut next_action = 0;
    for (i, e) in trace.iter().enumerate() {
        if e.name != TOOL_INVOKE_ACTION {
            continue;
        }
        let thing_id = thing_id_of(&e.args).unwrap_or_default();
        let verified = !thing_id.is_empty()
            && trace[i + 1..].iter().any(|later| {
                later.output.is_some()
                    && (later.name == TOOL_READ_PROPERTY || later.name == TOOL_QUERY_EVENTS)
                    && thing_id_of(&later.args).as_deref() == Some(thing_id.as_str())
            });
        if let Some(action) = actions.get_mut(next_action) {
            action.verified = verified;
        }
        next_action += 1;
    }
    actions.iter().all(|a| a.verified)
}

/// T11 结构化拒绝结果（`{"denied":true,"reason":...}`）判定。
fn is_denied_result(v: &serde_json::Value) -> bool {
    v.get("denied").and_then(|d| d.as_bool()) == Some(true)
}

/// X5/T17：run 内 LLM 尝试的所有 invoke_action 均被策略门拒绝。
/// 零动作返回 false（NoActionNeeded）；部分拒绝（有动作成功）返回 false（Acted）。
pub(crate) fn all_actions_policy_denied(actions: &[ActionRecord]) -> bool {
    !actions.is_empty()
        && actions
            .iter()
            .all(|a| matches!(&a.result, ActionResult::Success(v) if is_denied_result(v)))
}

/// Outcome 判定（纯函数）：预算截断 > LLM 失败 > 零动作尝试 > 全部策略拒绝 > Acted。
pub(crate) fn decide_outcome(
    truncated: Option<TruncationReason>,
    turn_failed: bool,
    actions: &[ActionRecord],
) -> Outcome {
    if truncated.is_some() {
        Outcome::BudgetExceeded
    } else if turn_failed {
        Outcome::Failed
    } else if actions.is_empty() {
        Outcome::NoActionNeeded
    } else if all_actions_policy_denied(actions) {
        Outcome::Rejected
    } else {
        Outcome::Acted
    }
}

/// Framework-synthesized summary (O1) for runs without usable LLM text:
/// trigger + action list with results + why the run ended
/// (budget truncation / policy rejection / LLM failure / timeout / no text).
fn synthesize_summary(inner: &RunContextInner, end: &TurnEnd, outcome: Outcome) -> String {
    use std::fmt::Write as _;

    let mut s = format!("触发: {}\n动作:", inner.trigger);
    let mut any = false;
    for e in inner.trace.iter().filter(|e| e.name == TOOL_INVOKE_ACTION) {
        any = true;
        let thing = thing_id_of(&e.args).unwrap_or_else(|| "?".into());
        let action = e.args.get("actionName").and_then(|v| v.as_str()).unwrap_or("?");
        let outcome_text = match &e.output {
            None => "已取消（截断时仍在执行）".to_string(),
            Some(out) => match serde_json::from_str::<serde_json::Value>(out) {
                Ok(v) if is_denied_result(&v) => {
                    let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("unknown");
                    format!("被策略拒绝（{reason}）")
                }
                Ok(_) => "成功".to_string(),
                Err(_) => format!("失败: {out}"),
            },
        };
        let _ = write!(s, "\n- {thing}.{action}: {outcome_text}");
    }
    if !any {
        s.push_str(" 无");
    }
    let reason = match end {
        TurnEnd::Cancelled | TurnEnd::Empty | TurnEnd::Text(_) => match inner.truncated {
            Some(TruncationReason::ToolCallBudget) => {
                let budget = inner
                    .tool_call_budget
                    .unwrap_or_else(|| inner.tool_calls.saturating_sub(1));
                format!("执行被预算截断（工具调用超过 {budget} 次）")
            }
            Some(TruncationReason::DurationBudget) => "执行被预算截断（时长超限）".to_string(),
            None if outcome == Outcome::Rejected => "动作被策略拒绝，建议检查自治策略配置".to_string(),
            None => "LLM 未产生总结文本".to_string(),
        },
        TurnEnd::Failed => "LLM 失败（turn 返回错误）".to_string(),
        TurnEnd::TimedOut => "LLM 失败（turn 超时未响应取消）".to_string(),
    };
    let _ = write!(s, "\n{reason}");
    s
}

/// Assemble the final report from the trajectory and turn ending.
fn assemble_report(ctx: &RunContext, end: TurnEnd, duration_ms: u64, inner: &RunContextInner) -> RunOutcome {
    let mut actions = build_actions(&inner.trace);
    let verified = judge_verified(&inner.trace, &mut actions);

    let outcome = decide_outcome(
        inner.truncated,
        matches!(end, TurnEnd::Failed | TurnEnd::TimedOut),
        &actions,
    );

    let (summary, llm_text) = match end {
        TurnEnd::Text(text) => (text.clone(), Some(text)),
        other => (synthesize_summary(inner, &other, outcome), None),
    };

    RunOutcome {
        report: RunReport {
            run_id: ctx.run_id.clone(),
            workspace_id: ctx.workspace_id.clone(),
            trigger: inner.trigger.clone(),
            outcome,
            summary,
            actions,
            verified,
            duration_ms,
            tool_calls: inner.tool_calls,
            tokens: inner.input_tokens + inner.cached_input_tokens + inner.output_tokens,
        },
        llm_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::loop_::thing_agent::types::ActionResult;

    fn call(id: &str, name: &str, args: serde_json::Value) -> TurnEvent {
        TurnEvent::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            args,
        }
    }

    fn result(id: &str, name: &str, output: &str) -> TurnEvent {
        TurnEvent::ToolResult {
            id: id.to_string(),
            name: name.to_string(),
            output: output.to_string(),
        }
    }

    fn invoke(id: &str, thing: &str, action: &str) -> TurnEvent {
        call(
            id,
            TOOL_INVOKE_ACTION,
            serde_json::json!({"thingId": thing, "actionName": action, "params": {"speed": 3}}),
        )
    }

    fn read(id: &str, thing: &str) -> TurnEvent {
        call(
            id,
            TOOL_READ_PROPERTY,
            serde_json::json!({"thingId": thing, "propertyName": "temp"}),
        )
    }

    fn entry(id: &str, name: &str, args: serde_json::Value, output: Option<&str>) -> ToolTraceEntry {
        ToolTraceEntry {
            id: id.to_string(),
            name: name.to_string(),
            args,
            output: output.map(str::to_string),
        }
    }

    /// StubAgent: replay a fixed TurnEvent sequence through the channel, then
    /// close it (sender drop = turn end).
    async fn run_stub(
        events: Vec<TurnEvent>,
        max_calls: u32,
        max_dur: Duration,
    ) -> (RunContextInner, CancellationToken) {
        let inner = Arc::new(RwLock::new(RunContextInner::default()));
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(32);
        let handle = tokio::spawn(consume_events(
            rx,
            Arc::clone(&inner),
            cancel.clone(),
            max_calls,
            max_dur,
        ));
        for evt in events {
            tx.send(evt).await.expect("send");
        }
        drop(tx);
        handle.await.expect("consume task");
        let inner = Arc::try_unwrap(inner).expect("single ref").into_inner();
        (inner, cancel)
    }

    #[tokio::test]
    async fn tool_call_budget_truncates_and_marks_excess_unknown_cancelled() {
        // Budget 3: feed 4 invoke_action calls but only 3 results (the 4th is
        // killed in flight by the abort, like a real turn would).
        let mut events = Vec::new();
        for i in 1..=3 {
            events.push(invoke(&format!("c{i}"), "t1", &format!("act{i}")));
            events.push(result(&format!("c{i}"), TOOL_INVOKE_ACTION, "{\"ok\":true}"));
        }
        events.push(invoke("c4", "t1", "act4"));

        let (inner, cancel) = run_stub(events, 3, Duration::from_secs(300)).await;

        assert_eq!(inner.truncated, Some(TruncationReason::ToolCallBudget));
        assert!(cancel.is_cancelled());
        assert_eq!(inner.tool_calls, 4);
        assert_eq!(inner.trace.len(), 4);
        assert_eq!(inner.action_counts.get("t1"), Some(&4));

        let actions = build_actions(&inner.trace);
        assert_eq!(actions.len(), 4);
        assert!(matches!(actions[0].result, ActionResult::Success(_)));
        assert!(matches!(actions[3].result, ActionResult::UnknownCancelled));
        assert_eq!(actions[3].action_name, "act4");
        assert_eq!(actions[0].params, serde_json::json!({"speed": 3}));
    }

    #[tokio::test(start_paused = true)]
    async fn duration_budget_truncates_at_deadline() {
        let inner = Arc::new(RwLock::new(RunContextInner::default()));
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(32);
        let handle = tokio::spawn(consume_events(
            rx,
            Arc::clone(&inner),
            cancel.clone(),
            25,
            Duration::from_secs(300),
        ));
        tx.send(read("r1", "t1")).await.expect("send");
        tx.send(result("r1", TOOL_READ_PROPERTY, "{\"value\":42}"))
            .await
            .expect("send");

        // Let the consume task poll once so its start/deadline anchor at t=0
        // (channel sends above complete without waking it).
        tokio::task::yield_now().await;

        // Before the deadline nothing is truncated.
        tokio::time::advance(Duration::from_secs(299)).await;
        tokio::task::yield_now().await;
        assert!(!cancel.is_cancelled());

        tokio::time::advance(Duration::from_secs(2)).await;
        // Timer wakeups are delivered by advance(); let the woken consume task
        // actually run (bounded, deterministic under paused time).
        for _ in 0..10 {
            if cancel.is_cancelled() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(cancel.is_cancelled());
        assert_eq!(inner.read().await.truncated, Some(TruncationReason::DurationBudget));

        drop(tx);
        handle.await.expect("consume task");
    }

    #[tokio::test]
    async fn usage_events_aggregate_tokens() {
        let events = vec![
            TurnEvent::Usage {
                input_tokens: Some(100),
                cached_input_tokens: Some(20),
                output_tokens: Some(7),
                cost_usd: Some(0.001),
            },
            TurnEvent::Usage {
                input_tokens: Some(50),
                cached_input_tokens: None,
                output_tokens: Some(3),
                cost_usd: None,
            },
        ];
        let (inner, cancel) = run_stub(events, 25, Duration::from_secs(300)).await;
        assert_eq!(inner.input_tokens, 150);
        assert_eq!(inner.cached_input_tokens, 20);
        assert_eq!(inner.output_tokens, 10);
        assert!(!cancel.is_cancelled());
        assert_eq!(inner.truncated, None);
    }

    #[test]
    fn verified_true_when_read_follows_invoke() {
        let trace = vec![
            entry(
                "c1",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                Some("{\"ok\":true}"),
            ),
            entry(
                "c2",
                TOOL_READ_PROPERTY,
                serde_json::json!({"thingId":"t1","propertyName":"temp"}),
                Some("{\"value\":21}"),
            ),
        ];
        let mut actions = build_actions(&trace);
        assert!(judge_verified(&trace, &mut actions));
        assert!(actions[0].verified);
    }

    #[test]
    fn verified_true_when_query_events_follows_invoke() {
        let trace = vec![
            entry(
                "c1",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"reboot"}),
                Some("{\"ok\":true}"),
            ),
            entry("c2", TOOL_QUERY_EVENTS, serde_json::json!({"thingId":"t1"}), Some("[]")),
        ];
        let mut actions = build_actions(&trace);
        assert!(judge_verified(&trace, &mut actions));
    }

    #[test]
    fn verified_false_without_followup_read() {
        // Read BEFORE the invoke does not count; read of a DIFFERENT thing
        // does not count; an unpaired (cancelled) read does not count.
        let trace = vec![
            entry(
                "c0",
                TOOL_READ_PROPERTY,
                serde_json::json!({"thingId":"t1","propertyName":"temp"}),
                Some("{\"value\":21}"),
            ),
            entry(
                "c1",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                Some("{\"ok\":true}"),
            ),
            entry(
                "c2",
                TOOL_READ_PROPERTY,
                serde_json::json!({"thingId":"t2","propertyName":"temp"}),
                Some("{\"value\":22}"),
            ),
            entry(
                "c3",
                TOOL_READ_PROPERTY,
                serde_json::json!({"thingId":"t1","propertyName":"temp"}),
                None,
            ),
        ];
        let mut actions = build_actions(&trace);
        assert!(!judge_verified(&trace, &mut actions));
        assert!(!actions[0].verified);
    }

    #[test]
    fn unpaired_tool_call_maps_to_unknown_cancelled() {
        let trace = vec![entry(
            "c1",
            TOOL_INVOKE_ACTION,
            serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
            None,
        )];
        let actions = build_actions(&trace);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].result, ActionResult::UnknownCancelled));
    }

    #[test]
    fn non_json_result_maps_to_action_failed() {
        let trace = vec![entry(
            "c1",
            TOOL_INVOKE_ACTION,
            serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
            Some("操作不支持: 物类型为 'space'"),
        )];
        let actions = build_actions(&trace);
        match &actions[0].result {
            ActionResult::Failed(msg) => assert!(msg.contains("操作不支持")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    fn ctx_with() -> (RunContext, RunContextInner) {
        let ctx = RunContext {
            run_id: "run_1".to_string(),
            workspace_id: "ws_1".to_string(),
            inner: Arc::new(RwLock::new(RunContextInner::default())),
        };
        (ctx, RunContextInner::default())
    }

    #[tokio::test]
    async fn parallel_tool_batch_reports_exact_budget_in_summary() {
        // zeroclaw dispatches tool calls in parallel batches: a batch of N
        // ToolCall events can jump tool_calls from 23 to 28 in one shot. The
        // summary must report the configured budget (25), not tool_calls - 1.
        let mut events = Vec::new();
        for i in 1..=23 {
            events.push(read(&format!("r{i}"), "t1"));
            events.push(result(&format!("r{i}"), TOOL_READ_PROPERTY, "{\"value\":1}"));
        }
        // One parallel batch of 5 calls: 23 → 28, budget is 25.
        for i in 24..=28 {
            events.push(read(&format!("r{i}"), "t1"));
        }

        let (inner, cancel) = run_stub(events, 25, Duration::from_secs(300)).await;
        assert!(cancel.is_cancelled());
        assert_eq!(inner.truncated, Some(TruncationReason::ToolCallBudget));
        assert_eq!(inner.tool_calls, 28);

        let (ctx, _) = ctx_with();
        let out = assemble_report(&ctx, TurnEnd::Cancelled, 1000, &inner);
        assert!(out.report.summary.contains("工具调用超过 25 次"));
        assert!(!out.report.summary.contains("27"));
    }

    #[test]
    fn llm_text_wins_summary_and_acted_outcome() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "thing:t1:event:temp_high".to_string();
        inner.trace = vec![
            entry(
                "c1",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                Some("{\"ok\":true}"),
            ),
            entry(
                "c2",
                TOOL_READ_PROPERTY,
                serde_json::json!({"thingId":"t1","propertyName":"temp"}),
                Some("{\"value\":21}"),
            ),
        ];
        inner.tool_calls = 2;

        let out = assemble_report(&ctx, TurnEnd::Text("已开启风扇并确认温度回落".into()), 1500, &inner);
        assert_eq!(out.report.outcome, Outcome::Acted);
        assert_eq!(out.report.summary, "已开启风扇并确认温度回落");
        assert_eq!(out.llm_text.as_deref(), Some("已开启风扇并确认温度回落"));
        assert!(out.report.verified);
        assert_eq!(out.report.tool_calls, 2);
        assert_eq!(out.report.duration_ms, 1500);
        assert_eq!(out.report.trigger, "thing:t1:event:temp_high");
    }

    #[test]
    fn truncation_synthesizes_summary_and_budget_exceeded() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "timer:ws_1".to_string();
        inner.trace = vec![
            entry(
                "c1",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                Some("{\"ok\":true}"),
            ),
            entry(
                "c2",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                None,
            ),
        ];
        inner.tool_calls = 26;
        inner.truncated = Some(TruncationReason::ToolCallBudget);

        let out = assemble_report(&ctx, TurnEnd::Cancelled, 61000, &inner);
        assert_eq!(out.report.outcome, Outcome::BudgetExceeded);
        assert_eq!(out.llm_text, None);
        // Synthesized from the trajectory (O1): trigger + action list + reason.
        assert!(out.report.summary.contains("timer:ws_1"));
        assert!(out.report.summary.contains("set_fan"));
        assert!(out.report.summary.contains("预算截断"));
        // Second action was cancelled in flight.
        assert!(matches!(out.report.actions[1].result, ActionResult::UnknownCancelled));
        assert!(!out.report.verified);
    }

    #[test]
    fn llm_failure_yields_failed_outcome_with_synthesized_summary() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "user:u1".to_string();
        inner.trace = vec![entry(
            "c1",
            TOOL_READ_PROPERTY,
            serde_json::json!({"thingId":"t1","propertyName":"temp"}),
            Some("{\"value\":21}"),
        )];
        inner.tool_calls = 1;

        let out = assemble_report(&ctx, TurnEnd::Failed, 800, &inner);
        assert_eq!(out.report.outcome, Outcome::Failed);
        assert!(out.report.summary.contains("LLM"));
        assert!(out.report.summary.contains("user:u1"));
    }

    #[test]
    fn empty_text_and_no_actions_yields_no_action_needed() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "timer:ws_1".to_string();

        let out = assemble_report(&ctx, TurnEnd::Empty, 500, &inner);
        assert_eq!(out.report.outcome, Outcome::NoActionNeeded);
        assert_eq!(out.llm_text, None);
        assert!(out.report.actions.is_empty());
        // Vacuous truth: nothing to verify.
        assert!(out.report.verified);
    }

    /// T11 结构化拒绝的 invoke_action 轨迹条目。
    fn denied_entry(id: &str, thing: &str, action: &str, reason: &str) -> ToolTraceEntry {
        ToolTraceEntry {
            id: id.to_string(),
            name: TOOL_INVOKE_ACTION.to_string(),
            args: serde_json::json!({"thingId": thing, "actionName": action}),
            output: Some(format!(r#"{{"denied":true,"reason":"{reason}"}}"#)),
        }
    }

    #[test]
    fn all_actions_denied_yields_rejected_even_with_llm_text() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "thing:t1:event:temp_high".to_string();
        inner.trace = vec![
            denied_entry("c1", "t1", "reboot", "action_not_allowed"),
            denied_entry("c2", "t1", "set_fan", "action_denied"),
        ];
        inner.tool_calls = 2;

        // LLM 文本照常作为 summary，但 outcome 由轨迹判定为 Rejected（T17）。
        let out = assemble_report(&ctx, TurnEnd::Text("两个动作都被拒绝".into()), 900, &inner);
        assert_eq!(out.report.outcome, Outcome::Rejected);
        assert_eq!(out.report.summary, "两个动作都被拒绝");
        assert_eq!(out.report.actions.len(), 2);
    }

    #[test]
    fn all_actions_denied_synthesized_summary_mentions_policy() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "thing:t1:event:temp_high".to_string();
        inner.trace = vec![denied_entry("c1", "t1", "reboot", "action_not_allowed")];
        inner.tool_calls = 1;

        let out = assemble_report(&ctx, TurnEnd::Empty, 900, &inner);
        assert_eq!(out.report.outcome, Outcome::Rejected);
        assert_eq!(out.llm_text, None);
        assert!(
            out.report.summary.contains("被策略拒绝（action_not_allowed）"),
            "动作清单明示拒绝: {}",
            out.report.summary
        );
        assert!(
            out.report.summary.contains("动作被策略拒绝，建议检查自治策略配置"),
            "结尾给出策略建议: {}",
            out.report.summary
        );
    }

    #[test]
    fn partial_denial_yields_acted() {
        let (ctx, mut inner) = ctx_with();
        inner.trigger = "thing:t1:event:temp_high".to_string();
        inner.trace = vec![
            denied_entry("c1", "t1", "reboot", "action_not_allowed"),
            entry(
                "c2",
                TOOL_INVOKE_ACTION,
                serde_json::json!({"thingId":"t1","actionName":"set_fan"}),
                Some("{\"ok\":true}"),
            ),
        ];
        inner.tool_calls = 2;

        let out = assemble_report(&ctx, TurnEnd::Empty, 900, &inner);
        assert_eq!(out.report.outcome, Outcome::Acted, "部分拒绝（有动作成功）仍为 Acted");
    }
}
