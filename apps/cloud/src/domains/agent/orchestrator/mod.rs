//! AI subsystem orchestrator -- top-level coordinator.
//!
//! Cross-domain communication flows through the Orchestrator:
//! AlarmCreated       --> EventBus --> Orchestrator --> HeartbeatBridge.dispatch()
//!                    (TriggerSource::Heartbeat 变体入统一循环)
//! (Chat reflection is handled directly in chat/service.rs)
//! HeartbeatCompleted --> 无订阅臂（X6 心跳桥自动派发 2026-10-16 退役：提案
//!                    一律走审批队列；事件枚举保留供 DLQ 重放兼容）
//! WorkspaceCreated    --> EventBus --> Orchestrator --> ThingAgentManager.start()
//! WorkspaceDeleted    --> EventBus --> Orchestrator --> ThingAgentManager.stop()

pub mod callbacks;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{info, warn};

use crate::domains::agent::event::bus::{AiEventPublisher, DropNotifier};
use crate::domains::agent::thing_agent::manager::ThingAgentManager;

use callbacks::AiEventHandler;

pub struct Orchestrator {
    event_bus: Arc<EventBus>,
    handler: Arc<AiEventHandler>,
    event_publisher: Arc<AiEventPublisher>,
    shutting_down: Arc<AtomicBool>,
    started: AtomicBool,
}

impl Orchestrator {
    pub fn new(
        event_bus: Arc<EventBus>,
        drop_notifier: Option<Arc<dyn DropNotifier>>,
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        heartbeat_bridge: Option<Arc<callbacks::HeartbeatBridge>>,
    ) -> Self {
        let mut publisher = AiEventPublisher::new(event_bus.clone());
        if let Some(n) = drop_notifier {
            publisher = publisher.with_drop_notifier(n);
        }
        let event_publisher = Arc::new(publisher);

        let shutting_down = Arc::new(AtomicBool::new(false));

        let handler = Arc::new(AiEventHandler::new(
            thing_agent_manager,
            heartbeat_bridge,
            shutting_down.clone(),
        ));

        Self {
            event_bus,
            handler,
            event_publisher,
            shutting_down,
            started: AtomicBool::new(false),
        }
    }

    pub fn start(&self) {
        // register_handler appends unconditionally — a second start() would
        // make every event get handled twice.
        if self.started.swap(true, Ordering::SeqCst) {
            warn!("Orchestrator already started, ignoring duplicate start()");
            return;
        }
        info!("Orchestrator starting -- registering AI event handler");
        self.event_bus.register_handler(self.handler.clone());
        info!("Orchestrator started");
    }

    pub async fn shutdown(&self) {
        info!("Orchestrator shutting down...");
        self.shutting_down.store(true, Ordering::SeqCst);
        self.event_publisher.shutdown().await;
        info!("Orchestrator shutdown complete");
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn event_publisher(&self) -> &Arc<AiEventPublisher> {
        &self.event_publisher
    }

    /// O11 ack 抑制入口（Task 6，fix round 1 行级保真）：cloud 侧 ack 端点
    /// DB 写成功后调用，按 run_id 转发到心跳桥的内存 dedup 真源；无桥时 no-op。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        self.handler.mark_problem_acked(workspace_id, problem_key, run_id);
    }

    /// Plan 4A Task 7：TriggerRouter 接管报警后关闭 legacy 心跳桥的告警派发；
    /// 无桥时为 no-op。
    pub fn set_alarm_dispatch(&self, enabled: bool) {
        self.handler.set_alarm_dispatch(enabled);
    }
}
