//! port AgentLoop → rig Agent 桥 + rig loop 工厂。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use futures::StreamExt;
use rig_agent::agent::hook::ToolCall as ToolCallEvent;
use rig_agent::agent::{
    Agent, AgentBuilder, AgentHook, CompletionCallAction, CompletionCallEvent, HookContext, MultiTurnStreamItem,
    RequestPatch, ToolCallAction,
};
use rig_agent::completion::{Document, Prompt};
use rig_agent::streaming::StreamingPrompt;
use rig_core::streaming::{StreamedAssistantContent, StreamedUserContent};
use sha2::Digest;

use crate::port::events::TurnEvent;
use crate::port::runtime::{AgentLoop, AgentLoopConfig, AgentLoopHandle, MAX_LOOP_TURNS, SteeringBuffer};

use super::memory::PortMemoryAsConversation;
use super::provider::{PortModelAsRig, port_messages_to_rig};
use super::tools::to_dynamic_tool;

/// 固定会话 id：现状 zeroclaw 各 turn 独立、无显式会话（Global Constraints #6）。
const CONVERSATION_ID: &str = "default";

/// turn 内软时长预算；runner 外层还有 timeout 兜底。
const MAX_TURN_DURATION: Duration = Duration::from_secs(300);

/// port AgentLoop 的 rig 引擎实现。
///
/// `conversation_memory=false`（chat/heartbeat 路径）时 rig 不配
/// ConversationMemory（无自动 load/append，避免与每轮 DB 重建重复累积），
/// seed_history 改为注入内部缓冲、turn_streamed/run_single 经
/// `.history(...)` 逐轮传入；`true`（thing_agent 自治路径）时照旧走
/// port Memory 承载的会话内存。
pub struct RigAgentLoop {
    agent: Agent,
    memory: Option<Arc<PortMemoryAsConversation>>,
    seeded: tokio::sync::Mutex<Vec<rig_core::message::Message>>,
}

impl RigAgentLoop {
    pub fn new(agent: Agent, memory: Option<Arc<PortMemoryAsConversation>>) -> Self {
        Self {
            agent,
            memory,
            seeded: tokio::sync::Mutex::new(Vec::new()),
        }
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

/// 工具数预算 hook（第二道防线；主防线是 runner 的 CancellationToken）。
struct BudgetHook {
    budget: usize,
    count: AtomicUsize,
    cancel: Option<CancellationToken>,
    steering: Option<SteeringBuffer>,
    media: tokio::sync::Mutex<RunMediaAccountant>,
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
            return ToolCallAction::stop("run cancelled");
        }
        let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        if n > self.budget {
            if let Some(token) = &self.cancel {
                token.cancel();
            }
            ToolCallAction::stop("tool call budget exceeded")
        } else {
            ToolCallAction::Run
        }
    }
}

#[derive(Default)]
struct RunMediaAccountant {
    budget: tinyiothub_runtime::media::evidence::RunMediaBudget,
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

    async fn on_tool_call(&self, _ctx: &HookContext, _event: ToolCallEvent<'_>) -> ToolCallAction {
        self.action()
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
        let mut request = self
            .agent
            .stream_prompt(user_message.to_string())
            .max_turns(MAX_LOOP_TURNS)
            .tool_concurrency(1)
            .add_hook(BudgetHook {
                budget: MAX_LOOP_TURNS,
                count: AtomicUsize::new(0),
                cancel: cancel_token.clone(),
                steering,
                media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
            });
        // conversation_memory=false：历史由 cloud DB 每轮 seed，经 .history() 传入。
        let seeded = self.seeded_history().await;
        if !seeded.is_empty() {
            request = request.history(seeded);
        }
        let mut stream = request.await;

        let mut final_text = String::new();
        let start = Instant::now();
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
                        }
                        MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ReasoningDelta { reasoning, .. },
                        ) => {
                            let _ = event_tx.send(TurnEvent::Thinking { delta: reasoning }).await;
                        }
                        // 工具调用事件取自 ToolExecutionCommitted（hook 后、每调用恰一次），
                        // 避免与 StreamAssistantItem::ToolCall 双计（runner 按 ToolCall 计数做预算）。
                        MultiTurnStreamItem::ToolExecutionCommitted { tool_call, .. } => {
                            let _ = event_tx.send(TurnEvent::ToolCall {
                                id: tool_call.id.to_string(),
                                name: tool_call.function.name.clone(),
                                args: tool_call.function.arguments.clone(),
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
                            let _ = event_tx.send(TurnEvent::Usage {
                                input_tokens: Some(call.usage.input_tokens),
                                cached_input_tokens: Some(call.usage.cached_input_tokens),
                                output_tokens: Some(call.usage.output_tokens),
                                cost_usd: None,
                            }).await;
                            if start.elapsed() > MAX_TURN_DURATION {
                                drop(stream);
                                return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
                            }
                        }
                        MultiTurnStreamItem::FinalResponse(resp) => {
                            final_text = resp.output().to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
        drop(event_tx);
        // I-2 取消竞态：stream 走 None（正常结束）后若 token 已触发
        // （如 BudgetHook 超预算先 cancel 再 stop），归一化为
        // ToolLoopCancelled，不让 runner 把预算超支误判为 TurnEnd::Text。
        if cancel_token.is_some_and(|t| t.is_cancelled()) {
            return Err(anyhow::Error::new(crate::port::outcome::ToolLoopCancelled));
        }
        Ok(final_text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        let mut request = self.agent.prompt(message.to_string()).add_hook(BudgetHook {
            budget: MAX_LOOP_TURNS,
            count: AtomicUsize::new(0),
            cancel: None,
            steering: None,
            media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
        });
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
        let rig_messages = port_messages_to_rig(messages)?;
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
    let model = PortModelAsRig::new(Arc::from(provider), cfg.model_name.clone());

    // E2：隐藏别名（旧设备工具等）保留注册但绝不进模型工具列表/提示词。
    let exposed_tools: Vec<Box<dyn crate::port::tool::Tool>> = cfg.tools.into_iter().filter(|t| t.exposed()).collect();
    let tool_specs: Vec<crate::port::tool::ToolSpec> = exposed_tools.iter().map(|t| t.spec()).collect();

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
        .default_max_turns(MAX_LOOP_TURNS);
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

    let handle: AgentLoopHandle = Arc::new(tokio::sync::Mutex::new(RigAgentLoop::new(agent, memory)));
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            budget: 25,
            count: AtomicUsize::new(0),
            cancel: Some(cancel),
            steering: None,
            media: tokio::sync::Mutex::new(RunMediaAccountant::default()),
        };
        assert_eq!(hook.action(), ToolCallAction::Stop("run cancelled".into()));
        assert_eq!(hook.count.load(Ordering::SeqCst), 0);
    }
}
