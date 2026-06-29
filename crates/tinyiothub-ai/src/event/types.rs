//! AI event types — shared event payloads for the AI subsystem.
//! Will be populated in Task 2.

use serde::{Deserialize, Serialize};

/// AI event payload carried via the EventBus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEvent {
    pub event_type: String,
    pub payload: String,
}
