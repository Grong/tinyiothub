// AgentPool — central agent lifecycle manager
//
// Composes capability services (Chat, Config, Tools) into a unified Agent API.
// Key design decisions:
//   - Lazy creation: agents built on first access, config read from DB
//   - Tool denylist: resolved at build time from AgentRuntimeConfig
//   - NamespacedMemory: workspace-level isolation via zeroclaw NamespacedMemory
//   - Invalidation: remove from pool on config change, rebuild on next access

use std::{sync::Arc, time::Instant};

use anyhow::anyhow;
use dashmap::DashMap;
use sqlx::SqlitePool;
use zeroclaw::{
    agent::{
        dispatcher::NativeToolDispatcher,
        prompt::{PromptContext, PromptSection, SystemPromptBuilder},
    },
    memory::{Memory, NamespacedMemory},
    observability::Observer,
    security::AutonomyLevel,
    tools::Tool,
};

use super::{
    chat::service as chat_service,
    config::service as config_service,
    reflection::{notifications::NotificationService, service::ReflectionService},
    tools::service as tool_service,
};
use crate::shared::agent::config::{AgentConfig, AgentError, AgentInfo, AgentRuntimeConfig};

// ============================================================================
// Skills Section (zeroclaw SystemPromptBuilder integration)
// ============================================================================

struct TinyIoTHubSkillsSection;

impl PromptSection for TinyIoTHubSkillsSection {
    fn name(&self) -> &str {
        "tinyiothub_skills"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String> {
        let skills_content = load_skills_sync(ctx.workspace_dir);
        if skills_content.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("## 技能（Skills）\n你可以使用以下技能来完成任务：\n\n{}", skills_content))
        }
    }
}

fn load_skills_sync(workspace_dir: &std::path::Path) -> String {
    let ws_skills = workspace_dir.join("skills");
    if ws_skills.exists()
        && let Some(content) = read_skills_dir_sync(&ws_skills)
            && !content.is_empty() {
                return content;
            }
    let global_skills = std::path::PathBuf::from("data/skills");
    if global_skills.exists()
        && let Some(content) = read_skills_dir_sync(&global_skills)
            && !content.is_empty() {
                return content;
            }
    String::new()
}

fn read_skills_dir_sync(dir: &std::path::Path) -> Option<String> {
    use std::fs;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };
    let mut skill_files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    skill_files.sort();

    let mut all_skills = String::new();
    for path in skill_files {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let body = content.trim();
        if body.is_empty() {
            continue;
        }
        all_skills.push_str(&format!("### {}\n{}\n", file_name, body));
    }

    if all_skills.is_empty() { None } else { Some(all_skills) }
}

// ============================================================================
// PoolEntry
// ============================================================================

pub(crate) struct PoolEntry {
    pub zeroclaw_agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
    #[allow(dead_code)]
    pub metadata: Agent,
    pub last_used: Instant,
}

impl PoolEntry {
    fn new(agent: zeroclaw::agent::Agent, metadata: Agent) -> Self {
        Self {
            zeroclaw_agent: Arc::new(tokio::sync::Mutex::new(agent)),
            metadata,
            last_used: Instant::now(),
        }
    }
}

// ============================================================================
// Agent (metadata)
// ============================================================================

pub struct Agent {
    pub agent_id: String,
    pub workspace_id: String,
    pub config: AgentRuntimeConfig,
}

// ============================================================================
// AgentPool
// ============================================================================

pub struct AgentPool {
    pub(crate) agents: Arc<DashMap<String, PoolEntry>>,
    pub(crate) db_pool: SqlitePool,
    pub(crate) shared_memory: Arc<dyn Memory>,
    pub(crate) observer: Arc<dyn Observer>,
    pub(crate) response_cache: Option<Arc<zeroclaw::memory::ResponseCache>>,
    #[allow(dead_code)]
    pub(crate) agent_settings: crate::shared::config::AgentSettings,
    pub chat_handles:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub memory_store: Arc<dyn tinyiothub_core::memory::MemoryStore>,
    pub reflection_service: Option<Arc<ReflectionService>>,
    pub notification_service: Arc<NotificationService>,
}

impl AgentPool {
    /// Create a new AgentPool with shared memory and observer backends.
    pub fn new(
        db_pool: SqlitePool,
        memory_store: Arc<dyn tinyiothub_core::memory::MemoryStore>,
        agent_settings: &crate::shared::config::AgentSettings,
    ) -> anyhow::Result<Self> {
        let workspace_dir = crate::shared::paths::default_workspace_dir();
        std::fs::create_dir_all(&workspace_dir).ok();

        let memory_config = zeroclaw::config::schema::MemoryConfig {
            backend: agent_settings.memory_backend.clone(),
            auto_save: true,
            hygiene_enabled: true,
            response_cache_enabled: true,
            ..Default::default()
        };

        let memory = zeroclaw::memory::create_memory(&memory_config, &workspace_dir, None)
            .map_err(|e| {
                anyhow!(
                    "Failed to create memory backend '{}': {}",
                    agent_settings.memory_backend,
                    e
                )
            })?;
        let shared_memory: Arc<dyn Memory> = Arc::from(memory);

        let response_cache =
            zeroclaw::memory::create_response_cache(&memory_config, &workspace_dir).map(Arc::new);

        let observer_config = zeroclaw::config::schema::ObservabilityConfig {
            backend: agent_settings.observer_backend.clone(),
            ..Default::default()
        };
        let observer = zeroclaw::observability::create_observer(&observer_config);
        let observer: Arc<dyn Observer> = Arc::from(observer);

        let minimax_auth_token = crate::shared::config::get()
            .minimax
            .as_ref()
            .map(|m| m.auth_token.clone())
            .unwrap_or_default();
        let notification_service = Arc::new(NotificationService::new());
        let reflection_service = Some(Arc::new(ReflectionService::new(
            Arc::clone(&memory_store),
            db_pool.clone(),
            Arc::clone(&notification_service),
            minimax_auth_token,
        )));

        Ok(Self {
            db_pool,
            agents: Arc::new(DashMap::new()),
            shared_memory,
            observer,
            response_cache,
            agent_settings: agent_settings.clone(),
            chat_handles: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            memory_store,
            reflection_service,
            notification_service,
        })
    }

    // ========================================================================
    // Agent lifecycle
    // ========================================================================

    /// Get or lazily create a per-agent zeroclaw Agent with NamespacedMemory isolation.
    ///
    /// Pool key: `agent_id`. Each agent reads its runtime config from
    /// `agent_configs` and filters tools via denylist on creation.
    pub async fn get_or_create(
        &self,
        agent_id: &str,
        workspace_id: &str,
    ) -> Result<Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>, AgentError> {
        use dashmap::mapref::entry::Entry;
        match self.agents.entry(agent_id.to_string()) {
            Entry::Occupied(mut occupied) => {
                let agent = Arc::clone(&occupied.get().zeroclaw_agent);
                occupied.get_mut().last_used = Instant::now();
                Ok(agent)
            }
            Entry::Vacant(vacant) => {
                let config = config_service::get_config(&self.db_pool, agent_id).await?;

                let namespaced: Arc<dyn Memory> = Arc::new(NamespacedMemory::new(
                    Arc::clone(&self.shared_memory),
                    workspace_id.to_string(),
                ));

                let minimax_config = crate::shared::config::get()
                    .minimax
                    .clone()
                    .ok_or_else(|| AgentError::BuildError("minimax config required".to_string()))?;

                let provider = zeroclaw::providers::create_provider(
                    "minimaxi",
                    Some(&minimax_config.auth_token),
                )
                .map_err(|e| AgentError::BuildError(format!("Failed to create provider: {}", e)))?;

                let ws_dir = crate::shared::paths::workspace_dir(workspace_id);

                let tools = tool_service::resolve_tools_for_agent(&config, workspace_id).await;

                let agent = Self::build_agent(
                    &namespaced,
                    &self.observer,
                    &config,
                    self.response_cache.clone(),
                    provider,
                    &ws_dir,
                    tools,
                )
                .map_err(|e| AgentError::BuildError(e.to_string()))?;

                let metadata = Agent {
                    agent_id: agent_id.to_string(),
                    workspace_id: workspace_id.to_string(),
                    config,
                };

                let entry = PoolEntry::new(agent, metadata);
                let agent_arc = Arc::clone(&entry.zeroclaw_agent);
                vacant.insert(entry);
                tracing::info!(agent_id = agent_id, pool_size = self.agents.len(), "Agent created");
                Ok(agent_arc)
            }
        }
    }

    /// Remove an agent from the pool (on config change).
    pub fn invalidate(&self, agent_id: &str) {
        self.agents.remove(agent_id);
        tracing::info!(agent_id = agent_id, "Agent invalidated");
    }

    /// Remove agents idle for more than 30 minutes.
    pub fn cleanup_idle(&self) -> usize {
        let cutoff = Instant::now()
            .checked_sub(std::time::Duration::from_secs(30 * 60))
            .unwrap_or(Instant::now());
        let before = self.agents.len();
        self.agents.retain(|_, entry| entry.last_used > cutoff);
        let removed = before - self.agents.len();
        if removed > 0 {
            tracing::info!(removed, remaining = self.agents.len(), "Cleaned up idle agents");
        }
        removed
    }

    /// Refresh tools by clearing all cached agents (lazy rebuild on next access).
    pub async fn refresh_tools(&self) -> anyhow::Result<()> {
        let cleared = self.agents.len();
        self.agents.clear();
        tracing::info!(cleared, "Agent tools refreshed: all cached agents cleared");
        Ok(())
    }

    // ========================================================================
    // Agent builder
    // ========================================================================

    fn build_agent(
        memory: &Arc<dyn Memory>,
        observer: &Arc<dyn Observer>,
        config: &AgentRuntimeConfig,
        response_cache: Option<Arc<zeroclaw::memory::ResponseCache>>,
        provider: Box<dyn zeroclaw::providers::traits::Provider>,
        workspace_dir: &std::path::Path,
        tools: Vec<Box<dyn Tool>>,
    ) -> anyhow::Result<zeroclaw::agent::Agent> {
        let tool_dispatcher = Box::new(NativeToolDispatcher);

        let prompt_builder =
            SystemPromptBuilder::with_defaults().add_section(Box::new(TinyIoTHubSkillsSection));

        zeroclaw::agent::Agent::builder()
            .provider(provider)
            .tools(tools)
            .memory(Arc::clone(memory))
            .observer(Arc::clone(observer))
            .tool_dispatcher(tool_dispatcher)
            .model_name(config.model.clone())
            .security_summary(Some(
                "IoT device operations: destructive actions (delete, write) require user approval. Read-only operations are auto-approved.".into(),
            ))
            .autonomy_level(AutonomyLevel::Supervised)
            .response_cache(response_cache)
            .prompt_builder(prompt_builder)
            .workspace_dir(workspace_dir.to_path_buf())
            .build()
            .map_err(|e| anyhow!("Agent build failed: {}", e))
    }

    // ========================================================================
    // Agent CRUD
    // ========================================================================

    pub async fn create_agent(&self, config: &AgentConfig) -> Result<String, AgentError> {
        let workspace_id = config.workspace_id.clone();
        let name = config.name.clone();
        let agent_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
             VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
        )
        .bind(&agent_id)
        .bind(&workspace_id)
        .bind(&name)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(agent_id)
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<(), AgentError> {
        let agent_id = agent_id.to_string();
        let result = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(AgentError::NotFound(agent_id));
        }
        let _ = sqlx::query("DELETE FROM agent_configs WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await;
        let _ = sqlx::query("DELETE FROM agent_tools WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await;
        self.invalidate(&agent_id);
        Ok(())
    }

    pub async fn get_agent(&self, agent_id: &str) -> Result<AgentInfo, AgentError> {
        let agent_id = agent_id.to_string();
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?",
        )
        .bind(&agent_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        match row {
            Some((id, _workspace, name, status)) => {
                Ok(AgentInfo { id, name, status, created_at: None })
            }
            None => Err(AgentError::NotFound(agent_id)),
        }
    }

    pub async fn list_agents(&self, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT agent_id, workspace_id, name, status FROM agents WHERE workspace_id = ? ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        let items: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|(id, _ws, name, status)| {
                serde_json::json!({"id": id, "name": name, "status": status})
            })
            .collect();

        Ok(serde_json::json!({"agents": items}))
    }

    // ========================================================================
    // Config (delegated to ConfigService)
    // ========================================================================

    pub async fn get_agent_config(
        &self,
        agent_id: &str,
        workspace_id: &str,
    ) -> Result<serde_json::Value, AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        config_service::get_config_json(&self.db_pool, agent_id).await
    }

    pub async fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        base_hash: Option<&str>,
        workspace_id: &str,
    ) -> Result<(), AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        config_service::set_config(&self.db_pool, agent_id, config).await?;
        self.invalidate(agent_id);
        // Silently ignore base_hash mismatch — last write wins
        let _ = base_hash;
        Ok(())
    }

    // ========================================================================
    // Tools (delegated to ToolService)
    // ========================================================================

    pub async fn tools_catalog(&self, _agent_id: &str) -> Result<serde_json::Value, AgentError> {
        Ok(tool_service::build_catalog().await)
    }

    pub async fn tools_effective(
        &self,
        agent_id: &str,
        workspace_id: &str,
    ) -> Result<serde_json::Value, AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let all_tools = tool_service::load_all_tools(workspace_id).await;
        let effective = tool_service::filter_by_denylist(all_tools, &config.tool_denylist);
        let names: Vec<&str> = effective.iter().map(|t| t.name()).collect();
        Ok(serde_json::json!({ "tools": names }))
    }

    pub async fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
        workspace_id: &str,
    ) -> Result<(), AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        let mut config = config_service::get_config(&self.db_pool, agent_id).await?;
        if enabled {
            config.tool_denylist.retain(|t| t != tool_name);
        } else if !config.tool_denylist.contains(&tool_name.to_string()) {
            config.tool_denylist.push(tool_name.to_string());
        }
        let config_str =
            serde_json::to_string(&config).map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        config_service::set_config(&self.db_pool, agent_id, &config_str).await?;
        self.invalidate(agent_id);
        Ok(())
    }

    // ========================================================================
    // Chat (delegated to ChatService)
    // ========================================================================

    pub async fn chat_send(
        &self,
        agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<super::types::ChatEvent>, AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        parsed.verify_workspace(&parsed.workspace_id)?;
        let agent = self.get_or_create(agent_id, &parsed.workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let enable_reflection = config.enable_reflection;
        let model = config.model.clone();
        chat_service::send_message(
            &agent,
            message,
            run_id,
            session_key,
            system_prompt,
            &self.chat_handles,
            self.reflection_service.clone(),
            enable_reflection,
            &model,
            &parsed.workspace_id,
            agent_id,
        )
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    pub async fn chat_history(
        &self,
        agent_id: &str,
        session_key: &str,
        _limit: u32,
    ) -> Result<serde_json::Value, AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        parsed.verify_workspace(&parsed.workspace_id)?;

        let agent = self.get_or_create(agent_id, &parsed.workspace_id).await?;
        let ag = agent.lock().await;
        let history = ag.history();

        let messages: Vec<serde_json::Value> = history
            .iter()
            .filter_map(|msg| {
                let value = serde_json::to_value(msg).ok()?;
                match value.get("type")?.as_str()? {
                    "Chat" => {
                        let mut data = value.get("data")?.clone();
                        // Skip system messages
                        if data.get("role")?.as_str()? == "system" {
                            return None;
                        }
                        // Frontend expects content as array: [{ type: "text", text: "..." }]
                        normalize_content(&mut data);
                        Some(data)
                    }
                    "AssistantToolCalls" => {
                        let data = value.get("data")?;
                        let text = data.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        let tool_calls =
                            data.get("tool_calls").cloned().unwrap_or(serde_json::json!([]));
                        let content = if text.is_empty() {
                            serde_json::json!([])
                        } else {
                            serde_json::json!([{ "type": "text", "text": text }])
                        };
                        Some(serde_json::json!({
                            "role": "assistant",
                            "content": content,
                            "toolCalls": tool_calls,
                        }))
                    }
                    _ => {
                        // ToolResults and other variants — skip for display
                        None
                    }
                }
            })
            .collect();

        Ok(serde_json::json!({ "messages": messages, "sessionKey": session_key }))
    }
}

/// Convert content from plain string to frontend-compatible array format.
///
/// Frontend expects: `[{ "type": "text", "text": "..." }]`
/// zeroclaw stores:   `"plain string"`
fn normalize_content(data: &mut serde_json::Value) {
    if let Some(content) = data.get("content") {
        if content.is_string() {
            let text = content.as_str().unwrap_or("");
            data["content"] = serde_json::json!([{ "type": "text", "text": text }]);
        } else if !content.is_array() {
            data["content"] = serde_json::json!([]);
        }
    }
}

impl AgentPool {
    pub async fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
    ) -> Result<(), AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        parsed.verify_workspace(&parsed.workspace_id)?;
        let _ = agent_id;
        if let Some(rid) = run_id {
            let mut handles = self.chat_handles.lock().await;
            if let Some(handle) = handles.remove(rid) {
                handle.abort();
            }
        }
        Ok(())
    }

    // ========================================================================
    // Run single (for cron jobs)
    // ========================================================================

    pub async fn run_single(
        &self,
        workspace_id: &str,
        message: &str,
    ) -> Result<String, AgentError> {
        let agent = self.get_or_create("default", workspace_id).await?;
        let mut ag = agent.lock().await;
        ag.run_single(message).await.map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    pub fn pool_size(&self) -> usize {
        self.agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_entry_creation() {
        // PoolEntry::new is tested indirectly via AgentPool::get_or_create
        // This test validates the metadata field layout
        let metadata = Agent {
            agent_id: "a1".to_string(),
            workspace_id: "ws1".to_string(),
            config: AgentRuntimeConfig::default(),
        };
        assert_eq!(metadata.agent_id, "a1");
        assert_eq!(metadata.workspace_id, "ws1");
        assert_eq!(metadata.config.model, "minimax-m2");
    }

    #[test]
    fn test_agent_metadata_defaults() {
        let config = AgentRuntimeConfig::default();
        assert_eq!(config.model, "minimax-m2");
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 4096);
        assert!(config.tool_denylist.contains(&"delete_device".to_string()));
    }
}
