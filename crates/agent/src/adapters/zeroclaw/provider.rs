//! port ModelProvider ↔ zeroclaw ModelProvider 双向桥。

use std::sync::Arc;

use crate::port::attribution::{self, Attributable};
use crate::port::provider::{
    ChatMessage as PortChatMessage, ChatRequest as PortChatRequest, ChatResponse as PortChatResponse,
    ModelProvider as PortModelProvider, TokenUsage as PortTokenUsage, ToolCall as PortToolCall,
};

use super::tools::{port_role, zc_role};

/// port ModelProvider 包装为 zeroclaw ModelProvider（zeroclaw loop 经此调我们的 provider）。
pub struct PortProviderAsZeroclaw {
    pub inner: Arc<dyn PortModelProvider>,
}

impl Attributable for PortProviderAsZeroclaw {
    fn role(&self) -> attribution::Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

impl zeroclaw_api::attribution::Attributable for PortProviderAsZeroclaw {
    fn role(&self) -> zeroclaw_api::attribution::Role {
        zc_role(&self.inner.role())
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

/// zeroclaw ChatMessage → port ChatMessage。
///
/// 当前恒等（两形状 vendored 时逐字段一致）；独立函数是为 phase 2 rig
/// 适配器的消息归一化预留挂点。
pub(crate) fn normalize_zeroclaw_messages(
    messages: &[zeroclaw::providers::traits::ChatMessage],
) -> Vec<PortChatMessage> {
    messages
        .iter()
        .map(|m| PortChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect()
}

fn port_tool_spec(spec: &zeroclaw::tools::ToolSpec) -> crate::port::tool::ToolSpec {
    crate::port::tool::ToolSpec {
        name: spec.name.clone(),
        description: spec.description.clone(),
        parameters: spec.parameters.clone(),
    }
}

fn zc_tool_spec(spec: &crate::port::tool::ToolSpec) -> zeroclaw::tools::ToolSpec {
    zeroclaw::tools::ToolSpec {
        name: spec.name.clone(),
        description: spec.description.clone(),
        parameters: spec.parameters.clone(),
    }
}

#[async_trait::async_trait]
impl zeroclaw::providers::traits::ModelProvider for PortProviderAsZeroclaw {
    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = system_prompt {
            messages.push(PortChatMessage::system(system));
        }
        messages.push(PortChatMessage::user(message));
        let response = self
            .inner
            .chat(
                PortChatRequest {
                    messages: &messages,
                    tools: None,
                },
                model,
                temperature,
            )
            .await?;
        Ok(response.text.unwrap_or_default())
    }

    async fn chat(
        &self,
        request: zeroclaw::providers::traits::ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<zeroclaw::providers::traits::ChatResponse> {
        let messages = normalize_zeroclaw_messages(request.messages);
        let tools = request
            .tools
            .map(|ts| ts.iter().map(port_tool_spec).collect::<Vec<_>>());
        let response = self
            .inner
            .chat(
                PortChatRequest {
                    messages: &messages,
                    tools: tools.as_deref(),
                },
                model,
                temperature,
            )
            .await?;
        Ok(zeroclaw::providers::traits::ChatResponse {
            text: response.text,
            tool_calls: response
                .tool_calls
                .into_iter()
                .map(|c| zeroclaw::providers::traits::ToolCall {
                    id: c.id,
                    name: c.name,
                    arguments: c.arguments,
                    extra_content: None,
                })
                .collect(),
            usage: response.usage.map(|u| zeroclaw::providers::traits::TokenUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                cached_input_tokens: u.cached_input_tokens,
            }),
            reasoning_content: response.reasoning_content,
        })
    }
}

/// zeroclaw ModelProvider 包装为 port ModelProvider（[`ZeroclawProviderAsPort`]）。
///
/// zeroclaw 收敛为 adapter 内部细节后，引擎的 minimax provider 构建也住这里：
/// [`crate::pool::provider::create_minimax_provider`] 与本函数同名，代理到此处。
pub fn create_minimax_provider(base_url: &str, auth_token: &str) -> anyhow::Result<Box<dyn PortModelProvider>> {
    let inner = zeroclaw::providers::create_model_provider_with_url("minimaxi", Some(auth_token), Some(base_url))?;
    Ok(Box::new(ZeroclawProviderAsPort { inner }))
}

pub struct ZeroclawProviderAsPort {
    pub inner: Box<dyn zeroclaw::providers::traits::ModelProvider>,
}

impl Attributable for ZeroclawProviderAsPort {
    fn role(&self) -> attribution::Role {
        port_role(self.inner.role())
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

#[async_trait::async_trait]
impl PortModelProvider for ZeroclawProviderAsPort {
    async fn chat(
        &self,
        request: PortChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<PortChatResponse> {
        let messages = request
            .messages
            .iter()
            .map(|m| zeroclaw::providers::traits::ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect::<Vec<_>>();
        let tools = request.tools.map(|ts| ts.iter().map(zc_tool_spec).collect::<Vec<_>>());
        let response = zeroclaw::providers::traits::ModelProvider::chat(
            self.inner.as_ref(),
            zeroclaw::providers::traits::ChatRequest {
                messages: &messages,
                tools: tools.as_deref(),
                thinking: None,
            },
            model,
            temperature,
        )
        .await?;
        Ok(PortChatResponse {
            text: response.text,
            tool_calls: response
                .tool_calls
                .into_iter()
                .map(|c| PortToolCall {
                    id: c.id,
                    name: c.name,
                    arguments: c.arguments,
                })
                .collect(),
            usage: response.usage.map(|u| PortTokenUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                cached_input_tokens: u.cached_input_tokens,
            }),
            reasoning_content: response.reasoning_content,
        })
    }
}
