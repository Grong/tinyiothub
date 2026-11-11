//! 运行时接口面 — AgentLoop 形状 vendored 自 zeroclaw-runtime Agent
//! （agent.rs:2520 run_single / turn_streamed）。

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub type SteeringBuffer = Arc<std::sync::Mutex<Vec<String>>>;

pub struct AgentLoopConfig {
    pub model_name: String,
    pub prompt_builder: crate::port::prompt::SystemPromptBuilder,
    pub tools: Vec<Box<dyn crate::port::tool::Tool>>,
    pub memory: Arc<dyn crate::port::memory::Memory>,
    pub observer: Arc<dyn crate::port::observer::Observer>,
    pub workspace_dir: PathBuf,
    pub security_summary: Option<String>,
    /// 每 agent 构建一个 model provider（provider 是 per-agent 的）。
    pub provider_factory: crate::provider::ProviderFactory,
    /// 是否给引擎接线会话级 ConversationMemory（自动 load/append）。
    ///
    /// chat/heartbeat 路径传 `false`：历史由 cloud 侧 DB 每轮重建
    /// （clear_history + seed_history），不配 ConversationMemory 可避免
    /// seed 与引擎自动 load/append 的重复累积。thing_agent 自治路径传
    /// `true`：保留跨轮 recall 行为。zeroclaw 适配器接受并忽略该字段
    /// （其 auto-save 是无害旁路，见 Task 7 报告）。
    pub conversation_memory: bool,
    /// Capabilities the composition root resolved from configuration beyond
    /// the provider port (for example a configured video source). The loop
    /// derives model capabilities such as vision from the provider itself and
    /// fails closed before the first model call if an exposed tool requires a
    /// capability that is missing.
    pub available_capabilities: Vec<tinyiothub_skills::registry::Capability>,
    /// Optional shared slot the loop fills with the provider tool-call id
    /// before it executes a tool. Tools that need a durable call identity read
    /// it at execution time.
    pub tool_call_ids: Option<crate::port::tool::ToolCallIdSlot>,
}

/// zeroclaw Agent::turn_streamed 的 port 形状（返回去掉 ConversationMessage，
/// 两个消费点均丢弃它 —— Global Constraints #2）。
#[async_trait::async_trait]
pub trait AgentLoop: Send + Sync {
    async fn turn_streamed(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<crate::port::events::TurnEvent>,
        cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String>;

    async fn turn_streamed_with_steering(
        &self,
        user_message: &str,
        event_tx: mpsc::Sender<crate::port::events::TurnEvent>,
        cancel_token: Option<CancellationToken>,
        _steering: Option<SteeringBuffer>,
    ) -> anyhow::Result<String> {
        self.turn_streamed(user_message, event_tx, cancel_token).await
    }

    /// zeroclaw run_single = turn()（zeroclaw-runtime/src/agent/agent.rs:2520），
    /// 即完整工具循环，对应 rig 的 `.prompt()`。
    async fn run_single(&self, message: &str) -> anyhow::Result<String>;

    /// 清空内存历史（zeroclaw Agent::clear_history）。cloud chat 每轮从
    /// 会话持久化消息重建上下文，先清后种。
    async fn clear_history(&self);

    /// 用会话持久化消息水合内存历史（zeroclaw Agent::seed_history）。
    async fn seed_history(&self, messages: &[crate::port::provider::ChatMessage]) -> anyhow::Result<()>;
}

pub type AgentLoopHandle = Arc<tokio::sync::Mutex<dyn AgentLoop>>;
pub type AgentLoopFactory = Arc<dyn Fn(AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> + Send + Sync>;

/// A tool required a capability the resolved provider/model does not offer.
/// Composition roots downcast this to write `capability.unavailable` before
/// any model call; the run fails with zero Effects.
#[derive(Debug, Clone, thiserror::Error)]
#[error("capability.unavailable: tool `{tool}` requires missing capability [{}]", missing.iter().map(|capability| capability.as_str()).collect::<Vec<_>>().join(", "))]
pub struct CapabilityUnavailableError {
    pub tool: String,
    pub missing: Vec<tinyiothub_skills::registry::Capability>,
    pub recovery: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::attribution::{Attributable, Role, ToolKind};
    use crate::port::memory::NoopMemory;
    use crate::port::observer::NoopObserver;
    use crate::port::tool::{Tool, ToolResult};
    use std::path::PathBuf;
    use std::sync::Arc;

    struct Dummy;
    impl Attributable for Dummy {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "dummy"
        }
    }
    #[async_trait::async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "d"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    /// 桩 factory：本组测试不走真实 provider，一律报错。
    fn stub_provider_factory() -> crate::provider::ProviderFactory {
        Arc::new(|| -> anyhow::Result<Box<dyn crate::port::provider::ModelProvider>> {
            anyhow::bail!("stub provider factory")
        })
    }

    #[test]
    fn agent_loop_config_constructs() {
        let config = AgentLoopConfig {
            model_name: "test-model".into(),
            prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
            tools: vec![Box::new(Dummy)],
            memory: Arc::new(NoopMemory),
            observer: Arc::new(NoopObserver),
            workspace_dir: PathBuf::from("/tmp"),
            security_summary: None,
            provider_factory: stub_provider_factory(),
            conversation_memory: false,
            available_capabilities: vec![],
            tool_call_ids: None,
        };
        assert_eq!(config.model_name, "test-model");
        assert_eq!(config.tools.len(), 1);
    }
}
