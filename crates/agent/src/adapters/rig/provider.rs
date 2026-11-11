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
//! 图像块经 `ChatMessage.attachments` 另存；Audio/Video/Document 无 canonical
//! 文本位——拍平时跳过，反向不可恢复（文档化差异）。

use std::sync::Arc;

use rig_core::completion::{CompletionError, CompletionModel, CompletionRequest, CompletionResponse, Usage};
use rig_core::message::{
    AssistantContent, DocumentSourceKind, Image, ImageMediaType, Message, Reasoning, ReasoningContent, Text,
    ToolCallId, ToolFunction, ToolResult as RigToolResult, ToolResultContent, UserContent,
};
use rig_core::streaming::{
    RawStreamingChoice, RawStreamingToolCall, StreamFinal, StreamPartId, StreamingCompletionResponse,
};

use crate::port::events::RetryNotice;
use crate::port::provider::{ChatMessage, ChatRequest, ModelProvider, ProviderError, ToolCall as PortToolCall};
use base64::Engine as _;
use tinyiothub_core::media::{MediaAttachment, MediaError};

pub(crate) fn check_vision_request(provider: &dyn ModelProvider, messages: &[ChatMessage]) -> anyhow::Result<()> {
    let has_images = messages.iter().any(|message| !message.attachments.is_empty());
    if has_images && !provider.supports_vision() {
        return Err(anyhow::Error::new(ProviderError::ImageUnavailable));
    }
    for message in messages {
        if message.attachments.len() > 1 {
            return Err(MediaError::TooManyImages.into());
        }
        for attachment in &message.attachments {
            media_attachment_to_rig(attachment)?;
        }
    }
    Ok(())
}

fn media_attachment_to_rig(attachment: &MediaAttachment) -> Result<ToolResultContent, MediaError> {
    if attachment.bytes.len() > tinyiothub_core::media::MAX_IMAGE_BYTES {
        return Err(MediaError::ImageTooLarge);
    }
    Ok(ToolResultContent::Image(Image {
        data: DocumentSourceKind::Raw(attachment.bytes.clone()),
        media_type: Some(match attachment.mime.as_str() {
            "image/jpeg" => ImageMediaType::JPEG,
            "image/png" => ImageMediaType::PNG,
            other => return Err(MediaError::UnsupportedMime(other.to_owned())),
        }),
        detail: None,
        additional_params: rig_core::message::AdditionalParams::from_entries([(
            "tinyiothub_evidence",
            serde_json::to_value(tinyiothub_core::media::EvidenceRef::from(attachment))
                .map_err(|error| MediaError::Validation(error.to_string()))?,
        )]),
    }))
}

pub(crate) fn tool_result_to_rig(result: &crate::port::tool::ToolResult) -> Result<Vec<ToolResultContent>, MediaError> {
    if result.attachments.len() > 1 {
        return Err(MediaError::TooManyImages);
    }
    let mut content = vec![ToolResultContent::Text(Text::new(result.output.clone()))];
    for attachment in &result.attachments {
        content.push(media_attachment_to_rig(attachment)?);
    }
    Ok(content)
}

pub(crate) fn tool_result_attachments(result: &RigToolResult) -> Result<Vec<MediaAttachment>, MediaError> {
    let mut attachments = Vec::new();
    for item in &result.content {
        if let ToolResultContent::Image(image) = item {
            let DocumentSourceKind::Raw(bytes) = &image.data else {
                return Err(MediaError::InvalidImage);
            };
            if bytes.len() > tinyiothub_core::media::MAX_IMAGE_BYTES {
                return Err(MediaError::ImageTooLarge);
            }
            let mime = match image.media_type {
                Some(ImageMediaType::JPEG) => "image/jpeg",
                Some(ImageMediaType::PNG) => "image/png",
                _ => return Err(MediaError::UnsupportedMime("unknown".into())),
            };
            let reference = image
                .additional_params
                .as_ref()
                .and_then(|params| params.get("tinyiothub_evidence"))
                .cloned()
                .map(serde_json::from_value::<tinyiothub_core::media::EvidenceRef>)
                .transpose()
                .map_err(|error| MediaError::Validation(error.to_string()))?;
            attachments.push(MediaAttachment {
                evidence_id: reference
                    .as_ref()
                    .map_or_else(String::new, |item| item.evidence_id.clone()),
                mime: mime.into(),
                bytes: bytes.clone(),
                source_seq: reference.as_ref().map_or(0, |item| item.source_seq),
                media_time: reference.map_or_else(chrono::Utc::now, |item| item.media_time),
            });
        }
    }
    if attachments.len() > 1 {
        return Err(MediaError::TooManyImages);
    }
    Ok(attachments)
}

const MAX_MODEL_RETRIES: u32 = 3;

enum ProviderStreamEvent {
    Delta(crate::port::provider::ChatStreamDelta),
    Retry(RetryNotice),
}

#[derive(Clone)]
struct RetryContext {
    tx: tokio::sync::mpsc::Sender<RetryNotice>,
    cancel: Option<tokio_util::sync::CancellationToken>,
}

/// Per-turn retry output shared by the model adapter and outer rig loop.
#[derive(Clone, Default)]
pub(crate) struct RetryEvents {
    inner: Arc<std::sync::Mutex<Option<RetryContext>>>,
}

pub(crate) struct RetryEventsGuard {
    events: RetryEvents,
}

impl Drop for RetryEventsGuard {
    fn drop(&mut self) {
        *self.events.inner.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }
}

impl RetryEvents {
    pub(crate) fn install(
        &self,
        tx: tokio::sync::mpsc::Sender<RetryNotice>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> RetryEventsGuard {
        *self.inner.lock().unwrap_or_else(|error| error.into_inner()) = Some(RetryContext { tx, cancel });
        RetryEventsGuard { events: self.clone() }
    }

    async fn notify(&self, notice: RetryNotice) {
        let tx = self
            .inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map(|context| context.tx.clone());
        if let Some(tx) = tx {
            let _ = tx.send(notice).await;
        }
    }

    fn cancellation(&self) -> Option<tokio_util::sync::CancellationToken> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .and_then(|context| context.cancel.clone())
    }
}

fn is_retryable_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<ProviderError>()
        .is_some_and(ProviderError::is_retryable)
        || error.downcast_ref::<reqwest::Error>().is_some()
}

/// port ModelProvider → rig CompletionModel。
///
/// 把 rig 结构化 chat_history 拍平为 port canonical 编码，调用底层 port
/// provider；因此 ScriptedProvider 与真实 provider 在两个引擎下看到完全相同的
/// 文本历史。
pub struct PortModelAsRig {
    pub(crate) inner: Arc<dyn ModelProvider>,
    pub(crate) model: String,
    retry_events: RetryEvents,
}

impl PortModelAsRig {
    pub fn new(inner: Arc<dyn ModelProvider>, model: String) -> Self {
        Self {
            inner,
            model,
            retry_events: RetryEvents::default(),
        }
    }

    pub(crate) fn with_retry_events(mut self, retry_events: RetryEvents) -> Self {
        self.retry_events = retry_events;
        self
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
pub(crate) fn rig_messages_to_port(messages: &[Message]) -> Result<Vec<ChatMessage>, MediaError> {
    let mut out = Vec::new();
    for message in messages {
        match message {
            Message::System { content } => out.push(ChatMessage::system(content.clone())),
            Message::User { content } => {
                for block in content {
                    match block {
                        UserContent::Text(t) => out.push(ChatMessage::user(t.text.clone())),
                        UserContent::ToolResult(tool_result) => {
                            let mut message = ChatMessage::tool(
                                serde_json::json!({
                                    "tool_call_id": tool_result.call.to_string(),
                                    "content": tool_result_text(tool_result),
                                })
                                .to_string(),
                            );
                            message.attachments = tool_result_attachments(tool_result)?;
                            out.push(message);
                        }
                        UserContent::Image(image) => {
                            let mut message = ChatMessage::user("");
                            let rig_result = RigToolResult {
                                call: ToolCallId::new_or_mint("image"),
                                provider: None,
                                name: String::new(),
                                content: vec![ToolResultContent::Image(image.clone())],
                            };
                            message.attachments = tool_result_attachments(&rig_result)?;
                            out.push(message);
                        }
                        UserContent::Audio(_) | UserContent::Video(_) | UserContent::Document(_) => {}
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
    Ok(out)
}

// ── port → rig 复原 ─────────────────────────────────────────

fn port_tool_message_to_rig(message: &ChatMessage) -> Result<Message, MediaError> {
    let content = &message.content;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        let id = v.get("tool_call_id").and_then(|x| x.as_str());
        let content_field = v.get("content");
        if let (Some(id), Some(content_field)) = (id, content_field) {
            let text = content_field
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| content_field.to_string());
            let mut blocks = vec![ToolResultContent::Text(Text::new(text))];
            for attachment in &message.attachments {
                blocks.push(media_attachment_to_rig(attachment)?);
            }
            return Ok(Message::User {
                content: vec![UserContent::ToolResult(RigToolResult {
                    call: ToolCallId::new_or_mint(id),
                    provider: None,
                    // name 不在 canonical 编码内；回程拍平不读取该字段。
                    name: String::new(),
                    content: blocks,
                })],
            });
        }
    }
    user_message_with_attachments(content, &message.attachments)
}

fn user_message_with_attachments(content: &str, attachments: &[MediaAttachment]) -> Result<Message, MediaError> {
    if attachments.is_empty() {
        return Ok(Message::user(content));
    }
    let mut blocks = vec![UserContent::Text(Text::new(content))];
    for attachment in attachments {
        let ToolResultContent::Image(image) = media_attachment_to_rig(attachment)? else {
            return Err(MediaError::InvalidImage);
        };
        blocks.push(UserContent::Image(image));
    }
    Ok(Message::User { content: blocks })
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
pub(crate) fn port_messages_to_rig(messages: &[ChatMessage]) -> Result<Vec<Message>, MediaError> {
    let mut out = Vec::new();
    for message in messages {
        if message.attachments.len() > 1 {
            return Err(MediaError::TooManyImages);
        }
        let converted = match message.role.as_str() {
            "system" | "assistant" if !message.attachments.is_empty() => {
                return Err(MediaError::Validation(
                    "attachments require a user or tool message".into(),
                ));
            }
            "system" => Message::system(message.content.clone()),
            "tool" => port_tool_message_to_rig(message)?,
            "assistant" => port_assistant_message_to_rig(&message.content),
            _ => user_message_with_attachments(&message.content, &message.attachments)?,
        };
        out.push(converted);
    }
    Ok(out)
}

fn port_messages_to_model_rig(messages: &[ChatMessage]) -> Result<Vec<Message>, MediaError> {
    let mut converted = port_messages_to_rig(messages)?;
    for message in &mut converted {
        let Message::User { content } = message else {
            continue;
        };
        for block in content {
            match block {
                UserContent::Image(image) => image.additional_params = None,
                UserContent::ToolResult(result) => {
                    for block in &mut result.content {
                        if let ToolResultContent::Image(image) = block {
                            image.additional_params = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(converted)
}

/// 历史回放前的工具配对清洗（2026-09-16 实测全灭事故）：thing-agent 全部
/// run 共享会话 "default"，recall 窗口（20 条）一旦切在 tool_call /
/// tool_result 配对中间，回放历史就带孤儿引用——严格校验配对的 provider
/// （MiniMax: "tool result's tool id not found" 400）会拒绝整轮，此后
/// 每次 run 都在 turn 1 失败。
///
/// 两个方向都清：
/// - 孤儿 tool_result：call id 不在窗口内的任何 assistant tool_calls 里 → 丢
/// - 孤儿 tool_call：窗口内没有对应 tool_result → 从 assistant 消息里摘除
///   （消息因此无任何内容块时整条丢弃）
pub(crate) fn sanitize_tool_pairing(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    use std::collections::HashSet;

    let mut call_ids = HashSet::new();
    let mut result_ids = HashSet::new();
    for m in &messages {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&m.content) else {
            continue; // 纯文本消息无配对约束
        };
        match m.role.as_str() {
            "assistant" => {
                if let Some(calls) = v.get("tool_calls").and_then(|x| x.as_array()) {
                    for c in calls {
                        if let Some(id) = c.get("id").and_then(|x| x.as_str()) {
                            call_ids.insert(id.to_string());
                        }
                    }
                }
            }
            "tool" => {
                if let Some(id) = v.get("tool_call_id").and_then(|x| x.as_str()) {
                    result_ids.insert(id.to_string());
                }
            }
            _ => {}
        }
    }

    messages
        .into_iter()
        .filter_map(|mut m| match m.role.as_str() {
            "tool" => {
                let keep = serde_json::from_str::<serde_json::Value>(&m.content)
                    .ok()
                    .and_then(|v| v.get("tool_call_id").and_then(|x| x.as_str()).map(str::to_string))
                    .is_some_and(|id| call_ids.contains(&id));
                keep.then_some(m) // 非 canonical 的 tool 消息一并丢弃（无法校验配对）
            }
            "assistant" => {
                let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&m.content) else {
                    return Some(m); // 纯文本 assistant 原样保留
                };
                if let Some(calls) = v.get("tool_calls").and_then(|x| x.as_array()) {
                    let kept: Vec<serde_json::Value> = calls
                        .iter()
                        .filter(|c| {
                            c.get("id")
                                .and_then(|x| x.as_str())
                                .is_some_and(|id| result_ids.contains(id))
                        })
                        .cloned()
                        .collect();
                    v["tool_calls"] = serde_json::Value::Array(kept);
                }
                let has_text = v.get("content").and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty());
                let has_calls = v
                    .get("tool_calls")
                    .and_then(|x| x.as_array())
                    .is_some_and(|a| !a.is_empty());
                let has_reasoning = v
                    .get("reasoning_content")
                    .and_then(|x| x.as_str())
                    .is_some_and(|s| !s.is_empty());
                if has_text || has_calls || has_reasoning {
                    m.content = v.to_string();
                    Some(m)
                } else {
                    None // 只剩孤儿 call 的空壳消息整条丢弃
                }
            }
            _ => Some(m),
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
    fn supports_vision(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        check_vision_request(self, request.messages)?;
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
            chat_history: port_messages_to_model_rig(request.messages)?,
            documents: vec![],
            tools,
            temperature,
            max_tokens: request.max_tokens,
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
        messages.extend(
            rig_messages_to_port(&request.chat_history)
                .map_err(|error| CompletionError::ProviderError(error.to_string()))?,
        );
        check_vision_request(self.inner.as_ref(), &messages)
            .map_err(|error| CompletionError::ProviderError(error.to_string()))?;

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
            max_tokens: request.max_tokens,
        };
        let resp = self
            .inner
            .chat(port_request, &self.model, request.temperature)
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;
        Ok(port_response_to_rig(resp))
    }

    /// True streaming: provider deltas are forwarded to the observer as they
    /// arrive, then the aggregated response becomes the terminal rig record.
    async fn stream(&self, request: CompletionRequest) -> Result<StreamingCompletionResponse, CompletionError> {
        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(preamble) = &request.preamble {
            messages.push(ChatMessage::system(preamble.clone()));
        }
        messages.extend(
            rig_messages_to_port(&request.chat_history)
                .map_err(|error| CompletionError::ProviderError(error.to_string()))?,
        );
        check_vision_request(self.inner.as_ref(), &messages)
            .map_err(|error| CompletionError::ProviderError(error.to_string()))?;

        let tool_specs: Vec<crate::port::tool::ToolSpec> = request
            .tools
            .iter()
            .map(|t| crate::port::tool::ToolSpec {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect();
        let provider = self.provider_name();
        let inner = self.inner.clone();
        let model = self.model.clone();
        let temperature = request.temperature;
        let max_tokens = request.max_tokens;
        let stream_provider = provider.clone();
        let retry_events = self.retry_events.clone();
        let call_cancel = retry_events.cancellation();
        let output = async_stream::stream! {
            let (delta_tx, mut delta_rx) =
                tokio::sync::mpsc::unbounded_channel::<ProviderStreamEvent>();
            let call = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                let port_request = ChatRequest {
                    messages: &messages,
                    tools: if tool_specs.is_empty() { None } else { Some(&tool_specs) },
                    max_tokens,
                };
                let mut retries = 0;
                loop {
                    let on_delta = |delta: crate::port::provider::ChatStreamDelta| {
                        let _ = delta_tx.send(ProviderStreamEvent::Delta(delta));
                    };
                    let result = if let Some(token) = call_cancel.clone() {
                        tokio::select! {
                            result = inner.chat_stream(port_request, &model, temperature, &on_delta) => result,
                            _ = token.cancelled() => {
                                Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled))
                            }
                        }
                    } else {
                        inner.chat_stream(port_request, &model, temperature, &on_delta).await
                    };
                    let Err(error) = result else {
                        return result;
                    };
                    if retries >= MAX_MODEL_RETRIES || !is_retryable_error(&error) {
                        return Err(error);
                    }
                    let cancel = call_cancel.clone();
                    if cancel.as_ref().is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
                        return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                    }
                    retries += 1;
                    let delay_ms = 1000_u64 << (retries - 1);
                    let notice = RetryNotice {
                        attempt: retries,
                        max_retries: MAX_MODEL_RETRIES,
                        delay_ms,
                        error: error.to_string(),
                    };
                    let _ = delta_tx.send(ProviderStreamEvent::Retry(notice));
                    let delay = tokio::time::sleep(std::time::Duration::from_millis(delay_ms));
                    tokio::pin!(delay);
                    if let Some(token) = cancel {
                        tokio::select! {
                            _ = &mut delay => {}
                            _ = token.cancelled() => {
                                return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                            }
                        }
                    } else {
                        delay.await;
                    }
                }
            }));

            // Retry notices share the delta channel so they cannot overtake
            // partial output from the failed attempt.
            while let Some(event) = delta_rx.recv().await {
                match event {
                    ProviderStreamEvent::Delta(delta) if delta.reasoning => {
                        yield Ok(RawStreamingChoice::ReasoningDelta {
                            id: StreamPartId::wire("reasoning"),
                            provider_id: None,
                            reasoning: delta.text,
                        });
                    }
                    ProviderStreamEvent::Delta(delta) => {
                        yield Ok(RawStreamingChoice::Message(delta.text));
                    }
                    ProviderStreamEvent::Retry(notice) => retry_events.notify(notice).await,
                }
            }

            match call.await {
                Ok(Ok(response)) => {
                    let resp = port_response_to_rig(response);
                    for block in &resp.choice {
                        if let AssistantContent::ToolCall(tc) = block {
                            yield Ok(RawStreamingChoice::ToolCall(RawStreamingToolCall::new(
                                StreamPartId::wire(tc.id.to_string()),
                                tc.function.name.clone(),
                                tc.function.arguments.clone(),
                            )));
                        }
                    }
                    yield Ok(RawStreamingChoice::FinalResponse(StreamFinal::new(
                        stream_provider.clone(),
                        resp.usage,
                    )));
                }
                Ok(Err(error)) => {
                    yield Err(CompletionError::ProviderError(error.to_string()));
                }
                Err(error) => {
                    yield Err(CompletionError::ProviderError(error.to_string()));
                }
            }
        };
        Ok(StreamingCompletionResponse::stream(provider, Box::pin(output)))
    }
}

// ── OpenAI-compatible streaming provider ─────────────────────────────
//
// The rig adapter historically called `completion()` and split the full
// response into one chunk, so no tokens ever reached the UI. This provider
// calls the provider's real SSE endpoint (`stream: true`) and emits text /
// reasoning deltas immediately, then returns the aggregated ChatResponse.

/// Convert the canonical port history into OpenAI chat-completions messages.
/// In particular, assistant tool calls must be structured `tool_calls`, and
/// tool results must carry the matching `tool_call_id`; sending the canonical
/// JSON as plain text makes MiniMax reject the next request.
fn openai_message(message: &ChatMessage) -> serde_json::Value {
    match message.role.as_str() {
        "assistant" => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content)
                && value.is_object()
                && (value.get("content").is_some()
                    || value.get("tool_calls").is_some()
                    || value.get("reasoning_content").is_some())
            {
                let mut out = serde_json::json!({
                    "role": "assistant",
                    "content": value.get("content").cloned().unwrap_or(serde_json::Value::Null),
                });
                if let Some(calls) = value.get("tool_calls").and_then(serde_json::Value::as_array) {
                    let calls: Vec<serde_json::Value> = calls
                        .iter()
                        .filter_map(|call| {
                            let function = call.get("function");
                            let name = function
                                .and_then(|function| function.get("name"))
                                .or_else(|| call.get("name"))
                                .and_then(serde_json::Value::as_str)?;
                            let arguments = function
                                .and_then(|function| function.get("arguments"))
                                .or_else(|| call.get("arguments"))
                                .map(|arguments| {
                                    arguments
                                        .as_str()
                                        .map(str::to_string)
                                        .unwrap_or_else(|| arguments.to_string())
                                })
                                .unwrap_or_default();
                            Some(serde_json::json!({
                                "id": call.get("id").and_then(serde_json::Value::as_str).unwrap_or(""),
                                "type": "function",
                                "function": {"name": name, "arguments": arguments},
                            }))
                        })
                        .collect();
                    if !calls.is_empty() {
                        out["tool_calls"] = serde_json::Value::Array(calls);
                    }
                }
                if let Some(reasoning) = value.get("reasoning_content") {
                    out["reasoning_content"] = reasoning.clone();
                }
                return out;
            }
            serde_json::json!({"role": "assistant", "content": message.content})
        }
        "tool" => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content)
                && let Some(id) = value.get("tool_call_id").and_then(serde_json::Value::as_str)
            {
                let content = value
                    .get("content")
                    .map(|content| {
                        content
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| content.to_string())
                    })
                    .unwrap_or_default();
                return serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": content,
                });
            }
            serde_json::json!({"role": "tool", "content": message.content})
        }
        _ => serde_json::json!({"role": message.role, "content": message.content}),
    }
}

#[derive(Debug, Clone)]
pub struct StreamingOpenAiPort {
    base_url: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl StreamingOpenAiPort {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model: crate::config::default_model(),
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl crate::port::attribution::Attributable for StreamingOpenAiPort {
    fn role(&self) -> crate::port::attribution::Role {
        crate::port::attribution::Role::Provider(crate::port::attribution::ProviderKind::Model(
            crate::port::attribution::ModelProviderKind::Minimax,
        ))
    }
    fn alias(&self) -> &str {
        "minimax"
    }
}

#[async_trait::async_trait]
impl ModelProvider for StreamingOpenAiPort {
    fn supports_vision(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        // Non-streaming callers (evals) still get a complete response via the
        // same SSE endpoint, aggregated below; no separate code path.
        self.chat_stream(request, model, temperature, &|_| {}).await
    }

    async fn chat_stream(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
        on_delta: &(dyn Fn(crate::port::provider::ChatStreamDelta) + Send + Sync),
    ) -> anyhow::Result<crate::port::provider::ChatResponse> {
        let model_name = if model.is_empty() { self.model.as_str() } else { model };
        let url = format!("{}/chat/completions", self.base_url);
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|message| {
                let mut out = openai_message(message);
                if !message.attachments.is_empty() {
                    let content: Vec<serde_json::Value> = std::iter::once(serde_json::json!({
                        "type": "text",
                        "text": message.content,
                    }))
                    .chain(message.attachments.iter().map(|attachment| {
                        let data = format!(
                            "data:{};base64,{}",
                            attachment.mime,
                            base64::engine::general_purpose::STANDARD.encode(&attachment.bytes)
                        );
                        serde_json::json!({ "type": "image_url", "image_url": { "url": data } })
                    }))
                    .collect();
                    out["content"] = serde_json::Value::Array(content);
                }
                out
            })
            .collect();
        let tools: Vec<serde_json::Value> = request
            .tools
            .unwrap_or(&[])
            .iter()
            .map(|tool| {
                serde_json::json!({ "type": "function", "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }})
            })
            .collect();
        let mut body = serde_json::json!({
            "model": model_name,
            "messages": messages,
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools);
            body["tool_choice"] = serde_json::json!("auto");
        }
        if let Some(temperature) = temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Http {
                status: status.as_u16(),
                message: text.chars().take(300).collect::<String>(),
            }
            .into());
        }

        let mut bytes = response.bytes_stream();
        let mut buffer = String::new();
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<PortToolCall> = Vec::new();
        let mut usage: Option<crate::port::provider::TokenUsage> = None;
        while let Some(chunk) = futures::StreamExt::next(&mut bytes).await {
            buffer.push_str(&String::from_utf8_lossy(&chunk?));
            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim_end_matches('\r').to_string();
                buffer = buffer[index + 1..].to_string();
                let data = match line.strip_prefix("data:") {
                    Some(data) => data.trim(),
                    None => continue,
                };
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(payload) = serde_json::from_str::<serde_json::Value>(data) else {
                    continue;
                };
                if let Some(error) = payload.get("error") {
                    let message = error
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| error.to_string());
                    return Err(ProviderError::Stream {
                        code: error
                            .get("code")
                            .or_else(|| error.get("type"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                        message: message.chars().take(300).collect::<String>(),
                    }
                    .into());
                }
                if let Some(u) = payload.get("usage") {
                    usage = Some(crate::port::provider::TokenUsage {
                        input_tokens: u.get("prompt_tokens").and_then(serde_json::Value::as_u64),
                        output_tokens: u.get("completion_tokens").and_then(serde_json::Value::as_u64),
                        cached_input_tokens: u.get("cached_tokens").and_then(serde_json::Value::as_u64),
                    });
                }
                for choice in payload
                    .get("choices")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let delta = choice.get("delta").cloned().unwrap_or(serde_json::json!({}));
                    if let Some(t) = delta.get("content").and_then(serde_json::Value::as_str) {
                        text.push_str(t);
                        on_delta(crate::port::provider::ChatStreamDelta {
                            text: t.to_string(),
                            reasoning: false,
                        });
                    }
                    if let Some(r) = delta
                        .get("reasoning_content")
                        .or_else(|| delta.get("reasoning"))
                        .and_then(serde_json::Value::as_str)
                    {
                        reasoning.push_str(r);
                        on_delta(crate::port::provider::ChatStreamDelta {
                            text: r.to_string(),
                            reasoning: true,
                        });
                    }
                    if let Some(calls) = delta.get("tool_calls").and_then(serde_json::Value::as_array) {
                        for call in calls {
                            let index = call.get("index").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
                            while tool_calls.len() <= index {
                                tool_calls.push(PortToolCall {
                                    id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                });
                            }
                            if let Some(id) = call.get("id").and_then(serde_json::Value::as_str) {
                                tool_calls[index].id = id.to_string();
                            }
                            if let Some(function) = call.get("function") {
                                if let Some(name) = function.get("name").and_then(serde_json::Value::as_str) {
                                    tool_calls[index].name = name.to_string();
                                }
                                if let Some(arguments) = function.get("arguments").and_then(serde_json::Value::as_str) {
                                    tool_calls[index].arguments.push_str(arguments);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Some providers omit tool call ids on the streamed deltas.
        for call in &mut tool_calls {
            if call.id.is_empty() {
                call.id = uuid::Uuid::new_v4().to_string();
            }
        }
        Ok(crate::port::provider::ChatResponse {
            text: if text.is_empty() { None } else { Some(text) },
            tool_calls,
            usage,
            reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
        })
    }
}
