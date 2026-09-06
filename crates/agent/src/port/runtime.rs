//! 运行时接口面 — AgentLoop 形状 vendored 自 zeroclaw-runtime Agent
//! （agent.rs:2520 run_single / turn_streamed）。

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 与 runner::MAX_TOOL_CALLS_PER_RUN 对齐的模型调用预算（含初始调用）。
pub const MAX_LOOP_TURNS: usize = 25;

pub struct AgentLoopConfig {
    pub model_name: String,
    pub prompt_builder: crate::port::prompt::SystemPromptBuilder,
    pub tools: Vec<Box<dyn crate::port::tool::Tool>>,
    pub memory: Arc<dyn crate::port::memory::Memory>,
    pub observer: Arc<dyn crate::port::observer::Observer>,
    pub workspace_dir: PathBuf,
    pub security_summary: Option<String>,
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

    /// zeroclaw run_single = turn()（zeroclaw-runtime/src/agent/agent.rs:2520），
    /// 即完整工具循环，对应 rig 的 `.prompt()`。
    async fn run_single(&self, message: &str) -> anyhow::Result<String>;
}

pub type AgentLoopHandle = Arc<tokio::sync::Mutex<dyn AgentLoop>>;
pub type AgentLoopFactory =
    Arc<dyn Fn(AgentLoopConfig) -> anyhow::Result<AgentLoopHandle> + Send + Sync>;

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
            Ok(ToolResult { success: true, output: "ok".into(), error: None })
        }
    }

    #[test]
    fn agent_loop_config_constructs_and_turn_budget() {
        let config = AgentLoopConfig {
            model_name: "test-model".into(),
            prompt_builder: crate::port::prompt::SystemPromptBuilder::with_defaults(),
            tools: vec![Box::new(Dummy)],
            memory: Arc::new(NoopMemory),
            observer: Arc::new(NoopObserver),
            workspace_dir: PathBuf::from("/tmp"),
            security_summary: None,
        };
        assert_eq!(config.model_name, "test-model");
        assert_eq!(config.tools.len(), 1);
        assert_eq!(MAX_LOOP_TURNS, 25);
    }
}
