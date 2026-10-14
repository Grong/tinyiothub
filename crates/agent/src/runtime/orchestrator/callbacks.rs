//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatBridge.dispatch() (Task 11:转
//!                        TriggerSource::Heartbeat 变体入统一循环,N3 优先级
//!                        映射+分键 dedup)
//! HeartbeatCompleted --> HeartbeatBridge.dispatch_proposals() (X6, O21)
//!                        （T15 起无生产发布者——统一 Run 循环内的心跳 run
//!                        提案由持久化订阅者 RunRecorded 支路落库；臂保留
//!                        供桥接单测与测试注入）
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> ThingAgentManager.start()
//! WorkspaceDeleted    --> ThingAgentManager.stop()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tracing::{debug, info, warn};

use crate::runtime::event::types::AiEvent;
use crate::runtime::heartbeat::types::{HeartbeatResult, HeartbeatSignal, SignalPriority};
use crate::runtime::thing_agent::governor::HeartbeatGovernor;
use crate::runtime::thing_agent::manager::ThingAgentManager;
use crate::runtime::thing_agent::registry::RunRegistry;
use crate::runtime::thing_agent::traits::DirectiveSink;
use crate::runtime::thing_agent::types::{HeartbeatAlarmContext, Outcome, Priority, TriggerSource, WakeSignal};
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

/// X6 心跳桥（O21 裁决）：订阅 `AiEvent::HeartbeatCompleted`（T15 起无
/// 生产发布者——统一 Run 循环内的心跳 run 提案由持久化订阅者落库；臂
/// 保留供测试注入），从心跳报告的结构化 proposals 提取问题，按 O11 dedup
/// 后投递 `UserDirective{ source: Some("heartbeat"), priority: Normal }`
/// 给 thing-agent loop 处置。
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
    /// 连败治理（T4）：告警信号到达时 resume 暂停态（外部信号恢复暂停）。
    /// 与 ThingAgentManager 的 heartbeat governor 共享同一实例（T13 bootstrap
    /// 接线；agent crate 内 runtime.rs 先行共享）。
    governor: HeartbeatGovernor,
}

/// O11 dedup 窗口：同 problem_key 6h 内的 Run 参与抑制判定；超窗旧 Run 不
/// 抑制（复发可再处置）。
pub const PROBLEM_WINDOW_HOURS: u32 = 6;
/// O11：人工 ack 后该 problem_key 的抑制时长（7 天）。
pub const ACK_SUPPRESS_HOURS: u32 = 7 * 24;

impl HeartbeatBridge {
    pub fn new(registry: RunRegistry, sink: Arc<dyn DirectiveSink>, governor: HeartbeatGovernor) -> Self {
        Self {
            registry,
            sink,
            governor,
        }
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

    /// T11（N3）：告警链转 Heartbeat 变体入统一循环——AlarmEvent 还原为
    /// HeartbeatSignal（沿用 T3 前 runner.signal 的信号形态：reason/context/
    /// 优先级按 severity），经 [`Self::dispatch`] 投递
    /// `TriggerSource::Heartbeat { alarm: Some(..) }`。
    ///
    /// 与 T3 旧路的差异：不再产 UserDirective 调查指令、不做 O11 run 级抑制
    /// ——防抖由调度层分键 dedup（`heartbeat:{ws}:{thing}:{alarm_type}` 的
    /// 30s merge window）承担；Critical 告警映射 Priority::Critical，绕 merge
    /// window 与节流（scheduler ②），不被在途 tick 吞（N3）。
    pub async fn dispatch_alarm_investigation(&self, alarm: &tinyiothub_core::models::event::AlarmEvent) {
        let signal = HeartbeatSignal {
            workspace_id: alarm.workspace_id.clone(),
            reason: format!("Alarm: {}", alarm.message),
            context: format!("thing_id={}, alarm_type={}", alarm.thing_id, alarm.alarm_type),
            priority: if alarm.severity.eq_ignore_ascii_case("critical") {
                SignalPriority::Critical
            } else {
                SignalPriority::High
            },
            thing_id: Some(alarm.thing_id.clone()),
            alarm_type: Some(alarm.alarm_type.clone()),
            rule_id: alarm.rule_id.clone(),
        };
        self.dispatch(&signal).await;
    }

    /// 告警信号统一入口（Task 11）：enqueue 前 `governor.resume()`——外部信号
    /// 恢复暂停（resume 仅 paused 时清态，未暂停为 no-op）。投递失败
    /// （队列满/loop 未启动）仅记录日志——告警信号不享
    /// "排队不丢"（O5 同心跳 directive 先例），丢弃可接受。
    pub async fn dispatch(&self, s: &HeartbeatSignal) {
        self.governor.resume(&s.workspace_id);
        let signal = to_wake_signal(s);
        match self.sink.enqueue(signal) {
            Ok(()) => info!(
                workspace_id = %s.workspace_id,
                priority = %s.priority.label(),
                reason = %s.reason,
                "alarm signal dispatched to unified loop"
            ),
            Err(e) => warn!(
                workspace_id = %s.workspace_id,
                reason = %s.reason,
                error = %e,
                "alarm signal not admitted (queue full/loop not started)"
            ),
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

/// HeartbeatSignal → WakeSignal 转换（优先级映射 N3：Critical→Critical，
/// High/Normal→Normal）。
fn to_wake_signal(s: &HeartbeatSignal) -> WakeSignal {
    let priority = match s.priority {
        SignalPriority::Critical => Priority::Critical, // 绕过 merge window 与节流（scheduler ②）
        SignalPriority::High | SignalPriority::Normal => Priority::Normal,
    };
    let dedup_key = match s.dedup_key() {
        Some((thing, alarm)) => format!("heartbeat:{}:{}:{}", s.workspace_id, thing, alarm),
        None => format!("heartbeat:{}", s.workspace_id),
    };
    WakeSignal {
        workspace_id: s.workspace_id.clone(),
        priority,
        source: TriggerSource::Heartbeat {
            alarm: Some(HeartbeatAlarmContext {
                thing_id: s.thing_id.clone(),
                alarm_type: s.alarm_type.clone(),
                rule_id: s.rule_id.clone(),
                reason: s.reason.clone(),
                context: s.context.clone(),
            }),
        },
        dedup_key: Some(dedup_key),
    }
}

/// 心跳来源 directive（O5/O24）：Normal 优先级、dedup_key=None 不参与合并、
/// source=Some("heartbeat") 标记来源（不走 60s 同文去重、不享排队不丢）。
fn heartbeat_directive(workspace_id: &str, problem_key: String, proposal: &Proposal) -> WakeSignal {
    WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority: Priority::Normal,
        source: TriggerSource::UserDirective {
            user_id: "heartbeat".to_string(),
            text: crate::prompt::investigation::heartbeat_directive_text(&problem_key, proposal),
            session_key: None,
            source: Some("heartbeat".to_string()),
            problem_key: Some(problem_key),
        },
        dedup_key: None,
    }
}

/// Cross-domain callback handler.
pub struct AiEventHandler {
    /// T15 thing-agent loop registry; None where the loop is not deployed.
    thing_agent_manager: Option<Arc<ThingAgentManager>>,
    /// T18 X6 心跳桥；None 时 HeartbeatCompleted/AlarmCreated 不投递。
    heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
    shutting_down: Arc<AtomicBool>,
}

impl AiEventHandler {
    pub fn new(
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
        shutting_down: Arc<AtomicBool>,
    ) -> Self {
        Self {
            thing_agent_manager,
            heartbeat_bridge,
            shutting_down,
        }
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
                // T11（N3）：告警转 Heartbeat 变体入统一循环（bridge 内做
                // 优先级映射+分键 dedup+ governor resume）。心跳保持主动巡检
                // 定位，告警是被动响应——两者共用统一串行循环但分键不互合并。
                if let Some(bridge) = &self.heartbeat_bridge {
                    bridge.dispatch_alarm_investigation(alarm).await;
                }
            }
            AiEvent::HeartbeatCompleted { workspace_id, result } => {
                // X6 心跳桥：O11 dedup 依据 RunRegistry 内存（Task 6），
                // 不依赖本次心跳结果的持久化（T15 起落库由持久化订阅者的
                // RunRecorded 心跳支路承担）。
                if let Some(bridge) = &self.heartbeat_bridge {
                    bridge.dispatch_proposals(workspace_id, result).await;
                }
            }
            AiEvent::WorkspaceCreated { workspace_id } => {
                if let Some(manager) = &self.thing_agent_manager {
                    manager.start(workspace_id);
                }
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
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
    use crate::runtime::heartbeat::types::HeartbeatStatus;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent};

    use tinyiothub_core::event::EventHandler;

    /// 显式构造夹具（无 Default 上下文）。
    fn make_handler(
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        bridge: Option<Arc<HeartbeatBridge>>,
        shutting_down: bool,
    ) -> AiEventHandler {
        AiEventHandler::new(thing_agent_manager, bridge, Arc::new(AtomicBool::new(shutting_down)))
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

    #[tokio::test]
    async fn test_handler_construction() {
        let handler = make_handler(None, None, false);
        assert_eq!(handler.name(), "AiEventHandler");
    }

    #[tokio::test]
    async fn test_should_handle_filters_ai_events() {
        let handler = make_handler(None, None, false);

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

    #[tokio::test]
    async fn test_alarm_created_non_critical_no_signal() {
        let handler = make_handler(None, None, false);

        // Non-critical alarm should not trigger heartbeat signal
        let alarm = AiEvent::AlarmCreated(tinyiothub_core::models::event::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws_1".into(),
            thing_id: "d1".into(),
            alarm_type: "high_temp".into(),
            severity: "warning".into(),
            message: "Temperature is high".into(),
            rule_id: None,
            condition_desc: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        });

        let event = wrap_ai_event(&alarm);
        handler.handle_ai_event(&event).await;
        // No assertion needed — just verifying no panic for non-critical alarms
    }

    #[tokio::test]
    async fn test_workspace_created_and_deleted_no_panic() {
        let handler = make_handler(None, None, false);

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

        let handler = make_handler(Some(manager.clone()), None, false);

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
        let handler = make_handler(None, None, false);

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
        let handler = make_handler(None, None, true); // shutting_down = true

        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_test".to_string(),
        });
        // 关停中跳过处理（无 manager 时本臂本就 no-op——仅断言不 panic 早退）。
        handler.handle_ai_event(&event).await;
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

        impl RecordingSink {
            fn last(&self) -> Option<WakeSignal> {
                self.signals.lock().unwrap().last().cloned()
            }
        }

        #[async_trait::async_trait]
        impl DirectiveSink for RecordingSink {
            fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
                self.signals.lock().unwrap().push(signal);
                Ok(())
            }

            async fn drain(&self, _workspace_id: &str) {}
        }

        /// 带 thing_id/alarm_type 的分键路径夹具。
        fn sample_alarm_signal(ws: &str) -> HeartbeatSignal {
            HeartbeatSignal {
                workspace_id: ws.into(),
                reason: "设备离线".into(),
                context: "{}".into(),
                priority: SignalPriority::Critical,
                thing_id: Some("d1".into()),
                alarm_type: Some("offline".into()),
                rule_id: Some("r1".into()),
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
                trace: vec![],
            }
        }

        fn bridge(registry: RunRegistry) -> (HeartbeatBridge, Arc<RecordingSink>) {
            let sink = Arc::new(RecordingSink::default());
            (
                HeartbeatBridge::new(registry, sink.clone(), HeartbeatGovernor::default()),
                sink,
            )
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

        // Orchestrator 接线：HeartbeatCompleted 驱动心跳桥投递（T15 起结果
        // 落库由持久化订阅者 RunRecorded 心跳支路承担，不再经本 handler）。
        #[tokio::test]
        async fn heartbeat_completed_drives_bridge() {
            let (bridge, sink) = bridge(RunRegistry::new());
            let handler = make_handler(None, Some(Arc::new(bridge)), false);

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;

            assert_eq!(sink.signals.lock().unwrap().len(), 1, "bridge dispatched the proposal");
        }

        // 无桥（None）时 HeartbeatCompleted 不投递、不 panic。
        #[tokio::test]
        async fn heartbeat_completed_without_bridge_is_noop() {
            let handler = make_handler(None, None, false);

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;
        }

        // ── T11 告警链转 Heartbeat 变体(N3)──

        #[test]
        fn alarm_signal_maps_priority_and_distinct_dedup_key() {
            // Critical→Critical;dedup 分键 heartbeat:{ws}:{thing}:{alarm_type}
            let sig = to_wake_signal(&sample_alarm_signal("ws1"));
            assert_eq!(sig.workspace_id, "ws1");
            assert_eq!(sig.priority, Priority::Critical);
            assert_eq!(sig.dedup_key.as_deref(), Some("heartbeat:ws1:d1:offline"));
            match &sig.source {
                TriggerSource::Heartbeat { alarm: Some(a) } => {
                    assert_eq!(a.thing_id.as_deref(), Some("d1"));
                    assert_eq!(a.alarm_type.as_deref(), Some("offline"));
                    assert_eq!(a.rule_id.as_deref(), Some("r1"));
                    assert_eq!(a.reason, "设备离线");
                    assert_eq!(a.context, "{}");
                }
                other => panic!("expected Heartbeat alarm source, got {other:?}"),
            }
            // High→Normal(不绕 merge window,但分键天然不合并)
            let mut high = sample_alarm_signal("ws1");
            high.priority = SignalPriority::High;
            assert_eq!(to_wake_signal(&high).priority, Priority::Normal);
            // 无 thing/alarm 时退化为 heartbeat:{ws}
            let mut bare = sample_alarm_signal("ws1");
            bare.thing_id = None;
            bare.alarm_type = None;
            assert_eq!(to_wake_signal(&bare).dedup_key.as_deref(), Some("heartbeat:ws1"));
            // N3:告警键 ≠ 定时 tick 键 heartbeat:ws1——Critical 告警不被在途 tick 吞
            assert_ne!(sig.dedup_key.as_deref(), Some("heartbeat:ws1"));
        }

        #[tokio::test]
        async fn alarm_signal_resumes_paused_governor_and_enqueues() {
            let governor = HeartbeatGovernor::default();
            for _ in 0..5 {
                governor.record_failure("ws1");
            }
            assert!(governor.is_paused("ws1", Duration::from_secs(3600)));

            // RecordingSink:实现 `DirectiveSink` 的测试桩,enqueue 记录 WakeSignal
            let sink = Arc::new(RecordingSink::default());
            let bridge = HeartbeatBridge::new(RunRegistry::new(), sink.clone(), governor.clone());
            bridge.dispatch(&sample_alarm_signal("ws1")).await;

            assert!(
                !governor.is_paused("ws1", Duration::from_secs(3600)),
                "告警信号恢复暂停"
            );
            let sig = sink.last().expect("wake signal enqueued");
            assert_eq!(sig.priority, Priority::Critical);
            assert_eq!(sig.dedup_key.as_deref(), Some("heartbeat:ws1:d1:offline"));
        }

        // ── T12 回归(T11 改写:告警链走 Heartbeat 变体)──

        fn alarm_event(severity: &str) -> AiEvent {
            AiEvent::AlarmCreated(tinyiothub_core::models::event::AlarmEvent {
                id: "alarm-1".into(),
                workspace_id: "ws_1".into(),
                thing_id: "dev-1".into(),
                alarm_type: "property_threshold".into(),
                severity: severity.into(),
                message: "温度超过阈值".into(),
                rule_id: Some("rule-9".into()),
                condition_desc: None,
                resolved: false,
                created_at: Utc::now(),
            })
        }

        /// T11(N3):告警(全严重级)→ Heartbeat 变体入统一循环,不再产
        /// UserDirective 调查指令。warning→High→Normal;分键 dedup 与定时
        /// tick 的 heartbeat:{ws} 不互合并。
        #[tokio::test]
        async fn alarm_created_dispatches_heartbeat_variant_with_distinct_dedup_key() {
            let (bridge, sink) = bridge(RunRegistry::new());
            let handler = make_handler(None, Some(Arc::new(bridge)), false);

            handler.handle_ai_event(&wrap_ai_event(&alarm_event("warning"))).await;

            let signals = sink.signals.lock().unwrap();
            assert_eq!(signals.len(), 1, "alarm dispatched to unified loop");
            let sig = &signals[0];
            assert_eq!(sig.workspace_id, "ws_1");
            // N3 映射:仅 Critical 绕 merge window+节流,其余 Normal。
            assert_eq!(sig.priority, Priority::Normal);
            assert_eq!(
                sig.dedup_key.as_deref(),
                Some("heartbeat:ws_1:dev-1:property_threshold"),
                "告警分键,不与在途 tick 合并"
            );
            match &sig.source {
                TriggerSource::Heartbeat { alarm: Some(a) } => {
                    assert_eq!(a.thing_id.as_deref(), Some("dev-1"));
                    assert_eq!(a.alarm_type.as_deref(), Some("property_threshold"));
                    assert_eq!(a.rule_id.as_deref(), Some("rule-9"));
                    assert!(a.reason.contains("温度超过阈值"), "告警原因入上下文:{}", a.reason);
                }
                other => panic!("expected Heartbeat alarm source, got {other:?}"),
            }
        }

        /// N3:critical 告警 → SignalPriority::Critical → Priority::Critical,
        /// 绕 merge window 与节流(scheduler ②)——不得被在途 tick 吞。
        #[tokio::test]
        async fn critical_alarm_maps_to_critical_priority() {
            let (bridge, sink) = bridge(RunRegistry::new());
            let handler = make_handler(None, Some(Arc::new(bridge)), false);

            handler.handle_ai_event(&wrap_ai_event(&alarm_event("critical"))).await;

            let signals = sink.signals.lock().unwrap();
            assert_eq!(signals.len(), 1);
            assert_eq!(signals[0].priority, Priority::Critical);
        }

        /// T11 回归:无桥时不 panic、不派发。
        #[tokio::test]
        async fn alarm_created_without_bridge_is_noop() {
            let handler = make_handler(None, None, false);
            handler.handle_ai_event(&wrap_ai_event(&alarm_event("error"))).await;
        }

        /// T-8(T11 形态):告警路径不做 outcome 抑制——历史 Failed run 不固化
        /// 盲区,同 thing+rule 的下一条告警仍入队。
        #[tokio::test]
        async fn alarm_dispatch_not_suppressed_by_prior_failed_run() {
            let registry = RunRegistry::new();
            registry.record_problem_run(
                "ws_1",
                "alarm:dev-1:rule-9",
                "run-failed-1",
                Outcome::Failed,
                false,
                Utc::now(),
            );
            let (bridge, sink) = bridge(registry);
            let AiEvent::AlarmCreated(alarm) = alarm_event("warning") else {
                unreachable!()
            };
            bridge.dispatch_alarm_investigation(&alarm).await;
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "Failed 后告警仍入队");
        }
    }
}
