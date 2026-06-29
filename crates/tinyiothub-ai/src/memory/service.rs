//! MemoryService — long-term memory for agents.

use crate::session::types::ChatTurnMessage;

use super::types::MemoryError;

/// Service for extracting long-term memory from conversations
/// and compiling user/workspace profiles.
pub struct MemoryService;

impl MemoryService {
    pub fn new() -> Self {
        Self
    }

    /// Reflect on a completed conversation turn. Called by Orchestrator
    /// in response to ChatCompleted events.
    pub async fn reflect_conversation_turn(
        &self,
        _workspace_id: &str,
        _agent_id: &str,
        _session_key: &str,
        _model: &str,
        messages: &[ChatTurnMessage],
    ) -> Result<(), MemoryError> {
        if messages.is_empty() {
            return Ok(());
        }
        super::reflect::reflect(messages).await
    }
}

impl Default for MemoryService {
    fn default() -> Self {
        Self::new()
    }
}
