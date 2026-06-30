//! Agent types — AgentHandle trait and AgentError.

use std::future::Future;
use std::pin::Pin;

/// Lightweight handle to a chat-capable agent.
/// Wraps a zeroclaw agent reference for the heartbeat loop consumer.
pub trait AgentHandle: Send + Sync {
    fn send_message(&self, prompt: &str) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent not found for workspace: {workspace_id}")]
    NotFound { workspace_id: String },
    #[error("Pool capacity exceeded")]
    PoolFull,
    #[error("Build error: {0}")]
    Build(String),
    #[error("LLM error: {0}")]
    LlmError(String),
}
