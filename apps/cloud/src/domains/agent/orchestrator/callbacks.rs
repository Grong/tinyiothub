//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> TriggerRouter (Plan 4A Task 7 owns alarm wakes)
//! HeartbeatCompleted --> 无订阅臂（X6 心跳桥自动派发已退役，2026-10-16 用户
//!                        裁定：提案一律走审批队列 agent_actions，不再自动派
//!                        UserDirective 修复 run；事件枚举保留供 DLQ 重放兼容）
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> ThingAgentManager.start_workspace_watch()
//! WorkspaceDeleted    --> ThingAgentManager.stop_workspace_watch()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tracing::{debug, info, warn};

use crate::domains::agent::event::types::AiEvent;
use crate::domains::agent::thing_agent::manager::ThingAgentManager;
use crate::domains::agent::thing_agent::registry::RunRegistry;

/// 告警桥（原 X6 心跳桥）只剩 O11 ack 回写：告警派发已迁至
/// `TriggerRouter`（Plan 4A Task 7），本桥的
/// `dispatch_alarm_investigation`/`to_wake_signal` 随 legacy workspace loop
/// 一并退役。
///
/// [`RunRegistry`] 的 problem_key 元数据映射保留：ack 端点回写链
/// （cloud agent_tasks → Orchestrator → [`Self::mark_problem_acked`]）仍是
/// 生产路径。
pub struct HeartbeatBridge {
    registry: RunRegistry,
}

impl HeartbeatBridge {
    pub fn new(registry: RunRegistry) -> Self {
        Self { registry }
    }

    /// 兼容保留：告警派发已由 TriggerRouter 接管，本开关为 no-op，下一
    /// 版本随桥一并删除。
    pub fn set_alarm_dispatch(&self, _enabled: bool) {}

    /// O11 ack 抑制入口（Task 6，fix round 1 行级保真）：cloud 侧 ack 端点
    /// DB 写成功后经 Orchestrator 转发至此，按 run_id 标记内存 dedup 真源中
    /// 对应 run 条目。
    ///
    /// X6 退役注（2026-10-16）：读侧（should_dispatch）已随 dispatch_proposals
    /// 删除，本写入暂无消费者；回写链保留待 O11 machinery 整体清理时一并裁决。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        self.registry.mark_problem_acked(workspace_id, problem_key, run_id);
    }
}

/// Cross-domain callback handler.
pub struct AiEventHandler {
    /// T15 thing-agent loop registry; None where the loop is not deployed.
    thing_agent_manager: Option<Arc<ThingAgentManager>>,
    /// 原 T18 X6 心跳桥；告警派发已迁至 TriggerRouter，现仅承接 O11 ack
    /// 回写。None 时 ack 转发 no-op。
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

    /// Plan 4A Task 7：转发报警派发开关到心跳桥；无桥时 no-op。
    pub fn set_alarm_dispatch(&self, enabled: bool) {
        if let Some(bridge) = &self.heartbeat_bridge {
            bridge.set_alarm_dispatch(enabled);
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
            AiEvent::AlarmCreated(_alarm) => {
                // 告警唤醒由 TriggerRouter::start 的 EventBus 订阅承担
                // （Plan 4A Task 7）；旧心跳桥派发已随 legacy loop 退役。
            }
            // X6 心跳桥自动派发已退役（2026-10-16 用户裁定）：提案一律走审批
            // 队列（agent_actions），不再自动派 UserDirective 修复 run——与 D9
            // 信任门语义一致（所有动作过门，无旁路自动修复）。HeartbeatCompleted
            // 自 T15 起无生产发布者，变体保留供 DLQ 重放兼容（no-op）。
            AiEvent::HeartbeatCompleted { .. } => {}
            AiEvent::WorkspaceCreated { workspace_id } => {
                if let Some(manager) = &self.thing_agent_manager {
                    manager.start_workspace_watch(workspace_id);
                }
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                if let Some(manager) = &self.thing_agent_manager {
                    manager.stop_workspace_watch(workspace_id).await;
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

pub(crate) fn extract_payload(content: &RichContent) -> Option<String> {
    content.elements().iter().find_map(|el| match el {
        ContentElement::Text { content, .. } => Some(content.clone()),
        _ => None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent};

    use tinyiothub_core::event::EventHandler;

    use crate::domains::agent::thing_agent::manager::tests::{RecordingLedger, stub_manager, watch_deps};

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
    async fn test_alarm_created_is_a_noop() {
        let handler = make_handler(None, None, false);

        // Alarm wakes are owned by TriggerRouter; the legacy bridge dispatch is
        // retired, so the handler arm must not panic (and does nothing).
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
    }

    #[tokio::test]
    async fn test_workspace_created_and_deleted_no_panic() {
        let handler = make_handler(None, None, false);

        // WorkspaceCreated (no manager → no-op, but no panic)
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

    // Workspace lifecycle events drive the per-workspace watch engine.
    #[tokio::test]
    async fn test_workspace_lifecycle_drives_thing_agent_manager() {
        let parts = stub_manager().await;
        let manager = parts.manager.clone();
        let ledger = Arc::new(RecordingLedger::default());
        manager.set_watch_deps(watch_deps(vec![], ledger));

        let handler = make_handler(Some(manager.clone()), None, false);

        assert!(!manager.is_running("ws_life"));
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(manager.is_running("ws_life"), "WorkspaceCreated must start the watch");

        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(!manager.is_running("ws_life"), "WorkspaceDeleted must stop the watch");
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

    #[test]
    fn alarm_dispatch_switch_is_a_retained_noop() {
        let bridge = HeartbeatBridge::new(RunRegistry::new());
        bridge.set_alarm_dispatch(false);
        bridge.set_alarm_dispatch(true);
    }
}
