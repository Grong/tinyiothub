//! Agent pool trait — interface for agent lifecycle management.
//!
//! Cloud implements this with CloudAgentPoolAdapter (wrapping zeroclaw).
//! AI crate uses the trait for type erasure (tests use mocks).

use async_trait::async_trait;

use crate::harness::StreamEvent;
use crate::tool::trust::TrustConfig;

/// Interface for the agent pool — allows PatrolManager to accept either
/// the real AgentPool or a mock in tests.
#[async_trait]
pub trait AgentPoolLike: Send + Sync {
    async fn get_or_create_agent(&self, workspace_id: &str) -> anyhow::Result<String>;
    /// Send a message to the workspace's agent and get the text response.
    async fn send_message(&self, workspace_id: &str, prompt: &str) -> anyhow::Result<String>;
    /// Send a message and observe the streaming events (tool calls, chunks, etc.).
    /// The returned Receiver yields events as the LLM generates them.
    /// The channel closes after the turn completes (Final or Error emitted).
    async fn send_message_streamed(
        &self,
        workspace_id: &str,
        prompt: &str,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamEvent>>;
    async fn shutdown(&self);
    fn set_trust_config(&self, workspace_id: &str, config: TrustConfig);
    fn cleanup_idle(&self) -> usize;
}
