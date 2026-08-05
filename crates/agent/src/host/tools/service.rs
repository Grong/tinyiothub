// ToolService — MCP loading, denylist filtering, catalog building
//
// Core tool orchestration layer. Loads tools from the MCP handler registry,
// wraps them as zeroclaw Tools, filters by denylist, and builds the tool
// catalog used by both the API and the agent runtime.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_storage::cache::DeviceCache;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{canvas::CanvasTool, thing::create_thing_tools};
use crate::host::ports::{ExternalToolContext, ExternalToolHandler, external_tool_registry};
use crate::host::shared::config::AgentRuntimeConfig;
use crate::loop_::thing_agent::DirectiveSink;
use crate::loop_::types::{TrustConfig, TrustDecision};

/// Runtime handles threaded into tool construction (P4-Task22; replaces the
/// old `Option<Arc<AppState>>` backdoor).
#[derive(Clone, Default)]
pub struct ToolRuntimeContext {
    pub device_cache: Option<Arc<DeviceCache>>,
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub directive_sink: Option<Arc<dyn DirectiveSink>>,
}

/// 根据工具名称推断是否危险操作 (mirrored from cloud's `mcp::tool_metadata`
/// until Task 23 reclaims the MCP plane — keep the patterns in sync).
fn name_infers_destructive(name: &str) -> bool {
    name.starts_with("delete_")
        || name.starts_with("remove_")
        || name.starts_with("unload_")
        || name.contains("firmware")
        || name.contains("reset")
        || name.contains("reboot")
        || name.contains("factory")
}

// ============================================================================
// IoTToolAdapter — wraps an external (MCP) tool handler as zeroclaw Tool
// ============================================================================

pub struct IoTToolAdapter {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    handler: Arc<dyn ExternalToolHandler>,
    workspace_id: String,
    safety: crate::loop_::types::ToolSafety,
}

impl IoTToolAdapter {
    pub fn new(
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: Arc<dyn ExternalToolHandler>,
        workspace_id: String,
    ) -> Self {
        let safety = handler.safety();
        Self { name, description, input_schema, handler, workspace_id, safety }
    }

    /// Handler-declared safety — authoritative for trust evaluation.
    pub fn safety(&self) -> crate::loop_::types::ToolSafety {
        self.safety
    }
}

impl Attributable for IoTToolAdapter {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        <Self as Tool>::name(self)
    }
}

#[async_trait]
impl Tool for IoTToolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // The composition layer's external-tool adapter applies the auth
        // context for this workspace/actor around the handler call.
        let ctx = ExternalToolContext {
            workspace_id: self.workspace_id.clone(),
            actor: "agent".to_string(),
        };
        match self.handler.execute(&ctx, args).await {
            Ok(output) => Ok(ToolResult {
                success: true,
                output: serde_json::to_string(&output).unwrap_or_default(),
                error: None,
            }),
            Err(err) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(err.to_string()),
            }),
        }
    }
}

// ============================================================================
// TrustAwareTool — wraps a Tool with trust-level enforcement
// ============================================================================

/// Proxies a `Box<dyn Tool>`, delegating trust evaluation to
/// `crate::loop_::evaluate_tool_trust`.
///
/// Trust decision comes from the AI crate — tool metadata (read/destructive)
/// is authoritative; the TrustConfig only provides overrides.
pub struct TrustAwareTool {
    inner: Box<dyn Tool>,
    trust_config: Arc<TrustConfig>,
    safety: crate::loop_::types::ToolSafety,
}

impl TrustAwareTool {
    pub fn new(
        inner: Box<dyn Tool>,
        trust_config: Arc<TrustConfig>,
        safety: crate::loop_::types::ToolSafety,
    ) -> Self {
        Self { inner, trust_config, safety }
    }
}

impl Attributable for TrustAwareTool {
    fn role(&self) -> Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

#[async_trait]
impl Tool for TrustAwareTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let tool_name = <Self as Tool>::name(self);

        // X3/T16: the legacy heartbeat trust path is converged onto the unified
        // engine via HeartbeatTrustAdapter. O23 equivalence: for the same
        // TrustConfig input the adapter's verdict equals
        // evaluate_tool_trust_with_safety (verified by the adapter's
        // parameterized equivalence tests).
        match crate::loop_::types::HeartbeatTrustAdapter::evaluate(
            &self.trust_config,
            tool_name,
            self.safety,
        ) {
            TrustDecision::Allow => self.inner.execute(args).await,
            TrustDecision::Block { reason } => {
                Ok(ToolResult { success: false, output: String::new(), error: Some(reason) })
            }
            TrustDecision::Propose { reason } => {
                Ok(ToolResult { success: false, output: String::new(), error: Some(reason) })
            }
        }
    }
}

// ============================================================================
// Tool loading
// ============================================================================

/// Load all tools: CanvasTool + externally-registered (MCP) handlers.
///
/// CanvasTool is always included first. External tools are
/// loaded from the composition-registered registry if available.
pub async fn load_all_tools(
    workspace_id: &str,
    db_pool: Option<SqlitePool>,
    runtime: &ToolRuntimeContext,
) -> Vec<Box<dyn Tool>> {
    load_all_tools_with_safety(workspace_id, db_pool, runtime)
        .await
        .into_iter()
        .map(|(tool, _)| tool)
        .collect()
}

/// Load all tools together with their (declared or name-inferred) safety.
/// External adapters carry the handler's declared safety; built-in tools are
/// classified by name.
async fn load_all_tools_with_safety(
    workspace_id: &str,
    db_pool: Option<SqlitePool>,
    runtime: &ToolRuntimeContext,
) -> Vec<(Box<dyn Tool>, crate::loop_::types::ToolSafety)> {
    use crate::loop_::types::{ToolSafety, classify_tool_safety};

    let mut tools: Vec<(Box<dyn Tool>, ToolSafety)> = Vec::new();
    tools.push((Box::new(CanvasTool), classify_tool_safety("canvas")));
    tools.push((Box::new(super::GetSkillTool), classify_tool_safety("get_skill")));

    // Built-in thing tool names — win over MCP handlers on collision (T7)
    const BUILTIN_THING_TOOL_NAMES: [&str; 10] = [
        "list_things",
        "get_thing",
        "get_thing_profile",
        "get_thing_tree",
        "read_property",
        "invoke_action",
        "query_events",
        "search_knowledge",
        "read_document",
        "dispatch_thing_task",
    ];

    // Thing Ontology tools (9) — always available, no denylist
    if let Some(ref pool) = db_pool {
        tools.extend(create_thing_tools(pool.clone(), workspace_id, runtime));
    }

    // 用户指令派发工具（T14）—— chat Agent 专用；自治 thing-agent 工厂
    // （autonomous_factory.rs）不注册它，避免 loop 自我派发。
    tools.push((
        Box::new(super::dispatch_task::DispatchThingTaskTool::new(
            workspace_id,
            runtime.directive_sink.clone(),
        )),
        classify_tool_safety("dispatch_thing_task"),
    ));

    if let Some(registry) = external_tool_registry() {
        for meta in registry.list_tools().await {
            if meta.name.trim().is_empty() {
                continue;
            }
            // Built-in thing tools take precedence: an external handler with
            // the same name (e.g. DeviceProfileHandler renamed to "get_thing")
            // would double-register with a different schema (eng-review T7)
            if BUILTIN_THING_TOOL_NAMES.contains(&meta.name.as_str()) {
                tracing::warn!(tool = %meta.name, "Skipping external handler colliding with built-in thing tool");
                continue;
            }
            let name = meta.name.clone();
            let description = meta.description.clone();
            let input_schema = meta.input_schema.clone();
            if let Some(handler) = registry.get_handler(&name).await {
                let adapter = IoTToolAdapter::new(
                    name,
                    description,
                    input_schema,
                    handler,
                    workspace_id.to_string(),
                );
                let safety = adapter.safety();
                tools.push((Box::new(adapter), safety));
            }
        }
    }

    tools
}

// ============================================================================
// Denylist filtering
// ============================================================================

/// Filter tools by denylist, always keeping CanvasTool.
///
/// CanvasTool (name == "canvas") is exempt from denylist filtering because
/// it is a safe A2UI rendering tool, not an IoT operation.
pub fn filter_by_denylist(tools: Vec<Box<dyn Tool>>, denylist: &[String]) -> Vec<Box<dyn Tool>> {
    if denylist.is_empty() {
        return tools;
    }

    tools
        .into_iter()
        .filter(|tool| {
            let name = tool.name();
            if name == "canvas" {
                return true;
            }
            !denylist.contains(&name.to_string())
        })
        .collect()
}

/// Load and filter tools for an agent based on its runtime config.
/// If `trust_config` is provided, wraps every tool with `TrustAwareTool`
/// for trust-level enforcement at execution time.
pub async fn resolve_tools_for_agent(
    config: &AgentRuntimeConfig,
    workspace_id: &str,
    trust_config: Option<Arc<TrustConfig>>,
    db_pool: Option<SqlitePool>,
    runtime: &ToolRuntimeContext,
) -> Vec<Box<dyn Tool>> {
    let all_tools =
        load_all_tools_with_safety(workspace_id, db_pool, runtime).await;
    let filtered: Vec<(Box<dyn Tool>, crate::loop_::types::ToolSafety)> = all_tools
        .into_iter()
        .filter(|(tool, _)| {
            let name = tool.name();
            name == "canvas" || !config.tool_denylist.contains(&name.to_string())
        })
        .collect();

    match trust_config {
        Some(tc) => filtered
            .into_iter()
            .map(|(tool, safety)| {
                let wrapped: Box<dyn Tool> =
                    Box::new(TrustAwareTool::new(tool, Arc::clone(&tc), safety));
                wrapped
            })
            .collect(),
        None => filtered.into_iter().map(|(tool, _)| tool).collect(),
    }
}

// ============================================================================
// Tool catalog
// ============================================================================

/// Label mapping for known tools (display name in Chinese).
fn tool_label(name: &str) -> &str {
    match name {
        // Device-runtime tools (MCP)
        "search_things" => "搜索物",
        "read_properties" => "读取属性",
        "write_properties" => "写入属性",
        "send_command" => "执行设备命令",
        "create_thing" => "创建物",
        "delete_thing" => "删除物",
        // Thing tools
        "list_things" => "列出物",
        "get_thing" => "查看物",
        "get_thing_profile" => "物完整快照",
        "get_thing_tree" => "物层级树",
        "read_property" => "读取属性值",
        "invoke_action" => "执行操作",
        "query_events" => "查询事件",
        "search_knowledge" => "搜索知识文档",
        "read_document" => "读取文档内容",
        // Alarm tools
        "alarm_list" => "查询告警列表",
        "alarm_acknowledge" => "确认告警",
        "alarm_rule_add" => "添加告警规则",
        // Workspace tools
        "search_workspace_resources" => "搜索工作空间资源",
        // Driver tools
        "list_drivers" => "查询驱动列表",
        "test_driver" => "测试驱动",
        // Job tools
        "list_schedules" => "查询任务列表",
        "create_schedule" => "创建调度任务",
        "update_schedule" => "更新调度任务",
        "delete_schedule" => "删除调度任务",
        _ => name,
    }
}

/// Infer group (id, label) from tool name.
fn tool_group(name: &str) -> (&str, &str) {
    if name == "search_workspace_resources" {
        ("workspace", "工作空间")
    } else if name.starts_with("search_")
        || matches!(name, "read_properties" | "write_properties" | "send_command")
    {
        ("device", "设备管理")
    } else if matches!(
        name,
        "list_things"
            | "get_thing"
            | "get_thing_profile"
            | "get_thing_tree"
            | "read_property"
            | "invoke_action"
            | "query_events"
            | "search_knowledge"
            | "read_document"
            | "create_thing"
            | "delete_thing"
    ) {
        ("thing", "物本体")
    } else if name.starts_with("alarm_") {
        ("alarm", "告警管理")
    } else if matches!(name, "list_drivers" | "test_driver") {
        ("driver", "驱动管理")
    } else if matches!(
        name,
        "list_schedules" | "create_schedule" | "update_schedule" | "delete_schedule"
    ) {
        ("job", "任务管理")
    } else {
        ("other", "其他")
    }
}

/// Build the tool catalog dynamically from the MCP registry.
///
/// Falls back to the static catalog (`build_tools_catalog_json()`) when the
/// MCP registry is empty or unavailable.
pub async fn build_catalog() -> serde_json::Value {
    let mut groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

    if let Some(registry) = external_tool_registry() {
        for meta in registry.list_tools().await {
            let name = meta.name.clone();
            let (group_id, _) = tool_group(&name);
            let label = tool_label(&name);
            let danger = name_infers_destructive(&name);

            let tool_json = serde_json::json!({
                "id": name,
                "name": name,
                "label": label,
                "description": meta.description,
                "danger": danger,
                "enabled": !danger,
            });

            groups.entry(group_id.to_string()).or_default().push(tool_json);
        }
    }

    if groups.is_empty() {
        return crate::host::shared::build_tools_catalog_json();
    }

    let group_order = [
        ("thing", "物本体"),
        ("device", "设备管理"),
        ("alarm", "告警管理"),
        ("monitoring", "系统监控"),
        ("driver", "驱动管理"),
        ("workspace", "工作空间"),
        ("job", "任务管理"),
        ("other", "其他"),
    ];

    let groups_vec: Vec<serde_json::Value> = group_order
        .into_iter()
        .filter_map(|(id, label)| {
            groups.get(id).map(|tools| {
                serde_json::json!({
                    "id": id,
                    "label": label,
                    "source": "core",
                    "tools": tools,
                })
            })
        })
        .collect();

    serde_json::json!({ "groups": groups_vec })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // filter_by_denylist tests
    // ========================================================================

    #[test]
    fn test_filter_by_denylist_empty() {
        let tools: Vec<Box<dyn Tool>> = vec![Box::new(CanvasTool)];
        let result = filter_by_denylist(tools, &[]);
        // Empty denylist should return all tools unchanged
        assert!(!result.is_empty());
    }

    // ========================================================================
    // tool_label tests
    // ========================================================================

    #[test]
    fn test_tool_label_mapping() {
        assert_eq!(tool_label("search_things"), "搜索物");
        assert_eq!(tool_label("get_thing"), "查看物");
        assert_eq!(tool_label("alarm_list"), "查询告警列表");
        assert_eq!(tool_label("list_drivers"), "查询驱动列表");
        assert_eq!(tool_label("list_schedules"), "查询任务列表");
        // Unknown tool returns its name as label
        assert_eq!(tool_label("unknown_tool"), "unknown_tool");
    }

    // ========================================================================
    // tool_group tests
    // ========================================================================

    #[test]
    fn test_tool_group_classification() {
        assert_eq!(tool_group("search_things"), ("device", "设备管理"));
        assert_eq!(tool_group("get_thing"), ("thing", "物本体"));
        assert_eq!(tool_group("delete_thing"), ("thing", "物本体"));

        assert_eq!(tool_group("alarm_list"), ("alarm", "告警管理"));
        assert_eq!(tool_group("alarm_acknowledge"), ("alarm", "告警管理"));

        assert_eq!(tool_group("list_drivers"), ("driver", "驱动管理"));
        assert_eq!(tool_group("test_driver"), ("driver", "驱动管理"));

        assert_eq!(tool_group("list_schedules"), ("job", "任务管理"));
        assert_eq!(tool_group("delete_schedule"), ("job", "任务管理"));

        assert_eq!(tool_group("unknown_tool"), ("other", "其他"));
    }

    // ========================================================================
    // build_catalog tests
    // ========================================================================

    #[tokio::test]
    async fn test_build_catalog_fallback() {
        // When MCP registry is not available, should return static catalog
        let catalog = build_catalog().await;
        let groups = catalog["groups"].as_array().unwrap();
        assert!(!groups.is_empty(), "Static catalog should have groups");
        let group_ids: Vec<&str> = groups.iter().filter_map(|g| g["id"].as_str()).collect();
        assert!(group_ids.contains(&"device"));
        assert!(group_ids.contains(&"alarm"));
    }

    // ========================================================================
    // Declared-safety trust enforcement tests
    // ========================================================================

    struct StubTool {
        name: &'static str,
    }

    impl Attributable for StubTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            self.name
        }
    }

    #[async_trait]
    impl Tool for StubTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult { success: true, output: "ran".into(), error: None })
        }
    }

    #[tokio::test]
    async fn test_trust_aware_tool_declared_read_only_wins_over_name() {
        // Name looks destructive ("delete_"), but declared safety is read-only.
        let wrapped = TrustAwareTool::new(
            Box::new(StubTool { name: "delete_stub" }),
            Arc::new(TrustConfig::default()),
            crate::loop_::types::ToolSafety::ReadOnly,
        );
        let result =
            <TrustAwareTool as Tool>::execute(&wrapped, serde_json::json!({})).await.unwrap();
        assert!(result.success, "declared read-only must auto-execute: {:?}", result.error);
    }

    #[tokio::test]
    async fn test_trust_aware_tool_declared_destructive_requires_approval() {
        // Innocent name, declared destructive → must not execute under default config.
        let wrapped = TrustAwareTool::new(
            Box::new(StubTool { name: "get_stub" }),
            Arc::new(TrustConfig::default()),
            crate::loop_::types::ToolSafety::Destructive,
        );
        let result =
            <TrustAwareTool as Tool>::execute(&wrapped, serde_json::json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("destructive"));
    }

    struct SafetyDeclaringHandler {
        tool_name: &'static str,
        safety: Option<crate::loop_::types::ToolSafety>,
    }

    #[async_trait]
    impl ExternalToolHandler for SafetyDeclaringHandler {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "stub handler"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(
            &self,
            _ctx: &ExternalToolContext,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({}))
        }
        fn safety(&self) -> crate::loop_::types::ToolSafety {
            self.safety.unwrap_or_else(|| crate::loop_::types::classify_tool_safety(self.name()))
        }
    }

    #[test]
    fn test_iot_tool_adapter_carries_handler_declared_safety() {
        let declared = SafetyDeclaringHandler {
            tool_name: "get_thing",
            safety: Some(crate::loop_::types::ToolSafety::Destructive),
        };
        let adapter = IoTToolAdapter::new(
            declared.name().to_string(),
            declared.description().to_string(),
            declared.input_schema(),
            Arc::new(declared),
            "ws".to_string(),
        );
        assert_eq!(adapter.safety(), crate::loop_::types::ToolSafety::Destructive);

        let defaulted = SafetyDeclaringHandler { tool_name: "get_thing", safety: None };
        let adapter = IoTToolAdapter::new(
            defaulted.name().to_string(),
            defaulted.description().to_string(),
            defaulted.input_schema(),
            Arc::new(defaulted),
            "ws".to_string(),
        );
        // No declaration → name-pattern classification applies.
        assert_eq!(adapter.safety(), crate::loop_::types::ToolSafety::ReadOnly);
    }
}
