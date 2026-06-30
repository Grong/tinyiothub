//! AI subsystem orchestrator -- top-level coordinator.
//!
//! Cross-domain communication flows through the Orchestrator:
//! AlarmCreated       --> EventBus --> Orchestrator --> HeartbeatRunner.signal()
//! ChatCompleted      --> EventBus --> Orchestrator --> MemoryService.reflect()
//! HeartbeatCompleted --> EventBus --> Orchestrator --> HeartbeatTaskRepository.insert_result()
//! WorkspaceCreated    --> EventBus --> Orchestrator --> HeartbeatRunner.start()
//! WorkspaceDeleted    --> EventBus --> Orchestrator --> HeartbeatRunner.stop()

pub mod callbacks;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tinyiothub_runtime::EventBus;
use tracing::info;

use crate::event::bus::{AiEventPublisher, DropNotifier};
use crate::event::dlq::DeadLetterQueue;
use crate::heartbeat::repo::HeartbeatTaskRepository;
use crate::heartbeat::runner::HeartbeatRunner;
use crate::memory::service::MemoryService;

use callbacks::AiEventHandler;

pub struct Orchestrator {
    event_bus: Arc<EventBus>,
    handler: Arc<AiEventHandler>,
    event_publisher: Arc<AiEventPublisher>,
    shutting_down: Arc<AtomicBool>,
}

impl Orchestrator {
    pub fn new(
        event_bus: Arc<EventBus>,
        heartbeat_runner: Arc<HeartbeatRunner>,
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        memory_service: Arc<MemoryService>,
        drop_notifier: Option<Arc<dyn DropNotifier>>,
        dlq: Option<Arc<dyn DeadLetterQueue>>,
    ) -> Self {
        let mut publisher = AiEventPublisher::new(event_bus.clone());
        if let Some(n) = drop_notifier {
            publisher = publisher.with_drop_notifier(n);
        }
        let event_publisher = Arc::new(publisher);

        let handler = Arc::new(AiEventHandler::new(
            heartbeat_runner,
            task_repo,
            memory_service,
            event_publisher.clone(),
            dlq,
        ));

        Self {
            event_bus,
            handler,
            event_publisher,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self) {
        info!("Orchestrator starting -- registering AI event handler");
        self.event_bus.register_handler(self.handler.clone());
        info!("Orchestrator started");
    }

    pub async fn shutdown(&self) {
        info!("Orchestrator shutting down...");
        self.shutting_down.store(true, Ordering::SeqCst);
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
}
