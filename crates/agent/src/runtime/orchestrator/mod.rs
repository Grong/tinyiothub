//! AI subsystem orchestrator -- top-level coordinator.
//!
//! Cross-domain communication flows through the Orchestrator:
//! AlarmCreated       --> EventBus --> Orchestrator --> HeartbeatRunner.signal()
//! (Chat reflection is handled directly in chat/service.rs)
//! HeartbeatCompleted --> EventBus --> Orchestrator --> AgentEventBus
//!                    (HeartbeatResultReady, Task 8 持久化订阅者落库） +
//!                    HeartbeatBridge.dispatch_proposals()
//! WorkspaceCreated    --> EventBus --> Orchestrator --> HeartbeatRunner.start() +
//! ThingAgentManager.start() WorkspaceDeleted    --> EventBus --> Orchestrator -->
//! HeartbeatRunner.remove_workspace() + ThingAgentManager.stop()

pub mod callbacks;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{info, warn};

use crate::runtime::event::bus::{AiEventPublisher, DropNotifier};
use crate::runtime::events::AgentEventBus;
use crate::runtime::heartbeat::runner::HeartbeatRunner;
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
        heartbeat_runner: Arc<HeartbeatRunner>,
        agent_events: Arc<AgentEventBus>,
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
            heartbeat_runner,
            agent_events,
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

    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        self.handler.heartbeat_runner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::runtime::event::types::AiEvent;
    use crate::runtime::events::AgentEventKind;
    use crate::runtime::heartbeat::types::{HeartbeatConfig, HeartbeatResult, HeartbeatStatus};

    fn sample_result() -> HeartbeatResult {
        HeartbeatResult {
            id: "test-tick".to_string(),
            workspace_id: "ws_1".into(),
            status: HeartbeatStatus::Complete,
            summary: "done".into(),
            task_count: 1,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        }
    }

    fn make_orchestrator(bus: Arc<EventBus>, events: Arc<AgentEventBus>) -> Orchestrator {
        let runner = Arc::new(HeartbeatRunner::new(
            Arc::new(AiEventPublisher::new(bus.clone())),
            HeartbeatConfig::default(),
        ));
        Orchestrator::new(bus, runner, events, None, None, None)
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let bus = Arc::new(EventBus::new());
        let events = Arc::new(AgentEventBus::new(16));
        let mut rx = events.subscribe();
        let orch = make_orchestrator(bus, events);

        orch.start();
        orch.start();

        orch.event_publisher().publish(AiEvent::HeartbeatCompleted {
            workspace_id: "ws_1".into(),
            result: sample_result(),
        });
        // publisher→worker→bus→handler→emit 为异步链，轮询等待首个事件。
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("HeartbeatResultReady emitted")
            .expect("channel open");
        assert!(matches!(first.kind, AgentEventKind::HeartbeatResultReady { .. }));
        // 重复 start() 不得二次注册 handler → 不再有第二个事件。
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            rx.try_recv().is_err(),
            "duplicate start() must not double-register the handler"
        );
        orch.shutdown().await;
    }
}
