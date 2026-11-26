// ToolService 数据面 — chat 内建工具 provider（thing 工具等数据实现的注册
// 闭包）+ effective_tool_names（db 校验）。
//
// Task 14：框架部分（ToolRuntimeContext / IoTToolAdapter / TrustAwareTool /
// ToolRegistry / catalog / denylist 过滤）已迁入 `crate::domains::agent::tools`；
// 本文件只剩数据实现接线。注册点见 `shared/service_manager.rs`。

use std::sync::Arc;

use crate::domains::agent::thing_agent::ArbitrationRegistry;
use crate::domains::agent::thing_agent::seams::tool_exec::{SeamTool, ToolExecSeam};
use crate::domains::agent::tools::{ToolProvider, ToolRegistry, ToolRuntimeContext, filter_by_denylist};
use sqlx::SqlitePool;
use tinyiothub_agent::AgentError;
use tinyiothub_agent::port::tool::Tool;
use tinyiothub_skills::trust::{ToolSafety, classify_tool_safety};
use tinyiothub_storage::cache::ThingCache;

use super::thing::{LiveSummarySource, ManagerSlot, PendingActionStore, ThingToolContext, create_thing_tools};
use super::{canvas::CanvasTool, get_skill::GetSkillTool};
use crate::domains::agent::session::config::service as config_service;

/// chat agent 的内建工具 provider：canvas + get_skill + thing 工具（9）。
///
/// 数据句柄（db pool / device cache / pending-action store）在组合层注册时
/// 由闭包捕获（D2 —— 这些类型不进 agent crate）；`data_server` /
/// `directive_sink` 走 `ToolRuntimeContext` 晚绑定（启动后经
/// `AgentPool::set_runtime_context` 注入）。
#[allow(clippy::too_many_arguments)] // 组合根 provider：参数即依赖清单，收敛于此一处
pub fn chat_builtin_tools_provider(
    db_pool: SqlitePool,
    db: Arc<tinyiothub_storage::Db>,
    device_cache: Option<Arc<ThingCache>>,
    pending_actions: Arc<PendingActionStore>,
    arbitration: Arc<ArbitrationRegistry>,
    live: Arc<dyn LiveSummarySource>,
    tool_exec: Arc<dyn ToolExecSeam>,
    event_bus: Option<Arc<tinyiothub_runtime::event_bus::EventBus>>,
    manager: ManagerSlot,
    access: Option<Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>>,
    resources: Option<Arc<dyn tinyiothub_runtime::access::resources::ResourceResolver>>,
    demand: Option<Arc<dyn tinyiothub_runtime::access::DemandRecorder>>,
    evidence: Option<Arc<tinyiothub_runtime::media::evidence::EvidenceService>>,
    jobs: Option<tinyiothub_runtime::job::service::JobService>,
    runtime_recorder: crate::domains::agent::runtime::stream::RuntimeRecorder,
    session: std::sync::Arc<crate::domains::agent::session::SessionRuntime>,
) -> ToolProvider {
    Arc::new(move |workspace_id: &str, runtime: &ToolRuntimeContext| {
        let mut tools: Vec<(Box<dyn Tool>, ToolSafety)> = Vec::new();
        let session_agent_id = session_primitive_agent(runtime);
        tools.push((Box::new(CanvasTool), classify_tool_safety("canvas")));
        tools.push((
            Box::new(GetSkillTool::new(
                workspace_id.to_string(),
                session_agent_id.clone(),
                session.contexts.clone(),
                session.events.clone(),
            )),
            classify_tool_safety("get_skill"),
        ));

        // Thing Ontology tools (9) — always available, no denylist
        tools.extend(create_thing_tools(
            db_pool.clone(),
            workspace_id,
            &ThingToolContext {
                db: Some(db.clone()),
                device_cache: device_cache.clone(),
                data_server: runtime.data_server.clone(),
                pending_actions: Some(pending_actions.clone()),
                arbitration: Some(arbitration.clone()),
                live: Some(live.clone()),
                event_bus: event_bus.clone(),
                manager: Some(manager.clone()),
                access: access.clone(),
                resources: resources.clone(),
                demand: demand.clone(),
                run_ctx: None,
                evidence: evidence.clone(),
                jobs: jobs.clone(),
                runtime_recorder: Some(runtime_recorder.clone()),
            },
        ));

        // Session 原语（Todo / 提问 / 子代理）—— 只在聊天/子代理 Turn 暴露：
        // 需要 agent_id 注册 TurnContext，并且仅聊天入口打开 flag。
        if let Some(agent_id) = session_agent_id {
            use crate::domains::agent::session::tools::{AskUserTool, SpawnSubagentTool, TodoWriteTool};
            tools.push((
                Box::new(TodoWriteTool::new(
                    agent_id.clone(),
                    session.contexts.clone(),
                    session.todos.clone(),
                    session.events.clone(),
                )),
                ToolSafety::ReadOnly,
            ));
            tools.push((
                Box::new(AskUserTool::new(
                    agent_id.clone(),
                    session.contexts.clone(),
                    session.questions.clone(),
                    session.events.clone(),
                    crate::domains::agent::session::ASK_USER_TIMEOUT,
                )),
                ToolSafety::ReadOnly,
            ));
            tools.push((
                Box::new(SpawnSubagentTool::new(
                    agent_id,
                    session.contexts.clone(),
                    session.subagents.clone(),
                    session.events.clone(),
                )),
                ToolSafety::ReadOnly,
            ));
        }

        // E2 ToolExec 接缝：所有内建工具经统一执行外壳（默认直通）。
        let tools: Vec<(Box<dyn Tool>, ToolSafety)> = tools
            .into_iter()
            .map(|(tool, safety)| {
                let wrapped: Box<dyn Tool> = Box::new(SeamTool::new(tool, Arc::clone(&tool_exec), workspace_id));
                (wrapped, safety)
            })
            .collect();

        // PermissionGuard：聊天/子代理 Turn 内按 Session 权限模式拦截写操作。
        // 复用 session_primitive_agent 作为唯一的会话 Turn 判定，避免条件漂移。
        apply_permission_guard(
            tools,
            session_primitive_agent(runtime),
            session.contexts.clone(),
            session.approvals.clone(),
            session.events.clone(),
            Some(std::sync::Arc::new(
                crate::domains::agent::session::ThingApprovalLabels::new(std::sync::Arc::new(
                    crate::domains::thing::service::ThingService::new(db_pool.clone()),
                )),
            )),
        )
    })
}

/// 按 Session 权限模式包裹写工具（ReadOnly 透传）。`agent_id` 为 `None`
/// 时原样返回——与会话原语使用同一 Turn 判定。
pub(crate) fn apply_permission_guard(
    tools: Vec<(Box<dyn Tool>, ToolSafety)>,
    agent_id: Option<String>,
    contexts: std::sync::Arc<crate::domains::agent::session::TurnContextRegistry>,
    approvals: std::sync::Arc<crate::domains::agent::session::ApprovalBroker>,
    events: std::sync::Arc<dyn crate::domains::agent::session::SessionEventSink>,
    labels: Option<std::sync::Arc<crate::domains::agent::session::ThingApprovalLabels>>,
) -> Vec<(Box<dyn Tool>, ToolSafety)> {
    let Some(agent_id) = agent_id else {
        return tools;
    };
    tools
        .into_iter()
        .map(|(tool, safety)| {
            if safety == ToolSafety::ReadOnly {
                (tool, safety)
            } else {
                (
                    Box::new(
                        crate::domains::agent::session::PermissionGuardTool::new(
                            tool,
                            agent_id.clone(),
                            contexts.clone(),
                            approvals.clone(),
                            events.clone(),
                            safety,
                            crate::domains::agent::session::APPROVAL_TIMEOUT,
                        )
                        .with_labels(labels.clone()),
                    ) as Box<dyn Tool>,
                    safety,
                )
            }
        })
        .collect()
}

fn session_primitive_agent(runtime: &crate::domains::agent::tools::context::ToolRuntimeContext) -> Option<String> {
    if runtime.session_tools_enabled {
        runtime.agent_id.clone()
    } else {
        None
    }
}

/// Effective (denylist-filtered) tool names for an agent — the
/// `/tools/effective` API payload. db access stays on the cloud side; the
/// caller supplies the pool's tool registry + runtime context snapshot.
pub async fn effective_tool_names(
    db_pool: &SqlitePool,
    registry: &ToolRegistry,
    runtime: &ToolRuntimeContext,
    agent_id: &str,
    workspace_id: &str,
) -> Result<serde_json::Value, AgentError> {
    config_service::verify_agent_workspace(db_pool, agent_id, workspace_id).await?;
    let config = config_service::get_config(db_pool, agent_id).await?;
    let all_tools = registry.load_all_tools(workspace_id, runtime).await;
    let effective = filter_by_denylist(all_tools, &config.tool_denylist);
    let names: Vec<&str> = effective.iter().map(|t| t.name()).collect();
    Ok(serde_json::json!({ "tools": names }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_primitives_require_flag_and_agent() {
        let mut runtime = crate::domains::agent::tools::context::ToolRuntimeContext::default();
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.agent_id = Some("agent_main".to_string());
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.session_tools_enabled = true;
        assert_eq!(session_primitive_agent(&runtime), Some("agent_main".to_string()));
    }

    /// PermissionGuard 的接线条件与 Session 原语共用同一判定，这里锁定三分支。
    #[test]
    fn session_primitives_gate_also_gates_permission_guard() {
        let mut runtime = crate::domains::agent::tools::context::ToolRuntimeContext::default();
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.agent_id = Some("agent_main".to_string());
        assert_eq!(session_primitive_agent(&runtime), None);

        runtime.session_tools_enabled = true;
        assert_eq!(session_primitive_agent(&runtime), Some("agent_main".to_string()));
    }

    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::types::PermissionMode;
    use crate::domains::agent::session::{ApprovalBroker, TurnContext, TurnContextRegistry, TurnGuard};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
    use tinyiothub_agent::port::tool::ToolResult;

    struct EchoTool {
        name: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "test"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    impl Attributable for EchoTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            self.name
        }
    }

    fn echo(name: &'static str) -> (Box<dyn Tool>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Box::new(EchoTool {
                name,
                calls: calls.clone(),
            }),
            calls,
        )
    }

    fn read_only_context(agent_id: &str) -> (Arc<TurnContextRegistry>, TurnGuard) {
        let contexts = Arc::new(TurnContextRegistry::default());
        let guard = contexts.set(
            TurnContext::new(
                "ws".into(),
                agent_id.into(),
                format!("agent:ws:{agent_id}/s1"),
                "run1".into(),
                0,
            )
            .with_permission_mode(PermissionMode::ReadOnly),
        );
        (contexts, guard)
    }

    #[tokio::test]
    async fn apply_permission_guard_wraps_only_writes() {
        let (write_inner, write_calls) = echo("set_device");
        let (read_inner, read_calls) = echo("get_device");
        let (contexts, _guard) = read_only_context("agent_main");

        let guarded = apply_permission_guard(
            vec![(write_inner, ToolSafety::Write), (read_inner, ToolSafety::ReadOnly)],
            Some("agent_main".into()),
            contexts,
            Arc::new(ApprovalBroker::default()),
            Arc::new(RecordingSink::default()),
            None,
        );
        assert_eq!(guarded.len(), 2);

        let read_result = guarded[1].0.execute(serde_json::json!({})).await.unwrap();
        assert!(read_result.success);
        assert_eq!(read_calls.load(Ordering::SeqCst), 1);

        let write_result = guarded[0].0.execute(serde_json::json!({})).await.unwrap();
        assert!(!write_result.success);
        assert!(write_result.error.unwrap().contains("只读"));
        assert_eq!(write_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn apply_permission_guard_wraps_child_agent_tools() {
        let child_id = "agent_ws_child";
        let (write_inner, write_calls) = echo("set_device");
        let (contexts, _guard) = read_only_context(child_id);

        let mut guarded = apply_permission_guard(
            vec![(write_inner, ToolSafety::Write)],
            Some(child_id.to_string()),
            contexts,
            Arc::new(ApprovalBroker::default()),
            Arc::new(RecordingSink::default()),
            None,
        );
        let result = guarded.pop().unwrap().0.execute(serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("只读"));
        assert_eq!(write_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn classify_tool_safety_marks_device_writes_non_read_only() {
        for name in [
            "call_device",
            "set_device",
            "stop_device",
            "write_properties",
            "send_command",
        ] {
            assert_ne!(
                classify_tool_safety(name),
                ToolSafety::ReadOnly,
                "{name} must require permission"
            );
        }
    }
}
