//! Global in-process broadcast bus for thing events (T6).
//!
//! One `tokio::sync::broadcast` channel (capacity 256) shared process-wide:
//! `route_thing_event` publishes a [`ThingEventSignal`] after each successful
//! persist; the thing-agent loop subscribes via `ThingAgentHost`. Lagging
//! subscribers compensate through `replay_events_since` (O27). Send failure
//! (no subscribers) is intentionally ignored.

use tinyiothub_ai::thing_agent::ThingEventSignal;
use tokio::sync::broadcast;

pub const THING_EVENT_BUS_CAPACITY: usize = 256;

pub struct ThingEventBus {
    tx: broadcast::Sender<ThingEventSignal>,
}

impl ThingEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(THING_EVENT_BUS_CAPACITY);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ThingEventSignal> {
        self.tx.subscribe()
    }

    /// Publish a signal. Returns the number of live receivers; 0 is fine
    /// (nobody subscribed yet — events are still persisted for replay).
    pub fn publish(&self, signal: ThingEventSignal) -> usize {
        self.tx.send(signal).unwrap_or(0)
    }
}

impl Default for ThingEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(actor: &str) -> ThingEventSignal {
        ThingEventSignal {
            workspace_id: "ws".to_string(),
            thing_id: "t".to_string(),
            event_name: "e".to_string(),
            event_id: 1,
            level: 1,
            data: serde_json::Value::Null,
            is_unknown: false,
            actor: actor.to_string(),
        }
    }

    #[tokio::test]
    async fn publish_without_subscribers_does_not_error() {
        let bus = ThingEventBus::new();
        assert_eq!(bus.publish(signal("device")), 0);
    }

    #[tokio::test]
    async fn subscriber_receives_published_signal() {
        let bus = ThingEventBus::new();
        let mut rx = bus.subscribe();
        assert_eq!(bus.publish(signal("agent")), 1);
        let got = rx.recv().await.expect("recv");
        assert_eq!(got.actor, "agent");
    }
}
