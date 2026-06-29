//! Agent pool — manages agent lifecycle.
//! Interface and stub — populated in a later task (Task 10/14).

use async_trait::async_trait;

use crate::patrol::types::TrustConfig;

/// Interface for the agent pool — allows PatrolManager to accept either
/// the real AgentPool (later task) or a mock in tests.
#[async_trait]
pub trait AgentPoolLike: Send + Sync {
    async fn get_or_create_agent(&self, workspace_id: &str) -> anyhow::Result<String>;
    /// Send a message to the workspace's agent and get the text response.
    async fn send_message(&self, workspace_id: &str, prompt: &str) -> anyhow::Result<String>;
    async fn shutdown(&self);
    fn set_trust_config(&self, workspace_id: &str, config: TrustConfig);
    fn cleanup_idle(&self) -> usize;
}

/// Stub pool — populated in a later task (Task 10/14).
pub struct AgentPool;

#[async_trait]
impl AgentPoolLike for AgentPool {
    async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
        Ok("stub-agent".to_string())
    }

    async fn send_message(&self, _workspace_id: &str, _prompt: &str) -> anyhow::Result<String> {
        // Stub: return a minimal valid JSON report until real LLM wiring (Task 10/14)
        Ok(r#"{"status": "complete", "summary": "Stub patrol tick — LLM not wired yet", "executed_actions": [], "pending_proposals": []}"#.to_string())
    }

    async fn shutdown(&self) {}

    fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}

    fn cleanup_idle(&self) -> usize {
        0
    }
}
