//! AI subsystem orchestrator -- top-level coordinator.
//!
//! Cross-domain communication flows through the Orchestrator:
//! AlarmCreated       --> EventBus --> Orchestrator --> HeartbeatRunner.signal()
//! (Chat reflection is handled directly in chat/service.rs)
//! HeartbeatCompleted --> EventBus --> Orchestrator --> HeartbeatTaskRepository.insert_result()
//! WorkspaceCreated    --> EventBus --> Orchestrator --> HeartbeatRunner.start() +
//! ThingAgentManager.start() WorkspaceDeleted    --> EventBus --> Orchestrator -->
//! HeartbeatRunner.stop() + ThingAgentManager.stop()

pub mod callbacks;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{info, warn};

use crate::loop_::event::bus::{AiEventPublisher, DropNotifier};
use crate::loop_::event::dlq::DeadLetterQueue;
use crate::loop_::heartbeat::repo::HeartbeatTaskRepository;
use crate::loop_::heartbeat::runner::HeartbeatRunner;
use crate::loop_::thing_agent::manager::ThingAgentManager;
use tinyiothub_memory::service::MemoryService;

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
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        memory_service: Arc<MemoryService>,
        drop_notifier: Option<Arc<dyn DropNotifier>>,
        dlq: Option<Arc<dyn DeadLetterQueue>>,
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
            task_repo,
            memory_service,
            event_publisher.clone(),
            dlq,
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
        // Wait for in-flight persist retries to abort before tearing down the
        // publisher — abandoning them mid-backoff leaves their fate unknown.
        self.handler.drain_retries().await;
        self.event_publisher.shutdown().await;
        info!("Orchestrator shutdown complete");
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn event_publisher(&self) -> &Arc<AiEventPublisher> {
        &self.event_publisher
    }

    pub fn memory_service(&self) -> &Arc<MemoryService> {
        self.handler.memory_service()
    }

    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        self.handler.heartbeat_runner()
    }

    /// Retry tasks currently alive — observability for shutdown/metrics.
    pub fn in_flight_retries(&self) -> usize {
        self.handler.in_flight_retries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use crate::loop_::event::types::AiEvent;
    use crate::loop_::heartbeat::types::{HeartbeatConfig, HeartbeatResult, HeartbeatStatus};
    use crate::loop_::orchestrator::callbacks::tests::{MockTaskRepo, make_memory_service};

    fn sample_result() -> HeartbeatResult {
        HeartbeatResult {
            workspace_id: "ws_1".into(),
            status: HeartbeatStatus::Complete,
            summary: "done".into(),
            task_count: 1,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        }
    }

    fn make_orchestrator(bus: Arc<EventBus>, repo: Arc<MockTaskRepo>) -> Orchestrator {
        let runner = Arc::new(HeartbeatRunner::new(
            repo.clone(),
            Arc::new(AiEventPublisher::new(bus.clone())),
            HeartbeatConfig::default(),
        ));
        Orchestrator::new(bus, runner, repo, make_memory_service(), None, None, None, None)
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let bus = Arc::new(EventBus::new());
        let repo = Arc::new(MockTaskRepo::new());
        let calls = repo.insert_result_calls();
        let orch = make_orchestrator(bus, repo);

        orch.start();
        orch.start();

        orch.event_publisher().publish(AiEvent::HeartbeatCompleted {
            workspace_id: "ws_1".into(),
            result: sample_result(),
        });
        tokio::time::sleep(Duration::from_millis(150)).await;
        orch.shutdown().await;

        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "duplicate start() must not double-register the handler"
        );
    }

    #[tokio::test]
    async fn shutdown_drains_in_flight_retries() {
        let bus = Arc::new(EventBus::new());
        let repo = Arc::new(MockTaskRepo::failing());
        let orch = make_orchestrator(bus, repo);
        orch.start();

        orch.event_publisher().publish(AiEvent::HeartbeatCompleted {
            workspace_id: "ws_1".into(),
            result: sample_result(),
        });
        // Let the first persist attempt fail and the retry task spawn. Poll
        // instead of a fixed sleep — the publisher→worker→bus→handler chain
        // can exceed any single sleep under CI load.
        let mut in_flight = 0;
        for _ in 0..100 {
            in_flight = orch.in_flight_retries();
            if in_flight >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(in_flight >= 1, "a retry task should be in flight");

        let started = Instant::now();
        orch.shutdown().await;

        assert_eq!(
            orch.in_flight_retries(),
            0,
            "shutdown must wait for retry tasks to finish"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "shutdown must abort retry backoff sleeps, not wait them out"
        );
    }
}
