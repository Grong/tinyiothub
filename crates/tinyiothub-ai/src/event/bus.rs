//! AiEventPublisher — fire-and-forget wrapper around the shared EventBus.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tinyiothub_runtime::EventBus;
use tracing::{error, warn};

use super::types::AiEvent;

/// Called when an event is dropped (EventBus full or publish failed).
/// Cloud wires this to alerting (metrics, webhook, log aggregation).
pub trait DropNotifier: Send + Sync {
    fn on_event_dropped(&self, event_type: &str, workspace_id: Option<&str>);
}

/// Wraps the shared EventBus for AI-specific publish semantics.
///
/// All publishes are fire-and-forget (spawned onto tokio).
/// Tracks `events_dropped` counter for observability.
/// An optional `DropNotifier` alerts external systems on drops.
pub struct AiEventPublisher {
    bus: Arc<EventBus>,
    events_published: Arc<AtomicU64>,
    events_dropped: Arc<AtomicU64>,
    drop_notifier: Option<Arc<dyn DropNotifier>>,
}

impl AiEventPublisher {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            events_published: Arc::new(AtomicU64::new(0)),
            events_dropped: Arc::new(AtomicU64::new(0)),
            drop_notifier: None,
        }
    }

    /// Attach a drop notifier for alerting on event loss.
    pub fn with_drop_notifier(mut self, notifier: Arc<dyn DropNotifier>) -> Self {
        self.drop_notifier = Some(notifier);
        self
    }

    /// Publish an AiEvent as a fire-and-forget operation.
    pub fn publish(&self, event: AiEvent) {
        let bus = self.bus.clone();
        let events_published = Arc::clone(&self.events_published);
        let events_dropped = Arc::clone(&self.events_dropped);
        let drop_notifier = self.drop_notifier.clone();
        let event_type = event.variant_name();
        let workspace_id = event.workspace_id().map(|s| s.to_string());

        tokio::spawn(async move {
            let ai_event_type = tinyiothub_core::models::event::AiEventType::from(&event);
            let event_type_obj = tinyiothub_core::models::event::EventType::Ai(ai_event_type);

            let payload = match serde_json::to_string(&event) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to serialize AiEvent: {}", e);
                    if let Some(ref notifier) = drop_notifier {
                        notifier.on_event_dropped(&event_type, workspace_id.as_deref());
                    }
                    return;
                }
            };

            use tinyiothub_core::models::event::{Event, EventLevel, EventSource, RichContent};
            let evt = match Event::new(
                event_type_obj,
                EventLevel::Info,
                EventSource::system("ai-subsystem".to_string(), None),
                RichContent::new_text("AiEvent".to_string(), payload),
            ) {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to create Event for AiEvent: {}", e);
                    if let Some(ref notifier) = drop_notifier {
                        notifier.on_event_dropped(&event_type, workspace_id.as_deref());
                    }
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
                    if let Some(ref notifier) = drop_notifier {
                        notifier.on_event_dropped(&event_type, workspace_id.as_deref());
                    }
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

impl AiEvent {
    /// Human-readable variant name for logging/alerting.
    fn variant_name(&self) -> String {
        match self {
            AiEvent::AlarmCreated(_) => "AlarmCreated".into(),
            AiEvent::AlarmResolved { .. } => "AlarmResolved".into(),
            AiEvent::HeartbeatCompleted { .. } => "HeartbeatCompleted".into(),
            AiEvent::ChatCompleted { .. } => "ChatCompleted".into(),
            AiEvent::WorkspaceCreated { .. } => "WorkspaceCreated".into(),
            AiEvent::WorkspaceDeleted { .. } => "WorkspaceDeleted".into(),
            AiEvent::HeartbeatPersistFailed { .. } => "HeartbeatPersistFailed".into(),
            AiEvent::ReflectionFailed { .. } => "ReflectionFailed".into(),
            AiEvent::ProposalCreated { .. } => "ProposalCreated".into(),
            AiEvent::ProposalResolved { .. } => "ProposalResolved".into(),
        }
    }
}

impl From<&AiEvent> for tinyiothub_core::models::event::AiEventType {
    fn from(event: &AiEvent) -> Self {
        match event {
            AiEvent::AlarmCreated(_) => tinyiothub_core::models::event::AiEventType::AlarmCreated,
            AiEvent::AlarmResolved { .. } => tinyiothub_core::models::event::AiEventType::AlarmResolved,
            AiEvent::HeartbeatCompleted { .. } => tinyiothub_core::models::event::AiEventType::HeartbeatCompleted,
            AiEvent::ChatCompleted { .. } => tinyiothub_core::models::event::AiEventType::ChatCompleted,
            AiEvent::WorkspaceCreated { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceCreated,
            AiEvent::WorkspaceDeleted { .. } => tinyiothub_core::models::event::AiEventType::WorkspaceDeleted,
            AiEvent::HeartbeatPersistFailed { .. } => {
                tinyiothub_core::models::event::AiEventType::HeartbeatPersistFailed
            }
            AiEvent::ReflectionFailed { .. } => tinyiothub_core::models::event::AiEventType::ReflectionFailed,
            AiEvent::ProposalCreated { .. } => tinyiothub_core::models::event::AiEventType::ProposalCreated,
            AiEvent::ProposalResolved { .. } => tinyiothub_core::models::event::AiEventType::ProposalResolved,
        }
    }
}
