//! AI event types — shared event payloads for the AI subsystem.

use serde::{Deserialize, Serialize};

use crate::alarm::types::AlarmEvent;
use crate::heartbeat::types::HeartbeatResult;
use crate::session::types::ChatTurnMessage;

/// AI subsystem domain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiEvent {
    AlarmCreated(AlarmEvent),
    AlarmResolved {
        alarm_id: String,
        device_id: String,
        rule_id: Option<String>,
    },
    HeartbeatCompleted {
        workspace_id: String,
        result: HeartbeatResult,
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
    HeartbeatPersistFailed {
        workspace_id: String,
        reason: String,
    },
    ReflectionFailed {
        workspace_id: String,
        agent_id: String,
        session_key: String,
        reason: String,
    },
    ProposalCreated {
        workspace_id: String,
        proposal_id: String,
        tool_name: String,
    },
    ProposalResolved {
        workspace_id: String,
        proposal_id: String,
        approved: bool,
    },
}

impl AiEvent {
    pub fn workspace_id(&self) -> Option<&str> {
        match self {
            AiEvent::AlarmCreated(a) => Some(&a.workspace_id),
            AiEvent::AlarmResolved { .. } => None,
            AiEvent::HeartbeatCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::ChatCompleted { workspace_id, .. } => Some(workspace_id),
            AiEvent::WorkspaceCreated { workspace_id } => Some(workspace_id),
            AiEvent::WorkspaceDeleted { workspace_id } => Some(workspace_id),
            AiEvent::HeartbeatPersistFailed { workspace_id, .. } => Some(workspace_id),
            AiEvent::ReflectionFailed { workspace_id, .. } => Some(workspace_id),
            AiEvent::ProposalCreated { workspace_id, .. } => Some(workspace_id),
            AiEvent::ProposalResolved { workspace_id, .. } => Some(workspace_id),
        }
    }
}
