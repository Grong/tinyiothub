use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<tinyiothub_core::media::MediaAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("image_unavailable")]
    ImageUnavailable,
    #[error("LLM streaming request failed: HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("LLM stream error: {message}")]
    Stream { code: Option<String>, message: String },
    #[error("{0}")]
    Internal(String),
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http { status, .. } => matches!(*status, 408 | 429 | 500..=599),
            Self::Stream { code, .. } => !code.as_deref().is_some_and(|code| {
                matches!(
                    code,
                    "authentication_error"
                        | "invalid_api_key"
                        | "permission_denied"
                        | "model_not_found"
                        | "invalid_request"
                        | "invalid_request_error"
                        | "insufficient_quota"
                )
            }),
            Self::Internal(_) => true,
            Self::ImageUnavailable => false,
        }
    }
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            attachments: Vec::new(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            attachments: Vec::new(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            attachments: Vec::new(),
        }
    }
    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            attachments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // JSON string
}

/// A streamed token/text delta (or a reasoning delta when `reasoning`).
#[derive(Debug, Clone, Default)]
pub struct ChatStreamDelta {
    pub text: String,
    pub reasoning: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

/// An LLM response that may contain text, tool calls, or both.
#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: Option<&'a [crate::port::tool::ToolSpec]>,
    /// 输出 token 预算（None = 服务端默认）。推理模型的默认预算可能被
    /// reasoning 吃光导致正文截断（2026-09-28 judgment eval 实测：
    /// intro 截断、verdict 块未生成）——调用方需要确定性输出时应显式给足。
    pub max_tokens: Option<u64>,
}

/// 极简 provider 抽象：agent loop 只走 `chat`。
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync + crate::port::attribution::Attributable {
    fn supports_vision(&self) -> bool {
        false
    }
    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse>;

    /// True streaming when implemented. The default adapter fallback returns
    /// the full response as one delta (no incremental tokens).
    async fn chat_stream(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
        on_delta: &(dyn Fn(ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<ChatResponse> {
        let response = self.chat(request, model, temperature).await?;
        if let Some(text) = &response.text
            && !text.is_empty()
        {
            on_delta(ChatStreamDelta {
                text: text.clone(),
                reasoning: false,
            });
        }
        if let Some(reasoning) = &response.reasoning_content
            && !reasoning.is_empty()
        {
            on_delta(ChatStreamDelta {
                text: reasoning.clone(),
                reasoning: true,
            });
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderError;

    #[test]
    fn retry_policy_separates_recoverable_system_errors() {
        for status in [408, 429, 500, 502, 503] {
            assert!(
                ProviderError::Http {
                    status,
                    message: "temporary".into()
                }
                .is_retryable()
            );
        }
        assert!(
            ProviderError::Stream {
                code: Some("rate_limit".into()),
                message: "slow down".into()
            }
            .is_retryable()
        );
        assert!(
            ProviderError::Stream {
                code: None,
                message: "connection reset".into()
            }
            .is_retryable()
        );
        assert!(ProviderError::Internal("temporary database error".into()).is_retryable());
    }

    #[test]
    fn retry_policy_stops_user_and_unrecoverable_errors() {
        for status in [400, 401, 403, 404, 422] {
            assert!(
                !ProviderError::Http {
                    status,
                    message: "bad request".into()
                }
                .is_retryable()
            );
        }
        for code in [
            "authentication_error",
            "invalid_api_key",
            "permission_denied",
            "model_not_found",
            "invalid_request",
        ] {
            assert!(
                !ProviderError::Stream {
                    code: Some(code.into()),
                    message: "stop".into()
                }
                .is_retryable()
            );
        }
        assert!(!ProviderError::ImageUnavailable.is_retryable());
    }
}
