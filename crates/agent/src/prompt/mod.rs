//! Prompt 组装 — workspace 文件 + 记忆层 + 动态上下文（Task 14 自 apps/cloud
//! `host/shared/mod.rs` 迁入）。
//!
//! 记忆层通过 [`PromptMemorySource`] 端口读取活跃记忆 —— 端口由组合层适配
//! 存储实现后注入（D2：本 crate 不触碰持久化）。

pub mod paths;
pub mod templates;

/// 记忆读取端口 —— prompt 组装只需要的两个操作（list_active / record_load）。
/// 组合层以 newtype 适配存储层 `MemoryStore` 后注入。
#[async_trait::async_trait]
pub trait PromptMemorySource: Send + Sync {
    async fn list_active(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> anyhow::Result<Vec<tinyiothub_core::memory::AgentMemory>>;
    async fn record_load(&self, id: &str) -> anyhow::Result<()>;
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
    system_prompts: &tinyiothub_core::config::SystemPromptsConfig,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
    memory_source: Option<&std::sync::Arc<dyn PromptMemorySource>>,
) -> String {
    let workspace_dir = get_workspace_dir(system_prompts, workspace_id);

    // Layer 1-6: Load from workspace files
    let workspace_prompt = load_workspace_prompt(&workspace_dir).await;

    // Layer 7: Dynamic memory layer (PROFILE.md or active memories)
    let memory_layer = if let Some(source) = memory_source {
        let ws_id = workspace_id.unwrap_or(paths::DEFAULT_WORKSPACE_ID);
        let a_id = agent_id.unwrap_or("default");
        build_memory_layer(source.as_ref(), &workspace_dir, ws_id, a_id, 4096).await
    } else {
        String::new()
    };

    // Skills are injected per-turn by the zeroclaw prompt builder
    // (TinyIoTHubSkillsSection) and the chat-layer trigger resolver, so the
    // seeded system prompt intentionally omits them to avoid duplication and
    // the unreliable `history().is_empty()` seeding gate.

    // Layer 9: Additional context from config (device snapshots injected at runtime)
    let context_layer = if !system_prompts.context.is_empty() {
        format!("\n\n## 当前状态上下文\n{}\n", system_prompts.context)
    } else {
        String::new()
    };

    let full_prompt = format!("{}{}{}", workspace_prompt, memory_layer, context_layer);
    tracing::info!(
        "[SYSTEM_PROMPT] {} ... (truncated, total {} chars)",
        &full_prompt[..full_prompt.len().min(2000)],
        full_prompt.len()
    );
    full_prompt
}

/// Get the workspace directory path for loading prompt files.
///
/// Uses system_prompts.workspace_dir as the base path and appends workspace_id.
fn get_workspace_dir(
    system_prompts: &tinyiothub_core::config::SystemPromptsConfig,
    workspace_id: Option<&str>,
) -> std::path::PathBuf {
    let ws = workspace_id.unwrap_or(paths::DEFAULT_WORKSPACE_ID);
    let base = &system_prompts.workspace_dir;
    if base.is_empty() {
        paths::workspace_dir(ws)
    } else {
        std::path::PathBuf::from(base).join(ws)
    }
}

/// Load workspace prompt files with two-tier fallback:
///   1. Workspace dir (user overrides)
///   2. Shared base dir (_default/) — updated on deploy
///   3. Embedded compile-time templates
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

    let shared_base = paths::shared_agent_base_dir();

    let mut sections = Vec::new();

    // Define workspace files and their section names
    let files = [
        ("IDENTITY.md", "Identity"),
        ("SOUL.md", "Principles"),
        ("TOOLS.md", "Capabilities"),
        ("AGENTS.md", "Agent Rules"),
        ("USER.md", "User Context"),
        ("MEMORY.md", "Memory"),
    ];

    for (filename, section_name) in files {
        // 1. Workspace override
        let ws_path = workspace_dir.join(filename);
        // 2. Shared base
        let shared_path = shared_base.join(filename);

        let content = if ws_path.exists()
            && let Ok(c) = fs::read_to_string(&ws_path).await
            && !c.trim().is_empty()
        {
            c
        } else if shared_path.exists()
            && let Ok(c) = fs::read_to_string(&shared_path).await
            && !c.trim().is_empty()
        {
            c
        } else if let Some(c) = get_embedded_template(filename) {
            c.to_string()
        } else {
            continue;
        };

        sections.push(format!("## {}\n{}\n", section_name, content.trim()));
    }

    sections.join("\n")
}

/// Get embedded template content by filename
fn get_embedded_template(filename: &str) -> Option<&'static str> {
    match filename {
        "IDENTITY.md" => Some(templates::IDENTITY_MD),
        "SOUL.md" => Some(templates::SOUL_MD),
        "AGENTS.md" => Some(templates::AGENTS_MD),
        "TOOLS.md" => Some(templates::TOOLS_MD),
        "USER.md" => Some(templates::USER_MD),
        "MEMORY.md" => Some(templates::MEMORY_MD),
        _ => None,
    }
}

/// Build the dynamic memory layer for the system prompt.
/// Prefers PROFILE.md if available; otherwise injects top active memories.
async fn build_memory_layer(
    memory_source: &dyn PromptMemorySource,
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
    let active = match memory_source.list_active(workspace_id, agent_id).await {
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

        let _ = memory_source.record_load(&mem.id).await;
    }

    fragments.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_full_system_prompt_no_persona() {
        // Verify that persona_layer is no longer injected
        let system_prompts = tinyiothub_core::config::SystemPromptsConfig {
            context: String::new(),
            workspace_dir: String::new(),
            ..Default::default()
        };
        let result = build_full_system_prompt(&system_prompts, None, None, None).await;
        // Should NOT contain the old persona header
        assert!(!result.contains("## Agent Persona（用户配置）"));
    }
}
