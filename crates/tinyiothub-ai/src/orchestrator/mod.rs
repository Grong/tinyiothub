//! AI subsystem orchestrator -- top-level coordinator.
//!
//! All cross-domain communication flows through the Orchestrator:
//! AlarmCreated       --> EventBus --> Orchestrator --> PatrolManager.wake()
//! ChatCompleted      --> EventBus --> Orchestrator --> MemoryService.reflect()
//! PatrolCompleted    --> EventBus --> Orchestrator --> ActionRepo.insert()
//! WorkspaceCreated   --> EventBus --> Orchestrator --> PatrolManager.start()
//! WorkspaceDeleted   --> EventBus --> Orchestrator --> PatrolManager.stop()

pub mod callbacks;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tinyiothub_runtime::EventBus;
use tracing::info;

use crate::event::bus::AiEventPublisher;
use crate::memory::service::MemoryService;
use crate::patrol::manager::PatrolManager;
use crate::patrol::repo::ActionRepository;

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
        patrol_manager: Arc<PatrolManager>,
        action_repo: Arc<dyn ActionRepository>,
        memory_service: Arc<MemoryService>,
    ) -> Self {
        let event_publisher = Arc::new(AiEventPublisher::new(event_bus.clone()));

        let handler = Arc::new(AiEventHandler::new(
            patrol_manager,
            action_repo,
            memory_service,
            event_publisher.clone(),
        ));

        Self {
            event_bus,
            handler,
            event_publisher,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Register all cross-domain callbacks on the event bus.
    pub fn start(&self) {
        info!("Orchestrator starting -- registering AI event handler");
        self.event_bus.register_handler(self.handler.clone());
        info!("Orchestrator started");
    }

    /// Graceful shutdown.
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
}
