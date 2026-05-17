// SessionKey — unified parse + verify_workspace + to_string
#![allow(dead_code)]

use crate::shared::agent::config::AgentError;

pub struct SessionKey {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_uuid: String,
}

impl SessionKey {
    /// Parse "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub fn parse(key: &str) -> Result<Self, AgentError> {
        let parts: Vec<&str> = key.split('/').collect();
        if parts.len() != 2 {
            return Err(AgentError::RequestFailed(format!(
                "Invalid session key format (missing '/' separator): {key}"
            )));
        }
        let prefix_parts: Vec<&str> = parts[0].split(':').collect();
        if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
            return Err(AgentError::RequestFailed(format!(
                "Invalid session key prefix (expected 'agent:{{ws}}:{{agent}}'): {key}"
            )));
        }
        Ok(Self {
            workspace_id: prefix_parts[1].to_string(),
            agent_id: prefix_parts[2].to_string(),
            session_uuid: parts[1].to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        format!("agent:{}:{}/{}", self.workspace_id, self.agent_id, self.session_uuid)
    }

    pub fn verify_workspace(&self, expected: &str) -> Result<(), AgentError> {
        if self.workspace_id == expected {
            Ok(())
        } else {
            Err(AgentError::NotFound(format!(
                "Session does not belong to workspace '{}'",
                expected
            )))
        }
    }
}
