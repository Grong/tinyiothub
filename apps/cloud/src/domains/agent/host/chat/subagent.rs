//! PoolSubagentRunner — 用 AgentPool 创建一次性子 Agent，执行后立即清除。
//!
//! 子代理继承父 Agent 的模型配置与工具集，但过滤掉 `ask_user` 与
//! `spawn_subagent`（只允许一层）。子 Turn 的上下文以子代理 id 注册在
//! `TurnContextRegistry` 下，与父 Turn 隔离。

use std::sync::Arc;

use tinyiothub_agent::port::provider::ChatMessage;

use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::runner::{SessionRunner, TurnOutcome};
use crate::domains::agent::session::subagent::{SubagentReport, SubagentRequest, SubagentRunner, SubagentStatus};

pub struct PoolSubagentRunner {
    pool: Arc<AgentPool>,
    runner: Arc<SessionRunner>,
}

impl PoolSubagentRunner {
    pub fn new(pool: Arc<AgentPool>, runner: Arc<SessionRunner>) -> Self {
        Self { pool, runner }
    }

    async fn run_inner(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
        let config = self
            .pool
            .agent_config(&request.parent_agent_id)
            .ok_or_else(|| format!("parent agent not found: {}", request.parent_agent_id))?;
        let child_id = format!(
            "{}_sub_{}",
            request.parent_agent_id,
            request.subagent_id.replace('-', "_")
        );

        let mut runtime = self.pool.runtime_context().await;
        runtime.agent_id = Some(child_id.clone());
        runtime.session_tools_enabled = true;
        let tools = self
            .pool
            .tool_registry()
            .resolve_tools_for_agent(
                &config,
                &request.workspace_id,
                self.pool.trust_config(&request.workspace_id),
                &runtime,
            )
            .await
            .into_iter()
            .filter(|tool| !matches!(tool.name(), "ask_user" | "spawn_subagent"))
            .collect();

        let agent = self
            .pool
            .create(&child_id, &request.workspace_id, &config, tools)
            .map_err(|error| error.to_string())?;

        let system_prompt = format!(
            "You are a focused {} subagent. Complete the assigned task and return a concise final report. Never ask the user questions.\n\nTask: {}",
            request.subagent_type, request.description
        );
        // 事件挂在父 Session 下（spec §7.3）；assistant_id 用子代理 id，
        // 让 TurnContextRegistry 与父 Turn 隔离。
        let context = TurnContext::new(
            request.workspace_id.clone(),
            child_id.clone(),
            request.parent_session_key.clone(),
            uuid::Uuid::new_v4().to_string(),
            request.depth,
        );

        let result = {
            let guard = agent.lock().await;
            guard.clear_history().await;
            if let Err(error) = guard.seed_history(&[ChatMessage::system(system_prompt)]).await {
                self.pool.invalidate(&child_id);
                return Err(error.to_string());
            }
            self.runner
                .run_turn(&*guard, &context, &request.prompt, None, None)
                .await
        };

        self.pool.invalidate(&child_id);

        match result {
            TurnOutcome::Completed { text, tool_calls } => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Completed,
                summary: text,
                tool_calls,
                // Plan 1 的模型只返回自由文本；结构化证据是 Plan 2 的扩展点。
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
            TurnOutcome::Cancelled => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Cancelled,
                summary: "subagent cancelled".to_string(),
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
            TurnOutcome::Failed { error } => Ok(SubagentReport {
                subagent_id: request.subagent_id,
                status: SubagentStatus::Failed,
                summary: error,
                tool_calls: 0,
                evidence_refs: vec![],
                recommended_actions: vec![],
            }),
        }
    }
}

#[async_trait::async_trait]
impl SubagentRunner for PoolSubagentRunner {
    async fn run(&self, request: SubagentRequest) -> Result<SubagentReport, String> {
        self.run_inner(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::pool::AgentPool;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::{
        SessionRunner, SubagentRequest, SubagentRunner, SubagentStatus, TurnContextRegistry,
    };
    use std::sync::Arc;
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ProviderKind, Role};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};

    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("subagent done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(
                tinyiothub_agent::port::attribution::ModelProviderKind::Custom,
            ))
        }

        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    #[tokio::test]
    async fn pool_subagent_runs_and_reports() {
        let pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
            )
            .unwrap(),
        );
        let config = AgentRuntimeConfig::default();
        let _parent = pool.create("agent_main", "ws-test", &config, vec![]).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink, contexts));
        let subagents = PoolSubagentRunner::new(pool.clone(), runner);

        let report = subagents
            .run(SubagentRequest {
                workspace_id: "ws-test".into(),
                parent_agent_id: "agent_main".into(),
                parent_session_key: "agent:ws-test:agent_main/s1".into(),
                subagent_id: "sub1".into(),
                subagent_type: "general-purpose".into(),
                description: "检查".into(),
                prompt: "检查全部设备".into(),
                depth: 1,
            })
            .await
            .unwrap();

        assert_eq!(report.status, SubagentStatus::Completed);
        assert_eq!(report.summary, "subagent done");
        assert_eq!(pool.pool_size(), 1, "child agent must be invalidated");
    }
}
