// ToolService — MCP loading, denylist filtering, catalog building
//
// Core tool orchestration layer. Loads tools from the MCP handler registry,
// wraps them as zeroclaw Tools, filters by denylist, and builds the tool
// catalog used by both the API and the agent runtime.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};

use super::canvas::CanvasTool;
use crate::{
    modules::{
        mcp::{
            handlers::{McpAuthContext, McpContextGuard},
            tool_metadata::{
                IoTToolMetadata, PermissionLevel, name_infers_concurrency_safe,
                name_infers_destructive, name_infers_read_only,
            },
            tool_registry::ToolHandler,
        },
        workspace::WorkspaceService,
    },
    shared::agent::config::AgentRuntimeConfig,
};

// ============================================================================
// IoTToolAdapter — wraps MCP ToolHandler as zeroclaw Tool
// ============================================================================

pub struct IoTToolAdapter {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    handler: Arc<dyn ToolHandler>,
    workspace_id: String,
}

impl IoTToolAdapter {
    pub fn new(
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: Arc<dyn ToolHandler>,
        workspace_id: String,
    ) -> Self {
        Self { name, description, input_schema, handler, workspace_id }
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
        let _guard = McpContextGuard::new(McpAuthContext::for_heartbeat(
            self.workspace_id.clone(),
            "agent".to_string(),
        ));
        match self.handler.execute(args).await {
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

impl IoTToolMetadata for IoTToolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    fn is_concurrency_safe(&self, _input: &serde_json::Value) -> bool {
        name_infers_concurrency_safe(&self.name)
    }

    fn is_read_only(&self, _input: &serde_json::Value) -> bool {
        name_infers_read_only(&self.name)
    }

    fn is_destructive(&self, _input: &serde_json::Value) -> bool {
        name_infers_destructive(&self.name)
    }

    fn permission_level(&self, input: &serde_json::Value) -> PermissionLevel {
        if self.is_destructive(input) {
            PermissionLevel::Ask
        } else if self.is_read_only(input) {
            PermissionLevel::Allow
        } else {
            PermissionLevel::Ask
        }
    }
}

// ============================================================================
// Tool loading
// ============================================================================

/// Load all tools: CanvasTool + MCP-registered handlers.
///
/// CanvasTool is always included first. MCP tools are loaded from the
/// global handler registry if available.
pub async fn load_all_tools(
    workspace_id: &str,
    workspace_service: Option<Arc<WorkspaceService>>,
) -> Vec<Box<dyn Tool>> {
    let mut tool_boxed: Vec<Box<dyn Tool>> = Vec::new();
    tool_boxed.push(Box::new(CanvasTool));

    if let Some(ws_svc) = workspace_service {
        tool_boxed.push(Box::new(
            super::search_resources::SearchWorkspaceResourcesTool::new(ws_svc),
        ));
    }

    if let Some(registry) = crate::modules::mcp::get_mcp_registry() {
        let reg = registry.read().await;
        for meta in reg.list_tools() {
            if meta.name.trim().is_empty() {
                continue;
            }
            let name = meta.name.clone();
            let description = meta.description.clone();
            let input_schema = meta.input_schema.clone();
            if let Some(handler) = reg.get_owned(&name) {
                tool_boxed.push(Box::new(IoTToolAdapter::new(
                    name,
                    description,
                    input_schema,
                    handler,
                    workspace_id.to_string(),
                )));
            }
        }
    }

    tool_boxed
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
pub async fn resolve_tools_for_agent(
    config: &AgentRuntimeConfig,
    workspace_id: &str,
    workspace_service: Option<Arc<WorkspaceService>>,
) -> Vec<Box<dyn Tool>> {
    let all_tools = load_all_tools(workspace_id, workspace_service).await;
    filter_by_denylist(all_tools, &config.tool_denylist)
}

// ============================================================================
// Tool catalog
// ============================================================================

/// Label mapping for known tools (display name in Chinese).
fn tool_label(name: &str) -> &str {
    match name {
        // Device tools
        "search_devices" => "搜索设备",
        "get_device" => "获取设备 Profile",
        "read_properties" => "读取属性",
        "write_properties" => "写入属性",
        "send_command" => "执行设备命令",
        "create_device" => "创建设备",
        "delete_device" => "删除设备",
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
    if name.starts_with("search_")
        || matches!(
            name,
            "get_device"
                | "read_properties"
                | "write_properties"
                | "send_command"
                | "create_device"
                | "delete_device"
        )
    {
        ("device", "设备管理")
    } else if name.starts_with("alarm_") {
        ("alarm", "告警管理")
    } else if matches!(name, "list_drivers" | "test_driver") {
        ("driver", "驱动管理")
    } else if matches!(
        name,
        "list_schedules" | "create_schedule" | "update_schedule" | "delete_schedule"
    ) {
        ("job", "任务管理")
    } else if name == "search_workspace_resources" {
        ("workspace", "工作空间")
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

    if let Some(registry) = crate::modules::mcp::get_mcp_registry() {
        let reg = registry.read().await;
        for meta in reg.list_tools() {
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
        return crate::shared::agent::build_tools_catalog_json();
    }

    let group_order = [
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
        assert_eq!(tool_label("search_devices"), "搜索设备");
        assert_eq!(tool_label("get_device"), "获取设备 Profile");
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
        assert_eq!(tool_group("search_devices"), ("device", "设备管理"));
        assert_eq!(tool_group("get_device"), ("device", "设备管理"));
        assert_eq!(tool_group("delete_device"), ("device", "设备管理"));

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
}
