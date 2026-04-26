use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use tinyiothub_core::error::Result;
use tinyiothub_core::models::event::Event;

/// Event handler interface
///
/// All event handlers must implement this interface
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle event
    async fn handle(&self, event: &Event) -> Result<()>;

    /// Get handler name (for logging)
    fn name(&self) -> &str;

    /// Determine whether this event should be handled
    fn should_handle(&self, event: &Event) -> bool;

    /// Get handler priority (lower number = higher priority)
    fn priority(&self) -> u8 {
        100 // Default priority
    }
}

/// Event bus - infrastructure layer message distribution mechanism
///
/// Responsibilities:
/// - Receive domain events
/// - Distribute to all registered handlers
/// - Provide real-time subscription capability
///
/// Contains no business logic, only responsible for technical message passing.
///
/// # Lock-free reads
///
/// `handlers` is stored in an `ArcSwap<Vec<...>>`.  Reads (the hot path,
/// `dispatch_to_handlers`) do a single atomic pointer load — no lock, no
/// contention with `register_handler`.  Writes are rare (startup only) and
/// perform a clone-then-swap of the entire vector.
pub struct EventBus {
    /// Real-time event broadcast channel
    event_sender: broadcast::Sender<Event>,
    /// Keep a receiver to prevent channel closure
    _event_receiver: broadcast::Receiver<Event>,

    /// Registered event handlers — lock-free snapshot via ArcSwap.
    handlers: Arc<ArcSwap<Vec<Arc<dyn EventHandler>>>>,
}

impl EventBus {
    /// Create new event bus
    pub fn new() -> Self {
        let (event_sender, event_receiver) = broadcast::channel(1000);

        Self {
            event_sender,
            _event_receiver: event_receiver,
            handlers: Arc::new(ArcSwap::from(Arc::new(Vec::new()))),
        }
    }

    /// Publish event
    ///
    /// Event will be:
    /// 1. Broadcast to all real-time subscribers
    /// 2. Distributed to all registered handlers
    pub async fn publish(&self, event: Event) -> Result<()> {
        debug!(
            "Publishing event: {} (type: {:?}, level: {:?})",
            event.id(),
            event.event_type(),
            event.level()
        );

        // 1. Broadcast to real-time subscribers
        match self.event_sender.send(event.clone()) {
            Ok(subscriber_count) => {
                debug!("Event {} broadcasted to {} subscribers", event.id(), subscriber_count);
            }
            Err(_) => {
                warn!("Event {} broadcast failed — channel may be full (capacity=1000)", event.id());
            }
        }

        // 2. Distribute to all handlers
        self.dispatch_to_handlers(&event).await?;

        Ok(())
    }

    /// Subscribe to events (for real-time push)
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_sender.subscribe()
    }

    /// Register event handler.
    ///
    /// Writes are lock-free: clone current Vec, push, sort, then atomic swap.
    /// Callers do not need to `await`.
    pub fn register_handler(&self, handler: Arc<dyn EventHandler>) {
        let current = self.handlers.load();
        let mut new: Vec<Arc<dyn EventHandler>> = (**current).clone();
        new.push(handler);
        new.sort_by_key(|h| h.priority());
        let name = new.last().map(|h| h.name().to_string()).unwrap_or_else(|| "unknown".to_string());
        self.handlers.store(Arc::new(new));
        info!("Registered event handler: {}", name);
    }

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.event_sender.receiver_count()
    }

    /// Get handler count
    pub fn handler_count(&self) -> usize {
        self.handlers.load().len()
    }

    /// Dispatch event to all handlers.
    ///
    /// Lock-free: a single atomic pointer load yields the sorted handler list.
    /// Handlers within the same priority run concurrently; priorities are
    /// respected sequentially (lower number = earlier).
    async fn dispatch_to_handlers(&self, event: &Event) -> Result<()> {
        let handlers = self.handlers.load(); // atomic pointer load — O(1), no lock

        let mut i = 0;
        while i < handlers.len() {
            let priority = handlers[i].priority();
            let mut j = i;
            while j < handlers.len() && handlers[j].priority() == priority {
                j += 1;
            }

            let batch = &handlers[i..j];
            let event_clone = event.clone();

            let futures: Vec<_> = batch
                .iter()
                .filter(|h| h.should_handle(&event_clone))
                .map(|handler| {
                    let handler = Arc::clone(handler);
                    let event = event_clone.clone();
                    async move {
                        if let Err(e) = handler.handle(&event).await {
                            error!(
                                "Handler {} failed to process event {}: {}",
                                handler.name(),
                                event.id(),
                                e
                            );
                        }
                    }
                })
                .collect();

            futures_util::future::join_all(futures).await;

            i = j;
        }

        Ok(())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Publish an event in a fire-and-forget manner.
///
/// Spawns a task to publish the event, logging any errors.
pub async fn publish_event_safe(event_bus: Arc<EventBus>, event: Event) {
    tokio::spawn(async move {
        if let Err(e) = event_bus.publish(event).await {
            tracing::error!("Failed to publish event: {}", e);
        }
    });
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscriber_count", &self.event_sender.receiver_count())
            .field("handler_count", &self.handler_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::event::{
        EventLevel, EventSource, EventType, RichContent, SystemEventType,
    };

    fn create_test_event() -> Event {
        Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .expect("Failed to create test event")
    }

    #[tokio::test]
    async fn test_event_bus_creation() {
        let bus = EventBus::new();
        // subscriber_count includes the internal _event_receiver that keeps the channel alive
        assert_eq!(bus.subscriber_count(), 1);
        assert_eq!(bus.handler_count(), 0);
    }

    #[tokio::test]
    async fn test_event_publishing() {
        let bus = EventBus::new();
        let event = create_test_event();

        let result = bus.publish(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        // subscriber_count = internal receiver + new subscriber
        assert_eq!(bus.subscriber_count(), 2);

        let event = create_test_event();
        let event_id = event.id().clone();

        // Publish event
        bus.publish(event).await.unwrap();

        // Receive event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.id(), &event_id);
    }
}
