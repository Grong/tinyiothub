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

use crate::runtime::event::bus::{AiEventPublisher, DropNotifier};
use crate::runtime::thing_agent::manager::ThingAgentManager;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    use crate::runtime::event::types::AiEvent;
    use crate::runtime::orchestrator::callbacks::HeartbeatBridge;
    use crate::runtime::thing_agent::governor::HeartbeatGovernor;
    use crate::runtime::thing_agent::registry::RunRegistry;
    use crate::runtime::thing_agent::scheduler::EnqueueError;
    use crate::runtime::thing_agent::traits::DirectiveSink;
    use crate::runtime::thing_agent::types::WakeSignal;

    /// 告警事件夹具（X6 退役后观测量改走 AlarmCreated 告警路径投递）。
    fn sample_alarm() -> tinyiothub_core::models::event::AlarmEvent {
        tinyiothub_core::models::event::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws_1".into(),
            thing_id: "dev-1".into(),
            alarm_type: "offline".into(),
            severity: "critical".into(),
            message: "设备离线".into(),
            rule_id: None,
            condition_desc: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        }
    }

    /// 计数 sink：重复注册 handler 会让同一事件处理两次——enqueue 计数即
    /// 观测量（替代旧事件发射断言，T15 后该事件退役）。
    #[derive(Default)]
    struct CountingSink {
        signals: Mutex<Vec<WakeSignal>>,
    }

    #[async_trait::async_trait]
    impl DirectiveSink for CountingSink {
        fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
            self.signals.lock().unwrap().push(signal);
            Ok(())
        }

        async fn drain(&self, _workspace_id: &str) {}
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let bus = Arc::new(EventBus::new());
        let sink = Arc::new(CountingSink::default());
        let bridge = Arc::new(HeartbeatBridge::new(
            RunRegistry::new(),
            sink.clone(),
            HeartbeatGovernor::default(),
        ));
        let orch = Orchestrator::new(bus, None, None, Some(bridge));

        orch.start();
        orch.start();

        orch.event_publisher().publish(AiEvent::AlarmCreated(sample_alarm()));
        // publisher→worker→bus→handler→bridge 为异步链，轮询等待首次投递。
        for _ in 0..100 {
            if !sink.signals.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // 重复 start() 不得二次注册 handler → 同一事件只投递一次。
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            sink.signals.lock().unwrap().len(),
            1,
            "duplicate start() must not double-register the handler"
        );
        orch.shutdown().await;
    }
}
