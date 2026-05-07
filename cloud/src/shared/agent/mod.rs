// Agent Runtime Module
//
// This module provides the consolidated agent runtime interface for TinyIoTHub.
// It includes:
// - AgentRuntime trait: the main interface for agent operations
// - AgentClient trait: lower-level agent operations
// - Config types: AgentConfig, AgentInfo, AgentError
// - AgentRuntimeImpl: the concrete implementation using zeroclaw

use async_trait::async_trait;

pub mod config;
pub mod heartbeat_service;
pub mod runtime;
pub mod scaffold_service;

pub use config::{AgentConfig, AgentInfo, AgentError, compute_hash, default_agent_config};
pub use heartbeat_service::HeartbeatService;
pub use runtime::AgentRuntimeImpl;

/// Trait for Agent operations — implemented by AgentRuntimeImpl
#[async_trait]
pub trait AgentClient: Send + Sync {
    /// Create a new agent for the given workspace
    async fn create_agent(&self, config: &AgentConfig) -> Result<String, AgentError>;

    /// Delete an agent by ID
    async fn delete_agent(&self, agent_id: &str) -> Result<(), AgentError>;

    /// Get agent info by ID
    async fn get_agent(&self, agent_id: &str) -> Result<AgentInfo, AgentError>;

    /// Update agent configuration
    async fn update_agent(&self, agent_id: &str, config: &str) -> Result<(), AgentError>;

    /// Send a chat message and get SSE stream response
    async fn chat_send(
        &self,
        agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
    ) -> Result<reqwest::Response, AgentError>;

    /// Get chat history
    async fn chat_history(
        &self,
        agent_id: &str,
        session_key: &str,
        limit: u32,
    ) -> Result<serde_json::Value, AgentError>;

    /// Abort a chat run
    async fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
    ) -> Result<(), AgentError>;

    /// List agents scoped to a workspace
    async fn list_agents(&self, workspace_id: &str) -> Result<serde_json::Value, AgentError>;

    /// Get agent config (verifies workspace ownership)
    async fn get_agent_config(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError>;

    /// Set agent config (verifies workspace ownership)
    async fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        base_hash: Option<&str>,
        workspace_id: &str,
    ) -> Result<(), AgentError>;

    /// Get tools catalog for an agent
    async fn tools_catalog(&self, agent_id: &str) -> Result<serde_json::Value, AgentError>;

    /// Get effective tools for an agent (verifies workspace ownership)
    async fn tools_effective(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError>;

    /// Toggle a tool on/off for an agent (verifies workspace ownership)
    async fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
        workspace_id: &str,
    ) -> Result<(), AgentError>;
}

/// Trait that consolidates all agent runtime functionality
///
/// This trait is implemented by AgentRuntimeImpl and provides:
/// - All AgentClient operations (chat, history, config, tools)
/// - Tool refresh capability (refresh_tools)
#[async_trait]
pub trait AgentRuntime: AgentClient + Send + Sync {
    /// Refresh the agent's tool registry
    async fn refresh_tools(&self) -> anyhow::Result<()>;

    /// Execute a single agent turn with the given message.
    ///
    /// This is useful for cron job execution where we want to run a prompt
    /// and get the complete response without SSE streaming.
    async fn run_single(&self, message: &str) -> Result<String, AgentError>;
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
                    { "id": "device_list",          "name": "device_list",          "label": "获取设备列表",       "description": "分页查询设备列表，支持按名称、类型、状态等过滤",          "danger": false, "enabled": true  },
                    { "id": "device_profile",       "name": "device_profile",       "label": "获取设备 Profile",   "description": "获取设备完整信息，包含属性定义和当前值",                 "danger": false, "enabled": true  },
                    { "id": "device_property_get",  "name": "device_property_get",  "label": "获取属性详情",       "description": "获取设备指定属性的定义信息（类型、单位、读写权限等）",    "danger": false, "enabled": true  },
                    { "id": "device_create",        "name": "device_create",        "label": "根据模板创建设备",   "description": "基于设备模板创建新设备，需先查询模板列表获取template_id", "danger": false, "enabled": true  },
                    { "id": "device_command",       "name": "device_command",       "label": "执行设备命令",       "description": "向设备下发控制命令并获取执行结果",                        "danger": false, "enabled": true  },
                    { "id": "device_template_list", "name": "device_template_list", "label": "查询设备模板列表",   "description": "列出系统中所有可用的设备模板，用于创建设备前查询template_id", "danger": false, "enabled": true },
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
                "id": "workspace",
                "label": "工作空间",
                "source": "core",
                "tools": [
                    { "id": "workspace_list",   "name": "workspace_list",   "label": "查询工作空间列表", "description": "列出所有工作空间",                  "danger": false, "enabled": true },
                    { "id": "workspace_get",    "name": "workspace_get",    "label": "获取工作空间详情", "description": "获取指定工作空间的详细信息",      "danger": false, "enabled": true },
                    { "id": "workspace_create", "name": "workspace_create", "label": "创建工作空间",     "description": "创建新的工作空间",                  "danger": false, "enabled": true },
                    { "id": "workspace_update", "name": "workspace_update", "label": "更新工作空间",     "description": "更新工作空间配置",                  "danger": false, "enabled": true },
                    { "id": "workspace_delete", "name": "workspace_delete", "label": "删除工作空间",     "description": "删除指定工作空间",                  "danger": true,  "enabled": false },
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
        ]
    })
}

/// Build the full system prompt by combining workspace files + skills + dynamic context
///
/// Prompt layers (in order):
/// 1. [Identity]    — from IDENTITY.md (who am I)
/// 2. [Principles]  — from SOUL.md (how I behave)
/// 3. [Capabilities] — from TOOLS.md (what I can do)
/// 4. [Skills]      — from skills/*.md (specialized workflows)
/// 5. [Memory]      — from MEMORY.md (curated long-term memory)
/// 6. [User]        — from USER.md (who I'm helping)
/// 7. [Persona]     — user persona override or default from config
/// 8. [Context]     — dynamic context (device snapshots, etc.)
pub async fn build_full_system_prompt(
    system_prompts: &crate::shared::config::SystemPromptsConfig,
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let workspace_dir = get_workspace_dir(system_prompts, workspace_id);

    // Layer 1-6: Load from workspace files
    let workspace_prompt = load_workspace_prompt(&workspace_dir).await;

    // Layer 7: Persona override (user can override the default persona from config)
    // Only add persona layer if user provided one explicitly (via systemPrompt field)
    let persona_layer = if !user_persona.trim().is_empty() {
        format!("\n\n## Agent Persona（用户配置）\n{}\n", user_persona)
    } else {
        String::new()
    };

    // Skills from skills/ directory (existing behavior)
    let skills_layer = load_skills_prompt(workspace_id, agent_id).await;

    // Layer 8: Additional context from config (device snapshots injected at runtime)
    let context_layer = if !system_prompts.context.is_empty() {
        format!("\n\n## 当前状态上下文\n{}\n", system_prompts.context)
    } else {
        String::new()
    };

    let full_prompt = format!("{}{}{}{}", workspace_prompt, persona_layer, skills_layer, context_layer);
    tracing::info!("[SYSTEM_PROMPT]\n{}", full_prompt);
    full_prompt
}

/// Get the workspace directory path for loading prompt files.
///
/// Uses system_prompts.workspace_dir as the base path and appends workspace_id.
fn get_workspace_dir(system_prompts: &crate::shared::config::SystemPromptsConfig, workspace_id: Option<&str>) -> std::path::PathBuf {
    use crate::shared::paths::DEFAULT_WORKSPACE_ID;
    let ws = workspace_id.unwrap_or(DEFAULT_WORKSPACE_ID);
    let base = &system_prompts.workspace_dir;
    if base.is_empty() {
        crate::shared::paths::workspace_dir(ws)
    } else {
        std::path::PathBuf::from(base).join(ws)
    }
}

/// Load workspace prompt files and concatenate them into a single prompt
///
/// Files loaded (in order):
/// - IDENTITY.md  → [Identity] section
/// - SOUL.md      → [Principles] section
/// - TOOLS.md     → [Capabilities] section
/// - USER.md      → [User Context] section
/// - MEMORY.md    → [Memory] section
///
/// Each file is wrapped with a markdown header indicating its section.
async fn load_workspace_prompt(workspace_dir: &std::path::Path) -> String {
    use tokio::fs;

    let mut sections = Vec::new();

    // Define workspace files and their section names
    let files = [
        ("IDENTITY.md", "Identity"),
        ("SOUL.md", "Principles"),
        ("TOOLS.md", "Capabilities"),
        ("USER.md", "User Context"),
        ("MEMORY.md", "Memory"),
    ];

    for (filename, section_name) in files {
        let file_path = workspace_dir.join(filename);
        if file_path.exists()
            && let Ok(content) = fs::read_to_string(&file_path).await {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    sections.push(format!("## {}\n{}\n", section_name, trimmed));
                }
            }
    }

    if sections.is_empty() {
        // Fallback to template files if workspace is empty
        return load_template_fallback();
    }

    sections.join("\n")
}

/// Fallback: load from embedded template files when workspace directory has no files
fn load_template_fallback() -> String {
    let mut sections = Vec::new();

    let files = [
        ("IDENTITY.md", "Identity"),
        ("SOUL.md", "Principles"),
        ("TOOLS.md", "Capabilities"),
        ("USER.md", "User Context"),
        ("MEMORY.md", "Memory"),
    ];

    for (filename, section_name) in files {
        // Templates are embedded at compile time in scaffold_service
        let content = get_embedded_template(filename);
        if let Some(c) = content {
            let trimmed = c.trim();
            if !trimmed.is_empty() {
                sections.push(format!("## {}\n{}\n", section_name, trimmed));
            }
        }
    }

    sections.join("\n")
}

/// Get embedded template content by filename
fn get_embedded_template(filename: &str) -> Option<&'static str> {
    match filename {
        "IDENTITY.md" => Some(include_str!("../../../templates/agent/IDENTITY.md")),
        "SOUL.md" => Some(include_str!("../../../templates/agent/SOUL.md")),
        "TOOLS.md" => Some(include_str!("../../../templates/agent/TOOLS.md")),
        "USER.md" => Some(include_str!("../../../templates/agent/USER.md")),
        "MEMORY.md" => Some(include_str!("../../../templates/agent/MEMORY.md")),
        _ => None,
    }
}

/// Load skill files and format as Layer 3 prompt.
///
/// Skills directory structure:
/// - Workspace-specific: ./data/agents/{workspace_id}/skills/  (priority)
/// - Global fallback:    ./data/skills/                         (all workspaces share)
///
/// For workspace-specific skills, also checks agent subdirectory:
/// - ./data/agents/{workspace_id}/{agent_id}/skills/
/// - ./data/agents/{workspace_id}/skills/
async fn load_skills_prompt(workspace_id: Option<&str>, agent_id: Option<&str>) -> String {
    use crate::shared::paths::{global_skills_dir, workspace_skills_dir, agent_skills_dir, DEFAULT_WORKSPACE_ID};

    let _ws = workspace_id.unwrap_or(DEFAULT_WORKSPACE_ID);

    // Build candidate paths: workspace-specific first, then global
    let candidates: Vec<std::path::PathBuf> = match (workspace_id, agent_id) {
        (Some(w), Some(a)) => vec![
            // Workspace + agent specific skills
            agent_skills_dir(w, a),
            workspace_skills_dir(w),
            // Global skills
            global_skills_dir(),
        ],
        (Some(w), None) => vec![
            // Workspace-specific skills (no agent)
            workspace_skills_dir(w),
            // Global skills
            global_skills_dir(),
        ],
        _ => vec![
            // No workspace: just global skills
            global_skills_dir(),
        ],
    };

    for dir in candidates {
        if dir.exists() {
            let result = read_skill_dir(&dir).await;
            if !result.is_empty() {
                return result;
            }
        }
    }

    String::new()
}

async fn read_skill_dir(dir: &std::path::Path) -> String {
    use crate::modules::agent::skill::AgentSkill;
    use tokio::fs;

    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
            return String::new();
        }
    };

    let mut skill_files: Vec<_> = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
        if entry.path().extension().is_some_and(|ext| ext == "md") {
            skill_files.push(entry);
        }
    }

    skill_files.sort_by_key(|e| e.file_name().to_string_lossy().to_string());

    let mut all_skills = String::new();

    for entry in skill_files {
        let content = match fs::read_to_string(entry.path()).await {
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
