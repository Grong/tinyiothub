//! Adapter: cloud AgentPool -> tinyiothub_ai AgentPoolLike
//!
//! Bridges the type mismatch between cloud's TrustConfig (HashMap-based)
//! and tinyiothub-ai's TrustConfig (struct-based).

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_ai::agent::pool::AgentPoolLike;
use tinyiothub_ai::tool::trust::TrustConfig;

/// Wraps cloud's AgentPool to implement tinyiothub_ai's AgentPoolLike trait.
pub struct CloudAgentPoolAdapter {
    pool: Arc<crate::modules::agent::agent::AgentPool>,
}

impl CloudAgentPoolAdapter {
    pub fn new(pool: Arc<crate::modules::agent::agent::AgentPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentPoolLike for CloudAgentPoolAdapter {
    async fn get_or_create_agent(&self, workspace_id: &str) -> anyhow::Result<String> {
        // Cloud AgentPool uses agent_id = workspace_id (one agent per workspace)
        let _agent = self
            .pool
            .get_or_create(workspace_id, workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("AgentPool error: {}", e))?;
        // Return workspace_id as the handle identifier
        Ok(workspace_id.to_string())
    }

    async fn send_message(&self, workspace_id: &str, prompt: &str) -> anyhow::Result<String> {
        // Delegate to AgentPool's run_streaming method and collect the response
        let result = self
            .pool
            .run_streaming(workspace_id, prompt)
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;
        Ok(result.final_text)
    }

    async fn shutdown(&self) {
        // AgentPool doesn't have explicit shutdown; agents are dropped naturally
    }

    fn set_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        // Pass AI crate's TrustConfig straight through.
        // Trust evaluation (evaluate_tool_trust) classifies tools by name pattern —
        // no per-tool name lists needed here.
        self.pool.set_trust_config(workspace_id, config);
    }

    fn cleanup_idle(&self) -> usize {
        self.pool.cleanup_idle()
    }
}
