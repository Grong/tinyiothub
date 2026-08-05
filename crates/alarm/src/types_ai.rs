//! Cross-domain alarm event payload + outbound port toward the AI subsystem.
//!
//! Moved from `crates/ai/src/alarm` in P4-Task19: `AlarmEvent` is the payload
//! carried by `AiEvent::AlarmCreated(..)` in crates/ai. The ai crate depends
//! on this crate for the payload type (ai → alarm, one-way); the alarm crate
//! never names ai types — `AlarmService` publishes through the
//! [`AlarmAiPublisher`] port and the composition layer (cloud) adapts
//! `tinyiothub_ai::event::bus::AiEventPublisher` to it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lightweight alarm event payload for cross-domain event dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

/// Outbound port: notify the AI subsystem that a significant alarm occurred.
///
/// Implemented in the composition layer by an adapter over
/// `tinyiothub_ai::event::bus::AiEventPublisher` (see
/// `cloud::shared::ai_adapter::AlarmAiPublisherAdapter`).
pub trait AlarmAiPublisher: Send + Sync {
    fn publish_alarm_created(&self, event: AlarmEvent);
}
