//! AiEventPublisher — serialized, bounded wrapper around the shared EventBus.
//!
//! Events are queued onto a bounded mpsc channel and published by a single
//! background worker, which guarantees FIFO ordering (the old per-event
//! `tokio::spawn` raced and could reorder events). A full or closed queue is
//! a real drop: it increments `events_dropped` and fires the `DropNotifier`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tinyiothub_runtime::EventBus;
use tokio::sync::mpsc;
use tracing::{error, warn};

use super::types::AiEvent;

/// Default capacity of the publish queue. Bounds memory under load spikes.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Called when an event is dropped (queue full, closed, or publish failed).
/// Cloud wires this to alerting (metrics, webhook, log aggregation).
pub trait DropNotifier: Send + Sync {
    fn on_event_dropped(&self, event_type: &str, workspace_id: Option<&str>);
}

/// Logs dropped events via `tracing::warn!`. Minimal production default.
pub struct LoggingDropNotifier;

impl DropNotifier for LoggingDropNotifier {
    fn on_event_dropped(&self, event_type: &str, workspace_id: Option<&str>) {
        tracing::warn!(
            event_type,
            workspace_id = workspace_id.unwrap_or("unknown"),
            "AiEvent dropped — EventBus channel may be full or publish failed"
        );
    }
}

type SharedNotifier = Arc<Mutex<Option<Arc<dyn DropNotifier>>>>;

/// Wraps the shared EventBus for AI-specific publish semantics.
///
/// `publish` is sync and non-blocking: the event is queued for the worker.
/// Tracks `events_published` / `events_dropped` counters for observability.
pub struct AiEventPublisher {
    tx: Mutex<Option<mpsc::Sender<AiEvent>>>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    events_published: Arc<AtomicU64>,
    events_dropped: Arc<AtomicU64>,
    drop_notifier: SharedNotifier,
}

impl AiEventPublisher {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self::with_queue_capacity(bus, DEFAULT_QUEUE_CAPACITY)
    }

    /// Same as `new` but with an explicit queue capacity (tests, tuning).
    pub fn with_queue_capacity(bus: Arc<EventBus>, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        let events_published = Arc::new(AtomicU64::new(0));
        let events_dropped = Arc::new(AtomicU64::new(0));
        let drop_notifier: SharedNotifier = Arc::new(Mutex::new(None));

        let worker = tokio::spawn(run_worker(
            bus,
            rx,
            Arc::clone(&events_published),
            Arc::clone(&events_dropped),
            Arc::clone(&drop_notifier),
        ));

        Self {
            tx: Mutex::new(Some(tx)),
            worker: Mutex::new(Some(worker)),
            events_published,
            events_dropped,
            drop_notifier,
        }
    }

    /// Attach a drop notifier for alerting on event loss.
    pub fn with_drop_notifier(self, notifier: Arc<dyn DropNotifier>) -> Self {
        *self.drop_notifier.lock().unwrap() = Some(notifier);
        self
    }

    /// Publish an AiEvent. Non-blocking; a full or closed queue drops the
    /// event and is counted + notified.
    pub fn publish(&self, event: AiEvent) {
        let guard = self.tx.lock().unwrap();
        match guard.as_ref() {
            Some(tx) => match tx.try_send(event) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(event)) => {
                    self.record_drop(&event, "publish queue full");
                }
                Err(mpsc::error::TrySendError::Closed(event)) => {
                    self.record_drop(&event, "publisher shut down");
                }
            },
            None => {
                self.record_drop(&event, "publisher shut down");
            }
        }
    }

    /// Close the queue and wait until all queued events are published.
    /// Events published after this call are dropped.
    pub async fn shutdown(&self) {
        // Dropping the sender closes the channel; the worker drains the
        // remaining queue and then exits.
        self.tx.lock().unwrap().take();
        if let Some(handle) = self.worker.lock().unwrap().take() {
            if let Err(e) = handle.await {
                error!("AiEventPublisher worker join failed: {}", e);
            }
        }
    }

    fn record_drop(&self, event: &AiEvent, reason: &str) {
        let dropped = self.events_dropped.fetch_add(1, Ordering::Relaxed) + 1;
        let event_type = event.variant_name();
        let workspace_id = event.workspace_id().map(|s| s.to_string());
        warn!(dropped, reason, "AiEvent dropped");
        if let Some(ref notifier) = *self.drop_notifier.lock().unwrap() {
            notifier.on_event_dropped(&event_type, workspace_id.as_deref());
        }
    }

    pub fn events_published(&self) -> u64 {
        self.events_published.load(Ordering::Relaxed)
    }

    pub fn events_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }
}

/// Single consumer of the publish queue — preserves FIFO ordering.
async fn run_worker(
    bus: Arc<EventBus>,
    mut rx: mpsc::Receiver<AiEvent>,
    events_published: Arc<AtomicU64>,
    events_dropped: Arc<AtomicU64>,
    drop_notifier: SharedNotifier,
) {
    while let Some(event) = rx.recv().await {
        let event_type = event.variant_name();
        let workspace_id = event.workspace_id().map(|s| s.to_string());

        let ai_event_type = tinyiothub_core::models::event::AiEventType::from(&event);
        let event_type_obj = tinyiothub_core::models::event::EventType::Ai(ai_event_type);

        let payload = match serde_json::to_string(&event) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to serialize AiEvent: {}", e);
                count_drop(&events_dropped, &drop_notifier, &event_type, workspace_id.as_deref());
                continue;
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
                count_drop(&events_dropped, &drop_notifier, &event_type, workspace_id.as_deref());
                continue;
            }
        };

        match bus.publish(evt).await {
            Ok(_) => {
                events_published.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                warn!("EventBus publish failed for {}: {}", event_type, e);
                count_drop(&events_dropped, &drop_notifier, &event_type, workspace_id.as_deref());
            }
        }
    }
}

fn count_drop(
    events_dropped: &AtomicU64,
    drop_notifier: &SharedNotifier,
    event_type: &str,
    workspace_id: Option<&str>,
) {
    let dropped = events_dropped.fetch_add(1, Ordering::Relaxed) + 1;
    warn!(dropped, "AiEvent dropped — EventBus publish failed");
    if let Some(ref notifier) = *drop_notifier.lock().unwrap() {
        notifier.on_event_dropped(event_type, workspace_id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct CountingDropNotifier {
        calls: Arc<AtomicU64>,
    }

    impl CountingDropNotifier {
        fn new(calls: Arc<AtomicU64>) -> Self {
            Self { calls }
        }
    }

    impl DropNotifier for CountingDropNotifier {
        fn on_event_dropped(&self, _event_type: &str, _workspace_id: Option<&str>) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_publisher_creation() {
        let bus = Arc::new(EventBus::new());
        let publisher = AiEventPublisher::new(bus);
        assert_eq!(publisher.events_published(), 0);
        assert_eq!(publisher.events_dropped(), 0);
    }

    #[tokio::test]
    async fn test_publisher_with_drop_notifier() {
        let bus = Arc::new(EventBus::new());
        let calls = Arc::new(AtomicU64::new(0));
        let notifier = Arc::new(CountingDropNotifier::new(Arc::clone(&calls)));
        let publisher = AiEventPublisher::new(bus).with_drop_notifier(notifier);
        assert_eq!(publisher.events_dropped(), 0);

        publisher.publish(AiEvent::WorkspaceCreated {
            workspace_id: "ws_1".into(),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(publisher.events_published(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_multiple_publishes() {
        let bus = Arc::new(EventBus::new());
        let publisher = AiEventPublisher::new(bus);

        for i in 0..3 {
            publisher.publish(AiEvent::WorkspaceCreated {
                workspace_id: format!("ws_{}", i),
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(publisher.events_published(), 3);
    }

    #[tokio::test]
    async fn test_variant_names() {
        let alarm = AiEvent::AlarmCreated(crate::alarm::types::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws".into(),
            device_id: "d1".into(),
            alarm_type: "high_temp".into(),
            severity: "critical".into(),
            message: "test".into(),
            rule_id: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        });
        assert_eq!(alarm.variant_name(), "AlarmCreated");

        let hc = AiEvent::HeartbeatCompleted {
            workspace_id: "ws".into(),
            result: crate::heartbeat::types::HeartbeatResult {
                workspace_id: "ws".into(),
                status: crate::heartbeat::types::HeartbeatStatus::Complete,
                summary: "ok".into(),
                task_count: 0,
                executed_actions: vec![],
                proposals: vec![],
                error: None,
            },
        };
        assert_eq!(hc.variant_name(), "HeartbeatCompleted");

        let cc = AiEvent::ChatCompleted {
            workspace_id: "ws".into(),
            agent_id: "a1".into(),
            session_key: "sk".into(),
            model: "gpt-4".into(),
            messages: vec![],
        };
        assert_eq!(cc.variant_name(), "ChatCompleted");
    }

    #[tokio::test]
    async fn test_workspace_id_extraction() {
        let ws_created = AiEvent::WorkspaceCreated {
            workspace_id: "ws_1".into(),
        };
        assert_eq!(ws_created.workspace_id(), Some("ws_1"));

        let ws_deleted = AiEvent::WorkspaceDeleted {
            workspace_id: "ws_2".into(),
        };
        assert_eq!(ws_deleted.workspace_id(), Some("ws_2"));

        let alarm_resolved = AiEvent::AlarmResolved {
            alarm_id: "a1".into(),
            device_id: "d1".into(),
            rule_id: None,
        };
        assert_eq!(alarm_resolved.workspace_id(), None);
    }

    struct RecordingHandler {
        seen: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl tinyiothub_core::event::EventHandler for RecordingHandler {
        async fn handle(
            &self,
            event: &tinyiothub_core::models::event::Event,
        ) -> tinyiothub_core::error::Result<()> {
            self.seen.lock().unwrap().push(event.content().to_plain_text());
            Ok(())
        }
        fn name(&self) -> &str {
            "recording"
        }
        fn should_handle(&self, _event: &tinyiothub_core::models::event::Event) -> bool {
            true
        }
    }

    async fn wait_for(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("condition not met within timeout");
    }

    #[tokio::test]
    async fn test_publish_order_preserved() {
        let bus = Arc::new(EventBus::new());
        let seen = Arc::new(RecordingHandler { seen: std::sync::Mutex::new(Vec::new()) });
        bus.register_handler(seen.clone());
        let publisher = AiEventPublisher::new(bus);

        for i in 0..20 {
            publisher.publish(AiEvent::WorkspaceCreated {
                workspace_id: format!("ws_{:02}", i),
            });
        }
        publisher.shutdown().await;

        let seen = seen.seen.lock().unwrap();
        assert_eq!(seen.len(), 20);
        for (i, payload) in seen.iter().enumerate() {
            assert!(
                payload.contains(&format!("ws_{:02}", i)),
                "event {i} out of order: {payload}"
            );
        }
    }

    #[tokio::test]
    async fn test_shutdown_drains_pending_events() {
        let bus = Arc::new(EventBus::new());
        let publisher = AiEventPublisher::new(bus);

        for i in 0..5 {
            publisher.publish(AiEvent::WorkspaceCreated {
                workspace_id: format!("ws_{}", i),
            });
        }
        publisher.shutdown().await;

        assert_eq!(publisher.events_published(), 5);
        assert_eq!(publisher.events_dropped(), 0);
    }

    #[tokio::test]
    async fn test_publish_after_shutdown_counts_dropped() {
        let bus = Arc::new(EventBus::new());
        let calls = Arc::new(AtomicU64::new(0));
        let notifier = Arc::new(CountingDropNotifier::new(Arc::clone(&calls)));
        let publisher = AiEventPublisher::new(bus).with_drop_notifier(notifier);

        publisher.shutdown().await;
        publisher.publish(AiEvent::WorkspaceCreated {
            workspace_id: "ws_late".into(),
        });

        assert_eq!(publisher.events_dropped(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_full_queue_counts_dropped() {
        struct BlockingHandler {
            started: Arc<tokio::sync::Notify>,
            release: Arc<tokio::sync::Notify>,
            blocked_once: std::sync::atomic::AtomicBool,
        }

        #[async_trait::async_trait]
        impl tinyiothub_core::event::EventHandler for BlockingHandler {
            async fn handle(
                &self,
                _event: &tinyiothub_core::models::event::Event,
            ) -> tinyiothub_core::error::Result<()> {
                if !self.blocked_once.swap(true, Ordering::SeqCst) {
                    self.started.notify_one();
                    self.release.notified().await;
                }
                Ok(())
            }
            fn name(&self) -> &str {
                "blocking"
            }
            fn should_handle(&self, _event: &tinyiothub_core::models::event::Event) -> bool {
                true
            }
        }

        let bus = Arc::new(EventBus::new());
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        bus.register_handler(Arc::new(BlockingHandler {
            started: started.clone(),
            release: release.clone(),
            blocked_once: std::sync::atomic::AtomicBool::new(false),
        }));
        let publisher = AiEventPublisher::with_queue_capacity(bus, 1);

        // e1 is picked up by the worker and blocks inside the handler.
        publisher.publish(AiEvent::WorkspaceCreated { workspace_id: "ws_1".into() });
        started.notified().await;

        // e2 fills the queue (capacity 1), e3 overflows and must be counted.
        publisher.publish(AiEvent::WorkspaceCreated { workspace_id: "ws_2".into() });
        publisher.publish(AiEvent::WorkspaceCreated { workspace_id: "ws_3".into() });
        assert_eq!(publisher.events_dropped(), 1);

        release.notify_waiters();
        publisher.shutdown().await;
        assert_eq!(publisher.events_published(), 2);
        assert_eq!(publisher.events_dropped(), 1);
    }
}
