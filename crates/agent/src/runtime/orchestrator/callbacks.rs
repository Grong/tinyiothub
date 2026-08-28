//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatRunner.signal()
//! HeartbeatCompleted --> AgentEventBus.emit(HeartbeatResultReady)（Task 6；
//!                        Task 8 持久化订阅者落库，零订阅者 emit 为 no-op）
//!                    --> HeartbeatBridge.dispatch_proposals() (X6, O21)
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> HeartbeatRunner.start() + ThingAgentManager.start()
//! WorkspaceDeleted    --> HeartbeatRunner.remove_workspace() + ThingAgentManager.stop()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tracing::{debug, info, warn};

use crate::runtime::event::types::AiEvent;
use crate::runtime::events::{AgentEventBus, AgentEventKind};
use crate::runtime::heartbeat::runner::HeartbeatRunner;
use crate::runtime::heartbeat::types::{HeartbeatResult, SignalPriority};
use crate::runtime::thing_agent::manager::ThingAgentManager;
use crate::runtime::thing_agent::registry::RunRegistry;
use crate::runtime::thing_agent::traits::DirectiveSink;
use crate::runtime::thing_agent::types::{Outcome, Priority, TriggerSource, WakeSignal};
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

/// X6 心跳桥（O21 裁决）：订阅既有 `AiEvent::HeartbeatCompleted`（loop_.rs
/// 零改动），从心跳报告的结构化 proposals 提取问题，按 O11 dedup 后投递
/// `UserDirective{ source: Some("heartbeat"), priority: Normal }` 给
/// thing-agent loop 处置。
///
/// problem_key 从结构化字段派生（`{tool_name}:{thing_id}`），不用自由文本
/// 摘要——LLM 措辞变化会击穿去重（O21）。
///
/// O11 dedup 依据 [`RunRegistry`] 的 problem_key 元数据内存映射（Task 6；
/// 原 db agent_runs 的 last_problem_run/count_problem_runs SQL 查询
/// 的等价承接，等价性论证见 registry.rs 模块文档）。
pub struct HeartbeatBridge {
    registry: RunRegistry,
    sink: Arc<dyn DirectiveSink>,
}

/// O11 dedup 窗口：同 problem_key 6h 内的 Run 参与抑制判定；超窗旧 Run 不
/// 抑制（复发可再处置）。
pub const PROBLEM_WINDOW_HOURS: u32 = 6;
/// O11：人工 ack 后该 problem_key 的抑制时长（7 天）。
pub const ACK_SUPPRESS_HOURS: u32 = 7 * 24;

impl HeartbeatBridge {
    pub fn new(registry: RunRegistry, sink: Arc<dyn DirectiveSink>) -> Self {
        Self { registry, sink }
    }

    /// problem_key：结构化字段 `{tool_name}:{thing_id}`；无目标设备的提案
    /// 用 "-" 占位（稳定，不随措辞变化）。
    pub fn problem_key_of(proposal: &Proposal) -> String {
        format!("{}:{}", proposal.tool_name, proposal.thing_id.as_deref().unwrap_or("-"))
    }

    /// O11 ack 抑制入口（Task 6，fix round 1 行级保真）：cloud 侧 ack 端点
    /// DB 写成功后经 Orchestrator 转发至此，按 run_id 标记内存 dedup 真源中
    /// 对应 run 条目。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        self.registry.mark_problem_acked(workspace_id, problem_key, run_id);
    }

    /// 对心跳报告中的每个 proposal 做 O11 dedup，通过则投递心跳 directive。
    /// 投递失败（队列满/节流/loop 未启动）仅记录日志——心跳 directive 不享
    /// "排队不丢"（O5），丢弃可接受。
    pub async fn dispatch_proposals(&self, workspace_id: &str, result: &HeartbeatResult) {
        for proposal in &result.proposals {
            // 心跳已裁决（approved/rejected）的提案不二次投递。
            if proposal.status != ProposalStatus::Pending {
                continue;
            }
            let problem_key = Self::problem_key_of(proposal);
            if self.should_dispatch(workspace_id, &problem_key) {
                let signal = heartbeat_directive(workspace_id, problem_key.clone(), proposal);
                if let Err(e) = self.sink.enqueue(signal) {
                    debug!(
                        workspace_id,
                        problem_key, error = %e, "heartbeat directive not admitted (droppable by design)"
                    );
                } else {
                    info!(
                        workspace_id,
                        problem_key, "heartbeat proposal dispatched to thing-agent loop"
                    );
                }
            } else {
                debug!(workspace_id, problem_key, "heartbeat proposal suppressed by O11 dedup");
            }
        }
    }

    /// O11 dedup（6h 窗口 + 窗口内计数 + 全 outcome 覆盖 + ack 抑制 7 天）：
    /// - 7d 窗口最近一次 Run 已 ack → 跳过（6h 窗口非空时 last(7d)==last(6h)， 6h 内 acked
    ///   由本分支覆盖；6h 空而 7d 内有 acked = 复发在 ack 抑制期内）
    /// - 6h 窗口无 Run → 放行（新问题 / 超 6h 复发）
    /// - 最近一次 failed/rejected/budget_exceeded → 跳过
    /// - acted+verified / no_action_needed → 跳过
    /// - acted+未 verified：窗口内仅 1 次 → 放行一次重试；第二次起跳过
    fn should_dispatch(&self, workspace_id: &str, problem_key: &str) -> bool {
        let ack_window = Duration::from_secs(u64::from(ACK_SUPPRESS_HOURS) * 3600);
        let problem_window = Duration::from_secs(u64::from(PROBLEM_WINDOW_HOURS) * 3600);
        if let Some((_, _, acked)) = self.registry.last_problem_run(workspace_id, problem_key, ack_window)
            && acked
        {
            return false;
        }
        let Some((outcome, verified, _acked)) =
            self.registry
                .last_problem_run(workspace_id, problem_key, problem_window)
        else {
            return true;
        };
        match outcome {
            Outcome::Failed | Outcome::Rejected | Outcome::BudgetExceeded | Outcome::NoActionNeeded => false,
            Outcome::Acted if verified => false,
            Outcome::Acted => {
                self.registry
                    .count_problem_runs(workspace_id, problem_key, problem_window)
                    <= 1
            }
        }
    }
}

/// 心跳 directive 文本：从 proposal 生成可执行指令（O2）。
fn heartbeat_directive_text(problem_key: &str, proposal: &Proposal) -> String {
    format!(
        "心跳巡检发现待处置问题 {problem_key}：{}（原因：{}；风险：{}）。请诊断并处置。",
        proposal.summary, proposal.reason, proposal.risk
    )
}

/// 心跳来源 directive（O5/O24）：Normal 优先级、dedup_key=None 不参与合并、
/// source=Some("heartbeat") 标记来源（不走 60s 同文去重、不享排队不丢）。
fn heartbeat_directive(workspace_id: &str, problem_key: String, proposal: &Proposal) -> WakeSignal {
    WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority: Priority::Normal,
        source: TriggerSource::UserDirective {
            user_id: "heartbeat".to_string(),
            text: heartbeat_directive_text(&problem_key, proposal),
            session_key: None,
            source: Some("heartbeat".to_string()),
            problem_key: Some(problem_key),
        },
        dedup_key: None,
    }
}

/// Cross-domain callback handler.
pub struct AiEventHandler {
    heartbeat_runner: Arc<HeartbeatRunner>,
    /// AgentEvent 出口（Task 2/6）：HeartbeatCompleted → HeartbeatResultReady。
    agent_events: Arc<AgentEventBus>,
    /// T15 thing-agent loop registry; None where the loop is not deployed.
    thing_agent_manager: Option<Arc<ThingAgentManager>>,
    /// T18 X6 心跳桥；None 时 HeartbeatCompleted 仅发射事件不投递 directive。
    heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
    shutting_down: Arc<AtomicBool>,
}

impl AiEventHandler {
    pub fn new(
        heartbeat_runner: Arc<HeartbeatRunner>,
        agent_events: Arc<AgentEventBus>,
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
        shutting_down: Arc<AtomicBool>,
    ) -> Self {
        Self {
            heartbeat_runner,
            agent_events,
            thing_agent_manager,
            heartbeat_bridge,
            shutting_down,
        }
    }

    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        &self.heartbeat_runner
    }

    /// O11 ack 抑制转发（Task 6）：无桥（None）时 no-op。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        if let Some(bridge) = &self.heartbeat_bridge {
            bridge.mark_problem_acked(workspace_id, problem_key, run_id);
        }
    }

    /// Handle an AiEvent variant, dispatched by the EventBus.
    pub async fn handle_ai_event(&self, event: &Event) {
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!("AiEventHandler is shutting down, skipping event");
            return;
        }

        let _ai_event_type = match event.event_type() {
            EventType::Ai(t) => t,
            _ => return,
        };

        let payload_str = match extract_payload(event.content()) {
            Some(s) => s,
            None => {
                debug!("AiEvent has no text payload -- skipping");
                return;
            }
        };

        let ai_event: AiEvent = match serde_json::from_str(&payload_str) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to deserialize AiEvent payload");
                return;
            }
        };

        match &ai_event {
            AiEvent::AlarmCreated(alarm) => {
                let severity = alarm.severity.to_lowercase();
                if severity == "critical" || severity == "error" {
                    self.heartbeat_runner
                        .signal(crate::runtime::heartbeat::types::HeartbeatSignal {
                            workspace_id: alarm.workspace_id.clone(),
                            reason: format!("Alarm: {}", alarm.message),
                            context: format!("thing_id={}, alarm_type={}", alarm.thing_id, alarm.alarm_type),
                            priority: if severity == "critical" {
                                SignalPriority::Critical
                            } else {
                                SignalPriority::High
                            },
                            thing_id: Some(alarm.thing_id.clone()),
                            alarm_type: Some(alarm.alarm_type.clone()),
                            rule_id: alarm.rule_id.clone(),
                        });
                }
            }
            AiEvent::HeartbeatCompleted { workspace_id, result } => {
                // 落库出口（Task 6）：HeartbeatResultReady 由 Task 8 持久化
                // 订阅者落库；零订阅者时 emit 为 no-op（Task 4 RunRecorded
                // 同先例）。
                // CEO review T22：同源写入 runner 近期结果窗口——dump_state
                // 导出与"已发射"一致，Lagged resync/周期对账可补回丢失行。
                self.heartbeat_runner.record_result(workspace_id, result.clone());
                self.agent_events.emit(AgentEventKind::HeartbeatResultReady {
                    result: Box::new(result.clone()),
                });
                // X6 心跳桥：O11 dedup 依据 RunRegistry 内存（Task 6），
                // 不依赖本次心跳结果的持久化。
                if let Some(bridge) = &self.heartbeat_bridge {
                    bridge.dispatch_proposals(workspace_id, result).await;
                }
            }
            AiEvent::WorkspaceCreated { workspace_id } => {
                self.heartbeat_runner.start(workspace_id).await;
                if let Some(manager) = &self.thing_agent_manager {
                    manager.start(workspace_id);
                }
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                // remove_workspace = stop + 清三表内存真源，防止已删工作区
                // 在内存与 dump_state 快照中残留（Task 5 fix round 1）。
                self.heartbeat_runner.remove_workspace(workspace_id).await;
                if let Some(manager) = &self.thing_agent_manager {
                    manager.stop(workspace_id).await;
                }
            }
            // Self-referential events published by the AI subsystem itself —
            // these are intentionally no-ops to avoid processing loops.
            AiEvent::AlarmResolved { .. } => {}
            AiEvent::HeartbeatPersistFailed { .. } => {}
            AiEvent::ReflectionFailed { .. } => {}
            // ChatCompleted was previously handled here but reflection
            // now happens directly in chat/service.rs. Variant retained
            // for future EventBus-based reflection.
            AiEvent::ChatCompleted { .. } => {}
            AiEvent::ProposalCreated {
                workspace_id,
                proposal_id,
                tool_name,
            } => {
                info!(
                    workspace_id,
                    proposal_id, tool_name, "HITL proposal created — awaiting human approval"
                );
            }
            AiEvent::ProposalResolved {
                workspace_id,
                proposal_id,
                approved,
            } => {
                info!(workspace_id, proposal_id, approved, "HITL proposal resolved");
            }
        }
    }
}

#[async_trait]
impl tinyiothub_core::event::EventHandler for AiEventHandler {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        self.handle_ai_event(event).await;
        Ok(())
    }

    fn name(&self) -> &str {
        "AiEventHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Ai(_))
    }
}

fn extract_payload(content: &RichContent) -> Option<String> {
    content.elements().iter().find_map(|el| match el {
        ContentElement::Text { content, .. } => Some(content.clone()),
        _ => None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::runtime::event::bus::AiEventPublisher;
    use crate::runtime::heartbeat::types::HeartbeatConfig;
    use crate::runtime::heartbeat::types::HeartbeatStatus;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent};
    use tinyiothub_runtime::EventBus;

    use tinyiothub_core::event::EventHandler;

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(EventBus::new())))
    }

    fn make_heartbeat_runner() -> Arc<HeartbeatRunner> {
        Arc::new(HeartbeatRunner::new(make_publisher(), HeartbeatConfig::default()))
    }

    /// 显式构造夹具（无 Default 上下文）：返回 handler 与其 AgentEventBus
    /// （测试经 bus.subscribe() 断言 HeartbeatResultReady 发射）。
    fn make_handler(
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        bridge: Option<Arc<HeartbeatBridge>>,
        shutting_down: bool,
    ) -> (AiEventHandler, Arc<AgentEventBus>) {
        let events = Arc::new(AgentEventBus::new(16));
        let handler = AiEventHandler::new(
            make_heartbeat_runner(),
            events.clone(),
            thing_agent_manager,
            bridge,
            Arc::new(AtomicBool::new(shutting_down)),
        );
        (handler, events)
    }

    /// Wrap an AiEvent inside a tinyiothub_core Event for handler dispatch.
    fn wrap_ai_event(ai_event: &AiEvent) -> Event {
        let payload = serde_json::to_string(ai_event).unwrap();
        let ai_event_type: tinyiothub_core::models::event::AiEventType = ai_event.into();
        Event::new(
            EventType::Ai(ai_event_type),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("AiEvent".to_string(), payload),
        )
        .expect("Failed to create test event")
    }

    fn ok_result(workspace_id: &str) -> HeartbeatResult {
        HeartbeatResult {
            id: "test-tick".to_string(),
            workspace_id: workspace_id.to_string(),
            status: HeartbeatStatus::Complete,
            summary: "All good".to_string(),
            task_count: 0,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        }
    }

    #[tokio::test]
    async fn test_handler_construction() {
        let (handler, _events) = make_handler(None, None, false);
        assert_eq!(handler.name(), "AiEventHandler");
    }

    #[tokio::test]
    async fn test_should_handle_filters_ai_events() {
        let (handler, _events) = make_handler(None, None, false);

        let ai_event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_1".into(),
        });
        assert!(handler.should_handle(&ai_event));

        // System event should not be handled
        let system_event = Event::new(
            EventType::System(tinyiothub_core::models::event::SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "data".to_string()),
        )
        .expect("Failed to create system event");
        assert!(!handler.should_handle(&system_event));
    }

    // Task 6：HeartbeatCompleted 不再直接落库，转为发射 HeartbeatResultReady
    // （Task 8 持久化订阅者消费）。
    #[tokio::test]
    async fn test_heartbeat_completed_emits_result_ready() {
        let (handler, events) = make_handler(None, None, false);
        let mut rx = events.subscribe();

        let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result: ok_result("ws_test"),
        });
        handler.handle_ai_event(&event).await;

        let ev = rx.try_recv().expect("HeartbeatResultReady emitted synchronously");
        match ev.kind {
            AgentEventKind::HeartbeatResultReady { result } => {
                assert_eq!(result.workspace_id, "ws_test");
                assert_eq!(result.summary, "All good");
            }
            _ => panic!("expected HeartbeatResultReady"),
        }
    }

    #[tokio::test]
    async fn test_alarm_created_non_critical_no_signal() {
        let (handler, _events) = make_handler(None, None, false);

        // Non-critical alarm should not trigger heartbeat signal
        let alarm = AiEvent::AlarmCreated(tinyiothub_core::models::event::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws_1".into(),
            thing_id: "d1".into(),
            alarm_type: "high_temp".into(),
            severity: "warning".into(),
            message: "Temperature is high".into(),
            rule_id: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        });

        let event = wrap_ai_event(&alarm);
        handler.handle_ai_event(&event).await;
        // No assertion needed — just verifying no panic for non-critical alarms
    }

    #[tokio::test]
    async fn test_workspace_created_and_deleted_no_panic() {
        let (handler, _events) = make_handler(None, None, false);

        // WorkspaceCreated (no tasks loaded → loop won't start, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;

        // WorkspaceDeleted (no loop running → no-op, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;
    }

    // T15: workspace lifecycle events start/stop the thing-agent loop
    // alongside the heartbeat runner.
    #[tokio::test]
    async fn test_workspace_lifecycle_drives_thing_agent_manager() {
        let parts = crate::runtime::thing_agent::manager::tests::stub_manager().await;
        let manager = parts.manager.clone();

        let (handler, _events) = make_handler(Some(manager.clone()), None, false);

        assert!(!manager.is_running("ws_life"));
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(manager.is_running("ws_life"), "WorkspaceCreated must start the loop");

        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(!manager.is_running("ws_life"), "WorkspaceDeleted must stop the loop");
    }

    #[tokio::test]
    async fn test_self_referential_events_are_noop() {
        let (handler, _events) = make_handler(None, None, false);

        // Self-referential events should not panic
        for event_variant in [
            AiEvent::AlarmResolved {
                alarm_id: "a1".into(),
                thing_id: "d1".into(),
                rule_id: None,
            },
            AiEvent::HeartbeatPersistFailed {
                workspace_id: "ws_1".into(),
                reason: "test".into(),
            },
            AiEvent::ReflectionFailed {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                reason: "test".into(),
            },
            AiEvent::ChatCompleted {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                model: "gpt-4".into(),
                messages: vec![],
            },
        ] {
            let event = wrap_ai_event(&event_variant);
            handler.handle_ai_event(&event).await;
        }
    }

    #[tokio::test]
    async fn test_shutting_down_skips_handling() {
        let (handler, events) = make_handler(None, None, true); // shutting_down = true
        let mut rx = events.subscribe();

        let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result: ok_result("ws_test"),
        });
        handler.handle_ai_event(&event).await;

        assert!(
            rx.try_recv().is_err(),
            "shutting down must skip HeartbeatResultReady emission"
        );
    }

    #[tokio::test]
    async fn test_extract_payload_from_text() {
        let content = RichContent::new_text("Test event".to_string(), r#"{"key":"value"}"#.to_string());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(r#"{"key":"value"}"#.to_string()));
    }

    #[tokio::test]
    async fn test_extract_payload_empty_content() {
        let content = RichContent::new_text("Test".to_string(), String::new());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(String::new()));
    }

    // ── T18 X6 心跳桥 ──────────────────────────────────────────

    mod heartbeat_bridge {
        use super::*;
        use crate::runtime::thing_agent::scheduler::{EnqueueError, Scheduler};
        use crate::runtime::thing_agent::traits::DirectiveSink;
        use chrono::Utc;
        use std::sync::Mutex;
        use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

        /// 内存 dedup 真源夹具（Task 6 替代 SQLite runs repo）：元组
        /// (outcome, verified, age_hours) 逐条写入（run_id 为 run_{i}），
        /// problem_key 固定为桥接测试的 "set_hvac:dev-1"。
        fn registry_with(runs: &[(Outcome, bool, u32)]) -> RunRegistry {
            let reg = RunRegistry::new();
            for (i, (outcome, verified, age_hours)) in runs.iter().enumerate() {
                reg.record_problem_run(
                    "ws_1",
                    "set_hvac:dev-1",
                    &format!("run_{i}"),
                    *outcome,
                    *verified,
                    Utc::now() - chrono::Duration::hours(i64::from(*age_hours)),
                );
            }
            reg
        }

        #[derive(Default)]
        struct RecordingSink {
            signals: Mutex<Vec<WakeSignal>>,
        }

        impl DirectiveSink for RecordingSink {
            fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
                self.signals.lock().unwrap().push(signal);
                Ok(())
            }
        }

        fn proposal(tool_name: &str, thing_id: Option<&str>) -> Proposal {
            Proposal {
                id: "p1".into(),
                workspace_id: "ws_1".into(),
                agent_id: "hb".into(),
                tool_name: tool_name.into(),
                thing_id: thing_id.map(str::to_string),
                summary: "车间温度超过阈值 30°C".into(),
                reason: "连续 3 次采样超限".into(),
                risk: "medium".into(),
                parameters: None,
                created_at: "2026-08-03T00:00:00Z".into(),
                status: ProposalStatus::Pending,
            }
        }

        fn result_with(proposals: Vec<Proposal>) -> HeartbeatResult {
            HeartbeatResult {
                id: "test-tick".to_string(),
                workspace_id: "ws_1".into(),
                status: HeartbeatStatus::Complete,
                summary: "tick done".into(),
                task_count: 1,
                executed_actions: vec![],
                proposals,
                error: None,
            }
        }

        fn bridge(registry: RunRegistry) -> (HeartbeatBridge, Arc<RecordingSink>) {
            let sink = Arc::new(RecordingSink::default());
            (HeartbeatBridge::new(registry, sink.clone()), sink)
        }

        async fn dispatched(runs: Vec<(Outcome, bool, u32)>) -> Arc<RecordingSink> {
            let (bridge, sink) = bridge(registry_with(&runs));
            bridge
                .dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            sink
        }

        #[test]
        fn problem_key_uses_structured_fields_not_free_text() {
            let p = proposal("set_hvac", Some("dev-1"));
            assert_eq!(HeartbeatBridge::problem_key_of(&p), "set_hvac:dev-1");
            // 无目标设备的提案用稳定占位符，不含摘要/原因等自由文本。
            let p = proposal("set_hvac", None);
            assert_eq!(HeartbeatBridge::problem_key_of(&p), "set_hvac:-");
        }

        #[tokio::test]
        async fn no_prior_run_dispatches_with_heartbeat_directive_shape() {
            let sink = dispatched(vec![]).await;
            let signals = sink.signals.lock().unwrap();
            assert_eq!(signals.len(), 1);
            let sig = &signals[0];
            assert_eq!(sig.workspace_id, "ws_1");
            // O5/O24：心跳来源 directive 降 Normal、不参与合并、标记来源。
            assert_eq!(sig.priority, Priority::Normal);
            assert_eq!(sig.dedup_key, None, "心跳 directive 不进合并窗口");
            match &sig.source {
                TriggerSource::UserDirective {
                    user_id,
                    text,
                    session_key,
                    source,
                    problem_key,
                } => {
                    assert_eq!(user_id, "heartbeat");
                    assert_eq!(source.as_deref(), Some("heartbeat"));
                    assert_eq!(problem_key.as_deref(), Some("set_hvac:dev-1"));
                    assert_eq!(*session_key, None);
                    assert!(text.contains("set_hvac:dev-1"), "指令文本携带 problem：{text}");
                    assert!(text.contains("车间温度超过阈值"), "指令文本携带摘要：{text}");
                    assert!(text.contains("请诊断并处置"), "指令文本可执行：{text}");
                }
                other => panic!("expected UserDirective, got {other:?}"),
            }
        }

        // O11 全 outcome 矩阵：窗口内最近一次 run 的 outcome 决定是否抑制。
        #[tokio::test]
        async fn outcome_matrix_suppresses_and_dispatches() {
            // 抑制：failed / rejected / budget_exceeded / no_action_needed /
            // acted+verified（各一例，年龄 1h 在 6h 窗口内）。
            for (outcome, verified) in [
                (Outcome::Failed, false),
                (Outcome::Rejected, false),
                (Outcome::BudgetExceeded, false),
                (Outcome::NoActionNeeded, false),
                (Outcome::Acted, true),
            ] {
                let sink = dispatched(vec![(outcome, verified, 1)]).await;
                assert!(
                    sink.signals.lock().unwrap().is_empty(),
                    "{outcome:?} (verified={verified}) must suppress"
                );
            }
        }

        #[tokio::test]
        async fn acted_unverified_allows_exactly_one_retry_in_window() {
            // 窗口内仅 1 次 acted+未 verified → 放行一次重试。
            let sink = dispatched(vec![(Outcome::Acted, false, 1)]).await;
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "first retry allowed");
            // 窗口内已有 2 次 → 第二次起跳过。
            let sink = dispatched(vec![(Outcome::Acted, false, 1), (Outcome::Acted, false, 2)]).await;
            assert!(sink.signals.lock().unwrap().is_empty(), "second retry suppressed");
        }

        #[tokio::test]
        async fn recurrence_beyond_6h_window_dispatches_again() {
            // 超 6h 旧 Run 不抑制：7h 前 acted+verified → 放行。
            let sink = dispatched(vec![(Outcome::Acted, true, 7)]).await;
            assert_eq!(
                sink.signals.lock().unwrap().len(),
                1,
                "recurrence after 6h must dispatch"
            );
        }

        #[tokio::test]
        async fn ack_suppresses_for_7_days() {
            // ack 抑制（行级，fix round 1）：6h 窗口内最新 run 被 ack → 跳过。
            let reg = registry_with(&[(Outcome::Acted, true, 1)]);
            reg.mark_problem_acked("ws_1", "set_hvac:dev-1", "run_0");
            let (b, sink) = bridge(reg);
            b.dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert!(sink.signals.lock().unwrap().is_empty(), "acked within 6h suppressed");

            // 6h 窗口外、7 天内 acked（72h）→ 仍跳过（复发在 ack 抑制期内）。
            let reg = registry_with(&[(Outcome::Acted, true, 72)]);
            reg.mark_problem_acked("ws_1", "set_hvac:dev-1", "run_0");
            let (b, sink) = bridge(reg);
            b.dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert!(sink.signals.lock().unwrap().is_empty(), "acked within 7d suppressed");

            // ack 超 7 天（192h）→ 抑制过期，放行。
            let reg = registry_with(&[(Outcome::Acted, true, 192)]);
            reg.mark_problem_acked("ws_1", "set_hvac:dev-1", "run_0");
            let (b, sink) = bridge(reg);
            b.dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert_eq!(
                sink.signals.lock().unwrap().len(),
                1,
                "ack older than 7d no longer suppresses"
            );
        }

        // 行级保真回归（fix round 1 审查反例）：旧模型把 ack 塌缩为
        // last_acked_at，ack 旧 run 会误抑制更新的未 ack run。场景：
        // run_0 在 7d 窗内但 6h 窗外（100h，acted+verified），run_1 在 6h
        // 窗内（1h，acted+未 verified，窗口内仅此 1 条——计数分支放行）。
        #[tokio::test]
        async fn ack_of_older_run_does_not_suppress_newer_unacked_run() {
            // ack 旧 run_0：最新 run_1 未 ack → 必须放行（DB 语义）。
            let reg = registry_with(&[(Outcome::Acted, true, 100), (Outcome::Acted, false, 1)]);
            reg.mark_problem_acked("ws_1", "set_hvac:dev-1", "run_0");
            let (b, sink) = bridge(reg);
            b.dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert_eq!(
                sink.signals.lock().unwrap().len(),
                1,
                "ack 旧 run 不得抑制更新的未 ack run（行级保真）"
            );

            // 对照：ack 最新 run_1 → ack 分支抑制。
            let reg = registry_with(&[(Outcome::Acted, true, 100), (Outcome::Acted, false, 1)]);
            reg.mark_problem_acked("ws_1", "set_hvac:dev-1", "run_1");
            let (b, sink) = bridge(reg);
            b.dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert!(
                sink.signals.lock().unwrap().is_empty(),
                "最新 run 已 ack → ack 分支抑制"
            );
        }

        #[tokio::test]
        async fn no_proposals_dispatches_nothing() {
            let sink = dispatched(vec![]).await; // sanity: one proposal dispatches
            assert_eq!(sink.signals.lock().unwrap().len(), 1);

            let (bridge, sink) = bridge(RunRegistry::new());
            bridge.dispatch_proposals("ws_1", &result_with(vec![])).await;
            assert!(
                sink.signals.lock().unwrap().is_empty(),
                "HeartbeatCompleted without proposals must not dispatch"
            );
        }

        // 已裁决（approved/rejected）的提案不二次投递——心跳结果里混入历史
        // 提案时不得重复打扰。
        #[tokio::test]
        async fn decided_proposals_are_not_dispatched() {
            for status in [ProposalStatus::Approved, ProposalStatus::Rejected] {
                let (bridge, sink) = bridge(RunRegistry::new());
                let mut decided = proposal("set_hvac", Some("dev-1"));
                decided.status = status.clone();
                bridge.dispatch_proposals("ws_1", &result_with(vec![decided])).await;
                assert!(
                    sink.signals.lock().unwrap().is_empty(),
                    "{status:?} proposal must not be re-dispatched"
                );
            }

            // 对照：同批 pending 提案照常投递。
            let sink = dispatched(vec![]).await;
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "pending sanity check");
        }

        // 心跳 directive 经真实调度器：不参与合并窗口（source=Some 绕过 30s
        // 窗口立即执行），且不触发 60s 同文去重（同文本连投两条都受理）。
        #[tokio::test]
        async fn heartbeat_directive_bypasses_merge_window_and_text_dedup() {
            tokio::time::pause();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let handle = Scheduler::spawn("ws_1".to_string(), move |sig| {
                let tx = tx.clone();
                Box::pin(async move {
                    let _ = tx.send(sig);
                })
            });

            let p = proposal("set_hvac", Some("dev-1"));
            let sig1 = heartbeat_directive("ws_1", "set_hvac:dev-1".into(), &p);
            let sig2 = heartbeat_directive("ws_1", "set_hvac:dev-1".into(), &p);
            handle.enqueue(sig1).expect("first heartbeat directive");
            // 同文本第二条：不是 60s Duplicate（那是 chat/API 用户指令的规则）。
            handle
                .enqueue(sig2)
                .expect("same-text heartbeat directive is not a Duplicate");

            // 暂停时钟下不 advance：若进了 30s 合并窗口则永远收不到。
            let first = rx.recv().await.expect("runs immediately, no merge window");
            let second = rx
                .recv()
                .await
                .expect("second identical directive also runs immediately");
            assert_eq!(first.priority, Priority::Normal);
            assert!(matches!(first.source, TriggerSource::UserDirective { .. }));
            assert!(
                !matches!(first.source, TriggerSource::Merged { .. }),
                "heartbeat directives must never be merged"
            );
            assert!(!matches!(second.source, TriggerSource::Merged { .. }));
        }

        // Orchestrator 接线（Task 6）：HeartbeatCompleted 发射
        // HeartbeatResultReady（Task 8 订阅者落库）并驱动心跳桥投递。
        #[tokio::test]
        async fn heartbeat_completed_emits_result_ready_and_drives_bridge() {
            let (bridge, sink) = bridge(RunRegistry::new());
            let (handler, events) = make_handler(None, Some(Arc::new(bridge)), false);
            let mut rx = events.subscribe();

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;

            let ev = rx.try_recv().expect("HeartbeatResultReady emitted synchronously");
            match ev.kind {
                AgentEventKind::HeartbeatResultReady { result } => {
                    assert_eq!(result.workspace_id, "ws_1");
                    assert_eq!(result.proposals.len(), 1);
                }
                _ => panic!("expected HeartbeatResultReady"),
            }
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "bridge dispatched the proposal");
        }

        // 无桥（None）时 HeartbeatCompleted 仅发射事件，不 panic。
        #[tokio::test]
        async fn heartbeat_completed_without_bridge_only_emits() {
            let (handler, events) = make_handler(None, None, false);
            let mut rx = events.subscribe();

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;
            let ev = rx.try_recv().expect("HeartbeatResultReady emitted synchronously");
            assert!(matches!(ev.kind, AgentEventKind::HeartbeatResultReady { .. }));
        }
    }
}
