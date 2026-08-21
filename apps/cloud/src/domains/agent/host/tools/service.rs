// ToolService 数据面 — chat 内建工具 provider（thing 工具等数据实现的注册
// 闭包）+ effective_tool_names（db 校验）。
//
// Task 14：框架部分（ToolRuntimeContext / IoTToolAdapter / TrustAwareTool /
// ToolRegistry / catalog / denylist 过滤）已迁入 `tinyiothub_agent::tools`；
// 本文件只剩数据实现接线。注册点见 `shared/service_manager.rs`。

use std::sync::Arc;

use sqlx::SqlitePool;
use tinyiothub_agent::AgentError;
use tinyiothub_agent::tools::{ToolProvider, ToolRegistry, ToolRuntimeContext, filter_by_denylist};
use tinyiothub_skills::trust::{ToolSafety, classify_tool_safety};
use tinyiothub_storage::cache::DeviceCache;
use zeroclaw::tools::Tool;

use super::thing::{PendingActionStore, ThingToolContext, create_thing_tools};
use super::{canvas::CanvasTool, dispatch_task::DispatchThingTaskTool, get_skill::GetSkillTool};
use crate::domains::agent::host::config::service as config_service;

/// chat agent 的内建工具 provider：canvas + get_skill + thing 工具（9）+
/// dispatch_thing_task。
///
/// 数据句柄（db pool / device cache / pending-action store）在组合层注册时
/// 由闭包捕获（D2 —— 这些类型不进 agent crate）；`data_server` /
/// `directive_sink` 走 `ToolRuntimeContext` 晚绑定（启动后经
/// `AgentPool::set_runtime_context` 注入）。
pub fn chat_builtin_tools_provider(
    db_pool: SqlitePool,
    device_cache: Option<Arc<DeviceCache>>,
    pending_actions: Arc<PendingActionStore>,
) -> ToolProvider {
    Arc::new(move |workspace_id: &str, runtime: &ToolRuntimeContext| {
        let mut tools: Vec<(Box<dyn Tool>, ToolSafety)> = Vec::new();
        tools.push((Box::new(CanvasTool), classify_tool_safety("canvas")));
        tools.push((Box::new(GetSkillTool), classify_tool_safety("get_skill")));

        // Thing Ontology tools (9) — always available, no denylist
        tools.extend(create_thing_tools(
            db_pool.clone(),
            workspace_id,
            &ThingToolContext {
                device_cache: device_cache.clone(),
                data_server: runtime.data_server.clone(),
                pending_actions: Some(pending_actions.clone()),
            },
        ));

        // 用户指令派发工具（T14）—— chat Agent 专用；自治 thing-agent 工厂
        // （autonomous_factory.rs）不注册它，避免 loop 自我派发。
        tools.push((
            Box::new(DispatchThingTaskTool::new(
                workspace_id,
                runtime.directive_sink.clone(),
            )),
            classify_tool_safety("dispatch_thing_task"),
        ));

        tools
    })
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
