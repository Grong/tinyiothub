//! Harness orchestrator — 6-stage pipeline: Wake → Load → Plan → Execute → Verify → Report.
//!
//! `run_harness_tick` is the main entry point for heartbeat-style execution.
//! `run_chat_harness` is the chat-specific entry point (streaming events → caller).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, error, info, warn};

use super::execute::execute_step;
use super::plan::{build_plan, inject_plan_prompt};
use super::types::{HarnessSignal, LieCounter, LoopContext, LoopReport, TickVerdict};
use super::verify::{check_degradation, verify_steps};
use crate::agent::pool::AgentPoolLike;
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::policy::PolicyEngine;

/// Run a full 6-stage harness tick for a heartbeat signal.
///
/// Stages:
/// 1. Wake — log entry
/// 2. Load Context — already assembled in `LoopContext`
/// 3. Plan — `plan::build_plan()` + `plan::inject_plan_prompt()`
/// 4. Execute — each step via `execute::execute_step()`
/// 5. Verify — `verify::verify_steps()` + lie counter
/// 6. Report — build `LoopReport`, publish events
pub async fn run_harness_tick(
    signal: &HarnessSignal,
    context: &LoopContext,
    agent_pool: &Arc<dyn AgentPoolLike>,
    policy_engine: &Arc<dyn PolicyEngine>,
    event_publisher: &AiEventPublisher,
    lie_counter: &mut LieCounter,
) -> LoopReport {
    let tick_start = Instant::now();
    let mut stage_durations: HashMap<String, u64> = HashMap::new();
    let ws_id = &context.workspace_id;

    // ── Stage 1: Wake ──────────────────────────────────────────────
    debug!(
        workspace_id = %ws_id,
        source = ?signal.source,
        "Harness tick: Wake"
    );

    // ── Stage 2: Load Context ──────────────────────────────────────
    // Context is already loaded by the caller (LoopContext struct).
    let load_start = Instant::now();
    debug!(
        workspace_id = %ws_id,
        tasks = context.tasks.len(),
        history = context.history.len(),
        "Harness tick: Load Context"
    );
    stage_durations.insert("load".into(), load_start.elapsed().as_millis() as u64);

    // ── Stage 3: Plan ──────────────────────────────────────────────
    let plan_start = Instant::now();
    let steps = build_plan(&context.tasks);

    if steps.is_empty() {
        info!(workspace_id = %ws_id, "No steps to execute, skipping tick");
        return LoopReport {
            workspace_id: ws_id.clone(),
            trigger_source: signal.source.clone(),
            verdict: TickVerdict::Pass,
            steps: vec![],
            executed_actions: vec![],
            proposals: vec![],
            duration_ms: tick_start.elapsed().as_millis() as u64,
            tool_call_count: 0,
            lie_detected: false,
            stage_durations,
        };
    }

    let _system_prompt = inject_plan_prompt(&steps, &context.system_prompt);
    info!(
        workspace_id = %ws_id,
        step_count = steps.len(),
        "Harness tick: Plan — {} steps",
        steps.len()
    );
    stage_durations.insert("plan".into(), plan_start.elapsed().as_millis() as u64);

    // ── Stage 4: Execute ───────────────────────────────────────────
    let exec_start = Instant::now();
    let mut step_results = Vec::with_capacity(steps.len());

    for step in &steps {
        let step_event_start = Instant::now();

        let result = execute_step(
            step,
            agent_pool,
            &context.trust_config,
            policy_engine,
            ws_id,
        )
        .await;

        let step_duration = step_event_start.elapsed().as_millis() as u64;

        // Publish step-level event
        let lie_detected = result.tool_calls.iter().all(|tc| !tc.success)
            && !result.tool_calls.is_empty()
            && result.output.contains("success");
        event_publisher.publish(AiEvent::HarnessStepCompleted {
            workspace_id: ws_id.clone(),
            step_id: step.id.clone(),
            status: format!("{:?}", result.status),
            lie_detected,
        });

        debug!(
            workspace_id = %ws_id,
            step_id = %step.id,
            tool_calls = result.tool_calls.len(),
            duration_ms = step_duration,
            "Harness tick: Execute step {} complete",
            step.id
        );

        step_results.push(result);
    }

    stage_durations.insert("execute".into(), exec_start.elapsed().as_millis() as u64);

    // ── Stage 5: Verify ────────────────────────────────────────────
    let verify_start = Instant::now();
    let (tick_verdict, lie_detected) = verify_steps(&step_results);

    let degraded = check_degradation(lie_counter, lie_detected);
    if degraded {
        warn!(
            workspace_id = %ws_id,
            consecutive = lie_counter.consecutive_ticks,
            "Agent degraded to read-only"
        );
        event_publisher.publish(AiEvent::AgentDegraded {
            workspace_id: ws_id.clone(),
            reason: format!(
                "Agent lied in {} consecutive ticks (threshold: {})",
                lie_counter.consecutive_ticks, lie_counter.degrade_threshold
            ),
        });
    }

    info!(
        workspace_id = %ws_id,
        verdict = ?tick_verdict,
        lie_detected,
        "Harness tick: Verify"
    );
    stage_durations.insert("verify".into(), verify_start.elapsed().as_millis() as u64);

    // ── Stage 6: Report ────────────────────────────────────────────
    let report_start = Instant::now();
    let total_duration = tick_start.elapsed().as_millis() as u64;

    let mut tool_call_count: u32 = 0;
    let mut executed_actions = Vec::new();
    for sr in &step_results {
        for tc in &sr.tool_calls {
            tool_call_count += 1;
            executed_actions.push(crate::heartbeat::types::ExecutedAction {
                tool_name: tc.name.clone(),
                device_id: None,
                success: tc.success,
                details: tc.output.clone(),
            });
        }
    }

    let proposals_count = step_results
        .iter()
        .flat_map(|sr| &sr.tool_calls)
        .filter(|tc| tc.proposed)
        .count() as u32;

    let steps_completed = step_results
        .iter()
        .filter(|sr| !matches!(sr.status, super::types::StepStatus::Failed { .. }))
        .count() as u32;

    let report = LoopReport {
        workspace_id: ws_id.clone(),
        trigger_source: signal.source.clone(),
        verdict: tick_verdict.clone(),
        steps: step_results,
        executed_actions,
        proposals: vec![], // Proposals are created by the cloud layer
        duration_ms: total_duration,
        tool_call_count,
        lie_detected,
        stage_durations: stage_durations.clone(),
    };

    // Publish tick-level event
    event_publisher.publish(AiEvent::HarnessTickCompleted {
        workspace_id: ws_id.clone(),
        verdict: format!("{:?}", tick_verdict),
        lie_detected,
        tool_call_count,
        duration_ms: total_duration,
        steps_completed,
        proposals_count,
    });

    stage_durations.insert("report".into(), report_start.elapsed().as_millis() as u64);

    info!(
        workspace_id = %ws_id,
        verdict = ?tick_verdict,
        duration_ms = total_duration,
        steps = steps_completed,
        tools = tool_call_count,
        "Harness tick: Report — complete"
    );

    report
}

/// Chat-specific harness entry point.
///
/// Returns a receiver of `StreamEvent` for real-time forwarding to the SSE client.
/// The harness spawns a background task that:
/// 1. Builds a single-step plan
/// 2. Executes via `send_message_streamed()`
/// 3. Forwards all events to the returned receiver in real-time
/// 4. After streaming: runs verify, builds LoopReport, publishes events
///
/// The channel closes after the report is published (or on error).
pub async fn run_chat_harness(
    workspace_id: String,
    message: String,
    _agent_id: String,
    _session_key: String,
    agent_pool: Arc<dyn AgentPoolLike>,
    trust_config: crate::tool::trust::TrustConfig,
    policy_engine: Arc<dyn PolicyEngine>,
    event_publisher: Arc<AiEventPublisher>,
) -> tokio::sync::mpsc::Receiver<super::types::StreamEvent> {
    let (tx, rx) = tokio::sync::mpsc::channel::<super::types::StreamEvent>(64);
    let publisher = event_publisher.clone();

    tokio::spawn(async move {
        let tick_start = Instant::now();
        let ws = workspace_id.clone();

        // Plan — single step for chat
        let steps = super::plan::build_chat_plan(&message);
        let _system_prompt = inject_plan_prompt(&steps, "");

        // Execute — use streaming agent
        match agent_pool.send_message_streamed(&ws, &message).await {
            Ok(mut event_rx) => {
                let mut tool_call_count: u32 = 0;
                let mut lie_detected = false;
                let mut final_text = String::new();
                let mut step_tool_calls = Vec::new();

                // Forward events in real-time
                while let Some(event) = event_rx.recv().await {
                    match &event {
                        super::types::StreamEvent::ToolCall { name, .. } => {
                            tool_call_count += 1;
                            // PreToolUse check
                            let decision = crate::tool::trust::evaluate_tool_trust(&trust_config, name);
                            let policy = policy_engine
                                .evaluate(
                                    &ws,
                                    crate::policy::PolicyCategory::ToolExecution,
                                    name,
                                )
                                .await;

                            if let crate::tool::trust::TrustDecision::Block { .. } = decision {
                                lie_detected = true;
                            }

                            if let crate::policy::PolicyDecision::Block { .. } = policy {
                                lie_detected = true;
                            }

                            step_tool_calls.push(super::types::ToolCallRecord {
                                name: name.clone(),
                                args: serde_json::Value::Null,
                                success: !lie_detected,
                                output: String::new(),
                                proposed: false,
                                readback_verified: None,
                                readback_detail: None,
                            });
                        }
                        super::types::StreamEvent::ToolResult {
                            name, output, success, ..
                        } => {
                            if let Some(last) = step_tool_calls.iter_mut().rev().find(|tc| tc.name == *name) {
                                last.success = *success;
                                last.output = output.clone();
                            }
                        }
                        super::types::StreamEvent::Final { text } => {
                            final_text = text.clone();
                        }
                        super::types::StreamEvent::Error { message } => {
                            warn!(workspace_id = %ws, error = %message, "Chat harness stream error");
                        }
                        _ => {}
                    }

                    // Forward to caller (ignore if receiver dropped)
                    if tx.send(event).await.is_err() {
                        debug!(workspace_id = %ws, "Chat harness: caller dropped receiver, stopping forward");
                        break;
                    }
                }

                let duration_ms = tick_start.elapsed().as_millis() as u64;

                // Build step result for verification
                let step_result = super::types::StepResult {
                    step_id: "chat".into(),
                    status: super::types::StepStatus::Done,
                    output: final_text,
                    tool_calls: step_tool_calls,
                    retries: 0,
                    duration_ms,
                };

                let (verdict, lie) = verify_steps(&[step_result]);

                publisher.publish(AiEvent::HarnessTickCompleted {
                    workspace_id: ws,
                    verdict: format!("{:?}", verdict),
                    lie_detected: lie,
                    tool_call_count,
                    duration_ms,
                    steps_completed: 1,
                    proposals_count: 0,
                });
            }
            Err(e) => {
                error!(workspace_id = %ws, error = %e, "Chat harness: agent call failed");
                let _ = tx
                    .send(super::types::StreamEvent::Error {
                        message: format!("Harness error: {}", e),
                    })
                    .await;
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::bus::AiEventPublisher;
    use crate::agent::pool::AgentPoolLike;
    use crate::harness::types::{
        HarnessSignal, SignalPayload, SignalSource, StreamEvent, TickVerdict,
    };
    use crate::heartbeat::types::{HeartbeatTask, SignalPriority};
    use crate::policy::NoopPolicyEngine;
    use crate::tool::trust::TrustConfig;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tinyiothub_runtime::EventBus;

    /// Mock AgentPoolLike that returns pre-programmed StreamEvents.
    struct MockAgentPool {
        events: Vec<StreamEvent>,
        send_message_response: Option<String>,
    }

    impl MockAgentPool {
        fn new(events: Vec<StreamEvent>) -> Self {
            Self {
                events,
                send_message_response: None,
            }
        }

        fn with_send_message(mut self, response: &str) -> Self {
            self.send_message_response = Some(response.to_string());
            self
        }
    }

    #[async_trait]
    impl AgentPoolLike for MockAgentPool {
        async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
            Ok("mock_agent".into())
        }

        async fn send_message(&self, _workspace_id: &str, _prompt: &str) -> anyhow::Result<String> {
            Ok(self
                .send_message_response
                .clone()
                .unwrap_or_else(|| "mock response".into()))
        }

        async fn send_message_streamed(
            &self,
            _workspace_id: &str,
            _prompt: &str,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamEvent>> {
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            let events = self.events.clone();
            tokio::spawn(async move {
                for event in events {
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            });
            Ok(rx)
        }

        async fn shutdown(&self) {}
        fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
        fn cleanup_idle(&self) -> usize {
            0
        }
    }

    fn make_heartbeat_task(text: &str) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws_test".into(),
            priority: "high".into(),
            text: text.into(),
            paused: false,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn make_context(tasks: Vec<HeartbeatTask>) -> LoopContext {
        LoopContext {
            workspace_id: "ws_test".into(),
            agent_id: "agent:ws_test".into(),
            trust_config: TrustConfig::default(),
            tasks,
            history: std::collections::VecDeque::new(),
            system_prompt: "You are a test agent.".into(),
        }
    }

    fn make_signal() -> HarnessSignal {
        HarnessSignal {
            workspace_id: "ws_test".into(),
            source: SignalSource::Timer,
            payload: SignalPayload::Timer,
            priority: SignalPriority::Normal,
        }
    }

    fn make_publisher() -> AiEventPublisher {
        AiEventPublisher::new(Arc::new(EventBus::new()))
    }

    // ── StreamEvent roundtrip (T12) ──────────────────────────────────

    #[tokio::test]
    async fn test_stream_event_roundtrip_chunk() {
        let pool = Arc::new(MockAgentPool::new(vec![
            StreamEvent::Chunk {
                delta: "Hello".into(),
            },
            StreamEvent::Final {
                text: "Hello".into(),
            },
        ]));

        let mut rx = pool.send_message_streamed("ws", "test").await.unwrap();

        let first = rx.recv().await.unwrap();
        match first {
            StreamEvent::Chunk { delta } => assert_eq!(delta, "Hello"),
            other => panic!("Expected Chunk, got {:?}", other),
        }

        let second = rx.recv().await.unwrap();
        match second {
            StreamEvent::Final { text } => assert_eq!(text, "Hello"),
            other => panic!("Expected Final, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_stream_event_roundtrip_tool_call() {
        let pool = Arc::new(MockAgentPool::new(vec![
            StreamEvent::ToolCall {
                id: "tc1".into(),
                name: "get_device".into(),
                args: serde_json::json!({"device_id": "d1"}),
            },
            StreamEvent::ToolResult {
                id: "tc1".into(),
                name: "get_device".into(),
                output: "Device is online".into(),
                success: true,
            },
            StreamEvent::Final {
                text: "Done".into(),
            },
        ]));

        let mut rx = pool.send_message_streamed("ws", "test").await.unwrap();

        let tc = rx.recv().await.unwrap();
        assert!(matches!(tc, StreamEvent::ToolCall { .. }));

        let tr = rx.recv().await.unwrap();
        match tr {
            StreamEvent::ToolResult { name, success, .. } => {
                assert_eq!(name, "get_device");
                assert!(success);
            }
            other => panic!("Expected ToolResult, got {:?}", other),
        }
    }

    // ── Full pipeline (T16) ──────────────────────────────────────────

    #[tokio::test]
    async fn test_run_harness_tick_simple_pass() {
        let tasks = vec![make_heartbeat_task("Check device connectivity")];
        let context = make_context(tasks);
        let signal = make_signal();
        let pool = Arc::new(
            MockAgentPool::new(vec![
                StreamEvent::Chunk {
                    delta: "Device is online".into(),
                },
                StreamEvent::Final {
                    text: "Device is online".into(),
                },
            ])
            .with_send_message("VERIFIED: yes — Device status confirmed online"),
        );
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());
        let mut lie_counter = LieCounter::new(3);

        let report = run_harness_tick(
            &signal,
            &context,
            &(pool as Arc<dyn AgentPoolLike>),
            &policy,
            &publisher,
            &mut lie_counter,
        )
        .await;

        assert_eq!(report.workspace_id, "ws_test");
        assert!(!report.lie_detected);
        assert!(!report.steps.is_empty());
    }

    #[tokio::test]
    async fn test_run_harness_tick_empty_tasks_returns_pass() {
        let context = make_context(vec![]);
        let signal = make_signal();
        let pool: Arc<dyn AgentPoolLike> =
            Arc::new(MockAgentPool::new(vec![]));
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());
        let mut lie_counter = LieCounter::new(3);

        let report = run_harness_tick(
            &signal,
            &context,
            &pool,
            &policy,
            &publisher,
            &mut lie_counter,
        )
        .await;

        assert_eq!(report.verdict, TickVerdict::Pass);
        assert_eq!(report.tool_call_count, 0);
    }

    #[tokio::test]
    async fn test_run_harness_tick_lie_detection() {
        let tasks = vec![make_heartbeat_task("Check device connectivity")];
        let context = make_context(tasks);
        let signal = make_signal();

        // LLM reports "Done" but all tool calls failed → lying
        let pool = Arc::new(
            MockAgentPool::new(vec![
                StreamEvent::ToolCall {
                    id: "tc1".into(),
                    name: "write_config".into(),
                    args: serde_json::json!({}),
                },
                StreamEvent::ToolResult {
                    id: "tc1".into(),
                    name: "write_config".into(),
                    output: "Error: timeout".into(),
                    success: false,
                },
                StreamEvent::Final {
                    text: "Done".into(),
                },
            ])
            .with_send_message("VERIFIED: no — config not found"),
        );
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());
        let mut lie_counter = LieCounter::new(3);

        let report = run_harness_tick(
            &signal,
            &context,
            &(pool as Arc<dyn AgentPoolLike>),
            &policy,
            &publisher,
            &mut lie_counter,
        )
        .await;

        assert!(report.lie_detected);
        assert!(matches!(report.verdict, TickVerdict::Fail { .. }));
    }

    #[tokio::test]
    async fn test_lie_counter_degradation_in_pipeline() {
        let tasks = vec![make_heartbeat_task("Check device")];
        let context = make_context(tasks);
        let signal = make_signal();
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());
        let mut lie_counter = LieCounter::new(3);

        // Run 3 lying ticks
        for _ in 0..3 {
            let pool = Arc::new(
                MockAgentPool::new(vec![
                    StreamEvent::ToolCall {
                        id: "tc".into(),
                        name: "bad_tool".into(),
                        args: serde_json::json!({}),
                    },
                    StreamEvent::ToolResult {
                        id: "tc".into(),
                        name: "bad_tool".into(),
                        output: "Error".into(),
                        success: false,
                    },
                    StreamEvent::Final {
                        text: "All good!".into(),
                    },
                ])
                .with_send_message("VERIFIED: no"),
            );

            let _ = run_harness_tick(
                &signal,
                &context,
                &(pool as Arc<dyn AgentPoolLike>),
                &policy,
                &publisher,
                &mut lie_counter,
            )
            .await;
        }

        assert!(lie_counter.is_degraded());
        assert_eq!(lie_counter.consecutive_ticks, 3);
    }

    // ── Chat harness (T17) ───────────────────────────────────────────

    #[tokio::test]
    async fn test_chat_harness_forwards_events() {
        let pool = Arc::new(MockAgentPool::new(vec![
            StreamEvent::Chunk {
                delta: "Hi".into(),
            },
            StreamEvent::ToolCall {
                id: "tc1".into(),
                name: "get_device".into(),
                args: serde_json::json!({}),
            },
            StreamEvent::ToolResult {
                id: "tc1".into(),
                name: "get_device".into(),
                output: "online".into(),
                success: true,
            },
            StreamEvent::Final {
                text: "Device is online".into(),
            },
        ]));
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());

        let mut rx = run_chat_harness(
            "ws_test".into(),
            "check device".into(),
            "agent1".into(),
            "session1".into(),
            pool,
            TrustConfig::default(),
            policy,
            publisher,
        )
        .await;

        let mut event_count = 0;
        let mut saw_tool_call = false;
        let mut saw_final = false;

        while let Some(event) = rx.recv().await {
            event_count += 1;
            match event {
                StreamEvent::Chunk { .. } => {}
                StreamEvent::ToolCall { .. } => saw_tool_call = true,
                StreamEvent::Final { .. } => saw_final = true,
                _ => {}
            }
        }

        assert!(event_count >= 4);
        assert!(saw_tool_call);
        assert!(saw_final);
    }

    #[tokio::test]
    async fn test_chat_harness_error_propagation() {
        // send_message_streamed returns an error
        struct ErrorPool;
        #[async_trait]
        impl AgentPoolLike for ErrorPool {
            async fn get_or_create_agent(&self, _: &str) -> anyhow::Result<String> {
                Ok("agent".into())
            }
            async fn send_message(&self, _: &str, _: &str) -> anyhow::Result<String> {
                Ok("ok".into())
            }
            async fn send_message_streamed(
                &self,
                _: &str,
                _: &str,
            ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamEvent>> {
                anyhow::bail!("connection lost")
            }
            async fn shutdown(&self) {}
            fn set_trust_config(&self, _: &str, _: TrustConfig) {}
            fn cleanup_idle(&self) -> usize {
                0
            }
        }

        let pool = Arc::new(ErrorPool);
        let policy: Arc<dyn PolicyEngine> = Arc::new(NoopPolicyEngine);
        let publisher = Arc::new(make_publisher());

        let mut rx = run_chat_harness(
            "ws".into(),
            "msg".into(),
            "a".into(),
            "s".into(),
            pool,
            TrustConfig::default(),
            policy,
            publisher,
        )
        .await;

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, StreamEvent::Error { .. }));
    }
}

