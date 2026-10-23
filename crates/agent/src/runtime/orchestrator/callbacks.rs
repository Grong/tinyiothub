//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatBridge.dispatch() (Task 11:转
//!                        TriggerSource::Heartbeat 变体入统一循环,N3 优先级
//!                        映射+分键 dedup)
//! HeartbeatCompleted --> 无订阅臂（X6 心跳桥自动派发已退役，2026-10-16 用户
//!                        裁定：提案一律走审批队列 agent_actions，不再自动派
//!                        UserDirective 修复 run；事件枚举保留供 DLQ 重放兼容）
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> ThingAgentManager.start()
//! WorkspaceDeleted    --> ThingAgentManager.stop()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tracing::{debug, info, warn};

use crate::runtime::event::types::AiEvent;
use crate::runtime::heartbeat::types::{HeartbeatSignal, SignalPriority};
use crate::runtime::thing_agent::governor::HeartbeatGovernor;
use crate::runtime::thing_agent::manager::ThingAgentManager;
use crate::runtime::thing_agent::registry::RunRegistry;
use crate::runtime::thing_agent::traits::DirectiveSink;
use crate::runtime::thing_agent::types::{HeartbeatAlarmContext, Priority, TriggerSource, WakeSignal};

/// 告警桥（原 X6 心跳桥）：订阅 `AiEvent::AlarmCreated`，将告警还原为
/// HeartbeatSignal 投递 `TriggerSource::Heartbeat { alarm: Some(..) }` 入
/// 统一循环（N3 优先级映射 + 分键 dedup + governor resume）。
///
/// X6 心跳桥自动派发已退役（2026-10-16 用户裁定）：提案一律走审批队列
/// （agent_actions），不再自动派 UserDirective 修复 run——`dispatch_proposals`/
/// `problem_key_of`/`should_dispatch`（O11 6h 窗口 + ack 抑制读侧）随之删除。
/// 告警 flapping 抑制由调度层 30s 分键合并窗承担（接受现状裁定）。
///
/// [`RunRegistry`] 的 problem_key 元数据映射保留：ack 端点回写链
/// （cloud agent_tasks → Orchestrator → [`Self::mark_problem_acked`]）仍是
/// 生产路径。X6 退役后写-读失衡（写 ack/run 元数据，无读侧消费者），
/// machinery 保留待后续清理。
pub struct HeartbeatBridge {
    registry: RunRegistry,
    sink: Arc<dyn DirectiveSink>,
    /// 连败治理（T4）：告警信号到达时 resume 暂停态（外部信号恢复暂停）。
    /// 与 ThingAgentManager 的 heartbeat governor 共享同一实例（T13 bootstrap
    /// 接线；agent crate 内 runtime.rs 先行共享）。
    governor: HeartbeatGovernor,
}

impl HeartbeatBridge {
    pub fn new(registry: RunRegistry, sink: Arc<dyn DirectiveSink>, governor: HeartbeatGovernor) -> Self {
        Self {
            registry,
            sink,
            governor,
        }
    }

    /// O11 ack 抑制入口（Task 6，fix round 1 行级保真）：cloud 侧 ack 端点
    /// DB 写成功后经 Orchestrator 转发至此，按 run_id 标记内存 dedup 真源中
    /// 对应 run 条目。
    ///
    /// X6 退役注（2026-10-16）：读侧（should_dispatch）已随 dispatch_proposals
    /// 删除，本写入暂无消费者；回写链保留待 O11 machinery 整体清理时一并裁决。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        self.registry.mark_problem_acked(workspace_id, problem_key, run_id);
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

/// Cross-domain callback handler.
pub struct AiEventHandler {
    /// T15 thing-agent loop registry; None where the loop is not deployed.
    thing_agent_manager: Option<Arc<ThingAgentManager>>,
    /// 告警桥（原 T18 X6 心跳桥；X6 自动派发退役后只剩告警路径）；None 时
    /// AlarmCreated 不投递。
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
            // X6 心跳桥自动派发已退役（2026-10-16 用户裁定）：提案一律走审批
            // 队列（agent_actions），不再自动派 UserDirective 修复 run——与 D9
            // 信任门语义一致（所有动作过门，无旁路自动修复）。HeartbeatCompleted
            // 自 T15 起无生产发布者，变体保留供 DLQ 重放兼容（no-op）。
            AiEvent::HeartbeatCompleted { .. } => {}
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
    use crate::runtime::thing_agent::types::Outcome;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
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

    // ── 告警桥（原 T18 X6 心跳桥；X6 自动派发 2026-10-16 退役后只剩告警路径）──

    mod heartbeat_bridge {
        use super::*;
        use crate::runtime::thing_agent::scheduler::EnqueueError;
        use chrono::Utc;
        use std::sync::Mutex;

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

        fn bridge(registry: RunRegistry) -> (HeartbeatBridge, Arc<RecordingSink>) {
            let sink = Arc::new(RecordingSink::default());
            (
                HeartbeatBridge::new(registry, sink.clone(), HeartbeatGovernor::default()),
                sink,
            )
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
