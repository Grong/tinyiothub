// Chat Service - Core orchestration service for AI Agent chat

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use serde_json::Value;
use tokio::sync::mpsc;

use super::types::{
    ChatError, ChatEvent, ChatMessage, ChatRequest, ChatServiceConfig, ChatStream,
    MemoryContext, ParsedSessionKey, SessionRepository,
};
use super::memory_service::AgentMemoryService;
use crate::shared::agent::AgentRuntime;

impl ChatStream {
    pub fn new(receiver: mpsc::Receiver<ChatEvent>) -> Self {
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
    config: ChatServiceConfig,
}

impl ChatService {
    pub fn new(
        runtime: Arc<dyn AgentRuntime>,
        session_repo: Arc<dyn SessionRepository>,
        memory_service: Arc<AgentMemoryService>,
        config: ChatServiceConfig,
    ) -> Self {
        Self { runtime, session_repo, memory_service, config }
    }

    /// Execute a chat request and return a stream of events
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatStream, ChatError> {
        let parsed_key = request.parse_session_key()?;

        let session = self
            .session_repo
            .get_or_create(&request.session_key)
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;

        let memory_context = self
            .memory_service
            .build_context(&parsed_key.workspace_id, &parsed_key.agent_id)
            .await
            .map_err(|e| ChatError::MemoryError(e.to_string()))?;

        let should_compact = if self.config.enable_compaction {
            self.check_compaction_needed(&session.session_key).await?
        } else {
            false
        };

        let system_prompt = self.build_system_prompt(
            request.system_prompt_override.as_deref(),
            &memory_context,
        );

        let (tx, rx) = mpsc::channel::<ChatEvent>(100);
        self.spawn_chat_task(request, parsed_key, system_prompt, should_compact, tx).await?;
        Ok(ChatStream::new(rx))
    }

    pub async fn abort_chat(&self, session_key: &str, run_id: Option<&str>) -> Result<(), ChatError> {
        let parsed = ParsedSessionKey::parse_str(session_key)?;
        self.runtime
            .chat_abort(&parsed.agent_id, session_key, run_id)
            .await
            .map_err(|e| ChatError::RuntimeError(e.to_string()))?;
        Ok(())
    }

    pub async fn get_history(&self, session_key: &str, limit: u32) -> Result<Value, ChatError> {
        let parsed = ParsedSessionKey::parse_str(session_key)?;
        let history = self.runtime.chat_history(&parsed.agent_id, session_key, limit).await?;
        Ok(history)
    }

    async fn check_compaction_needed(&self, session_key: &str) -> Result<bool, ChatError> {
        let message_count = self
            .session_repo
            .get_message_count(session_key)
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;
        Ok(message_count > self.config.max_messages_before_compact)
    }

    fn build_system_prompt(&self, override_prompt: Option<&str>, memory_context: &MemoryContext) -> String {
        let base_prompt = override_prompt.unwrap_or("");
        let mut full_prompt = base_prompt.to_string();
        if !memory_context.is_empty() {
            full_prompt.push_str(&memory_context.to_prompt_fragment());
        }
        full_prompt
    }

    async fn spawn_chat_task(
        &self,
        request: ChatRequest,
        parsed_key: ParsedSessionKey,
        system_prompt: String,
        _should_compact: bool,
        tx: mpsc::Sender<ChatEvent>,
    ) -> Result<(), ChatError> {
        let runtime = Arc::clone(&self.runtime);
        let session_key = request.session_key.clone();
        let run_id = request.run_id.clone();
        let message = request.message.clone();
        let agent_id = parsed_key.agent_id.clone();

        self.session_repo
            .add_message(&session_key, ChatMessage {
                role: "user".to_string(),
                content: message.clone(),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
                run_id: Some(run_id.clone()),
                tool_call_id: None,
                tool_name: None,
            })
            .await
            .map_err(|e| ChatError::RepositoryError(e.to_string()))?;

        let session_repo = Arc::clone(&self.session_repo);

        tokio::spawn(async move {
            match runtime.chat_send(&agent_id, &session_key, &message, &run_id, &system_prompt).await {
                Ok(response) => {
                    if let Err(e) = Self::process_sse_response(session_repo, response, &tx, &run_id, &session_key).await {
                        let _ = tx.send(ChatEvent::Error {
                            run_id: run_id.clone(),
                            session_key: session_key.clone(),
                            error: format!("SSE processing error: {}", e),
                        }).await;
                    }
                }
                Err(e) => {
                    let _ = tx.send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: format!("Chat send failed: {}", e),
                    }).await;
                }
            }
        });

        Ok(())
    }

    async fn process_sse_response(
        session_repo: Arc<dyn SessionRepository>,
        response: reqwest::Response,
        tx: &mpsc::Sender<ChatEvent>,
        run_id: &str,
        session_key: &str,
    ) -> Result<(), ChatError> {
        use futures::StreamExt;

        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ")
                            && let Ok(event_value) = serde_json::from_str::<Value>(data) {
                                if let Some(event) = Self::convert_runtime_event(event_value.clone(), run_id, session_key) {
                                    if let ChatEvent::Final { message, .. } = &event
                                        && let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                                            let content_json = serde_json::json!(content);
                                            let _ = session_repo.add_message(
                                                session_key,
                                                ChatMessage {
                                                    role: "assistant".to_string(),
                                                    content: content_json.to_string(),
                                                    timestamp: Some(chrono::Utc::now().timestamp_millis()),
                                                    run_id: Some(run_id.to_string()),
                                                    tool_call_id: None,
                                                    tool_name: None,
                                                },
                                            ).await;
                                        }
                                    if tx.send(event).await.is_err() {
                                        return Ok(());
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

    fn convert_runtime_event(value: Value, default_run_id: &str, default_session_key: &str) -> Option<ChatEvent> {
        let state = value.get("state")?.as_str()?;
        let run_id = value.get("runId").and_then(|v| v.as_str()).unwrap_or(default_run_id).to_string();
        let session_key = value.get("sessionKey").and_then(|v| v.as_str()).unwrap_or(default_session_key).to_string();

        match state {
            "delta" => Some(ChatEvent::Delta { run_id, session_key, message: value.get("message")?.clone() }),
            "thinking" => Some(ChatEvent::Thinking {
                run_id, session_key,
                thinking: value.get("thinking").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }),
            "tool_call_start" => Some(ChatEvent::ToolCallStart {
                run_id, session_key,
                tool_name: value.get("toolName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                tool_args: value.get("toolArgs").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                a2ui: value.get("a2ui").and_then(|v| v.as_str()).map(String::from),
            }),
            "tool_result" => Some(ChatEvent::ToolResult {
                run_id, session_key,
                tool_name: value.get("toolName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                result: value.get("result").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }),
            "final" => Some(ChatEvent::Final { run_id, session_key, message: value.get("message")?.clone() }),
            "error" => Some(ChatEvent::Error {
                run_id, session_key,
                error: value.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string(),
            }),
            _ => None,
        }
    }
}
