//! port ModelProvider → rig CompletionModel 桥 + canonical 消息编码。
//!
//! canonical 编码（与 zeroclaw 适配器看到的历史文本一致，Global Constraints #4）：
//!
//! - `Message::System{content}` → `ChatMessage::system(content)`
//! - `UserContent::Text` → `ChatMessage::user(text)`
//! - `UserContent::ToolResult` → `ChatMessage::tool({"tool_call_id":..., "content": 文本化})`
//!   （content 为 Text 块拼接；Json 块 serde 序列化）
//! - `Message::Assistant` 永远拍平为一条 JSON：
//!   `{"content": 文本或 null, "tool_calls": [{"id","name","arguments"}]}`
//!   （arguments 由 `Value::to_string` 得 JSON 字符串；reasoning 有值时附加
//!   `"reasoning_content"` 键）
//!
//! 反向（port → rig）按同一编码解析；非 canonical 文本（旧 zeroclaw 持久化
//! 的纯文本 assistant 等）按原样回退为对应角色的纯文本消息。
//! 多媒体块（Image/Audio/Video/Document）无 canonical 文本位——拍平时跳过，
//! 反向不可恢复（文档化差异）。

use std::sync::Arc;

use rig_core::completion::{CompletionError, CompletionModel, CompletionRequest, CompletionResponse, Usage};
use rig_core::message::{
    AssistantContent, Message, Reasoning, ReasoningContent, Text, ToolCallId, ToolFunction,
    ToolResult as RigToolResult, ToolResultContent, UserContent,
};
use rig_core::streaming::{RawStreamingChoice, RawStreamingToolCall, StreamFinal, StreamingCompletionResponse};

use crate::port::provider::{ChatMessage, ChatRequest, ModelProvider, ToolCall as PortToolCall};

/// port ModelProvider → rig CompletionModel。
///
/// 把 rig 结构化 chat_history 拍平为 port canonical 编码，调用底层 port
/// provider；因此 ScriptedProvider 与真实 provider 在两个引擎下看到完全相同的
/// 文本历史。
pub struct PortModelAsRig {
    pub(crate) inner: Arc<dyn ModelProvider>,
    pub(crate) model: String,
}

impl PortModelAsRig {
    pub fn new(inner: Arc<dyn ModelProvider>, model: String) -> Self {
        Self { inner, model }
    }
}

/// 本适配器在 rig 侧的 provider 描述符：用 port provider 的 attribution alias。
impl PortModelAsRig {
    fn provider_name(&self) -> String {
        format!("port:{}", self.inner.alias())
    }
}

// ── rig → port 拍平 ─────────────────────────────────────────

pub(crate) fn tool_result_text(result: &RigToolResult) -> String {
    result
        .content
        .iter()
        .map(|c| match c {
            ToolResultContent::Text(t) => t.text.clone(),
            ToolResultContent::Json { value } => value.to_string(),
            ToolResultContent::Image(_) => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// rig 结构化 Message 历史 → port canonical ChatMessage 历史。
pub(crate) fn rig_messages_to_port(messages: &[Message]) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for message in messages {
        match message {
            Message::System { content } => out.push(ChatMessage::system(content.clone())),
            Message::User { content } => {
                for block in content {
                    match block {
                        UserContent::Text(t) => out.push(ChatMessage::user(t.text.clone())),
                        UserContent::ToolResult(tool_result) => out.push(ChatMessage::tool(
                            serde_json::json!({
                                "tool_call_id": tool_result.call.to_string(),
                                "content": tool_result_text(tool_result),
                            })
                            .to_string(),
                        )),
                        // 多媒体块：跳过（文档化差异）。
                        UserContent::Image(_)
                        | UserContent::Audio(_)
                        | UserContent::Video(_)
                        | UserContent::Document(_) => {}
                    }
                }
            }
            Message::Assistant { content, .. } => {
                let mut text = String::new();
                let mut reasoning = String::new();
                let mut calls: Vec<serde_json::Value> = Vec::new();
                for block in content {
                    match block {
                        AssistantContent::Text(t) => text.push_str(&t.text),
                        AssistantContent::ToolCall(tc) => calls.push(serde_json::json!({
                            "id": tc.id.to_string(),
                            "name": tc.function.name,
                            "arguments": tc.function.arguments.to_string(),
                        })),
                        AssistantContent::Reasoning(r) => {
                            for rb in &r.content {
                                if let ReasoningContent::Text { text: t, .. } = rb {
                                    reasoning.push_str(t);
                                }
                            }
                        }
                        AssistantContent::Image(_) => {}
                    }
                }
                if !text.is_empty() || !calls.is_empty() || !reasoning.is_empty() {
                    let mut v = serde_json::json!({
                        "content": if text.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::Value::String(text)
                        },
                        "tool_calls": calls,
                    });
                    if !reasoning.is_empty() {
                        v["reasoning_content"] = serde_json::Value::String(reasoning);
                    }
                    out.push(ChatMessage::assistant(v.to_string()));
                }
            }
        }
    }
    out
}

// ── port → rig 复原 ─────────────────────────────────────────

fn port_tool_message_to_rig(content: &str) -> Message {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        let id = v.get("tool_call_id").and_then(|x| x.as_str());
        let content_field = v.get("content");
        if let (Some(id), Some(content_field)) = (id, content_field) {
            let text = content_field
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| content_field.to_string());
            return Message::User {
                content: vec![UserContent::ToolResult(RigToolResult {
                    call: ToolCallId::new_or_mint(id),
                    provider: None,
                    // name 不在 canonical 编码内；回程拍平不读取该字段。
                    name: String::new(),
                    content: vec![ToolResultContent::Text(Text::new(text))],
                })],
            };
        }
    }
    Message::user(content)
}

fn port_assistant_message_to_rig(content: &str) -> Message {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content)
        && (v.get("tool_calls").is_some() || v.get("content").is_some())
    {
        let mut blocks: Vec<AssistantContent> = Vec::new();
        if let Some(c) = v.get("content").and_then(|x| x.as_str())
            && !c.is_empty()
        {
            blocks.push(AssistantContent::Text(Text::new(c.to_string())));
        }
        if let Some(r) = v.get("reasoning_content").and_then(|x| x.as_str()) {
            blocks.push(AssistantContent::Reasoning(Reasoning::new(r)));
        }
        if let Some(calls) = v.get("tool_calls").and_then(|x| x.as_array()) {
            for call in calls {
                let id = call.get("id").and_then(|x| x.as_str()).unwrap_or_default().to_string();
                let name = call
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string();
                let arguments = call
                    .get("arguments")
                    .and_then(|x| x.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::Value::Null);
                blocks.push(AssistantContent::ToolCall(rig_core::message::ToolCall::new(
                    ToolCallId::new_or_mint(id),
                    ToolFunction::new(name, arguments),
                )));
            }
        }
        return Message::Assistant {
            id: None,
            content: blocks,
        };
    }
    // 非 canonical 文本（旧 zeroclaw 持久化的纯文本 assistant）按原样回退。
    Message::assistant(content)
}

/// port canonical ChatMessage 历史 → rig 结构化 Message 历史。
pub(crate) fn port_messages_to_rig(messages: &[ChatMessage]) -> Vec<Message> {
    messages
        .iter()
        .map(|m| match m.role.as_str() {
            "system" => Message::system(m.content.clone()),
            "tool" => port_tool_message_to_rig(&m.content),
            "assistant" => port_assistant_message_to_rig(&m.content),
            _ => Message::user(m.content.clone()),
        })
        .collect()
}

// ── port → rig 响应组装 ──────────────────────────────────────

fn port_tool_call_to_rig(tc: &PortToolCall) -> Result<rig_core::message::ToolCall, CompletionError> {
    let arguments: serde_json::Value = serde_json::from_str(&tc.arguments)
        .map_err(|e| CompletionError::ResponseError(format!("invalid tool arguments: {e}")))?;
    Ok(rig_core::message::ToolCall::new(
        ToolCallId::new_or_mint(tc.id.clone()),
        ToolFunction::new(tc.name.clone(), arguments),
    ))
}

fn port_response_to_rig(resp: crate::port::provider::ChatResponse) -> CompletionResponse {
    let mut choice: Vec<AssistantContent> = Vec::new();
    if let Some(text) = &resp.text
        && !text.is_empty()
    {
        choice.push(AssistantContent::Text(Text::new(text.clone())));
    }
    if let Some(reasoning) = &resp.reasoning_content
        && !reasoning.is_empty()
    {
        choice.push(AssistantContent::Reasoning(Reasoning::new(reasoning)));
    }
    for tc in &resp.tool_calls {
        if let Ok(call) = port_tool_call_to_rig(tc) {
            choice.push(AssistantContent::ToolCall(call));
        }
    }
    let usage = resp.usage.map(|u| Usage {
        input_tokens: u.input_tokens.unwrap_or(0),
        output_tokens: u.output_tokens.unwrap_or(0),
        total_tokens: u.input_tokens.unwrap_or(0) + u.output_tokens.unwrap_or(0),
        cached_input_tokens: u.cached_input_tokens.unwrap_or(0),
        cache_creation_input_tokens: 0,
        tool_use_prompt_tokens: 0,
        reasoning_tokens: 0,
    });
    CompletionResponse::new(choice, usage.unwrap_or_default(), "port")
}

// ── rig CompletionModel → port ModelProvider ────────────────

/// rig CompletionModel（如 minimax）→ port ModelProvider。
///
/// [`PortModelAsRig`] 的镜像：port canonical ChatRequest 按
/// [`port_messages_to_rig`] 解析回 rig 结构化 CompletionRequest，调 rig
/// `completion()`，响应逆映射回 port ChatResponse。
pub struct RigMinimaxAsPort<M> {
    inner: M,
}

impl<M> RigMinimaxAsPort<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<M> crate::port::attribution::Attributable for RigMinimaxAsPort<M> {
    fn role(&self) -> crate::port::attribution::Role {
        crate::port::attribution::Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Minimax,
        ))
    }
    fn alias(&self) -> &str {
        "minimax"
    }
}

/// rig CompletionResponse → port ChatResponse（`port_response_to_rig` 的逆）。
fn rig_response_to_port(resp: CompletionResponse) -> crate::port::provider::ChatResponse {
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<PortToolCall> = Vec::new();
    for block in resp.choice {
        match block {
            AssistantContent::Text(t) => text.push_str(&t.text),
            AssistantContent::Reasoning(r) => {
                for rb in &r.content {
                    if let ReasoningContent::Text { text: t, .. } = rb {
                        reasoning.push_str(t);
                    }
                }
            }
            AssistantContent::ToolCall(tc) => tool_calls.push(PortToolCall {
                id: tc.id.to_string(),
                name: tc.function.name,
                arguments: tc.function.arguments.to_string(),
            }),
            AssistantContent::Image(_) => {}
        }
    }
    let usage = resp.usage;
    crate::port::provider::ChatResponse {
        text: if text.is_empty() { None } else { Some(text) },
        tool_calls,
        // rig Usage 字段非 Option（未上报即 0）——按 rig 语义 Some 透传。
        usage: Some(crate::port::provider::TokenUsage {
            input_tokens: Some(usage.input_tokens),
            output_tokens: Some(usage.output_tokens),
            cached_input_tokens: Some(usage.cached_input_tokens),
        }),
        reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
    }
}

#[async_trait::async_trait]
impl<M: CompletionModel> ModelProvider for RigMinimaxAsPort<M> {
    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        let tools: Vec<rig_core::completion::ToolDefinition> = request
            .tools
            .map(|specs| {
                specs
                    .iter()
                    .map(|s| rig_core::completion::ToolDefinition {
                        name: s.name.clone(),
                        description: s.description.clone(),
                        parameters: s.parameters.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rig_request = CompletionRequest {
            model: Some(model.to_string()),
            // canonical 编码里 system 是首条 Message::System，不走 legacy preamble。
            preamble: None,
            chat_history: port_messages_to_rig(request.messages),
            documents: vec![],
            tools,
            temperature,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
            record_telemetry_content: false,
        };
        let resp = self
            .inner
            .completion(rig_request)
            .await
            .map_err(|e| anyhow::anyhow!("rig completion failed: {e}"))?;
        Ok(rig_response_to_port(resp))
    }
}

// ── CompletionModel 实现 ────────────────────────────────────

impl CompletionModel for PortModelAsRig {
    async fn completion(&self, request: CompletionRequest) -> Result<CompletionResponse, CompletionError> {
        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(preamble) = &request.preamble {
            messages.push(ChatMessage::system(preamble.clone()));
        }
        messages.extend(rig_messages_to_port(&request.chat_history));

        let tool_specs: Vec<crate::port::tool::ToolSpec> = request
            .tools
            .iter()
            .map(|t| crate::port::tool::ToolSpec {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect();

        let port_request = ChatRequest {
            messages: &messages,
            tools: if tool_specs.is_empty() { None } else { Some(&tool_specs) },
        };
        let resp = self
            .inner
            .chat(port_request, &self.model, request.temperature)
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;
        Ok(port_response_to_rig(resp))
    }

    /// 非流式代理：调 completion() 后把整份响应包装成一个单段流
    /// （Text → Message chunk，ToolCall → RawStreamingToolCall，
    /// 末尾补 StreamFinal 终端记录）。
    async fn stream(&self, request: CompletionRequest) -> Result<StreamingCompletionResponse, CompletionError> {
        let resp = self.completion(request).await?;
        let provider = self.provider_name();
        let mut items: Vec<Result<RawStreamingChoice, CompletionError>> = Vec::new();
        for block in &resp.choice {
            match block {
                AssistantContent::Text(t) => {
                    items.push(Ok(RawStreamingChoice::Message(t.text.clone())));
                }
                AssistantContent::ToolCall(tc) => {
                    items.push(Ok(RawStreamingChoice::ToolCall(RawStreamingToolCall::new(
                        rig_core::streaming::StreamPartId::wire(tc.id.to_string()),
                        tc.function.name.clone(),
                        tc.function.arguments.clone(),
                    ))));
                }
                // Reasoning：非流式代理无法回放出 delta 序列，跳过（choice 聚合
                // 已在 CompletionResponse 侧保留，文档化差异）。
                AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {}
            }
        }
        items.push(Ok(RawStreamingChoice::FinalResponse(StreamFinal::new(
            provider.clone(),
            resp.usage,
        ))));
        Ok(StreamingCompletionResponse::stream(
            provider,
            Box::pin(futures::stream::iter(items)),
        ))
    }
}
