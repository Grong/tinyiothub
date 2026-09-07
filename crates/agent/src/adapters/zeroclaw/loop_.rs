//! port AgentLoop → zeroclaw Agent 桥 + zeroclaw loop 工厂。

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::port::runtime::{AgentLoop, AgentLoopConfig, AgentLoopHandle};

use super::memory::PortMemoryAsZeroclaw;
use super::observer::PortObserverAsZeroclaw;
use super::provider::PortProviderAsZeroclaw;
use super::prompt::to_zeroclaw_builder;
use super::tools::wrap_tools;

/// zeroclaw 的取消错误归一化为 port 的 [`crate::port::outcome::ToolLoopCancelled`]。
fn map_cancelled(err: anyhow::Error) -> anyhow::Error {
    if zeroclaw::agent::loop_::is_tool_loop_cancelled(&err) {
        anyhow::Error::new(crate::port::outcome::ToolLoopCancelled)
    } else {
        err
    }
}

/// zeroclaw TurnEvent → port TurnEvent（两形状 vendored 时逐变体一致）。
fn port_event(event: zeroclaw_api::agent::TurnEvent) -> crate::port::events::TurnEvent {
    use crate::port::events::TurnEvent as P;
    use zeroclaw_api::agent::TurnEvent as Z;
    match event {
        Z::Chunk { delta } => P::Chunk { delta },
        Z::Thinking { delta } => P::Thinking { delta },
        Z::ToolCall { id, name, args } => P::ToolCall { id, name, args },
        Z::ToolResult { id, name, output } => P::ToolResult { id, name, output },
        Z::ApprovalRequest {
            request_id,
            tool_name,
            arguments_summary,
            timeout_secs,
        } => P::ApprovalRequest {
            request_id,
            tool_name,
            arguments_summary,
            timeout_secs,
        },
        Z::Usage {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            cost_usd,
        } => P::Usage {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            cost_usd,
        },
    }
}

/// zeroclaw Agent 包装为 port AgentLoop。
pub struct ZeroclawAgentLoop {
    agent: tokio::sync::Mutex<zeroclaw::agent::Agent>,
}

impl ZeroclawAgentLoop {
    pub fn new(agent: zeroclaw::agent::Agent) -> Self {
        Self { agent: tokio::sync::Mutex::new(agent) }
    }
}

#[async_trait::async_trait]
impl AgentLoop for ZeroclawAgentLoop {
    async fn turn_streamed(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<crate::port::events::TurnEvent>,
        cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        // zeroclaw 的 sender 消费事件流；起一个转发任务做逐事件类型转换。
        let (zc_tx, mut zc_rx) = mpsc::channel(64);
        let forwarder = tokio::spawn(async move {
            while let Some(event) = zc_rx.recv().await {
                if event_tx.send(port_event(event)).await.is_err() {
                    break;
                }
            }
        });
        let (text, _conv) = self
            .agent
            .lock()
            .await
            .turn_streamed(user_message, zc_tx, cancel_token)
            .await
            .map_err(map_cancelled)?;
        // sender 已随 turn 结束 drop，转发任务自然退出；等它排空事件。
        let _ = forwarder.await;
        Ok(text)
    }

    async fn run_single(&self, message: &str) -> anyhow::Result<String> {
        self.agent.lock().await.turn(message).await.map_err(map_cancelled)
    }
}

/// 用 port 零件组一个 zeroclaw 引擎的 AgentLoop（参数链对齐
/// `pool.rs` 的 `build_agent`，response_cache 不传）。
pub fn zeroclaw_loop_factory(cfg: AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> {
    let provider = PortProviderAsZeroclaw { inner: Arc::from((cfg.provider_factory)()?) };
    let prompt_builder = to_zeroclaw_builder(cfg.prompt_builder);

    let agent = zeroclaw::agent::Agent::builder()
        .model_provider(Box::new(provider))
        .tools(wrap_tools(cfg.tools))
        .memory(Arc::new(PortMemoryAsZeroclaw(cfg.memory)))
        .observer(Arc::new(PortObserverAsZeroclaw(cfg.observer)))
        .tool_dispatcher(Box::new(zeroclaw::agent::dispatcher::NativeToolDispatcher))
        .model_name(cfg.model_name)
        .security_summary(cfg.security_summary)
        .autonomy_level(zeroclaw::security::AutonomyLevel::Supervised)
        .prompt_builder(prompt_builder)
        .workspace_dir(cfg.workspace_dir)
        .build()
        .map_err(|e| anyhow::anyhow!("Agent build failed: {}", e))?;

    let handle: AgentLoopHandle = Arc::new(tokio::sync::Mutex::new(ZeroclawAgentLoop::new(agent)));
    Ok(handle)
}
