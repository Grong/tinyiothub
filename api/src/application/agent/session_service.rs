// Session Service - Session management for AI Agent chat
//
// This module provides:
// - SessionRepository trait for persistence abstraction
// - Session and ChatMessage domain types
// - SessionService for session lifecycle management

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur during session operations
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Invalid session data: {0}")]
    InvalidData(String),
}

/// A chat session representing a conversation between user and agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session key: "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub session_key: String,
    /// Associated workspace ID
    pub workspace_id: String,
    /// Associated agent ID
    pub agent_id: String,
    /// Optional session label/title
    pub label: Option<String>,
    /// Session creation timestamp (Unix millis)
    pub created_at: i64,
    /// Last update timestamp (Unix millis)
    pub updated_at: i64,
    /// Session metadata (arbitrary JSON)
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Session {
    /// Create a new session
    pub fn new(session_key: String, workspace_id: String, agent_id: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            session_key,
            workspace_id,
            agent_id,
            label: None,
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Update the label
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Update metadata
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        if let Some(obj) = self.metadata.as_object_mut() {
            obj.insert(key.into(), value);
        }
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Touch the session (update updated_at)
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// A single chat message within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role: "user", "assistant", "system", "tool"
    pub role: String,
    /// Message content
    pub content: String,
    /// Optional timestamp (Unix millis)
    pub timestamp: Option<i64>,
    /// Optional tool call ID for tool messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional tool name for tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Optional run ID this message belongs to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl ChatMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            tool_call_id: None,
            tool_name: None,
            run_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            tool_call_id: None,
            tool_name: None,
            run_id: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            tool_call_id: None,
            tool_name: None,
            run_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: None,
            run_id: None,
        }
    }

    /// Check if this is a user message
    pub fn is_user(&self) -> bool {
        self.role == "user"
    }

    /// Check if this is an assistant message
    pub fn is_assistant(&self) -> bool {
        self.role == "assistant"
    }

    /// Check if this is a system message
    pub fn is_system(&self) -> bool {
        self.role == "system"
    }

    /// Estimate token count (rough approximation)
    pub fn estimate_tokens(&self) -> usize {
        // Rough estimate: 4 chars per token + overhead
        self.content.len() / 4 + 20
    }
}

/// Compacted session data after conversation compression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedSession {
    /// Original session key
    pub session_key: String,
    /// System messages (preserved)
    pub system_messages: Vec<ChatMessage>,
    /// Summary message (the compaction result)
    pub summary_message: Option<ChatMessage>,
    /// Recent messages preserved after compaction
    pub recent_messages: Vec<ChatMessage>,
    /// Compaction timestamp
    pub compacted_at: i64,
    /// Original message count before compaction
    pub original_message_count: usize,
}

/// Repository trait for session persistence
///
/// This trait abstracts the storage layer, allowing different implementations
/// (SQLite, Redis, etc.) to be used interchangeably.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Get a session by key, returning None if not found
    async fn get(&self, session_key: &str) -> Result<Option<Session>, SessionError>;

    /// Create a new session
    async fn create(&self, session: &Session) -> Result<(), SessionError>;

    /// Update an existing session
    async fn update(&self, session: &Session) -> Result<(), SessionError>;

    /// Delete a session and all its messages
    async fn delete(&self, session_key: &str) -> Result<(), SessionError>;

    /// List sessions for a workspace/agent
    async fn list(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError>;

    /// Get or create a session (creates if not exists)
    async fn get_or_create(&self, session_key: &str) -> Result<Session, SessionError> {
        if let Some(session) = self.get(session_key).await? {
            return Ok(session);
        }

        // Parse session key to extract workspace and agent
        let parts: Vec<&str> = session_key.split('/').collect();
        if parts.len() != 2 {
            return Err(SessionError::InvalidData(
                format!("Invalid session key format: {}", session_key)
            ));
        }

        let prefix_parts: Vec<&str> = parts[0].split(':').collect();
        if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
            return Err(SessionError::InvalidData(
                format!("Invalid session key prefix: {}", session_key)
            ));
        }

        let workspace_id = prefix_parts[1].to_string();
        let agent_id = prefix_parts[2].to_string();

        let session = Session::new(
            session_key.to_string(),
            workspace_id,
            agent_id,
        );

        self.create(&session).await?;
        Ok(session)
    }

    /// Add a message to a session
    async fn add_message(&self, session_key: &str, message: ChatMessage) -> Result<(), SessionError>;

    /// Get messages for a session
    async fn get_messages(
        &self,
        session_key: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ChatMessage>, SessionError>;

    /// Get message count for a session
    async fn get_message_count(&self, session_key: &str) -> Result<usize, SessionError>;

    /// Delete messages older than a certain timestamp
    async fn delete_messages_before(
        &self,
        session_key: &str,
        timestamp: i64,
    ) -> Result<usize, SessionError>;

    /// Save compacted session data
    async fn save_compacted(&self, compacted: &CompactedSession) -> Result<(), SessionError>;

    /// Get compacted session data
    async fn get_compacted(&self, session_key: &str) -> Result<Option<CompactedSession>, SessionError>;
}

/// Session service for managing session lifecycle
pub struct SessionService {
    repo: Arc<dyn SessionRepository>,
}

impl SessionService {
    /// Create a new session service
    pub fn new(repo: Arc<dyn SessionRepository>) -> Self {
        Self { repo }
    }

    /// Get a session by key
    pub async fn get_session(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        self.repo.get(session_key).await
    }

    /// Create a new session
    pub async fn create_session(
        &self,
        session_key: String,
        workspace_id: String,
        agent_id: String,
    ) -> Result<Session, SessionError> {
        let session = Session::new(session_key, workspace_id, agent_id);
        self.repo.create(&session).await?;
        Ok(session)
    }

    /// Update session label
    pub async fn update_label(
        &self,
        session_key: &str,
        label: impl Into<String>,
    ) -> Result<Session, SessionError> {
        let mut session = self
            .repo
            .get(session_key)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_key.to_string()))?;

        session.set_label(label);
        self.repo.update(&session).await?;
        Ok(session)
    }

    /// Delete a session
    pub async fn delete_session(&self, session_key: &str) -> Result<(), SessionError> {
        self.repo.delete(session_key).await
    }

    /// List sessions with optional filtering
    pub async fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        self.repo.list(workspace_id, agent_id, limit, offset).await
    }

    /// Add a message to a session
    pub async fn add_message(
        &self,
        session_key: &str,
        message: ChatMessage,
    ) -> Result<(), SessionError> {
        // Touch the session to update updated_at
        if let Some(mut session) = self.repo.get(session_key).await? {
            session.touch();
            self.repo.update(&session).await?;
        }

        self.repo.add_message(session_key, message).await
    }

    /// Get messages for a session
    pub async fn get_messages(
        &self,
        session_key: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ChatMessage>, SessionError> {
        self.repo.get_messages(session_key, limit, offset).await
    }

    /// Get message count
    pub async fn get_message_count(&self, session_key: &str) -> Result<usize, SessionError> {
        self.repo.get_message_count(session_key).await
    }

    /// Compact a session's conversation history
    pub async fn compact_session(
        &self,
        session_key: &str,
        summary: impl Into<String>,
    ) -> Result<CompactedSession, SessionError> {
        use crate::domain::agent::compact_service::CompactService;

        let messages = self.repo.get_messages(session_key, usize::MAX, 0).await?;

        // Convert to domain ChatMessage for compaction
        let domain_messages: Vec<crate::domain::agent::compact_service::ChatMessage> = messages
            .iter()
            .map(|m| crate::domain::agent::compact_service::ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                timestamp: m.timestamp,
            })
            .collect();

        let compacted = CompactService::compact(&domain_messages, &summary.into());
        let original_count = messages.len();

        // Convert back to application ChatMessage
        let compacted_session = CompactedSession {
            session_key: session_key.to_string(),
            system_messages: compacted
                .system_messages
                .into_iter()
                .map(|m| ChatMessage {
                    role: m.role,
                    content: m.content,
                    timestamp: m.timestamp,
                    tool_call_id: None,
                    tool_name: None,
                    run_id: None,
                })
                .collect(),
            summary_message: compacted.summary_message.map(|m| ChatMessage {
                role: m.role,
                content: m.content,
                timestamp: m.timestamp,
                tool_call_id: None,
                tool_name: None,
                run_id: None,
            }),
            recent_messages: compacted
                .recent_messages
                .into_iter()
                .map(|m| ChatMessage {
                    role: m.role,
                    content: m.content,
                    timestamp: m.timestamp,
                    tool_call_id: None,
                    tool_name: None,
                    run_id: None,
                })
                .collect(),
            compacted_at: chrono::Utc::now().timestamp_millis(),
            original_message_count: original_count,
        };

        self.repo.save_compacted(&compacted_session).await?;
        Ok(compacted_session)
    }

    /// Get compacted session data
    pub async fn get_compacted(
        &self,
        session_key: &str,
    ) -> Result<Option<CompactedSession>, SessionError> {
        self.repo.get_compacted(session_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Session::new(
            "agent:ws:agent/sess".to_string(),
            "ws".to_string(),
            "agent".to_string(),
        );

        assert_eq!(session.session_key, "agent:ws:agent/sess");
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent");
        assert!(session.label.is_none());
    }

    #[test]
    fn test_session_set_label() {
        let mut session = Session::new(
            "agent:ws:agent/sess".to_string(),
            "ws".to_string(),
            "agent".to_string(),
        );

        let before = session.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        session.set_label("Test Session");

        assert_eq!(session.label, Some("Test Session".to_string()));
        assert!(session.updated_at > before);
    }

    #[test]
    fn test_chat_message_helpers() {
        let user_msg = ChatMessage::user("Hello");
        assert_eq!(user_msg.role, "user");
        assert!(user_msg.is_user());
        assert!(!user_msg.is_assistant());

        let assistant_msg = ChatMessage::assistant("Hi there");
        assert_eq!(assistant_msg.role, "assistant");
        assert!(assistant_msg.is_assistant());

        let system_msg = ChatMessage::system("You are helpful");
        assert_eq!(system_msg.role, "system");
        assert!(system_msg.is_system());

        let tool_msg = ChatMessage::tool_result("call-123", "Result data");
        assert_eq!(tool_msg.role, "tool");
        assert_eq!(tool_msg.tool_call_id, Some("call-123".to_string()));
    }

    #[test]
    fn test_chat_message_estimate_tokens() {
        let msg = ChatMessage::user("Hello world, this is a test message.");
        let tokens = msg.estimate_tokens();
        assert!(tokens > 20); // At least the overhead
    }

    #[test]
    fn test_compacted_session_creation() {
        let compacted = CompactedSession {
            session_key: "agent:ws:agent/sess".to_string(),
            system_messages: vec![ChatMessage::system("You are helpful")],
            summary_message: Some(ChatMessage::assistant("Summary of conversation")),
            recent_messages: vec![ChatMessage::user("Recent message")],
            compacted_at: chrono::Utc::now().timestamp_millis(),
            original_message_count: 100,
        };

        assert_eq!(compacted.system_messages.len(), 1);
        assert!(compacted.summary_message.is_some());
        assert_eq!(compacted.recent_messages.len(), 1);
        assert_eq!(compacted.original_message_count, 100);
    }

    #[test]
    fn test_session_error_display() {
        let err = SessionError::NotFound("sess-123".to_string());
        assert!(err.to_string().contains("sess-123"));

        let err = SessionError::AlreadyExists("sess-123".to_string());
        assert!(err.to_string().contains("already exists"));
    }
}
