// Agent Configuration Types
//
// This module provides configuration types and utilities for agent functionality.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors from Agent operations
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent API request failed: {0}")]
    RequestFailed(String),
    #[error("Agent API returned error: {0}")]
    ApiError(String),
    #[error("Agent API timeout")]
    Timeout,
    #[error("Agent unavailable: {0}")]
    Unavailable(String),
    #[error("agent not found: {0}")]
    NotFound(String),
    #[error("agent build failed: {0}")]
    BuildError(String),
    #[error("agent stream error: {0}")]
    StreamError(String),
}

/// Agent configuration passed when creating an agent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl AgentConfig {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

/// Agent runtime configuration persisted in agent_configs table.
///
/// This is the persistent "blueprint" used to build a zeroclaw Agent.
/// Separate from `AgentConfig` (the create-request DTO).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeConfig {
    /// Model name (provider-specific, e.g. "minimax-m2")
    #[serde(default = "default_model")]
    pub model: String,
    /// Temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Max output tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Top-P sampling
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    /// System prompt — deprecated: use USER.md workspace file instead
    #[deprecated(note = "Use USER.md workspace file instead")]
    #[serde(default)]
    pub system_prompt: String,
    /// Preset persona id — deprecated: persona is now inferred from workspace context
    #[deprecated(note = "Persona is now inferred from workspace context")]
    #[serde(default)]
    pub persona_preset: String,
    /// Tool names disabled for this agent (denylist mode)
    #[serde(default = "default_tool_denylist")]
    pub tool_denylist: Vec<String>,
    /// Enable the reflection engine (post-turn memory/skill extraction).
    /// Safe to disable — only affects background processing, never core chat.
    #[serde(default = "default_enable_reflection")]
    pub enable_reflection: bool,
}

fn default_enable_reflection() -> bool {
    true
}

fn default_model() -> String {
    "minimax-m2".into()
}

fn default_temperature() -> f64 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_top_p() -> f64 {
    1.0
}

fn default_tool_denylist() -> Vec<String> {
    vec!["delete_device".into(), "delete_schedule".into()]
}

#[allow(deprecated)]
impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: default_top_p(),
            system_prompt: String::new(),
            persona_preset: String::new(),
            tool_denylist: default_tool_denylist(),
            enable_reflection: default_enable_reflection(),
        }
    }
}

/// Default agent config returned when no persisted config exists
pub fn default_agent_config() -> serde_json::Value {
    serde_json::to_value(AgentRuntimeConfig::default()).unwrap_or_else(|_| {
        serde_json::json!({
            "model": "minimax-m2",
            "temperature": 0.7,
            "max_tokens": 4096,
            "top_p": 1.0,
            "tool_denylist": ["delete_device", "delete_schedule"]
        })
    })
}

/// Compute SHA-256 hex digest of a string
pub fn compute_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    Digest::update(&mut hasher, s.as_bytes());
    hex::encode(hasher.finalize())
}

/// Agent info returned on creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig {
            workspace_id: "ws1".to_string(),
            name: "test".to_string(),
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            system_prompt: None,
        };
        assert_eq!(config.workspace_id, "ws1");
        assert_eq!(config.name, "test");
        assert!(config.model.is_none());
        assert!(config.temperature.is_none());
    }

    #[test]
    fn test_agent_info_creation() {
        let info = AgentInfo {
            id: "agent-1".to_string(),
            name: "Test Agent".to_string(),
            status: "active".to_string(),
            created_at: Some("2026-04-07T00:00:00Z".to_string()),
        };
        assert_eq!(info.id, "agent-1");
        assert_eq!(info.status, "active");
    }

    #[test]
    fn test_agent_error_display() {
        let err = AgentError::Unavailable("connection refused".to_string());
        assert!(err.to_string().contains("Agent unavailable"));
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_agent_error_not_found() {
        let err = AgentError::NotFound("missing-agent".to_string());
        assert!(err.to_string().contains("agent not found"));
        assert!(err.to_string().contains("missing-agent"));
    }

    #[test]
    fn test_agent_config_to_json_roundtrip() {
        let config = AgentConfig {
            workspace_id: "ws-test-001".to_string(),
            name: "TestAgent".to_string(),
            model: Some("claude-sonnet-4-5".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(1.0),
            system_prompt: Some("You are a helpful assistant.".to_string()),
        };

        let json_str = config.to_json().expect("should serialize");
        let parsed: AgentConfig = serde_json::from_str(&json_str).expect("should deserialize");

        assert_eq!(parsed.workspace_id, "ws-test-001");
        assert_eq!(parsed.name, "TestAgent");
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.max_tokens, Some(4096));
        assert_eq!(parsed.top_p, Some(1.0));
        assert_eq!(parsed.system_prompt.as_deref(), Some("You are a helpful assistant."));
    }

    #[test]
    fn test_agent_config_to_json_partial_fields() {
        let config = AgentConfig {
            workspace_id: "ws-x".to_string(),
            name: "MinimalAgent".to_string(),
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            system_prompt: None,
        };

        let json_str = config.to_json().expect("should serialize");
        let parsed: AgentConfig = serde_json::from_str(&json_str).expect("should deserialize");

        assert_eq!(parsed.workspace_id, "ws-x");
        assert!(parsed.model.is_none());
        assert!(parsed.system_prompt.is_none());
    }

    #[test]
    fn test_agent_config_default_values() {
        let config = AgentConfig::default();
        assert_eq!(config.workspace_id, "");
        assert_eq!(config.name, "");
        assert!(config.model.is_none());
        assert!(config.temperature.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.top_p.is_none());
        assert!(config.system_prompt.is_none());
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let input = r#"{"model":"claude-sonnet-4-5","temperature":0.7}"#;
        let hash1 = compute_hash(input);
        let hash2 = compute_hash(input);
        assert_eq!(hash1, hash2, "hash should be deterministic");
        assert_eq!(hash1.len(), 64, "SHA-256 produces 64 hex chars");
    }

    #[test]
    fn test_compute_hash_different_inputs_different_hashes() {
        let hash1 = compute_hash(r#"{"model":"claude-sonnet-4-5"}"#);
        let hash2 = compute_hash(r#"{"model":"claude-opus"}"#);
        assert_ne!(hash1, hash2, "different inputs should produce different hashes");
    }

    #[test]
    fn test_compute_hash_empty_string() {
        let hash = compute_hash("");
        assert_eq!(hash.len(), 64);
        // Known SHA-256 of empty string
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_default_agent_config_format() {
        let config = default_agent_config();
        let obj = config.as_object().expect("should be an object");

        assert_eq!(obj.get("model").and_then(|v| v.as_str()), Some("minimax-m2"));
        assert_eq!(obj.get("temperature").and_then(|v| v.as_f64()), Some(0.7));
        assert_eq!(obj.get("maxTokens").and_then(|v| v.as_u64()), Some(4096));
        assert_eq!(obj.get("topP").and_then(|v| v.as_f64()), Some(1.0));
        assert_eq!(obj.get("systemPrompt").and_then(|v| v.as_str()), Some(""));
        assert!(obj.get("toolDenylist").and_then(|v| v.as_array()).is_some());
    }

    #[test]
    fn test_agent_error_request_failed() {
        let err = AgentError::RequestFailed("connection reset".to_string());
        assert!(err.to_string().contains("Agent API request failed"));
        assert!(err.to_string().contains("connection reset"));
    }

    #[test]
    fn test_agent_error_api_error() {
        let err = AgentError::ApiError("invalid json".to_string());
        assert!(err.to_string().contains("Agent API returned error"));
        assert!(err.to_string().contains("invalid json"));
    }

    #[test]
    fn test_agent_error_timeout() {
        let err = AgentError::Timeout;
        assert!(err.to_string().contains("Agent API timeout"));
    }

    #[test]
    fn test_agent_error_stream_error() {
        let err = AgentError::StreamError("connection closed".to_string());
        assert!(err.to_string().contains("agent stream error"));
        assert!(err.to_string().contains("connection closed"));
    }
}
