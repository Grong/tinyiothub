//! port AgentLoop → rig Agent 桥 + rig loop 工厂。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use futures::StreamExt;
use rig_agent::agent::hook::ToolCall as ToolCallEvent;
use rig_agent::agent::{Agent, AgentBuilder, AgentHook, HookContext, MultiTurnStreamItem, ToolCallAction};
use rig_agent::completion::Prompt;
use rig_agent::streaming::StreamingPrompt;
use rig_core::streaming::{StreamedAssistantContent, StreamedUserContent};

use crate::port::events::TurnEvent;
use crate::port::runtime::{AgentLoop, AgentLoopConfig, AgentLoopHandle, MAX_LOOP_TURNS};

use super::memory::PortMemoryAsConversation;
use super::provider::{PortModelAsRig, port_messages_to_rig};
use super::tools::to_dynamic_tool;

/// 固定会话 id：现状 zeroclaw 各 turn 独立、无显式会话（Global Constraints #6）。
const CONVERSATION_ID: &str = "default";

/// turn 内软时长预算；runner 外层还有 timeout 兜底。
const MAX_TURN_DURATION: Duration = Duration::from_secs(300);

/// port AgentLoop 的 rig 引擎实现。
pub struct RigAgentLoop {
    agent: Agent,
    memory: Arc<PortMemoryAsConversation>,
}

impl RigAgentLoop {
    pub fn new(agent: Agent, memory: Arc<PortMemoryAsConversation>) -> Self {
        Self { agent, memory }
    }
}

/// 工具数预算 hook（第二道防线；主防线是 runner 的 CancellationToken）。
struct BudgetHook {
    budget: usize,
    count: AtomicUsize,
    cancel: Option<CancellationToken>,
}

impl AgentHook for BudgetHook {
    async fn on_tool_call(&self, _ctx: &HookContext, _event: ToolCallEvent<'_>) -> ToolCallAction {
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
        let request = self
            .agent
            .stream_prompt(user_message.to_string())
            .max_turns(MAX_LOOP_TURNS)
            .tool_concurrency(1)
            .add_hook(BudgetHook {
                budget: MAX_LOOP_TURNS,
                count: AtomicUsize::new(0),
                cancel: cancel_token.clone(),
            });
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
                            let _ = event_tx.send(TurnEvent::ToolResult {
                                id: tool_result.call.to_string(),
                                name: tool_result.name.clone(),
                                output,
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
        Ok(final_text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        let text = self
            .agent
            .prompt(message.to_string())
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(text)
    }

    async fn clear_history(&self) {
        // port Memory 无会话级删除原语（Global Constraints #6），no-op。
        let _ = rig_core::memory::ConversationMemory::clear(self.memory.as_ref(), CONVERSATION_ID).await;
    }

    async fn seed_history(&self, messages: &[crate::port::provider::ChatMessage]) {
        let rig_messages = port_messages_to_rig(messages);
        let _ = rig_core::memory::ConversationMemory::append(self.memory.as_ref(), CONVERSATION_ID, rig_messages).await;
    }
}

/// 用 port 零件组一个 rig 引擎的 AgentLoop（参数链对齐
/// `adapters::zeroclaw::loop_::zeroclaw_loop_factory`）。
pub fn rig_loop_factory(cfg: AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> {
    let provider = (cfg.provider_factory)()?;
    let model = PortModelAsRig::new(Arc::from(provider), cfg.model_name.clone());

    let preamble = cfg.prompt_builder.build(&crate::port::prompt::PromptContext {
        workspace_dir: &cfg.workspace_dir,
        agent_workspace_dir: &cfg.workspace_dir,
        model_name: &cfg.model_name,
        tool_specs: cfg.tools.iter().map(|t| t.spec()).collect(),
        security_summary: cfg.security_summary.clone(),
    })?;

    let memory = Arc::new(PortMemoryAsConversation::new(Arc::clone(&cfg.memory)));

    let agent = if cfg.tools.is_empty() {
        AgentBuilder::new(model)
            .preamble(&preamble)
            .default_max_turns(MAX_LOOP_TURNS)
            .memory(Arc::clone(&memory))
            .conversation(CONVERSATION_ID)
            .build()
    } else {
        AgentBuilder::new(model)
            .preamble(&preamble)
            .default_max_turns(MAX_LOOP_TURNS)
            .memory(Arc::clone(&memory))
            .conversation(CONVERSATION_ID)
            .dynamic_tools(cfg.tools.into_iter().map(|t| to_dynamic_tool(Arc::from(t))).collect())
            .build()
    };

    let handle: AgentLoopHandle = Arc::new(tokio::sync::Mutex::new(RigAgentLoop::new(agent, memory)));
    Ok(handle)
}
