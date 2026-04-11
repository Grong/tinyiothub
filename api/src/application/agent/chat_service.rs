// Chat Service - Core orchestration service for AI Agent chat
//
// This module provides the main ChatService which orchestrates:
// - Session management
// - Memory context building
// - Chat streaming with the AgentRuntime
// - Conversation compaction

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::application::agent::memory_service::{AgentMemoryService, MemoryContext};
use crate::application::agent::session_service::{SessionRepository, CompactedSession};
use crate::domain::agent::compact_service::CompactService;
use crate::infrastructure::agent::{AgentRuntime, AgentError, AgentConfig};

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

impl From<AgentError> for ChatError {
    fn from(err: AgentError) -> Self {
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
        ParsedSessionKey::from_str(&self.session_key)
    }
}

/// Parsed components of a session key
#[derive(Debug, Clone)]
pub struct ParsedSessionKey {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_uuid: String,
}

impl ParsedSessionKey {
    /// Parse session key in format: "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub fn from_str(key: &str) -> Result<Self, ChatError> {
        // Expected format: agent:{workspace_id}:{agent_id}/{session_uuid}
        let parts: Vec<&str> = key.split('/').collect();
        if parts.len() != 2 {
            return Err(ChatError::InvalidSessionKey(
                format!("Session key must contain '/' separator: {}", key)
            ));
        }

        let prefix_parts: Vec<&str> = parts[0].split(':').collect();
        if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
            return Err(ChatError::InvalidSessionKey(
                format!("Session key prefix must be 'agent:{{workspace}}:{{agent}}': {}", key)
            ));
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
    /// Delta message chunk from the assistant
    Delta {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        message: Value,
    },
    /// Thinking/reasoning content
    Thinking {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        thinking: String,
    },
    /// Tool call started
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
    /// Tool execution result
    ToolResult {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: String,
    },
    /// Final complete message
    Final {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        message: Value,
    },
    /// Error during chat
    Error {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "sessionKey")]
        session_key: String,
        error: String,
    },
}

impl ChatEvent {
    /// Get the run_id from any event variant
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

    /// Get the session_key from any event variant
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

/// Configuration for the ChatService
#[derive(Debug, Clone)]
pub struct ChatServiceConfig {
    /// Default system prompt to use if none provided
    pub default_system_prompt: String,
    /// Maximum messages before compaction
    pub max_messages_before_compact: usize,
    /// Enable conversation compaction
    pub enable_compaction: bool,
}

impl Default for ChatServiceConfig {
    fn default() -> Self {
        Self {
            default_system_prompt: String::from("You are a helpful IoT assistant."),
            max_messages_before_compact: 50,
            enable_compaction: true,
        }
    }
}

/// Chat stream that yields ChatEvent items
pub struct ChatStream {
    receiver: mpsc::UnboundedReceiver<ChatEvent>,
}

impl ChatStream {
    /// Create a new chat stream from a receiver
    pub fn new(receiver: mpsc::UnboundedReceiver<ChatEvent>) -> Self {
        Self { receiver }
    }
}

impl Stream for ChatStream {
    type Item = ChatEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Core chat orchestration service
pub struct ChatService {
    runtime: Arc<dyn AgentRuntime>,
    session_repo: Arc<dyn SessionRepository>,
    memory_service: Arc<AgentMemoryService>,
    compact_service: Arc<CompactService>,
    config: ChatServiceConfig,
}

impl ChatService {
    /// Create a new ChatService with all dependencies
    pub fn new(
        runtime: Arc<dyn AgentRuntime>,
        session_repo: Arc<dyn SessionRepository>,
        memory_service: Arc<AgentMemoryService>,
        compact_service: Arc<CompactService>,
        config: ChatServiceConfig,
    ) -> Self {
        Self {
            runtime,
            session_repo,
            memory_service,
            compact_service,
            config,
        }
    }

    /// Execute a chat request and return a stream of events
    ///
    /// This is the main entry point for chat functionality. It:
    /// 1. Validates and parses the session key
    /// 2. Loads or creates the session
    /// 3. Builds memory context
    /// 4. Checks if compaction is needed
    /// 5. Initiates the chat with the runtime
    /// 6. Returns a stream of ChatEvent items
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatStream, ChatError> {
        // Parse session key to extract workspace/agent info
        let parsed_key = request.parse_session_key()?;

        // Load or create session
        let session = self
            .session_repo
            .get_or_create(&request.session_key)
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;

        // Build memory context for this workspace/agent
        let memory_context = self
            .memory_service
            .build_context(&parsed_key.workspace_id, &parsed_key.agent_id)
            .await
            .map_err(|e| ChatError::MemoryError(e.to_string()))?;

        // Check if conversation needs compaction
        let should_compact = if self.config.enable_compaction {
            self.check_compaction_needed(&session).await?
        } else {
            false
        };

        // Build system prompt
        let system_prompt = self.build_system_prompt(
            request.system_prompt_override.as_deref(),
            &memory_context,
        );

        // Create channel for streaming events
        let (tx, rx) = mpsc::unbounded_channel::<ChatEvent>();

        // Spawn the chat processing
        self.spawn_chat_task(
            request,
            parsed_key,
            system_prompt,
            should_compact,
            tx,
        ).await?;

        Ok(ChatStream::new(rx))
    }

    /// Abort an ongoing chat run
    pub async fn abort_chat(
        &self,
        session_key: &str,
        run_id: Option<&str>,
    ) -> Result<(), ChatError> {
        let parsed = ParsedSessionKey::from_str(session_key)?;

        self.runtime
            .chat_abort(&parsed.agent_id, session_key, run_id)
            .await
            .map_err(|e| ChatError::RuntimeError(e.to_string()))?;

        Ok(())
    }

    /// Get chat history for a session
    pub async fn get_history(
        &self,
        session_key: &str,
        limit: u32,
    ) -> Result<Value, ChatError> {
        let parsed = ParsedSessionKey::from_str(session_key)?;

        let history = self
            .runtime
            .chat_history(&parsed.agent_id, session_key, limit)
            .await?;

        Ok(history)
    }

    /// Check if the session conversation needs compaction
    async fn check_compaction_needed(&self, session: &crate::application::agent::session_service::Session) -> Result<bool, ChatError> {
        let message_count = self
            .session_repo
            .get_message_count(&session.session_key)
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;

        Ok(message_count > self.config.max_messages_before_compact)
    }

    /// Build the complete system prompt
    fn build_system_prompt(
        &self,
        override_prompt: Option<&str>,
        memory_context: &MemoryContext,
    ) -> String {
        let base_prompt = override_prompt
            .unwrap_or(&self.config.default_system_prompt);

        let mut full_prompt = base_prompt.to_string();

        // Add memory context if available
        if !memory_context.device_snapshots.is_empty() {
            full_prompt.push_str("\n\n## Device Context\n");
            for snapshot in &memory_context.device_snapshots {
                full_prompt.push_str(&format!(
                    "- {}: {}\n",
                    snapshot.device_id,
                    snapshot.snapshot_data
                ));
            }
        }

        full_prompt
    }

    /// Spawn the async chat task that will feed events to the stream
    async fn spawn_chat_task(
        &self,
        request: ChatRequest,
        parsed_key: ParsedSessionKey,
        system_prompt: String,
        _should_compact: bool,
        tx: mpsc::UnboundedSender<ChatEvent>,
    ) -> Result<(), ChatError> {
        let runtime = Arc::clone(&self.runtime);
        let session_key = request.session_key.clone();
        let run_id = request.run_id.clone();
        let message = request.message.clone();
        let agent_id = parsed_key.agent_id.clone();

        // Store user message
        self.session_repo
            .add_message(&session_key, crate::application::agent::session_service::ChatMessage {
                role: "user".to_string(),
                content: message.clone(),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
                run_id: Some(run_id.clone()),
                tool_call_id: None,
                tool_name: None,
            })
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;

        tokio::spawn(async move {
            // Initiate chat with runtime
            match runtime
                .chat_send(&agent_id, &session_key, &message, &run_id, &system_prompt)
                .await
            {
                Ok(response) => {
                    // Process SSE response and forward events
                    if let Err(e) = Self::process_sse_response(response, &tx, &run_id, &session_key).await {
                        let _ = tx.send(ChatEvent::Error {
                            run_id: run_id.clone(),
                            session_key: session_key.clone(),
                            error: format!("SSE processing error: {}", e),
                        });
                    }
                }
                Err(e) => {
                    let _ = tx.send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: format!("Chat send failed: {}", e),
                    });
                }
            }
        });

        Ok(())
    }

    /// Process SSE response from the runtime and convert to ChatEvents
    async fn process_sse_response(
        response: reqwest::Response,
        tx: &mpsc::UnboundedSender<ChatEvent>,
        run_id: &str,
        session_key: &str,
    ) -> Result<(), ChatError> {
        use futures::StreamExt;

        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);

                    // Parse SSE data lines
                    for line in text.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];

                            if let Ok(event_value) = serde_json::from_str::<Value>(data) {
                                // Convert runtime event to ChatEvent
                                if let Some(event) = Self::convert_runtime_event(
                                    event_value,
                                    run_id,
                                    session_key,
                                ) {
                                    if tx.send(event).is_err() {
                                        // Receiver dropped, stop processing
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(ChatError::StreamError(format!("Stream error: {}", e)));
                }
            }
        }

        Ok(())
    }

    /// Convert a runtime SSE event to a ChatEvent
    fn convert_runtime_event(
        value: Value,
        default_run_id: &str,
        default_session_key: &str,
    ) -> Option<ChatEvent> {
        let state = value.get("state")?.as_str()?;
        let run_id = value
            .get("runId")
            .and_then(|v| v.as_str())
            .unwrap_or(default_run_id)
            .to_string();
        let session_key = value
            .get("sessionKey")
            .and_then(|v| v.as_str())
            .unwrap_or(default_session_key)
            .to_string();

        match state {
            "delta" => Some(ChatEvent::Delta {
                run_id,
                session_key,
                message: value.get("message")?.clone(),
            }),
            "thinking" => Some(ChatEvent::Thinking {
                run_id,
                session_key,
                thinking: value
                    .get("thinking")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            }),
            "tool_call_start" => Some(ChatEvent::ToolCallStart {
                run_id,
                session_key,
                tool_name: value
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_args: value
                    .get("toolArgs")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                a2ui: value
                    .get("a2ui")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            }),
            "tool_result" => Some(ChatEvent::ToolResult {
                run_id,
                session_key,
                tool_name: value
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                result: value
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            }),
            "final" => Some(ChatEvent::Final {
                run_id,
                session_key,
                message: value.get("message")?.clone(),
            }),
            "error" => Some(ChatEvent::Error {
                run_id,
                session_key,
                error: value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_key_valid() {
        let key = ParsedSessionKey::from_str("agent:ws-123:agent-456/sess-789").unwrap();
        assert_eq!(key.workspace_id, "ws-123");
        assert_eq!(key.agent_id, "agent-456");
        assert_eq!(key.session_uuid, "sess-789");
    }

    #[test]
    fn test_parse_session_key_invalid_format() {
        let result = ParsedSessionKey::from_str("invalid-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_session_key_missing_separator() {
        let result = ParsedSessionKey::from_str("agent:ws-123:agent-456");
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
        let parsed = ParsedSessionKey::from_str(&key).unwrap();

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
    fn test_chat_service_config_default() {
        let config = ChatServiceConfig::default();
        assert!(!config.default_system_prompt.is_empty());
        assert_eq!(config.max_messages_before_compact, 50);
        assert!(config.enable_compaction);
    }

    #[test]
    fn test_chat_error_from_agent_error() {
        let agent_err = AgentError::RequestFailed("test error".to_string());
        let chat_err: ChatError = agent_err.into();

        match chat_err {
            ChatError::RuntimeError(msg) => {
                assert!(msg.contains("test error"));
            }
            _ => panic!("Expected RuntimeError variant"),
        }
    }
}
