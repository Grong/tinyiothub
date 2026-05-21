// Agent types — domain types and DTOs

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Re-export sub-domain types
pub use super::device_memory::DeviceMemory;
pub use super::skill::{AgentSkill, SkillType};

// --- Session types ---

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

// --- Chat types ---

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

impl From<crate::shared::agent::AgentError> for ChatError {
    fn from(err: crate::shared::agent::AgentError) -> Self {
        ChatError::RuntimeError(err.to_string())
    }
}

/// Request to initiate a chat
#[derive(Debug, Clone)]
pub struct ChatRequest {
    /// Session key format: "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub session_key: String,
    /// User message content
    pub message: String,
    /// Unique run identifier for this chat turn
    pub run_id: String,
    /// Optional system prompt override
    pub system_prompt_override: Option<String>,
}

impl ChatRequest {
    /// Parse the session key to extract workspace_id, agent_id, and session_uuid
    pub fn parse_session_key(&self) -> Result<ParsedSessionKey, ChatError> {
        ParsedSessionKey::parse_str(&self.session_key)
    }
}

/// DEPRECATED: Use `crate::modules::agent::session::SessionKey` instead.
/// Will be removed in T9.
///
/// Parsed components of a session key
#[derive(Debug, Clone)]
pub struct ParsedSessionKey {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_uuid: String,
}

impl ParsedSessionKey {
    /// Parse session key in format: "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub fn parse_str(key: &str) -> Result<Self, ChatError> {
        let parts: Vec<&str> = key.split('/').collect();
        if parts.len() != 2 {
            return Err(ChatError::InvalidSessionKey(format!(
                "Session key must contain '/' separator: {}",
                key
            )));
        }

        let prefix_parts: Vec<&str> = parts[0].split(':').collect();
        if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
            return Err(ChatError::InvalidSessionKey(format!(
                "Session key prefix must be 'agent:{{workspace}}:{{agent}}': {}",
                key
            )));
        }

        Ok(Self {
            workspace_id: prefix_parts[1].to_string(),
            agent_id: prefix_parts[2].to_string(),
            session_uuid: parts[1].to_string(),
        })
    }

    /// Reconstruct the full session key
    pub fn to_session_key(&self) -> String {
        format!("agent:{}:{}/{}", self.workspace_id, self.agent_id, self.session_uuid)
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
    Error {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        error: String,
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
            ChatEvent::Error { run_id, .. } => run_id,
        }
    }

    pub fn session_key(&self) -> &str {
        match self {
            ChatEvent::Delta { session_key, .. } => session_key,
            ChatEvent::Thinking { session_key, .. } => session_key,
            ChatEvent::ToolCallStart { session_key, .. } => session_key,
            ChatEvent::ToolResult { session_key, .. } => session_key,
            ChatEvent::Final { session_key, .. } => session_key,
            ChatEvent::Error { session_key, .. } => session_key,
        }
    }
}

// ChatServiceConfig and ChatStream moved to chat_service.rs

// --- Memory types ---

/// Errors that can occur during memory operations
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Context build failed: {0}")]
    ContextBuildFailed(String),
}

/// A snapshot of device state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub device_id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub snapshot_data: serde_json::Value,
    pub snapshot_time: i64,
    pub timestamp_formatted: String,
}

impl DeviceSnapshot {
    /// Create a new device snapshot from domain DeviceMemory
    pub fn from_domain(memory: &DeviceMemory) -> Result<Self, MemoryError> {
        let snapshot_data = memory.parse_snapshot().ok_or_else(|| {
            MemoryError::SerializationError(format!(
                "Failed to parse snapshot for device {}",
                memory.device_id
            ))
        })?;

        let timestamp_formatted = chrono::DateTime::from_timestamp_millis(memory.snapshot_time)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        Ok(Self {
            device_id: memory.device_id.clone(),
            workspace_id: memory.workspace_id.clone(),
            agent_id: memory.agent_id.clone(),
            snapshot_data,
            snapshot_time: memory.snapshot_time,
            timestamp_formatted,
        })
    }

    /// Format the snapshot for inclusion in a prompt
    pub fn to_prompt_fragment(&self) -> String {
        format!(
            "[{}] Device {}: {}",
            self.timestamp_formatted,
            self.device_id,
            serde_json::to_string(&self.snapshot_data).unwrap_or_default()
        )
    }

    pub fn get_field(&self, field: &str) -> Option<&serde_json::Value> {
        self.snapshot_data.get(field)
    }

    pub fn has_property(&self, property: &str) -> bool {
        self.snapshot_data.get(property).is_some()
    }
}

/// An individual memory item for the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryItem {
    pub item_type: String,
    pub key: String,
    pub value: serde_json::Value,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<f32>,
}

impl AgentMemoryItem {
    pub fn device_snapshot(snapshot: &DeviceSnapshot) -> Self {
        Self {
            item_type: "device_snapshot".to_string(),
            key: snapshot.device_id.clone(),
            value: serde_json::json!({
                "workspace_id": snapshot.workspace_id,
                "agent_id": snapshot.agent_id,
                "snapshot": snapshot.snapshot_data,
                "timestamp": snapshot.snapshot_time,
            }),
            timestamp: snapshot.snapshot_time,
            relevance: None,
        }
    }

    pub fn user_preference(key: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            item_type: "user_preference".to_string(),
            key: key.into(),
            value,
            timestamp: chrono::Utc::now().timestamp_millis(),
            relevance: Some(1.0),
        }
    }

    pub fn conversation_summary(summary: impl Into<String>, topics: Vec<String>) -> Self {
        Self {
            item_type: "conversation_summary".to_string(),
            key: "latest_summary".to_string(),
            value: serde_json::json!({
                "summary": summary.into(),
                "topics": topics,
            }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            relevance: Some(0.8),
        }
    }
}

/// Complete memory context for an agent session
#[derive(Debug, Clone, Default)]
pub struct MemoryContext {
    pub device_snapshots: Vec<DeviceSnapshot>,
    pub user_preferences: Vec<AgentMemoryItem>,
    pub conversation_summaries: Vec<AgentMemoryItem>,
    pub other_items: Vec<AgentMemoryItem>,
}

impl MemoryContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_device_snapshot(&mut self, snapshot: DeviceSnapshot) {
        self.device_snapshots.push(snapshot);
    }

    pub fn add_item(&mut self, item: AgentMemoryItem) {
        match item.item_type.as_str() {
            "device_snapshot" => self.device_snapshots.push(
                DeviceSnapshot::from_domain(&DeviceMemory {
                    id: None,
                    workspace_id: item
                        .value
                        .get("workspace_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    agent_id: item
                        .value
                        .get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    device_id: item.key.clone(),
                    snapshot_data: item
                        .value
                        .get("snapshot")
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    snapshot_time: item.timestamp,
                    created_at: None,
                })
                .unwrap_or_else(|_| DeviceSnapshot {
                    device_id: item.key.clone(),
                    workspace_id: String::new(),
                    agent_id: String::new(),
                    snapshot_data: item.value.clone(),
                    snapshot_time: item.timestamp,
                    timestamp_formatted: String::new(),
                }),
            ),
            "user_preference" => self.user_preferences.push(item),
            "conversation_summary" => self.conversation_summaries.push(item),
            _ => self.other_items.push(item),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.device_snapshots.is_empty()
            && self.user_preferences.is_empty()
            && self.conversation_summaries.is_empty()
            && self.other_items.is_empty()
    }

    pub fn total_items(&self) -> usize {
        self.device_snapshots.len()
            + self.user_preferences.len()
            + self.conversation_summaries.len()
            + self.other_items.len()
    }

    pub fn to_prompt_fragment(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut fragments = vec!["\n\n## Context from Memory\n".to_string()];

        if !self.device_snapshots.is_empty() {
            fragments.push("### Device States\n".to_string());
            for snapshot in &self.device_snapshots {
                fragments.push(snapshot.to_prompt_fragment());
                fragments.push("\n".to_string());
            }
        }

        if !self.user_preferences.is_empty() {
            fragments.push("### User Preferences\n".to_string());
            for pref in &self.user_preferences {
                fragments.push(format!("- {}: {}\n", pref.key, pref.value));
            }
        }

        if !self.conversation_summaries.is_empty() {
            fragments.push("### Previous Conversations\n".to_string());
            for summary in &self.conversation_summaries {
                if let Some(summary_text) = summary.value.get("summary").and_then(|v| v.as_str()) {
                    fragments.push(format!("- {}\n", summary_text));
                }
            }
        }

        fragments.concat()
    }

    pub fn get_device_snapshots(&self, device_id: &str) -> Vec<&DeviceSnapshot> {
        self.device_snapshots.iter().filter(|s| s.device_id == device_id).collect()
    }

    pub fn get_latest_device_snapshot(&self, device_id: &str) -> Option<&DeviceSnapshot> {
        self.device_snapshots
            .iter()
            .filter(|s| s.device_id == device_id)
            .max_by_key(|s| s.snapshot_time)
    }
}

// --- Plugin context ---

/// Application context shared by plugins
pub struct AppContext {
    pub device_cache: Arc<tinyiothub_storage::cache::DeviceCache>,
}

// --- Session repository trait ---

/// Repository trait for session persistence
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn get(&self, session_key: &str) -> Result<Option<Session>, SessionError>;
    async fn create(&self, session: &Session) -> Result<(), SessionError>;
    async fn update(&self, session: &Session) -> Result<(), SessionError>;
    async fn delete(&self, session_key: &str) -> Result<(), SessionError>;
    async fn list(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError>;

    async fn get_or_create(&self, session_key: &str) -> Result<Session, SessionError> {
        if let Some(session) = self.get(session_key).await? {
            return Ok(session);
        }

        let parts: Vec<&str> = session_key.split('/').collect();
        if parts.len() != 2 {
            return Err(SessionError::InvalidData(format!(
                "Invalid session key format: {}",
                session_key
            )));
        }

        let prefix_parts: Vec<&str> = parts[0].split(':').collect();
        if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
            return Err(SessionError::InvalidData(format!(
                "Invalid session key prefix: {}",
                session_key
            )));
        }

        let workspace_id = prefix_parts[1].to_string();
        let agent_id = prefix_parts[2].to_string();
        let session = Session::new(session_key.to_string(), workspace_id, agent_id);

        match self.create(&session).await {
            Ok(()) => Ok(session),
            Err(SessionError::RepositoryError(ref e)) if e.contains("UNIQUE") => self
                .get(session_key)
                .await?
                .ok_or_else(|| SessionError::NotFound(session_key.to_string())),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session =
            Session::new("agent:ws:agent/sess".to_string(), "ws".to_string(), "agent".to_string());
        assert_eq!(session.session_key, "agent:ws:agent/sess");
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent");
        assert!(session.label.is_none());
    }

    #[test]
    fn test_session_set_label() {
        let mut session =
            Session::new("agent:ws:agent/sess".to_string(), "ws".to_string(), "agent".to_string());
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
        assert!(tokens > 20);
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

    #[test]
    fn test_parse_session_key_valid() {
        let key = ParsedSessionKey::parse_str("agent:ws-123:agent-456/sess-789").unwrap();
        assert_eq!(key.workspace_id, "ws-123");
        assert_eq!(key.agent_id, "agent-456");
        assert_eq!(key.session_uuid, "sess-789");
    }

    #[test]
    fn test_parse_session_key_invalid_format() {
        let result = ParsedSessionKey::parse_str("invalid-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_session_key_missing_separator() {
        let result = ParsedSessionKey::parse_str("agent:ws-123:agent-456");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_key_roundtrip() {
        let original = ParsedSessionKey {
            workspace_id: "ws-test".to_string(),
            agent_id: "agent-test".to_string(),
            session_uuid: "sess-uuid".to_string(),
        };
        let key = original.to_session_key();
        let parsed = ParsedSessionKey::parse_str(&key).unwrap();
        assert_eq!(original.workspace_id, parsed.workspace_id);
        assert_eq!(original.agent_id, parsed.agent_id);
        assert_eq!(original.session_uuid, parsed.session_uuid);
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
        let agent_err = crate::shared::agent::AgentError::RequestFailed("test error".to_string());
        let chat_err: ChatError = agent_err.into();
        match chat_err {
            ChatError::RuntimeError(msg) => assert!(msg.contains("test error")),
            _ => panic!("Expected RuntimeError variant"),
        }
    }

    #[test]
    fn test_device_snapshot_from_domain() {
        let memory = DeviceMemory::new(
            "ws-123".to_string(),
            "agent-456".to_string(),
            "device-789".to_string(),
            serde_json::json!({"temperature": 25.5, "status": "online"}),
        );
        let snapshot = DeviceSnapshot::from_domain(&memory).unwrap();
        assert_eq!(snapshot.device_id, "device-789");
        assert_eq!(snapshot.workspace_id, "ws-123");
        assert_eq!(snapshot.snapshot_data.get("temperature").unwrap().as_f64().unwrap(), 25.5);
    }

    #[test]
    fn test_device_snapshot_to_prompt_fragment() {
        let snapshot = DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1234567890000,
            timestamp_formatted: "2009-02-13 23:31:30".to_string(),
        };
        let fragment = snapshot.to_prompt_fragment();
        assert!(fragment.contains("dev-1"));
        assert!(fragment.contains("2009-02-13"));
    }

    #[test]
    fn test_memory_context_empty() {
        let context = MemoryContext::new();
        assert!(context.is_empty());
        assert_eq!(context.total_items(), 0);
    }

    #[test]
    fn test_memory_context_add_snapshot() {
        let mut context = MemoryContext::new();
        context.add_device_snapshot(DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1234567890000,
            timestamp_formatted: "2009-02-13 23:31:30".to_string(),
        });
        assert!(!context.is_empty());
        assert_eq!(context.total_items(), 1);
    }

    #[test]
    fn test_memory_context_get_device_snapshots() {
        let mut context = MemoryContext::new();
        context.add_device_snapshot(DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1000,
            timestamp_formatted: "time-1".to_string(),
        });
        context.add_device_snapshot(DeviceSnapshot {
            device_id: "dev-2".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 30}),
            snapshot_time: 2000,
            timestamp_formatted: "time-2".to_string(),
        });
        let dev1_snapshots = context.get_device_snapshots("dev-1");
        assert_eq!(dev1_snapshots.len(), 1);
        assert_eq!(dev1_snapshots[0].device_id, "dev-1");
    }

    #[test]
    fn test_agent_memory_item_helpers() {
        let snapshot_item = AgentMemoryItem::device_snapshot(&DeviceSnapshot {
            device_id: "dev-1".to_string(),
            workspace_id: "ws".to_string(),
            agent_id: "agent".to_string(),
            snapshot_data: serde_json::json!({"temp": 25}),
            snapshot_time: 1000,
            timestamp_formatted: "time".to_string(),
        });
        assert_eq!(snapshot_item.item_type, "device_snapshot");
        assert_eq!(snapshot_item.key, "dev-1");

        let pref_item = AgentMemoryItem::user_preference("theme", serde_json::json!("dark"));
        assert_eq!(pref_item.item_type, "user_preference");

        let summary_item = AgentMemoryItem::conversation_summary(
            "Talked about devices",
            vec!["devices".to_string()],
        );
        assert_eq!(summary_item.item_type, "conversation_summary");
    }

    #[test]
    fn test_memory_error_display() {
        let err = MemoryError::DeviceNotFound("dev-123".to_string());
        assert!(err.to_string().contains("dev-123"));
    }
}
