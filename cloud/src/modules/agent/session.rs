// SessionKey — unified parse + verify_workspace + to_string
#![allow(dead_code)]

use crate::shared::agent::config::AgentError;

#[derive(Debug)]
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

    pub fn session_key(&self) -> String {
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

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "agent:{}:{}/{}", self.workspace_id, self.agent_id, self.session_uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let key = SessionKey::parse("agent:ws-123:agent-456/sess-789").unwrap();
        assert_eq!(key.workspace_id, "ws-123");
        assert_eq!(key.agent_id, "agent-456");
        assert_eq!(key.session_uuid, "sess-789");
    }

    #[test]
    fn test_parse_default_workspace() {
        let key = SessionKey::parse("agent:default:agent_main/session_x").unwrap();
        assert_eq!(key.workspace_id, "default");
    }

    #[test]
    fn test_parse_missing_separator() {
        let err = SessionKey::parse("agent:ws:agent").unwrap_err();
        assert!(err.to_string().contains("missing '/' separator"));
    }

    #[test]
    fn test_parse_invalid_prefix() {
        let err = SessionKey::parse("chat:ws:agent/sess").unwrap_err();
        assert!(err.to_string().contains("expected 'agent:"));
    }

    #[test]
    fn test_to_string_roundtrip() {
        let key = SessionKey {
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            session_uuid: "uuid".to_string(),
        };
        let s = key.to_string();
        assert_eq!(s, "agent:ws:agent/uuid");
        let parsed = SessionKey::parse(&s).unwrap();
        assert_eq!(parsed.workspace_id, "ws");
        assert_eq!(parsed.agent_id, "agent");
        assert_eq!(parsed.session_uuid, "uuid");
    }

    #[test]
    fn test_verify_workspace_match() {
        let key = SessionKey {
            workspace_id: "ws1".to_string(),
            agent_id: "a".to_string(),
            session_uuid: "s".to_string(),
        };
        assert!(key.verify_workspace("ws1").is_ok());
    }

    #[test]
    fn test_verify_workspace_mismatch() {
        let key = SessionKey {
            workspace_id: "ws1".to_string(),
            agent_id: "a".to_string(),
            session_uuid: "s".to_string(),
        };
        assert!(key.verify_workspace("ws2").is_err());
    }
}
