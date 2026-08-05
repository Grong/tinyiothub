//! Cross-domain alarm event payload + outbound port toward the AI subsystem.
//!
//! Moved from `crates/ai/src/alarm` in P4-Task19. In P4-Task22 the payload
//! struct itself was re-homed to `tinyiothub_event::AlarmEvent` (the agent
//! crate must not depend on this crate; both depend on the event crate) and
//! is re-exported here so `tinyiothub_alarm::AlarmEvent` keeps resolving.
//! The alarm crate never names agent types — `AlarmService` publishes through
//! the [`AlarmAiPublisher`] port and the composition layer (cloud) adapts
//! `tinyiothub_agent::loop_::event::bus::AiEventPublisher` to it.

pub use tinyiothub_event::AlarmEvent;

/// Outbound port: notify the AI subsystem that a significant alarm occurred.
///
/// Implemented in the composition layer by an adapter over
/// `tinyiothub_agent::loop_::event::bus::AiEventPublisher` (see
/// `cloud::shared::ai_adapter::AlarmAiPublisherAdapter`).
pub trait AlarmAiPublisher: Send + Sync {
    fn publish_alarm_created(&self, event: AlarmEvent);
}
