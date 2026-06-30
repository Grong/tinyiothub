//! Minimax LLM provider — implements tinyiothub_ai::LlmProvider
//! so MemoryService can use the real LLM backend.

use async_trait::async_trait;
use tinyiothub_ai::memory::provider::{LlmCallMetadata, LlmProvider, LlmResponse};

/// Wraps zeroclaw's ModelProvider to implement AI crate's LlmProvider trait.
pub struct MinimaxLlmProvider;

impl MinimaxLlmProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MinimaxLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MinimaxLlmProvider {
    async fn chat(
        &self,
        system: Option<&str>,
        prompt: &str,
        model: &str,
        temperature: f32,
    ) -> anyhow::Result<LlmResponse> {
        let provider = crate::shared::config::create_minimax_provider()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let content = provider
            .chat_with_system(system, prompt, model, Some(temperature as f64))
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;

        Ok(LlmResponse {
            content,
            metadata: LlmCallMetadata {
                model_used: model.to_string(),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        })
    }
}
