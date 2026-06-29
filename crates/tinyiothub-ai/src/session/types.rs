//! Session types — chat messages and session key parsing.

use serde::{Deserialize, Serialize};

/// A single turn in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

/// Session key format: agent:<agentId>:<mainKey>
#[derive(Debug, Clone)]
pub struct SessionKey {
    pub agent_id: String,
    pub main_key: String,
}

impl SessionKey {
    pub fn parse(key: &str) -> Option<Self> {
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "agent" {
            return None;
        }
        Some(Self {
            agent_id: parts[1].to_string(),
            main_key: parts[2].to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        format!("agent:{}:{}", self.agent_id, self.main_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key_parse_valid() {
        let key = SessionKey::parse("agent:workspace_A:user_123/sess_uuid").unwrap();
        assert_eq!(key.agent_id, "workspace_A");
        assert_eq!(key.main_key, "user_123/sess_uuid");
    }

    #[test]
    fn test_session_key_parse_invalid_prefix() {
        assert!(SessionKey::parse("user:workspace_A:key").is_none());
    }

    #[test]
    fn test_session_key_parse_too_short() {
        assert!(SessionKey::parse("agent:workspace_A").is_none());
    }

    #[test]
    fn test_session_key_roundtrip() {
        let original = "agent:ws1:user_abc/sess_xyz";
        let key = SessionKey::parse(original).unwrap();
        assert_eq!(key.to_string(), original);
    }
}
