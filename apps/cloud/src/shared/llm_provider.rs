//! Minimax LLM provider — implements tinyiothub_llm::LlmProvider
//! so MemoryService can use the real LLM backend.

use async_trait::async_trait;
use tinyiothub_agent::port::provider::ChatMessage;
use tinyiothub_llm::provider::{LlmCallMetadata, LlmProvider, LlmResponse};

/// Wraps the agent crate's port ModelProvider to implement AI crate's
/// LlmProvider trait. Holds its MiniMax config slice (G6 — no process-global
/// config reads); when None, `chat` errors exactly as the former
/// missing-`[minimax]` path did.
pub struct MinimaxLlmProvider {
    minimax: Option<tinyiothub_core::config::MinimaxConfig>,
}

impl MinimaxLlmProvider {
    pub fn new(minimax: Option<tinyiothub_core::config::MinimaxConfig>) -> Self {
        Self { minimax }
    }
}

impl Default for MinimaxLlmProvider {
    fn default() -> Self {
        Self::new(None)
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
        let cfg = self
            .minimax
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
        let provider = crate::shared::config::create_minimax_provider(cfg).map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = system {
            messages.push(ChatMessage::system(system));
        }
        messages.push(ChatMessage::user(prompt));
        let response = provider
            .chat(
                tinyiothub_agent::port::provider::ChatRequest {
                    messages: &messages,
                    tools: None,
                },
                model,
                Some(temperature as f64),
            )
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;

        Ok(LlmResponse {
            content: response.text.unwrap_or_default(),
            metadata: LlmCallMetadata {
                model_used: model.to_string(),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        })
    }
}
