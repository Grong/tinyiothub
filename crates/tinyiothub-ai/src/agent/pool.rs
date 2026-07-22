//! Agent pool trait — interface for agent lifecycle management.
//!
//! Cloud implements this with CloudAgentPoolAdapter (wrapping zeroclaw).
//! AI crate uses the trait for type erasure (tests use mocks).

use async_trait::async_trait;

use crate::tool::trust::TrustConfig;

/// A tool call actually executed by the agent framework during a run.
/// This is the ground truth for audit trails — never trust the LLM's
/// self-reported action list.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub success: bool,
    pub details: String,
}

/// Result of sending a message to an agent: text reply plus the tool calls
/// the framework actually executed while producing it.
#[derive(Debug, Clone)]
pub struct AgentRunOutput {
    pub text: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

/// Interface for the agent pool — allows PatrolManager to accept either
/// the real AgentPool or a mock in tests.
#[async_trait]
pub trait AgentPoolLike: Send + Sync {
    async fn get_or_create_agent(&self, workspace_id: &str) -> anyhow::Result<String>;
    /// Send a message to the workspace's agent and get the response plus
    /// the tool calls actually executed during the run.
    async fn send_message(&self, workspace_id: &str, prompt: &str) -> anyhow::Result<AgentRunOutput>;
    async fn shutdown(&self);
    fn set_trust_config(&self, workspace_id: &str, config: TrustConfig);
    fn cleanup_idle(&self) -> usize;
}
