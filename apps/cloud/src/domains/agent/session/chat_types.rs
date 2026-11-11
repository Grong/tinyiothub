//! Chat types — errors and events emitted on the chat SSE stream.
//!
//! Session row/error types live in storage and are re-exported here for the
//! session index. The old agent-memory DTOs (`ChatMessage`, `CompactedSession`,
//! `ThingSnapshot`, `AgentMemoryItem`, `MemoryContext`, `MemoryError`) and the
//! deprecated `ParsedSessionKey`/`ChatRequest` pair had no consumers and were
//! removed in the legacy-cleanup batch.

pub use tinyiothub_storage::session::{Session, SessionError};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during chat operations
#[derive(Debug, Error)]
pub enum ChatError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Agent runtime error: {0}")]
    RuntimeError(String),

    #[error("Invalid session key format: {0}")]
    InvalidSessionKey(String),

    #[error("Memory service error: {0}")]
    MemoryError(String),

    #[error("Session repository error: {0}")]
    RepositoryError(String),

    #[error("Chat stream error: {0}")]
    StreamError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

impl From<tinyiothub_agent::AgentError> for ChatError {
    fn from(err: tinyiothub_agent::AgentError) -> Self {
        ChatError::RuntimeError(err.to_string())
    }
}

/// Events emitted during a chat stream
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ChatEvent {
    Delta {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        message: serde_json::Value,
    },
    Thinking {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        thinking: String,
    },
    ToolCallStart {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "toolArgs")]
        tool_args: String,
        a2ui: Option<String>,
    },
    ToolResult {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: String,
    },
    Final {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        message: serde_json::Value,
    },
    Retry {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        attempt: u32,
        #[serde(rename = "maxRetries")]
        max_retries: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        #[serde(rename = "errorMessage")]
        error: String,
    },
    Error {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        #[serde(rename = "errorMessage")]
        error: String,
        source: String,
    },
}

impl ChatEvent {
    pub fn run_id(&self) -> &str {
        match self {
            ChatEvent::Delta { run_id, .. } => run_id,
            ChatEvent::Thinking { run_id, .. } => run_id,
            ChatEvent::ToolCallStart { run_id, .. } => run_id,
            ChatEvent::ToolResult { run_id, .. } => run_id,
            ChatEvent::Final { run_id, .. } => run_id,
            ChatEvent::Retry { run_id, .. } | ChatEvent::Error { run_id, .. } => run_id,
        }
    }

    pub fn session_key(&self) -> &str {
        match self {
            ChatEvent::Delta { session_key, .. } => session_key,
            ChatEvent::Thinking { session_key, .. } => session_key,
            ChatEvent::ToolCallStart { session_key, .. } => session_key,
            ChatEvent::ToolResult { session_key, .. } => session_key,
            ChatEvent::Final { session_key, .. } => session_key,
            ChatEvent::Retry { session_key, .. } | ChatEvent::Error { session_key, .. } => session_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Session::new("agent:ws:agent/sess".to_string(), "ws".to_string(), "agent".to_string());
        assert_eq!(session.session_key, "agent:ws:agent/sess");
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent");
        assert!(session.label.is_none());
    }

    #[test]
    fn test_session_set_label() {
        let mut session = Session::new("agent:ws:agent/sess".to_string(), "ws".to_string(), "agent".to_string());
        let before = session.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.set_label("Test Session");
        assert_eq!(session.label, Some("Test Session".to_string()));
        assert!(session.updated_at > before);
    }

    #[test]
    fn test_session_error_display() {
        let err = SessionError::NotFound("sess-123".to_string());
        assert!(err.to_string().contains("sess-123"));
        let err = SessionError::AlreadyExists("sess-123".to_string());
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn test_chat_event_accessors() {
        let event = ChatEvent::Delta {
            run_id: "run-123".to_string(),
            session_key: "sess-456".to_string(),
            message: serde_json::json!({"text": "hello"}),
        };
        assert_eq!(event.run_id(), "run-123");
        assert_eq!(event.session_key(), "sess-456");
    }

    #[test]
    fn test_chat_error_from_agent_error() {
        let agent_err = tinyiothub_agent::AgentError::RequestFailed("test error".to_string());
        let chat_err: ChatError = agent_err.into();
        match chat_err {
            ChatError::RuntimeError(msg) => assert!(msg.contains("test error")),
            _ => panic!("Expected RuntimeError variant"),
        }
    }

    #[test]
    fn chat_error_uses_frontend_wire_fields() {
        let payload = serde_json::to_value(ChatEvent::Error {
            run_id: "run-1".into(),
            session_key: "agent:ws:a/s1".into(),
            error: "HTTP 503: unavailable".into(),
            source: "system".into(),
        })
        .expect("serialize error");
        assert_eq!(payload["state"], "error");
        assert_eq!(payload["errorMessage"], "HTTP 503: unavailable");
        assert_eq!(payload["source"], "system");
        assert!(payload.get("error").is_none());
    }
}
