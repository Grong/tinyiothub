// Agent Runtime Module
//
// Provides shared agent configuration types and system prompt building utilities.
// Runtime/execution logic lives in modules/agent/.

pub mod config;

pub use config::{
    AgentConfig, AgentError, AgentInfo, AgentRuntimeConfig, compute_hash, default_agent_config,
};

/// Returns the static catalog of all available TinyIoTHub tools grouped by category.
/// Aligned with the 16 MCP-registered handlers in modules/mcp/mod.rs.
pub fn build_tools_catalog_json() -> serde_json::Value {
    serde_json::json!({
        "groups": [
            {
                "id": "device",
                "label": "设备管理",
                "source": "core",
                "tools": [
                    { "id": "search_devices",   "name": "search_devices",   "label": "搜索设备",         "description": "分页搜索设备列表，支持按名称、类型、状态等过滤",           "danger": false, "enabled": true  },
                    { "id": "get_device",       "name": "get_device",       "label": "获取设备 Profile", "description": "获取设备完整信息，包含属性定义和当前值",                   "danger": false, "enabled": true  },
                    { "id": "read_properties",  "name": "read_properties",  "label": "读取属性",         "description": "读取设备指定属性的当前值",                                   "danger": false, "enabled": true  },
                    { "id": "write_properties", "name": "write_properties", "label": "写入属性",         "description": "写入设备指定属性的值",                                       "danger": false, "enabled": true  },
                    { "id": "send_command",     "name": "send_command",     "label": "执行设备命令",     "description": "向设备下发控制命令并获取执行结果",                          "danger": false, "enabled": true  },
                    { "id": "create_device",    "name": "create_device",    "label": "创建设备",         "description": "根据模板创建新设备",                                        "danger": false, "enabled": true  },
                    { "id": "delete_device",    "name": "delete_device",    "label": "删除设备",         "description": "删除指定设备",                                              "danger": true,  "enabled": false },
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
/// 8. [Dynamic]     — PROFILE.md or active agent memories (NEW)
/// 9. [Context]     — dynamic context (device snapshots, etc.)
pub async fn build_full_system_prompt(
    system_prompts: &crate::shared::config::SystemPromptsConfig,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
    memory_store: Option<&std::sync::Arc<dyn tinyiothub_core::memory::MemoryStore>>,
) -> String {
    let workspace_dir = get_workspace_dir(system_prompts, workspace_id);

    // Layer 1-6: Load from workspace files
    let workspace_prompt = load_workspace_prompt(&workspace_dir).await;

    // Layer 7: Dynamic memory layer (PROFILE.md or active memories)
    let memory_layer = if let Some(store) = memory_store {
        let ws_id = workspace_id.unwrap_or(crate::shared::paths::DEFAULT_WORKSPACE_ID);
        let a_id = agent_id.unwrap_or("default");
        build_memory_layer(store.as_ref(), &workspace_dir, ws_id, a_id, 4096).await
    } else {
        String::new()
    };

    // Skills from skills/ directory (existing behavior)
    let skills_layer = load_skills_prompt(workspace_id, agent_id).await;

    // Layer 9: Additional context from config (device snapshots injected at runtime)
    let context_layer = if !system_prompts.context.is_empty() {
        format!("\n\n## 当前状态上下文\n{}\n", system_prompts.context)
    } else {
        String::new()
    };

    let full_prompt =
        format!("{}{}{}{}", workspace_prompt, memory_layer, skills_layer, context_layer);
    tracing::info!("[SYSTEM_PROMPT] {} ... (truncated, total {} chars)", &full_prompt[..full_prompt.len().min(2000)], full_prompt.len());
    full_prompt
}

/// Get the workspace directory path for loading prompt files.
///
/// Uses system_prompts.workspace_dir as the base path and appends workspace_id.
fn get_workspace_dir(
    system_prompts: &crate::shared::config::SystemPromptsConfig,
    workspace_id: Option<&str>,
) -> std::path::PathBuf {
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
            && let Ok(content) = fs::read_to_string(&file_path).await
        {
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
        // Templates are embedded at compile time via include_str!
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
    use crate::shared::paths::{
        DEFAULT_WORKSPACE_ID, agent_skills_dir, global_skills_dir, workspace_skills_dir,
    };

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
    use tokio::fs;

    use crate::modules::agent::skill::AgentSkill;

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

        let version =
            fm.as_ref().and_then(|f| f.get("version")).and_then(|v| v.as_str()).unwrap_or("");

        all_skills.push_str(&format!(
            "### {}{}\n{}\n{}\n",
            skill_name,
            if version.is_empty() { String::new() } else { format!(" (v{})", version) },
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

/// Build the dynamic memory layer for the system prompt.
/// Prefers PROFILE.md if available; otherwise injects top active memories.
async fn build_memory_layer(
    memory_store: &dyn tinyiothub_core::memory::MemoryStore,
    workspace_dir: &std::path::Path,
    workspace_id: &str,
    agent_id: &str,
    max_tokens: usize,
) -> String {
    // 1. Prefer compiled PROFILE.md
    let profile_path = workspace_dir.join("PROFILE.md");
    if profile_path.exists()
        && let Ok(profile) = tokio::fs::read_to_string(&profile_path).await
    {
        let trimmed = profile.trim();
        if !trimmed.is_empty() {
            return format!("\n## Agent Memory (Compiled Profile)\n{}\n", trimmed);
        }
    }

    // 2. Fall back to dynamic memory injection
    let active = match memory_store.list_active(workspace_id, agent_id).await {
        Ok(memories) => memories,
        Err(e) => {
            tracing::warn!(%e, "Failed to load active memories");
            return String::new();
        }
    };

    if active.is_empty() {
        return String::new();
    }

    let mut fragments = vec!["\n## Dynamic Memory\n".to_string()];
    let mut token_budget = max_tokens / 5;

    for mem in &active {
        if mem.source == tinyiothub_core::memory::MemorySource::DeviceSnapshot {
            continue;
        }
        let entry = format!("- [{}] {}\n", mem.zone.as_str(), mem.content);
        let entry_tokens = entry.len() / 4;
        if entry_tokens > token_budget {
            break;
        }
        token_budget -= entry_tokens;
        fragments.push(entry);

        let _ = memory_store.record_load(&mem.id).await;
    }

    fragments.concat()
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

        assert!(!groups.is_empty(), "catalog should have at least one tool group");

        // Verify at least the device and workspace groups exist
        let group_ids: Vec<&str> =
            groups.iter().filter_map(|g| g.get("id").and_then(|v| v.as_str())).collect();

        assert!(group_ids.contains(&"device"), "catalog should have a 'device' group");
        assert!(group_ids.contains(&"alarm"), "catalog should have an 'alarm' group");
        assert!(group_ids.contains(&"driver"), "catalog should have a 'driver' group");
        assert!(group_ids.contains(&"job"), "catalog should have a 'job' group");

        // Verify each group has required fields
        for group in groups {
            let g_obj = group.as_object().expect("group should be an object");
            assert!(g_obj.contains_key("id"), "group should have 'id' field");
            assert!(g_obj.contains_key("label"), "group should have 'label' field");
            assert!(g_obj.contains_key("tools"), "group should have 'tools' field");

            let tools =
                g_obj.get("tools").and_then(|v| v.as_array()).expect("tools should be an array");

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

    #[tokio::test]
    async fn test_build_full_system_prompt_no_persona() {
        // Verify that persona_layer is no longer injected
        let system_prompts = crate::shared::config::SystemPromptsConfig {
            context: String::new(),
            workspace_dir: String::new(),
            ..Default::default()
        };
        let result = build_full_system_prompt(&system_prompts, None, None, None).await;
        // Should NOT contain the old persona header
        assert!(!result.contains("## Agent Persona（用户配置）"));
    }
}
