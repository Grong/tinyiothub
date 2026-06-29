//! AI event types — shared event payloads for the AI subsystem.

use serde::{Deserialize, Serialize};

use crate::alarm::types::Alarm;
use crate::patrol::types::PatrolReport;
use crate::session::types::ChatTurnMessage;

/// AI subsystem domain events.
///
/// Published through the shared `tinyiothub_runtime::EventBus` as
/// `EventType::Ai(AiEventType::...)`. The payload variants carry
/// typed data that handlers downcast from the event content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiEvent {
    AlarmCreated(Alarm),
    AlarmResolved {
        alarm_id: String,
        device_id: String,
        rule_id: Option<String>,
    },
    PatrolCompleted {
        workspace_id: String,
        report: PatrolReport,
    },
    ChatCompleted {
        workspace_id: String,
        agent_id: String,
        session_key: String,
        model: String,
        messages: Vec<ChatTurnMessage>,
    },
    WorkspaceCreated {
        workspace_id: String,
    },
    WorkspaceDeleted {
        workspace_id: String,
    },
}

impl AiEvent {
    pub fn workspace_id(&self) -> Option<&str> {
        match self {
            AiEvent::AlarmCreated(a) => Some(&a.workspace_id),
            AiEvent::AlarmResolved { .. } => None,
            AiEvent::PatrolCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::ChatCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::WorkspaceCreated { workspace_id } => Some(workspace_id),
            AiEvent::WorkspaceDeleted { workspace_id } => Some(workspace_id),
        }
    }
}
