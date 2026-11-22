//! port AgentLoop → rig Agent 桥 + rig loop 工厂。

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use futures::StreamExt;
use rig_agent::agent::hook::{ToolCall as ToolCallEvent, ToolResultEvent};
use rig_agent::agent::{
    Agent, AgentBuilder, AgentHook, CompletionCallAction, CompletionCallEvent, HookContext, InvalidToolCallAction,
    InvalidToolCallContext, MultiTurnStreamItem, RequestPatch, ToolCallAction, ToolResultAction,
};
use rig_agent::completion::{Document, Prompt};
use rig_agent::streaming::StreamingPrompt;
use rig_core::streaming::{StreamedAssistantContent, StreamedUserContent};
use sha2::Digest;

use crate::harness::{GuardDecision, ToolGuard};
use crate::port::events::TurnEvent;
use crate::port::runtime::{AgentLoop, AgentLoopConfig, AgentLoopHandle, SteeringBuffer};

use super::memory::PortMemoryAsConversation;
use super::provider::{PortModelAsRig, port_messages_to_rig};
use super::tools::to_dynamic_tool;

/// 固定会话 id：现状 zeroclaw 各 turn 独立、无显式会话（Global Constraints #6）。
const CONVERSATION_ID: &str = "default";
const MAX_SEEDED_HISTORY_MESSAGES: usize = 120;

fn compact_seeded_messages(
    mut messages: Vec<crate::port::provider::ChatMessage>,
) -> Vec<crate::port::provider::ChatMessage> {
    if messages.len() <= MAX_SEEDED_HISTORY_MESSAGES {
        return messages;
    }
    let dropped = messages.len() - MAX_SEEDED_HISTORY_MESSAGES;
    let mut kept = messages.split_off(dropped);
    kept.insert(
        0,
        crate::port::provider::ChatMessage {
            role: "user".into(),
            content: format!("[earlier conversation compacted: {dropped} messages omitted]"),
            attachments: Vec::new(),
        },
    );
    kept
}

/// port AgentLoop 的 rig 引擎实现。
///
/// `conversation_memory=false`（chat/heartbeat 路径）时 rig 不配
/// ConversationMemory（无自动 load/append，避免与每轮 DB 重建重复累积），
/// seed_history 改为注入内部缓冲、turn_streamed/run_single 经
/// `.history(...)` 逐轮传入；`true`（thing_agent 自治路径）时照旧走
/// port Memory 承载的会话内存。
/// Public tool arguments must fail closed: an unknown or hidden tool name gets
/// the default redaction policy rather than passing raw arguments through.
fn redact_public_args(
    redactions: &std::collections::HashMap<String, tinyiothub_skills::registry::RedactionPolicy>,
    tool_name: &str,
    args: &serde_json::Value,
) -> serde_json::Value {
    redactions.get(tool_name).cloned().unwrap_or_default().redact(args)
}

pub struct RigAgentLoop {
    agent: Agent,
    memory: Option<Arc<PortMemoryAsConversation>>,
    seeded: tokio::sync::Mutex<Vec<rig_core::message::Message>>,
    /// Per-tool redaction policies for public `TurnEvent` arguments. Raw
    /// arguments are still what the tool receives; only the observable event
    /// carries the redacted summary.
    redactions: std::collections::HashMap<String, tinyiothub_skills::registry::RedactionPolicy>,
    /// Provider tool-call id of the call about to execute. Filled by the
    /// pre-execution hook and read by tools that freeze a durable identity.
    tool_call_ids: Option<crate::port::tool::ToolCallIdSlot>,
    checkpoint_before_tool: bool,
    retry_events: super::provider::RetryEvents,
}

impl RigAgentLoop {
    pub fn new(agent: Agent, memory: Option<Arc<PortMemoryAsConversation>>) -> Self {
        Self {
            agent,
            memory,
            seeded: tokio::sync::Mutex::new(Vec::new()),
            redactions: std::collections::HashMap::new(),
            tool_call_ids: None,
            checkpoint_before_tool: false,
            retry_events: super::provider::RetryEvents::default(),
        }
    }

    pub fn with_tool_call_ids(mut self, slot: Option<crate::port::tool::ToolCallIdSlot>) -> Self {
        self.tool_call_ids = slot;
        self
    }

    fn with_retry_events(mut self, retry_events: super::provider::RetryEvents) -> Self {
        self.retry_events = retry_events;
        self
    }

    pub fn with_checkpoints(mut self, enabled: bool) -> Self {
        self.checkpoint_before_tool = enabled;
        self
    }

    pub fn with_redactions(
        mut self,
        redactions: std::collections::HashMap<String, tinyiothub_skills::registry::RedactionPolicy>,
    ) -> Self {
        self.redactions = redactions;
        self
    }

    fn public_tool_args(&self, tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
        redact_public_args(&self.redactions, tool_name, args)
    }

    /// conversation_memory=false 时的每轮重建历史（seed_history 注入）。
    async fn seeded_history(&self) -> Vec<rig_core::message::Message> {
        if self.memory.is_some() {
            Vec::new()
        } else {
            self.seeded.lock().await.clone()
        }
    }
}

/// Turn 运行 hook：处理外部取消、steering 与媒体约束。
struct BudgetHook {
    cancel: Option<CancellationToken>,
    steering: Option<SteeringBuffer>,
    media: tokio::sync::Mutex<RunMediaAccountant>,
    tool_call_ids: Option<crate::port::tool::ToolCallIdSlot>,
}

fn take_steering_patch(steering: &SteeringBuffer) -> Option<RequestPatch> {
    let facts = {
        let mut queued = steering.lock().unwrap_or_else(|error| error.into_inner());
        if queued.is_empty() {
            return None;
        }
        std::mem::take(&mut *queued)
    };
    Some(
        RequestPatch::new().extra_context(facts.into_iter().enumerate().map(|(index, text)| Document {
            id: format!("task-wake-{index}"),
            text,
            additional_props: Default::default(),
        })),
    )
}

impl BudgetHook {
    fn action(&self) -> ToolCallAction {
        if self.cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            ToolCallAction::stop("run cancelled")
        } else {
            ToolCallAction::Run
        }
    }
}

#[derive(Default)]
struct RunMediaBudget {
    image_bytes: usize,
}

impl RunMediaBudget {
    fn record(&mut self, bytes: usize) -> Result<(), tinyiothub_core::media::MediaError> {
        let next = self.image_bytes.saturating_add(bytes);
        if next > 8 * 1024 * 1024 {
            return Err(tinyiothub_core::media::MediaError::RunImageBudget);
        }
        self.image_bytes = next;
        Ok(())
    }
}

#[derive(Default)]
struct RunMediaAccountant {
    budget: RunMediaBudget,
    seen: std::collections::HashSet<String>,
}

impl RunMediaAccountant {
    fn check(
        &mut self,
        prompt: &rig_core::message::Message,
        history: &[rig_core::message::Message],
    ) -> Result<(), tinyiothub_core::media::MediaError> {
        for message in history.iter().chain(std::iter::once(prompt)) {
            let rig_core::message::Message::User { content } = message else {
                continue;
            };
            for block in content {
                match block {
                    rig_core::message::UserContent::Image(image) => self.record_image(image)?,
                    rig_core::message::UserContent::ToolResult(result) => {
                        for block in &result.content {
                            if let rig_core::message::ToolResultContent::Image(image) = block {
                                self.record_image(image)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn record_image(&mut self, image: &rig_core::message::Image) -> Result<(), tinyiothub_core::media::MediaError> {
        let rig_core::message::DocumentSourceKind::Raw(bytes) = &image.data else {
            return Err(tinyiothub_core::media::MediaError::InvalidImage);
        };
        let identity = image
            .additional_params
            .as_ref()
            .and_then(|params| params.get("tinyiothub_evidence"))
            .and_then(|reference| reference.get("evidenceId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| hex::encode(sha2::Sha256::digest(bytes)));
        if self.seen.contains(&identity) {
            return Ok(());
        }
        self.budget.record(bytes.len())?;
        self.seen.insert(identity);
        Ok(())
    }
}

impl AgentHook for BudgetHook {
    async fn on_completion_call(&self, _ctx: &HookContext, event: CompletionCallEvent<'_>) -> CompletionCallAction {
        let error = self.media.lock().await.check(event.prompt, event.history).err();
        if let Some(error) = error {
            return CompletionCallAction::stop(error.to_string());
        }
        self.steering
            .as_ref()
            .and_then(take_steering_patch)
            .map_or_else(CompletionCallAction::continue_run, CompletionCallAction::patch)
    }

    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCallEvent<'_>) -> ToolCallAction {
        if let Some(slot) = &self.tool_call_ids {
            let id = event.tool_call_id.unwrap_or(event.internal_call_id).to_string();
            *slot.lock().unwrap_or_else(|error| error.into_inner()) = Some(id);
        }
        self.action()
    }
}

/// Waits for the runner to persist `tool/checkpoint` before a tool body runs.
struct ToolCheckpointHook {
    events: mpsc::Sender<TurnEvent>,
}

impl AgentHook for ToolCheckpointHook {
    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCallEvent<'_>) -> ToolCallAction {
        let id = event.tool_call_id.unwrap_or(event.internal_call_id).to_string();
        let ack = crate::port::events::ToolCheckpointAck::new();
        if self
            .events
            .send(TurnEvent::ToolCheckpoint {
                id,
                name: event.tool_name.to_string(),
                ack: ack.clone(),
            })
            .await
            .is_err()
        {
            return ToolCallAction::Stop("tool checkpoint channel closed".into());
        }
        match tokio::time::timeout(std::time::Duration::from_secs(5), ack.wait()).await {
            Ok(true) => ToolCallAction::Run,
            Ok(false) => ToolCallAction::Stop("tool checkpoint was not persisted".into()),
            Err(_) => ToolCallAction::Stop("tool checkpoint timed out".into()),
        }
    }
}

/// Enforces the harness tool-call budget and semantic repeat guard before
/// every dispatch. It is intentionally a hook, not a tool wrapper: tool
/// behavior, approval, and Effect handling stay with the tool layer.
struct ToolGuardHook {
    guard: Arc<std::sync::Mutex<ToolGuard>>,
}

impl AgentHook for ToolGuardHook {
    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCallEvent<'_>) -> ToolCallAction {
        let decision = self
            .guard
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .before(event.tool_name, event.args);
        match decision {
            GuardDecision::Run => ToolCallAction::Run,
            GuardDecision::Skip(message) => ToolCallAction::Skip(message),
            GuardDecision::Stop(message) => ToolCallAction::Stop(message),
        }
    }

    async fn on_tool_result(&self, _ctx: &HookContext, event: ToolResultEvent<'_>) -> ToolResultAction {
        let Some(message) = failed_result_message(&event) else {
            return ToolResultAction::Keep;
        };
        let decision =
            self.guard
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .after(event.tool_name, event.args, &message);
        match decision {
            Some(GuardDecision::Stop(reason)) => ToolResultAction::Stop(reason),
            _ => ToolResultAction::Keep,
        }
    }
}

/// The port returns ordinary `Ok(ToolResult { success: false })` values for
/// most device errors, so inspect both the rig error channel and the rendered
/// JSON result. Successful outputs are never classified as failures.
fn failed_result_message(event: &ToolResultEvent<'_>) -> Option<String> {
    if let Some(error) = event.raw_result.error() {
        return Some(error.message().to_string());
    }
    let text = event
        .raw_result
        .output()
        .as_content()
        .iter()
        .filter_map(|block| match block {
            rig_core::message::ToolResultContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (text.contains("\"success\":false") || text.contains("\"error\":")).then_some(text)
}

/// rig 默认把模型调用未暴露的工具当致命错误（`PromptError::UnknownToolCall`）
/// 终结整轮。模型在长工具链里偶尔会臆造旧工具名或新名字——一次臆造就让
/// 整个 Run 失败、Task 变 Failed（2026-11-02 实测：`read_property`）。
/// 这里修复已知旧别名，其余未知调用改为跳过并回传纠正反馈，让模型自我纠正。
struct InvalidToolCallRecovery;

/// 旧工具名 → E2 现行工具名（只映射语义等价、参数兼容的只读别名）。
const LEGACY_TOOL_ALIASES: &[(&str, &str)] = &[
    ("list_things", "list_devices"),
    ("get_thing", "read_device"),
    ("get_thing_profile", "read_device"),
    ("read_property", "read_device"),
];

impl AgentHook for InvalidToolCallRecovery {
    async fn on_invalid_tool_call(
        &self,
        _ctx: &HookContext,
        event: &InvalidToolCallContext,
    ) -> Option<InvalidToolCallAction> {
        if let Some((_, current)) = LEGACY_TOOL_ALIASES
            .iter()
            .find(|(legacy, _)| *legacy == event.tool_name.as_str())
            && event.available_tools.iter().any(|tool| tool == current)
        {
            tracing::info!(
                emitted = %event.tool_name,
                repaired = %current,
                "legacy tool name repaired"
            );
            return Some(InvalidToolCallAction::repair(*current));
        }
        tracing::warn!(
            tool = %event.tool_name,
            available = ?event.available_tools,
            "unknown tool call skipped"
        );
        Some(InvalidToolCallAction::skip(format!(
            "未知工具 `{}`，本次不可用。可用工具：{}。请只用可用工具重新调用，不要编造工具名。",
            event.tool_name,
            event.available_tools.join(", ")
        )))
    }
}

fn map_stream_error(error: rig_agent::agent::StreamingError, cancel: Option<&CancellationToken>) -> anyhow::Error {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return anyhow::Error::new(crate::port::outcome::ToolLoopCancelled);
    }
    anyhow::anyhow!(error.to_string())
}

#[async_trait::async_trait]
impl AgentLoop for RigAgentLoop {
    async fn turn_streamed(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<TurnEvent>,
        cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        self.turn_streamed_with_steering(user_message, event_tx, cancel_token, None)
            .await
    }

    async fn turn_streamed_with_steering(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<TurnEvent>,
        cancel_token: Option<CancellationToken>,
        steering: Option<SteeringBuffer>,
    ) -> anyhow::Result<String> {
        let (retry_tx, mut retry_rx) = mpsc::channel(8);
        let _retry_guard = self.retry_events.install(retry_tx, cancel_token.clone());
        let mut request = self
            .agent
            .stream_prompt(user_message.to_string())
            .max_turns(usize::MAX)
            .tool_concurrency(1)
            .add_hook(BudgetHook {
                cancel: cancel_token.clone(),
                steering,
                media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
                tool_call_ids: self.tool_call_ids.clone(),
            })
            .add_hook(InvalidToolCallRecovery);
        if self.checkpoint_before_tool {
            request = request.add_hook(ToolCheckpointHook {
                events: event_tx.clone(),
            });
        }
        let mut request = request.add_hook(ToolGuardHook {
            guard: Arc::new(std::sync::Mutex::new(ToolGuard::default())),
        });
        // conversation_memory=false：历史由 cloud DB 每轮 seed，经 .history() 传入。
        let seeded = self.seeded_history().await;
        if !seeded.is_empty() {
            request = request.history(seeded);
        }
        let mut stream = request.await;

        let mut final_text = String::new();
        let mut current_model_text = String::new();
        let cancelled = cancel_token.clone();
        loop {
            tokio::select! {
                _ = async {
                    match &cancelled {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    // rig 取消语义 = drop stream（Task 1 spike 已验证）。
                    drop(stream);
                    return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                }
                notice = retry_rx.recv() => {
                    let Some(notice) = notice else { continue };
                    final_text.truncate(final_text.len() - current_model_text.len());
                    current_model_text.clear();
                    let _ = event_tx.send(TurnEvent::Retry(notice)).await;
                }
                item = stream.next() => {
                    let Some(item) = item else { break };
                    let item = match item {
                        Ok(item) => item,
                        Err(e) => return Err(map_stream_error(e, cancelled.as_ref())),
                    };
                    match item {
                        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(t)) => {
                            let _ = event_tx.send(TurnEvent::Chunk { delta: t.text.clone() }).await;
                            final_text.push_str(&t.text);
                            current_model_text.push_str(&t.text);
                        }
                        MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ReasoningDelta { reasoning, .. },
                        ) => {
                            let _ = event_tx.send(TurnEvent::Thinking { delta: reasoning }).await;
                        }
                        // 工具调用事件取自 ToolExecutionCommitted（hook 后、每调用恰一次），
                        // 避免与 StreamAssistantItem::ToolCall 双计（runner 按 ToolCall 计数做预算）。
                        MultiTurnStreamItem::ToolExecutionCommitted { tool_call, .. } => {
                            // The tool receives the raw arguments; the public
                            // stream only ever sees the descriptor-redacted view
                            // (nested password/token/secret fields included).
                            let public_args =
                                self.public_tool_args(&tool_call.function.name, &tool_call.function.arguments);
                            let _ = event_tx.send(TurnEvent::ToolCall {
                                id: tool_call.id.to_string(),
                                name: tool_call.function.name.clone(),
                                args: public_args,
                            }).await;
                        }
                        MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
                            tool_result,
                            ..
                        }) => {
                            let output = super::provider::tool_result_text(&tool_result);
                            let attachments = match super::provider::tool_result_attachments(&tool_result) {
                                Ok(attachments) => attachments,
                                Err(error) => return Err(anyhow::Error::new(error)),
                            };
                            let _ = event_tx.send(TurnEvent::ToolResult {
                                id: tool_result.call.to_string(),
                                name: tool_result.name.clone(),
                                output,
                                attachments,
                            }).await;
                        }
                        MultiTurnStreamItem::CompletionCall(call) => {
                            current_model_text.clear();
                            let _ = event_tx.send(TurnEvent::Usage {
                                input_tokens: Some(call.usage.input_tokens),
                                cached_input_tokens: Some(call.usage.cached_input_tokens),
                                output_tokens: Some(call.usage.output_tokens),
                                cost_usd: None,
                            }).await;
                        }
                        MultiTurnStreamItem::FinalResponse(resp) => {
                            final_text = resp.output().to_string();
                            current_model_text.clear();
                        }
                        _ => {}
                    }
                }
            }
        }
        drop(event_tx);
        // I-2 取消竞态：stream 正常结束时若外部 token 已触发，仍归一化为
        // ToolLoopCancelled，不让 runner 把取消误判为 TurnEnd::Text。
        if cancel_token.is_some_and(|t| t.is_cancelled()) {
            return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
        }
        Ok(final_text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        let mut request = self
            .agent
            .prompt(message.to_string())
            .add_hook(BudgetHook {
                cancel: None,
                steering: None,
                media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
                tool_call_ids: self.tool_call_ids.clone(),
            })
            .add_hook(InvalidToolCallRecovery);
        let seeded = self.seeded_history().await;
        if !seeded.is_empty() {
            request = request.history(seeded);
        }
        let text = request.await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(text)
    }

    async fn clear_history(&self) {
        self.seeded.lock().await.clear();
        if let Some(memory) = &self.memory {
            // port Memory 无会话级删除原语（Global Constraints #6），no-op。
            let _ = rig_core::memory::ConversationMemory::clear(memory.as_ref(), CONVERSATION_ID).await;
        }
    }

    async fn seed_history(&self, messages: &[crate::port::provider::ChatMessage]) -> anyhow::Result<()> {
        let compacted = compact_seeded_messages(messages.to_vec());
        let rig_messages = port_messages_to_rig(&compacted)?;
        if let Some(memory) = &self.memory {
            rig_core::memory::ConversationMemory::append(memory.as_ref(), CONVERSATION_ID, rig_messages)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        } else {
            *self.seeded.lock().await = rig_messages;
        }
        Ok(())
    }
}

/// 用 port 零件组一个 rig 引擎的 AgentLoop（参数链对齐
/// `adapters::zeroclaw::loop_::zeroclaw_loop_factory`）。
pub fn rig_loop_factory(cfg: AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> {
    let provider = (cfg.provider_factory)()?;
    let provider_supports_vision = provider.supports_vision();
    let retry_events = super::provider::RetryEvents::default();
    let model = PortModelAsRig::new(Arc::from(provider), cfg.model_name.clone())
        .with_retry_events(retry_events.clone())
        .with_context_budget_required(cfg.require_context_budget);

    // Resolve the capabilities actually available at composition time. Vision
    // comes from the provider port; callers add configuration-derived
    // capabilities (for example a configured video source) explicitly.
    let mut available_capabilities = cfg.available_capabilities.clone();
    if provider_supports_vision && !available_capabilities.contains(&tinyiothub_skills::registry::Capability::Vision) {
        available_capabilities.push(tinyiothub_skills::registry::Capability::Vision);
    }

    // E2：隐藏别名（旧设备工具等）保留注册但绝不进模型工具列表/提示词。
    let exposed_tools: Vec<Box<dyn crate::port::tool::Tool>> = cfg.tools.into_iter().filter(|t| t.exposed()).collect();
    // Missing capability is a hard gate before any model call: never drop the
    // tool silently and never let a non-vision provider invent observations.
    for tool in &exposed_tools {
        let descriptor = tool.descriptor();
        let missing = descriptor.missing_capabilities(&available_capabilities);
        if !missing.is_empty() {
            return Err(anyhow::Error::new(crate::port::runtime::CapabilityUnavailableError {
                tool: descriptor.name,
                missing,
                recovery: "configure a provider/model with those capabilities and start a new run".to_string(),
            }));
        }
    }
    let tool_specs: Vec<crate::port::tool::ToolSpec> = exposed_tools.iter().map(|t| t.spec()).collect();
    let redactions: std::collections::HashMap<String, tinyiothub_skills::registry::RedactionPolicy> = exposed_tools
        .iter()
        .filter_map(|tool| {
            let descriptor = tool.descriptor();
            descriptor.exposed.then_some((descriptor.name, descriptor.redaction))
        })
        .collect();

    let preamble = cfg.prompt_builder.build(&crate::port::prompt::PromptContext {
        workspace_dir: &cfg.workspace_dir,
        agent_workspace_dir: &cfg.workspace_dir,
        model_name: &cfg.model_name,
        tool_specs,
        security_summary: cfg.security_summary.clone(),
    })?;

    let memory = if cfg.conversation_memory {
        Some(Arc::new(PortMemoryAsConversation::new(Arc::clone(&cfg.memory))))
    } else {
        None
    };

    // conversation_memory=false 时不调用 .memory()：rig 无 ConversationMemory
    // 即无自动 load/append，chat/heartbeat 的每轮 DB 重建不会重复累积。
    let mut builder = AgentBuilder::new(model)
        .preamble(&preamble)
        .default_max_turns(usize::MAX);
    if let Some(m) = &memory {
        builder = builder.memory(Arc::clone(m)).conversation(CONVERSATION_ID);
    }

    let agent = if exposed_tools.is_empty() {
        builder.build()
    } else {
        builder
            .dynamic_tools(
                exposed_tools
                    .into_iter()
                    .map(|t| to_dynamic_tool(Arc::from(t)))
                    .collect(),
            )
            .build()
    };

    let handle: AgentLoopHandle = Arc::new(tokio::sync::Mutex::new(
        RigAgentLoop::new(agent, memory)
            .with_redactions(redactions)
            .with_tool_call_ids(cfg.tool_call_ids)
            .with_checkpoints(cfg.checkpoint_before_tool)
            .with_retry_events(retry_events),
    ));
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_history_is_compacted_with_a_marker() {
        let messages = (0..MAX_SEEDED_HISTORY_MESSAGES + 5)
            .map(|index| crate::port::provider::ChatMessage {
                role: "user".into(),
                content: format!("m{index}"),
                attachments: Vec::new(),
            })
            .collect::<Vec<_>>();
        let compacted = compact_seeded_messages(messages);
        assert_eq!(compacted.len(), MAX_SEEDED_HISTORY_MESSAGES + 1);
        assert!(compacted[0].content.contains("5 messages omitted"));
        assert_eq!(compacted[1].content, "m5");
        assert_eq!(compacted.last().unwrap().content, "m124");
    }

    #[test]
    fn run_budget_charges_distinct_evidence_once_across_completions() {
        fn image(id: &str, byte: u8) -> rig_core::message::Message {
            rig_core::message::Message::User {
                content: vec![rig_core::message::UserContent::Image(rig_core::message::Image {
                    data: rig_core::message::DocumentSourceKind::Raw(vec![byte; 2 * 1024 * 1024]),
                    media_type: Some(rig_core::message::ImageMediaType::PNG),
                    detail: None,
                    additional_params: rig_core::message::AdditionalParams::from_entries([(
                        "tinyiothub_evidence",
                        serde_json::json!({
                            "evidenceId": id,
                            "mime": "image/png",
                            "sourceSeq": 1,
                            "mediaTime": "2026-10-27T10:00:00Z"
                        }),
                    )]),
                })],
            }
        }

        let mut run = RunMediaAccountant::default();
        let prompt = rig_core::message::Message::user("inspect");
        let mut history = vec![image("a", 1)];
        run.check(&prompt, &history).expect("A");
        history.push(image("a", 1));
        history.push(image("b", 2));
        run.check(&prompt, &history).expect("repeated A and B");
        history.push(image("c", 3));
        run.check(&prompt, &history).expect("A+B+C");
        history.push(image("d", 4));
        run.check(&prompt, &history).expect("four distinct images");
        history.push(image("e", 5));
        assert!(matches!(
            run.check(&prompt, &history),
            Err(tinyiothub_core::media::MediaError::RunImageBudget)
        ));

        RunMediaAccountant::default()
            .check(&prompt, std::slice::from_ref(&history[0]))
            .expect("a fresh Run has a fresh budget");
    }

    #[test]
    fn queued_task_wake_becomes_next_completion_context_exactly_once() {
        let steering = Arc::new(std::sync::Mutex::new(vec![
            "wake_event_id: event_2\nfacts: continue".into(),
        ]));
        let patch = take_steering_patch(&steering).expect("queued wake patch");
        assert_eq!(patch.extra_context.len(), 1);
        assert_eq!(patch.extra_context[0].text, "wake_event_id: event_2\nfacts: continue");
        assert!(steering.lock().expect("steering").is_empty());
        assert!(take_steering_patch(&steering).is_none());
    }

    #[test]
    fn cancelled_token_stops_before_tool_dispatch() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let hook = BudgetHook {
            cancel: Some(cancel),
            steering: None,
            media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
            tool_call_ids: None,
        };
        assert_eq!(hook.action(), ToolCallAction::Stop("run cancelled".into()));
    }

    #[test]
    fn tool_calls_are_not_capped_by_an_execution_budget() {
        let hook = BudgetHook {
            cancel: None,
            steering: None,
            media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
            tool_call_ids: None,
        };
        for _ in 0..26 {
            assert_eq!(hook.action(), ToolCallAction::Run);
        }
    }

    #[test]
    fn unknown_tool_names_fail_closed_to_default_redaction() {
        let redactions = std::collections::HashMap::new();
        let public = redact_public_args(
            &redactions,
            "not_registered",
            &serde_json::json!({
                "thingId": "camera-1",
                "credentials": {"value": "hunter2"},
                "tokens": ["abc", "def"]
            }),
        );
        let serialized = public.to_string();
        assert!(!serialized.contains("hunter2"), "{serialized}");
        assert!(!serialized.contains("abc"), "{serialized}");
        assert!(!serialized.contains("def"), "{serialized}");
        assert!(serialized.contains("[REDACTED]"), "{serialized}");
    }
}
