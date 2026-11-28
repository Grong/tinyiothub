//! 工具目录 — 分组/标签/危险推断 + 动态目录构建 + 静态兜底目录
//!（Task 14 自 apps/cloud `host/tools/service.rs` 与 `host/shared/mod.rs` 迁入）。

use tinyiothub_skills::registry::ToolRegistry as _;

use super::registry::ToolRegistry;

/// One stable tool identity. `name` is the canonical/model-visible name where
/// applicable; `aliases` stay executable for approval replay but are not
/// exposed to the model. `view_key` is the frontend contract.
#[derive(Debug, Clone, Copy)]
pub struct ToolCatalogEntry {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub view_key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub group_label: &'static str,
    pub description: &'static str,
    pub danger: bool,
    pub visible: bool,
}

pub const TOOL_CATALOG: &[ToolCatalogEntry] = &[
    ToolCatalogEntry {
        name: "list_devices",
        aliases: &["list_things", "search_things"],
        view_key: "device.list",
        label: "列出设备",
        group: "device",
        group_label: "设备管理",
        description: "列出当前工作区设备，可按名称、类型、状态过滤",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "get_device_profile",
        aliases: &["get_thing_profile"],
        view_key: "device.profile",
        label: "读取设备档案",
        group: "device",
        group_label: "设备管理",
        description: "一次读取设备/空间的完整档案：属性定义与当前值、动作、知识文档与最近事件",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "read_device",
        aliases: &["get_thing"],
        view_key: "device.read",
        label: "读取设备",
        group: "device",
        group_label: "设备管理",
        description: "读取设备实时状态、健康度、空间归属和属性值",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "read_device_knowledge",
        aliases: &[],
        view_key: "device.knowledge",
        label: "读取设备知识文档",
        group: "device",
        group_label: "设备管理",
        description: "按需读取指定设备关联的知识文档正文",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "read_property",
        aliases: &["read_properties"],
        view_key: "property.read",
        label: "读取属性",
        group: "property",
        group_label: "设备管理",
        description: "读取单个设备属性的当前值（旧审批别名保留）",
        danger: false,
        visible: false,
    },
    ToolCatalogEntry {
        name: "set_device",
        aliases: &["write_properties"],
        view_key: "device.write",
        label: "写入设备属性",
        group: "device",
        group_label: "设备管理",
        description: "写入设备属性并等待回读验证",
        danger: true,
        visible: true,
    },
    ToolCatalogEntry {
        name: "call_device",
        aliases: &["invoke_action", "send_command"],
        view_key: "device.action",
        label: "执行设备操作",
        group: "device",
        group_label: "设备管理",
        description: "向设备下发动作/命令，受策略、审批和 Effect 约束",
        danger: true,
        visible: true,
    },
    ToolCatalogEntry {
        name: "stop",
        aliases: &[],
        view_key: "device.stop",
        label: "停止设备操作",
        group: "device",
        group_label: "设备管理",
        description: "停止当前设备动作或流程",
        danger: true,
        visible: true,
    },
    ToolCatalogEntry {
        name: "watch",
        aliases: &[],
        view_key: "watch.create",
        label: "建立条件等待",
        group: "device",
        group_label: "设备管理",
        description: "等待设备属性或事件满足条件",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "unwatch",
        aliases: &[],
        view_key: "watch.remove",
        label: "取消条件等待",
        group: "device",
        group_label: "设备管理",
        description: "取消之前建立的条件等待",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "get_thing_tree",
        aliases: &[],
        view_key: "thing.tree",
        label: "物层级树",
        group: "thing",
        group_label: "物本体",
        description: "读取空间、楼层、产线和设备的层级关系",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "query_events",
        aliases: &["get_device_history"],
        view_key: "event.history",
        label: "查询事件",
        group: "event",
        group_label: "事件",
        description: "按设备、时间范围查询事件与历史",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "alarm_list",
        aliases: &["list_alarms"],
        view_key: "alarm.list",
        label: "查询告警列表",
        group: "alarm",
        group_label: "告警管理",
        description: "列出当前告警和历史告警记录",
        danger: false,
        visible: false,
    },
    ToolCatalogEntry {
        name: "alarm_acknowledge",
        aliases: &[],
        view_key: "alarm.action",
        label: "确认告警",
        group: "alarm",
        group_label: "告警管理",
        description: "确认或关闭一条告警",
        danger: false,
        visible: false,
    },
    ToolCatalogEntry {
        name: "search_knowledge",
        aliases: &[],
        view_key: "knowledge.search",
        label: "搜索知识库",
        group: "knowledge",
        group_label: "知识库",
        description: "搜索工作区知识文档",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "read_document",
        aliases: &[],
        view_key: "knowledge.document",
        label: "读取文档",
        group: "knowledge",
        group_label: "知识库",
        description: "读取知识文档正文",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "list_composites",
        aliases: &[],
        view_key: "composite.list",
        label: "列出动作链",
        group: "device",
        group_label: "设备管理",
        description: "列出可执行的动作链",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "run_composite",
        aliases: &[],
        view_key: "composite.run",
        label: "执行动作链",
        group: "device",
        group_label: "设备管理",
        description: "执行一个动作链",
        danger: true,
        visible: true,
    },
    ToolCatalogEntry {
        name: "todo_write",
        aliases: &["update_todo"],
        view_key: "todo.write",
        label: "更新任务计划",
        group: "workflow",
        group_label: "工作流",
        description: "用完整列表替换当前任务计划",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "ask_user",
        aliases: &[],
        view_key: "interaction.ask",
        label: "请求用户决定",
        group: "workflow",
        group_label: "工作流",
        description: "向用户提问并等待回答",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "spawn_subagent",
        aliases: &[],
        view_key: "subagent.spawn",
        label: "派生子代理",
        group: "workflow",
        group_label: "工作流",
        description: "派生子代理执行独立任务",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "get_skill",
        aliases: &[],
        view_key: "skill.load",
        label: "加载技能",
        group: "workflow",
        group_label: "工作流",
        description: "加载任务所需技能说明",
        danger: false,
        visible: true,
    },
    ToolCatalogEntry {
        name: "complete_skill",
        aliases: &[],
        view_key: "skill.complete",
        label: "结算技能",
        group: "workflow",
        group_label: "工作流",
        description: "结算当前技能并加载下一个",
        danger: false,
        visible: true,
    },
];

/// 平台扩展平面（MCP registry，见 `domains/mcp/tools/`）的模型可见工具名。
/// 这些工具不属静态目录，是否可用取决于当前工作区工具列表；技能正文引用它们时
/// 必须写明"仅在当前工具列表提供时可用"。
pub const PLATFORM_PLANE_TOOLS: &[&str] = &[
    "alarm_list",
    "alarm_acknowledge",
    "alarm_rule_add",
    "list_drivers",
    "test_driver",
    "list_schedules",
    "create_schedule",
    "update_schedule",
    "delete_schedule",
    "search_workspace_resources",
];

pub fn catalog_entry(name: &str) -> Option<&'static ToolCatalogEntry> {
    TOOL_CATALOG
        .iter()
        .find(|entry| entry.name == name || entry.aliases.contains(&name))
}

pub fn canonical_tool_name(name: &str) -> &str {
    catalog_entry(name).map_or(name, |entry| entry.name)
}

pub fn tool_view_key(name: &str) -> &'static str {
    catalog_entry(name).map_or("generic", |entry| entry.view_key)
}

/// 根据工具名称推断是否危险操作 (mirrored from cloud's `mcp::tool_metadata`
/// until the MCP plane is reclaimed — keep the patterns in sync).
pub(crate) fn name_infers_destructive(name: &str) -> bool {
    name.starts_with("delete_")
        || name.starts_with("remove_")
        || name.starts_with("unload_")
        || name.contains("firmware")
        || name.contains("reset")
        || name.contains("reboot")
        || name.contains("factory")
}

/// Label mapping for known tools (display name in Chinese).
pub(crate) fn tool_label(name: &str) -> &str {
    if let Some(entry) = catalog_entry(name) {
        return entry.label;
    }
    match name {
        // Device-runtime tools (MCP)
        // E2 fixed device tools
        "list_devices" => "列出设备",
        "read_device" => "读取设备",
        "call_device" => "执行设备操作",
        "set_device" => "写入属性",
        "stop" => "立即停止",
        "watch" => "条件等待",
        "unwatch" => "取消条件等待",
        "list_composites" => "列出动作链",
        "run_composite" => "执行动作链",
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
pub(crate) fn tool_group(name: &str) -> (&str, &str) {
    if let Some(entry) = catalog_entry(name) {
        return (entry.group, entry.group_label);
    }
    if name == "search_workspace_resources" {
        ("workspace", "工作空间")
    } else if matches!(
        name,
        "list_devices"
            | "read_device"
            | "call_device"
            | "set_device"
            | "stop"
            | "watch"
            | "unwatch"
            | "list_composites"
            | "run_composite"
    ) || name.starts_with("search_")
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

impl ToolRegistry {
    /// Build the tool catalog from the same descriptors that drive LLM
    /// exposure, policy, and redaction. Built-in and external tools are both
    /// included; hidden approval-only aliases are excluded by the descriptor.
    ///
    /// Falls back to the static catalog ([`build_tools_catalog_json`]) only
    /// when no providers/external tools are registered.
    pub async fn build_catalog(
        &self,
        workspace_id: &str,
        runtime: &super::context::ToolRuntimeContext,
    ) -> serde_json::Value {
        let descriptors = self.descriptors(workspace_id, runtime).await;
        if descriptors.is_empty() {
            return build_tools_catalog_json();
        }
        tinyiothub_skills::registry::DeclaredToolRegistry::new(descriptors).catalog()
    }
}

/// Returns the static catalog of all available TinyIoTHub tools grouped by category.
/// Aligned with the 16 MCP-registered handlers in the composition layer's mcp plane.
pub fn build_tools_catalog_json() -> serde_json::Value {
    let mut groups: std::collections::BTreeMap<&'static str, (&'static str, Vec<serde_json::Value>)> =
        std::collections::BTreeMap::new();
    for entry in TOOL_CATALOG.iter().filter(|entry| entry.visible) {
        let (_, tools) = groups
            .entry(entry.group)
            .or_insert_with(|| (entry.group_label, Vec::new()));
        tools.push(serde_json::json!({
            "id": entry.name,
            "name": entry.name,
            "label": entry.label,
            "description": entry.description,
            "viewKey": entry.view_key,
            "danger": entry.danger,
            "enabled": !entry.danger,
        }));
    }
    serde_json::json!({
        "groups": groups
            .into_iter()
            .map(|(id, (label, tools))| serde_json::json!({
                "id": id,
                "label": label,
                "source": "core",
                "tools": tools,
            }))
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_label_mapping() {
        assert_eq!(tool_label("list_devices"), "列出设备");
        assert_eq!(tool_label("read_device"), "读取设备");
        assert_eq!(tool_label("watch"), "建立条件等待");
        assert_eq!(tool_label("search_things"), "列出设备");
        assert_eq!(tool_label("get_thing"), "读取设备");
        assert_eq!(tool_label("alarm_list"), "查询告警列表");
        assert_eq!(tool_label("list_drivers"), "查询驱动列表");
        assert_eq!(tool_label("list_schedules"), "查询任务列表");
        // Unknown tool returns its name as label
        assert_eq!(tool_label("unknown_tool"), "unknown_tool");
    }

    #[test]
    fn tool_catalog_has_unique_names_aliases_and_view_keys() {
        let mut names = std::collections::HashSet::new();
        let mut aliases = std::collections::HashSet::new();
        for entry in TOOL_CATALOG {
            assert!(!entry.name.is_empty());
            assert!(!entry.view_key.is_empty());
            assert!(names.insert(entry.name), "duplicate canonical name {}", entry.name);
            for alias in entry.aliases {
                assert!(!alias.is_empty());
                assert!(!names.contains(*alias), "alias {alias} collides with a canonical name");
                assert!(aliases.insert(*alias), "duplicate alias {alias}");
            }
        }
        assert_eq!(canonical_tool_name("get_thing"), "read_device");
        assert_eq!(canonical_tool_name("invoke_action"), "call_device");
        assert_eq!(canonical_tool_name("get_thing_profile"), "get_device_profile");
        assert_eq!(tool_view_key("get_thing_profile"), "device.profile");
        assert_eq!(tool_view_key("alarm_list"), "alarm.list");
        assert_eq!(tool_view_key("unknown_tool"), "generic");
    }

    #[test]
    fn test_tool_group_classification() {
        assert_eq!(tool_group("search_things"), ("device", "设备管理"));
        assert_eq!(tool_group("get_thing"), ("device", "设备管理"));
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
        let catalog = registry
            .build_catalog("ws", &crate::domains::agent::tools::ToolRuntimeContext::default())
            .await;
        let groups = catalog["groups"].as_array().unwrap();
        assert!(!groups.is_empty(), "Static catalog should have groups");
        let group_ids: Vec<&str> = groups.iter().filter_map(|g| g["id"].as_str()).collect();
        assert!(group_ids.contains(&"device"));
        assert!(group_ids.contains(&"workflow"));
    }

    #[test]
    fn catalog_uses_only_canonical_visible_tool_names() {
        let catalog = build_tools_catalog_json();
        let tools: Vec<&serde_json::Value> = catalog["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["tools"].as_array().unwrap())
            .collect();
        let ids: Vec<&str> = tools.iter().filter_map(|t| t["id"].as_str()).collect();
        for expected in [
            "list_devices",
            "read_device",
            "set_device",
            "call_device",
            "stop",
            "watch",
            "unwatch",
            "get_thing_tree",
            "query_events",
            "search_knowledge",
            "read_document",
            "list_composites",
            "run_composite",
            "todo_write",
            "ask_user",
            "spawn_subagent",
            "get_skill",
        ] {
            assert!(ids.contains(&expected), "catalog missing canonical name {expected}");
        }
        for stale in [
            "get_thing",
            "search_things",
            "list_things",
            "read_properties",
            "write_properties",
            "send_command",
            "invoke_action",
            "alarm_list",
        ] {
            assert!(!ids.contains(&stale), "hidden/legacy name {stale} leaked");
        }
        assert!(tools.iter().all(|tool| tool["viewKey"].as_str().is_some()));
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

        assert!(group_ids.contains(&"device"), "catalog should have a 'device' group");
        assert!(group_ids.contains(&"thing"), "catalog should have a 'thing' group");
        assert!(
            group_ids.contains(&"workflow"),
            "catalog should have a 'workflow' group"
        );
        assert!(
            group_ids.contains(&"knowledge"),
            "catalog should have a 'knowledge' group"
        );

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
                assert!(t_obj.contains_key("viewKey"), "tool should have 'viewKey' field");
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
