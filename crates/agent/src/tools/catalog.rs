//! 工具目录 — 分组/标签/危险推断 + 动态目录构建 + 静态兜底目录
//!（Task 14 自 apps/cloud `host/tools/service.rs` 与 `host/shared/mod.rs` 迁入）。

use std::collections::HashMap;

use super::registry::ToolRegistry;

/// 根据工具名称推断是否危险操作 (mirrored from cloud's `mcp::tool_metadata`
/// until the MCP plane is reclaimed — keep the patterns in sync).
fn name_infers_destructive(name: &str) -> bool {
    name.starts_with("delete_")
        || name.starts_with("remove_")
        || name.starts_with("unload_")
        || name.contains("firmware")
        || name.contains("reset")
        || name.contains("reboot")
        || name.contains("factory")
}

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
    } else if name.starts_with("search_") || matches!(name, "read_properties" | "write_properties" | "send_command") {
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

impl ToolRegistry {
    /// Build the tool catalog dynamically from the external (MCP) registry.
    ///
    /// Falls back to the static catalog ([`build_tools_catalog_json`]) when
    /// the external registry is empty or unavailable.
    pub async fn build_catalog(&self) -> serde_json::Value {
        let mut groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

        if let Some(registry) = self.external_registry() {
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
            return build_tools_catalog_json();
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
}

/// Returns the static catalog of all available TinyIoTHub tools grouped by category.
/// Aligned with the 16 MCP-registered handlers in the composition layer's mcp plane.
pub fn build_tools_catalog_json() -> serde_json::Value {
    serde_json::json!({
        "groups": [
            {
                "id": "thing",
                "label": "设备管理",
                "source": "core",
                "tools": [
                    { "id": "search_things",    "name": "search_things",    "label": "搜索设备",         "description": "分页搜索设备列表，支持按名称、类型、状态等过滤",           "danger": false, "enabled": true  },
                    { "id": "get_thing",        "name": "get_thing",        "label": "获取设备 Profile", "description": "获取设备完整信息，包含属性定义和当前值",                   "danger": false, "enabled": true  },
                    { "id": "read_properties",  "name": "read_properties",  "label": "读取属性",         "description": "读取设备指定属性的当前值",                                   "danger": false, "enabled": true  },
                    { "id": "write_properties", "name": "write_properties", "label": "写入属性",         "description": "写入设备指定属性的值",                                       "danger": false, "enabled": true  },
                    { "id": "send_command",     "name": "send_command",     "label": "执行设备命令",     "description": "向设备下发控制命令并获取执行结果",                          "danger": false, "enabled": true  },
                    { "id": "create_thing",     "name": "create_thing",     "label": "创建设备",         "description": "根据模板创建新设备",                                        "danger": false, "enabled": true  },
                    { "id": "delete_thing",     "name": "delete_thing",     "label": "删除设备",         "description": "删除指定设备",                                              "danger": true,  "enabled": false },
                ]
            },
            {
                "id": "alarm",
                "label": "告警管理",
                "source": "core",
                "tools": [
                    { "id": "alarm_list",        "name": "alarm_list",        "label": "查询告警列表", "description": "列出当前告警和历史告警记录",                  "danger": false, "enabled": true },
                    { "id": "alarm_acknowledge", "name": "alarm_acknowledge", "label": "确认告警",     "description": "确认并关闭一条告警",                          "danger": false, "enabled": true },
                    { "id": "alarm_rule_add",    "name": "alarm_rule_add",    "label": "添加告警规则", "description": "创建新的告警规则",                            "danger": false, "enabled": true },
                ]
            },
            {
                "id": "driver",
                "label": "驱动管理",
                "source": "core",
                "tools": [
                    { "id": "list_drivers", "name": "list_drivers", "label": "查询驱动列表", "description": "列出系统中所有已注册的协议驱动（Modbus/ONVIF等）", "danger": false, "enabled": true },
                    { "id": "test_driver",  "name": "test_driver",  "label": "测试驱动",     "description": "测试驱动的连接状态",                             "danger": false, "enabled": true },
                ]
            },
            {
                "id": "job",
                "label": "任务管理",
                "source": "core",
                "tools": [
                    { "id": "list_schedules",   "name": "list_schedules",   "label": "查询任务列表",   "description": "列出系统中所有调度任务",                "danger": false, "enabled": true },
                    { "id": "create_schedule",  "name": "create_schedule",  "label": "创建调度任务",   "description": "创建新的调度任务",                      "danger": false, "enabled": true },
                    { "id": "update_schedule",  "name": "update_schedule",  "label": "更新调度任务",   "description": "更新已有调度任务的配置",                "danger": false, "enabled": true },
                    { "id": "delete_schedule",  "name": "delete_schedule",  "label": "删除调度任务",   "description": "删除指定的调度任务",                    "danger": true,  "enabled": false },
                ]
            },
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_build_catalog_fallback() {
        // When no external registry is wired, should return static catalog
        let registry = ToolRegistry::default();
        let catalog = registry.build_catalog().await;
        let groups = catalog["groups"].as_array().unwrap();
        assert!(!groups.is_empty(), "Static catalog should have groups");
        let group_ids: Vec<&str> = groups.iter().filter_map(|g| g["id"].as_str()).collect();
        assert!(group_ids.contains(&"thing"));
        assert!(group_ids.contains(&"alarm"));
    }

    #[test]
    fn catalog_tool_ids_match_mcp_registry_names() {
        let catalog = build_tools_catalog_json();
        let ids: Vec<&str> = catalog["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["tools"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()))
            .collect();
        for expected in [
            "search_things",
            "get_thing",
            "create_thing",
            "delete_thing",
            "read_properties",
            "write_properties",
            "send_command",
        ] {
            assert!(ids.contains(&expected), "catalog missing MCP name {expected}");
        }
        for stale in ["search_devices", "get_device", "create_device", "delete_device"] {
            assert!(!ids.contains(&stale), "stale name {stale} leaked");
        }
    }

    #[test]
    fn test_tools_catalog_structure() {
        let catalog = build_tools_catalog_json();
        let obj = catalog.as_object().expect("should be an object");

        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("catalog should have 'groups' array");

        assert!(!groups.is_empty(), "catalog should have at least one tool group");

        let group_ids: Vec<&str> = groups
            .iter()
            .filter_map(|g| g.get("id").and_then(|v| v.as_str()))
            .collect();

        assert!(group_ids.contains(&"thing"), "catalog should have a 'thing' group");
        assert!(group_ids.contains(&"alarm"), "catalog should have an 'alarm' group");
        assert!(group_ids.contains(&"driver"), "catalog should have a 'driver' group");
        assert!(group_ids.contains(&"job"), "catalog should have a 'job' group");

        for group in groups {
            let g_obj = group.as_object().expect("group should be an object");
            assert!(g_obj.contains_key("id"), "group should have 'id' field");
            assert!(g_obj.contains_key("label"), "group should have 'label' field");
            assert!(g_obj.contains_key("tools"), "group should have 'tools' field");

            let tools = g_obj
                .get("tools")
                .and_then(|v| v.as_array())
                .expect("tools should be an array");

            for tool in tools {
                let t_obj = tool.as_object().expect("tool should be an object");
                assert!(t_obj.contains_key("id"), "tool should have 'id' field");
                assert!(t_obj.contains_key("danger"), "tool should have 'danger' field");
                assert!(t_obj.contains_key("enabled"), "tool should have 'enabled' field");
            }
        }
    }

    #[test]
    fn test_tools_catalog_dangerous_tools_are_disabled_by_default() {
        let catalog = build_tools_catalog_json();
        let groups = catalog
            .as_object()
            .and_then(|v| v.get("groups"))
            .and_then(|v| v.as_array())
            .expect("catalog should have groups");

        for group in groups {
            let tools = group
                .as_object()
                .and_then(|v| v.get("tools"))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            for tool in tools {
                let is_dangerous = tool
                    .as_object()
                    .and_then(|v| v.get("danger"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_enabled = tool
                    .as_object()
                    .and_then(|v| v.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_dangerous {
                    assert!(!is_enabled, "dangerous tool {:?} should be disabled by default", tool);
                }
            }
        }
    }
}
