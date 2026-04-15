// Agent Runtime Module
//
// This module provides the consolidated agent runtime interface for TinyIoTHub.
// It includes:
// - AgentRuntime trait: the main interface for agent operations
// - AgentClient trait: lower-level agent operations
// - Config types: AgentConfig, AgentInfo, AgentError
// - AgentRuntimeImpl: the concrete implementation using zeroclaw

use std::pin::Pin;

pub mod config;
pub mod heartbeat_service;
pub mod runtime;
pub mod scaffold_service;

pub use config::{AgentConfig, AgentInfo, AgentError, compute_hash, default_agent_config};
pub use heartbeat_service::HeartbeatService;
pub use runtime::AgentRuntimeImpl;

/// Trait for Agent operations — implemented by AgentRuntimeImpl
pub trait AgentClient: Send + Sync {
    /// Create a new agent for the given workspace
    fn create_agent(
        &self,
        config: &AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>>;

    /// Delete an agent by ID
    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Get agent info by ID
    fn get_agent(
        &self,
        agent_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>>;

    /// Update agent configuration
    fn update_agent(
        &self,
        agent_id: &str,
        config: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Send a chat message and get SSE stream response
    fn chat_send(
        &self,
        agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, AgentError>> + Send + '_>>;

    /// Get chat history
    fn chat_history(
        &self,
        agent_id: &str,
        session_key: &str,
        limit: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Abort a chat run
    fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// List all agents
    fn list_agents(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Get agent config
    fn get_agent_config(
        &self,
        agent_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Set agent config
    fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        base_hash: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Get tools catalog for an agent
    fn tools_catalog(
        &self,
        agent_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Get effective tools for an agent
    fn tools_effective(
        &self,
        agent_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Toggle a tool on/off for an agent
    fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;
}

/// Trait that consolidates all agent runtime functionality
///
/// This trait is implemented by AgentRuntimeImpl and provides:
/// - All AgentClient operations (chat, history, config, tools)
/// - Tool refresh capability (refresh_tools)
pub trait AgentRuntime: AgentClient + Send + Sync {
    /// Refresh the agent's tool registry
    fn refresh_tools(&self) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>>;
}

/// Returns the static catalog of all available TinyIoTHub tools grouped by category.
pub fn build_tools_catalog_json() -> serde_json::Value {
    serde_json::json!({
        "groups": [
            {
                "id": "device",
                "label": "设备管理",
                "source": "core",
                "tools": [
                    { "id": "device_list",           "name": "device_list",           "label": "查询设备列表",     "description": "列出所有已注册的IoT设备，支持分页和过滤",           "danger": false, "enabled": true  },
                    { "id": "device_get",             "name": "device_get",             "label": "获取设备详情",     "description": "根据设备ID获取设备完整信息",                          "danger": false, "enabled": true  },
                    { "id": "device_create",          "name": "device_create",          "label": "创建设备",         "description": "注册一个新的IoT设备到系统",                           "danger": false, "enabled": true  },
                    { "id": "device_update",          "name": "device_update",          "label": "更新设备",         "description": "更新已有设备的基本信息或配置",                       "danger": false, "enabled": true  },
                    { "id": "device_delete",          "name": "device_delete",          "label": "删除设备",         "description": "永久删除一个设备及其所有数据",                        "danger": true,  "enabled": false },
                    { "id": "device_read",            "name": "device_read",            "label": "读取设备属性",     "description": "从设备读取当前属性/遥测数据",                       "danger": false, "enabled": true  },
                    { "id": "device_write",           "name": "device_write",           "label": "写入设备属性",     "description": "向设备写入属性值或下发控制指令",                    "danger": false, "enabled": true  },
                    { "id": "device_batch_read",      "name": "device_batch_read",      "label": "批量读取设备",     "description": "批量读取多个设备的属性数据",                       "danger": false, "enabled": true  },
                    { "id": "batch_command_execute",  "name": "batch_command_execute",  "label": "批量执行命令",     "description": "向多个设备批量下发控制命令",                        "danger": true,  "enabled": false },
                    { "id": "batch_property_write",   "name": "batch_property_write",   "label": "批量写入属性",     "description": "批量写入多个设备的属性值",                          "danger": true,  "enabled": false },
                    { "id": "device_template_list",   "name": "device_template_list",   "label": "查询设备模板",     "description": "列出系统中所有设备模板",                            "danger": false, "enabled": true  },
                ]
            },
            {
                "id": "alarm",
                "label": "告警管理",
                "source": "core",
                "tools": [
                    { "id": "alarm_list",      "name": "alarm_list",      "label": "查询告警列表",  "description": "列出当前告警和历史告警记录",                  "danger": false, "enabled": true },
                    { "id": "alarm_get",       "name": "alarm_get",       "label": "获取告警详情",  "description": "获取指定告警的详细信息",                    "danger": false, "enabled": true },
                    { "id": "alarm_ack",       "name": "alarm_ack",       "label": "确认告警",      "description": "确认并关闭一条告警",                      "danger": false, "enabled": true },
                    { "id": "alarm_rule_list", "name": "alarm_rule_list", "label": "查询告警规则",  "description": "列出系统中所有告警规则",                  "danger": false, "enabled": true },
                    { "id": "alarm_stats",     "name": "alarm_stats",     "label": "告警统计",      "description": "获取告警统计摘要（总数、等级分布等）",      "danger": false, "enabled": true },
                ]
            },
            {
                "id": "workspace",
                "label": "工作空间",
                "source": "core",
                "tools": [
                    { "id": "workspace_list",    "name": "workspace_list",    "label": "查询工作空间", "description": "列出当前用户所属的所有工作空间",           "danger": false, "enabled": true },
                    { "id": "workspace_get",    "name": "workspace_get",    "label": "获取工作空间", "description": "获取指定工作空间的详细信息",           "danger": false, "enabled": true },
                    { "id": "workspace_create", "name": "workspace_create", "label": "创建工作空间", "description": "创建一个新的工作空间",                   "danger": false, "enabled": true },
                    { "id": "workspace_update", "name": "workspace_update", "label": "更新工作空间", "description": "更新工作空间的名称、描述等",           "danger": false, "enabled": true },
                    { "id": "workspace_delete", "name": "workspace_delete", "label": "删除工作空间", "description": "删除指定工作空间（不可恢复）",           "danger": true,  "enabled": false },
                    { "id": "agent_list",       "name": "agent_list",       "label": "查询 Agent",   "description": "列出当前工作空间中的所有 Agent 实例",   "danger": false, "enabled": true },
                ]
            },
            {
                "id": "monitoring",
                "label": "系统监控",
                "source": "core",
                "tools": [
                    { "id": "system_health", "name": "system_health", "label": "系统健康检查", "description": "查询系统各组件的运行状态和健康度",    "danger": false, "enabled": true },
                    { "id": "event_list",    "name": "event_list",    "label": "查询事件列表", "description": "列出系统事件日志，支持过滤和分页",  "danger": false, "enabled": true },
                ]
            },
            {
                "id": "driver",
                "label": "驱动管理",
                "source": "core",
                "tools": [
                    { "id": "driver_list", "name": "driver_list", "label": "查询驱动列表", "description": "列出系统中所有已注册的协议驱动（Modbus/ONVIF等）", "danger": false, "enabled": true },
                    { "id": "driver_get",  "name": "driver_get",  "label": "获取驱动详情", "description": "获取指定驱动的配置和状态信息",                     "danger": false, "enabled": true },
                ]
            },
            {
                "id": "job",
                "label": "任务管理",
                "source": "core",
                "tools": [
                    { "id": "job_list",   "name": "job_list",   "label": "查询任务列表", "description": "列出系统中所有调度任务",                    "danger": false, "enabled": true },
                    { "id": "job_get",    "name": "job_get",    "label": "获取任务详情", "description": "获取指定任务的执行状态和历史记录",          "danger": false, "enabled": true },
                    { "id": "job_cancel", "name": "job_cancel", "label": "取消任务",     "description": "取消一个正在等待或运行中的调度任务",        "danger": true,  "enabled": false },
                ]
            },
            {
                "id": "mcp",
                "label": "MCP 工具",
                "source": "plugin",
                "tools": [
                    { "id": "mcp_workspace_list", "name": "mcp_workspace_list", "label": "查询 MCP 工作空间", "description": "列出 AI Agent 可用的 MCP 工作空间资源", "danger": false, "enabled": true },
                    { "id": "mcp_workspace_read", "name": "mcp_workspace_read", "label": "读取 MCP 资源",      "description": "读取 MCP 工作空间中的文件或配置资源",  "danger": false, "enabled": true },
                ]
            },
        ]
    })
}

/// Build the full system prompt by combining Layer 1 (platform base) + Layer 2 (user persona) + Layer 3 (skills)
///
/// - layer1: from SystemPromptsConfig.base (platform identity, protocols, A2UI)
/// - layer2: user persona (from agent config systemPrompt field)
/// - layer3: skills loaded from filesystem
pub fn build_full_system_prompt(
    system_prompts: &crate::infrastructure::config::SystemPromptsConfig,
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let base = &system_prompts.base;

    // Layer 2: user persona — falls back to config persona if not provided
    let layer2 = if !user_persona.trim().is_empty() {
        format!("\n\n## Agent 灵魂设定（用户配置）\n{}\n", user_persona)
    } else if !system_prompts.persona.is_empty() {
        format!("\n\n## Agent 设定\n{}\n", system_prompts.persona)
    } else {
        String::new()
    };

    // Layer 3: skills loaded from filesystem
    let layer3 = load_skills_prompt(workspace_id, agent_id);

    // Layer 4: additional context from config
    let layer4 = if !system_prompts.context.is_empty() {
        format!("\n\n## 当前状态上下文\n{}\n", system_prompts.context)
    } else {
        String::new()
    };

    format!("{}{}{}{}", base, layer2, layer3, layer4)
}

/// Load skill files from the skills/ directory and format as Layer 3 prompt
/// Priority: skills/<ws>/<ag>/prompts/ > skills/<ws>/prompts/ > skills/<ws>/ > skills/tinyiothub/prompts/
fn load_skills_prompt(workspace_id: Option<&str>, agent_id: Option<&str>) -> String {
    

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let ws = workspace_id.unwrap_or("tinyiothub");

    // Try in order: prompts subdir first, then flat directory
    let candidates: Vec<std::path::PathBuf> = match (workspace_id, agent_id) {
        (Some(w), Some(a)) => vec![
            base.join(w).join(a).join("prompts"),
            base.join(w).join("prompts"),
            base.join(w).join(a),
            base.join(w),
        ],
        (Some(_w), None) => vec![
            base.join(ws).join("prompts"),
            base.join(ws),
        ],
        _ => vec![
            base.join("tinyiothub").join("prompts"),
            base.join("tinyiothub"),
        ],
    };

    for dir in candidates {
        if dir.exists() {
            let result = read_skill_dir(&dir);
            if !result.is_empty() {
                return result;
            }
        }
    }

    String::new()
}

fn read_skill_dir(dir: &std::path::Path) -> String {
    use crate::domain::agent::skill::AgentSkill;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
            return String::new();
        }
    };

    let mut skill_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();

    skill_files.sort_by_key(|e| e.file_name().to_string_lossy().to_string());

    let mut all_skills = String::new();

    for entry in skill_files {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy().into_owned();
        let skill_name = file_name.trim_end_matches(".md");

        let (fm, body) = AgentSkill::parse_frontmatter(&content);
        let body = body.trim();

        if body.is_empty() {
            continue;
        }

        let description = fm
            .as_ref()
            .and_then(|f| f.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or(skill_name);

        let version = fm
            .as_ref()
            .and_then(|f| f.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        all_skills.push_str(&format!(
            "### {}{}\n{}\n{}\n",
            skill_name,
            if version.is_empty() {
                String::new()
            } else {
                format!(" (v{})", version)
            },
            description,
            body
        ));
    }

    if all_skills.is_empty() {
        String::new()
    } else {
        format!("\n\n## 技能（Skills）\n你可以使用以下技能来完成任务：\n\n{}\n", all_skills)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_catalog_structure() {
        let catalog = build_tools_catalog_json();
        let obj = catalog.as_object().expect("should be an object");

        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("catalog should have 'groups' array");

        assert!(
            !groups.is_empty(),
            "catalog should have at least one tool group"
        );

        // Verify at least the device and workspace groups exist
        let group_ids: Vec<&str> = groups
            .iter()
            .filter_map(|g| g.get("id").and_then(|v| v.as_str()))
            .collect();

        assert!(
            group_ids.contains(&"device"),
            "catalog should have a 'device' group"
        );
        assert!(
            group_ids.contains(&"workspace"),
            "catalog should have a 'workspace' group"
        );

        // Verify each group has required fields
        for group in groups {
            let g_obj = group.as_object().expect("group should be an object");
            assert!(
                g_obj.contains_key("id"),
                "group should have 'id' field"
            );
            assert!(
                g_obj.contains_key("label"),
                "group should have 'label' field"
            );
            assert!(
                g_obj.contains_key("tools"),
                "group should have 'tools' field"
            );

            let tools = g_obj
                .get("tools")
                .and_then(|v| v.as_array())
                .expect("tools should be an array");

            for tool in tools {
                let t_obj = tool.as_object().expect("tool should be an object");
                assert!(
                    t_obj.contains_key("id"),
                    "tool should have 'id' field"
                );
                assert!(
                    t_obj.contains_key("danger"),
                    "tool should have 'danger' field"
                );
                assert!(
                    t_obj.contains_key("enabled"),
                    "tool should have 'enabled' field"
                );
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
                    assert!(
                        !is_enabled,
                        "dangerous tool {:?} should be disabled by default",
                        tool
                    );
                }
            }
        }
    }
}
