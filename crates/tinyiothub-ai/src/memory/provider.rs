//! LLM provider trait — abstracts the LLM backend for MemoryService.
//!
//! Cloud implements this with Minimax; tests use a mock.

use async_trait::async_trait;

/// Metadata from an LLM call for observability (tokens, latency, model).
#[derive(Debug, Clone, Default)]
pub struct LlmCallMetadata {
    /// Actual model used (may differ from requested if fallback occurred).
    pub model_used: String,
    /// Input tokens consumed.
    pub prompt_tokens: u32,
    /// Output tokens generated.
    pub completion_tokens: u32,
    /// Time-to-first-token in milliseconds.
    pub ttft_ms: u64,
    /// Total call latency in milliseconds.
    pub total_latency_ms: u64,
    /// Why the model stopped: "stop", "length", "content_filter", etc.
    pub finish_reason: String,
}

/// LLM response with content and observability metadata.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub metadata: LlmCallMetadata,
}

/// Abstract LLM call interface. MemoryService depends on this trait
/// instead of any specific provider, keeping the AI crate provider-agnostic.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a prompt to the LLM and get the response with metadata.
    async fn chat(
        &self,
        system: Option<&str>,
        prompt: &str,
        model: &str,
        temperature: f32,
    ) -> anyhow::Result<LlmResponse>;
}
