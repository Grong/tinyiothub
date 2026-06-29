//! AiEventPublisher — fire-and-forget wrapper around the shared EventBus.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tinyiothub_runtime::EventBus;
use tracing::{error, warn};

use super::types::AiEvent;

/// Wraps the shared EventBus for AI-specific publish semantics.
///
/// All publishes are fire-and-forget (spawned onto tokio).
/// Tracks `events_dropped` counter for observability.
pub struct AiEventPublisher {
    bus: Arc<EventBus>,
    events_published: Arc<AtomicU64>,
    events_dropped: Arc<AtomicU64>,
}

impl AiEventPublisher {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            events_published: Arc::new(AtomicU64::new(0)),
            events_dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Publish an AiEvent as a fire-and-forget operation.
    pub fn publish(&self, event: AiEvent) {
        let bus = self.bus.clone();
        let events_published = Arc::clone(&self.events_published);
        let events_dropped = Arc::clone(&self.events_dropped);

        tokio::spawn(async move {
            let ai_event_type = tinyiothub_core::models::event::AiEventType::from(&event);
            let event_type = tinyiothub_core::models::event::EventType::Ai(ai_event_type);

            let payload = match serde_json::to_string(&event) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to serialize AiEvent: {}", e);
                    return;
                }
            };

            use tinyiothub_core::models::event::{
                Event, EventLevel, EventSource, RichContent,
            };
            let evt = match Event::new(
                event_type,
                EventLevel::Info,
                EventSource::system("ai-subsystem".to_string(), None),
                RichContent::new_text("AiEvent".to_string(), payload),
            ) {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to create Event for AiEvent: {}", e);
                    return;
                }
            };

            match bus.publish(evt).await {
                Ok(_) => {
                    events_published.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    events_dropped.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        dropped = events_dropped.load(Ordering::Relaxed),
                        "AiEvent dropped — EventBus channel may be full"
                    );
                }
            }
        });
    }

    pub fn events_published(&self) -> u64 {
        self.events_published.load(Ordering::Relaxed)
    }

    pub fn events_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }
}

impl From<&AiEvent> for tinyiothub_core::models::event::AiEventType {
    fn from(event: &AiEvent) -> Self {
        match event {
            AiEvent::AlarmCreated(_) => tinyiothub_core::models::event::AiEventType::AlarmCreated,
            AiEvent::AlarmResolved { .. } => tinyiothub_core::models::event::AiEventType::AlarmResolved,
            AiEvent::PatrolCompleted { .. } => tinyiothub_core::models::event::AiEventType::PatrolCompleted,
            AiEvent::ChatCompleted { .. } => tinyiothub_core::models::event::AiEventType::ChatCompleted,
            AiEvent::WorkspaceCreated { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceCreated,
            AiEvent::WorkspaceDeleted { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceDeleted,
        }
    }
}
