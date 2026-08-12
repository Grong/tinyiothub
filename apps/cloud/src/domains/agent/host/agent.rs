// AgentPool — central agent lifecycle manager
//
// Composes capability services (Chat, Config, Tools) into a unified Agent API.
// Key design decisions:
//   - Lazy creation: agents built on first access, config read from DB
//   - Tool denylist: resolved at build time from AgentRuntimeConfig
//   - WorkspaceScopedMemory: workspace-level isolation via namespace wrapper
//   - Invalidation: remove from pool on config change, rebuild on next access

use std::{sync::Arc, time::Instant};

use anyhow::anyhow;
use dashmap::DashMap;
use sqlx::SqlitePool;
use tinyiothub_memory::workspace_memory::WorkspaceScopedMemory;
use zeroclaw::{
    agent::{
        dispatcher::NativeToolDispatcher,
        prompt::{PromptContext, PromptSection, SystemPromptBuilder},
    },
    memory::Memory,
    observability::Observer,
    security::AutonomyLevel,
    tools::Tool,
};

use super::{chat::service as chat_service, config::service as config_service, tools::service as tool_service};
use crate::domains::agent::host::shared::config::{AgentConfig, AgentError, AgentInfo, AgentRuntimeConfig};

// ============================================================================
// Skills Section (zeroclaw SystemPromptBuilder integration)
// ============================================================================

struct TinyIoTHubSkillsSection;

impl PromptSection for TinyIoTHubSkillsSection {
    fn name(&self) -> &str {
        "tinyiothub_skills"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String> {
        let skills = load_workspace_skills(ctx.workspace_dir);
        Ok(tinyiothub_skills::build_skill_index_prompt(&skills))
    }
}

/// Load skills for a workspace, workspace-specific dir overriding the global one.
pub(crate) fn load_workspace_skills(workspace_dir: &std::path::Path) -> Vec<tinyiothub_skills::LoadedSkill> {
    let dirs = vec![workspace_dir.join("skills"), std::path::PathBuf::from("data/skills")];
    tinyiothub_skills::load_skills_from_dirs(&dirs)
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
// Streaming run result types
// ============================================================================

/// Result of a streaming heartbeat run
pub struct StreamingRunResult {
    pub final_text: String,
    pub tool_calls: Vec<StreamingToolCall>,
}

/// Tool call captured during streaming execution
#[derive(Debug, Clone)]
pub struct StreamingToolCall {
    pub name: String,
    pub args: serde_json::Value,
    pub result: Option<String>,
    pub success: bool,
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
    pub(crate) agent_settings: tinyiothub_core::config::AgentSettings,
    pub chat_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub memory_store: Arc<tinyiothub_storage::memory::MemoryStore>,
    pub trust_configs: DashMap<String, crate::domains::agent::loop_::types::TrustConfig>,
    pub memory_service: tokio::sync::RwLock<Option<Arc<tinyiothub_memory::service::MemoryService>>>,
    pub event_publisher: tokio::sync::RwLock<Option<Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>>>,
    /// Builds a fresh model provider per agent (injected by the composition
    /// layer — providers are per-agent in zeroclaw).
    provider_factory: super::autonomous_factory::ProviderFactory,
    /// Late-bound runtime handles for tool construction (device cache /
    /// data server / directive sink), set once at startup after the
    /// composition state exists (P4-Task22; replaces the AppState backdoor).
    pub(crate) runtime: tokio::sync::RwLock<super::tools::service::ToolRuntimeContext>,
}

impl AgentPool {
    /// Create a new AgentPool with shared memory and observer backends.
    pub fn new(
        db_pool: SqlitePool,
        memory_store: Arc<tinyiothub_storage::memory::MemoryStore>,
        agent_settings: &tinyiothub_core::config::AgentSettings,
        provider_factory: super::autonomous_factory::ProviderFactory,
    ) -> anyhow::Result<Self> {
        let workspace_dir = crate::domains::agent::host::shared::paths::default_workspace_dir();
        std::fs::create_dir_all(&workspace_dir).ok();

        let memory_config = zeroclaw::config::schema::MemoryConfig {
            backend: agent_settings.memory_backend.clone(),
            auto_save: true,
            hygiene_enabled: true,
            response_cache_enabled: true,
            ..Default::default()
        };

        let memory = zeroclaw::memory::create_memory(&memory_config, &workspace_dir, None).map_err(|e| {
            anyhow!(
                "Failed to create memory backend '{}': {}",
                agent_settings.memory_backend,
                e
            )
        })?;
        let shared_memory: Arc<dyn Memory> = Arc::from(memory);

        let response_cache = zeroclaw::memory::create_response_cache(&memory_config, &workspace_dir).map(Arc::new);

        let observer_backend = match agent_settings.observer_backend.as_str() {
            "none" | "noop" => zeroclaw::config::schema::ObservabilityBackend::None,
            "verbose" => zeroclaw::config::schema::ObservabilityBackend::Verbose,
            "prometheus" => zeroclaw::config::schema::ObservabilityBackend::Prometheus,
            "otel" | "opentelemetry" | "otlp" => zeroclaw::config::schema::ObservabilityBackend::Otel,
            _ => zeroclaw::config::schema::ObservabilityBackend::Log,
        };
        let observer_config = zeroclaw::config::schema::ObservabilityConfig {
            backend: observer_backend,
            ..Default::default()
        };
        let observer = zeroclaw::observability::create_observer(&observer_config);
        let observer: Arc<dyn Observer> = Arc::from(observer);

        Ok(Self {
            db_pool,
            agents: Arc::new(DashMap::new()),
            shared_memory,
            observer,
            response_cache,
            agent_settings: agent_settings.clone(),
            chat_handles: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            memory_store,
            trust_configs: DashMap::new(),
            memory_service: tokio::sync::RwLock::new(None),
            event_publisher: tokio::sync::RwLock::new(None),
            provider_factory,
            runtime: tokio::sync::RwLock::new(Default::default()),
        })
    }

    /// Bind runtime handles for tool construction (chat thing tools read
    /// `device_cache` / `data_server` / `directive_sink` through them).
    pub async fn set_runtime_context(&self, ctx: super::tools::service::ToolRuntimeContext) {
        let mut guard = self.runtime.write().await;
        *guard = ctx;
    }

    pub async fn set_event_publisher(
        &self,
        publisher: Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>,
    ) {
        let mut guard = self.event_publisher.write().await;
        *guard = Some(publisher);
    }

    pub async fn set_memory_service(&self, service: Arc<tinyiothub_memory::service::MemoryService>) {
        let mut guard = self.memory_service.write().await;
        *guard = Some(service);
    }

    /// Shared memory backend (composition layer wires it into the
    /// autonomous factory so all agent planes share one backend).
    pub fn shared_memory(&self) -> Arc<dyn Memory> {
        Arc::clone(&self.shared_memory)
    }

    /// Shared observer backend (same wiring rationale as [`Self::shared_memory`]).
    pub fn observer(&self) -> Arc<dyn Observer> {
        Arc::clone(&self.observer)
    }

    pub fn set_trust_config(&self, workspace_id: &str, config: crate::domains::agent::loop_::types::TrustConfig) {
        self.trust_configs.insert(workspace_id.to_string(), config);
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
        // Fast path: clone under a brief shard lock. Never hold a DashMap
        // entry across .await — creation below does DB and tool-resolution
        // I/O and would stall every other agent on the same shard.
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            let agent = Arc::clone(&entry.zeroclaw_agent);
            entry.last_used = Instant::now();
            return Ok(agent);
        }

        let config = config_service::get_config(&self.db_pool, agent_id).await?;

        let namespaced: Arc<dyn Memory> = Arc::new(WorkspaceScopedMemory::new(
            Arc::clone(&self.shared_memory),
            workspace_id.to_string(),
        ));

        let provider = (self.provider_factory)()
            .map_err(|e| AgentError::BuildError(format!("Failed to create provider: {}", e)))?;

        let ws_dir = crate::domains::agent::host::shared::paths::workspace_dir(workspace_id);

        let trust_config = self
            .trust_configs
            .get(workspace_id)
            .map(|e| std::sync::Arc::new(e.value().clone()));
        let tools = {
            let runtime = self.runtime.read().await.clone();
            tool_service::resolve_tools_for_agent(
                &config,
                workspace_id,
                trust_config,
                Some(self.db_pool.clone()),
                &runtime,
            )
            .await
        };

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

        // Double-checked insert: a concurrent creator may have won the race
        // while we were building. The loser's agent is dropped unstarted.
        use dashmap::mapref::entry::Entry;
        match self.agents.entry(agent_id.to_string()) {
            Entry::Occupied(mut occupied) => {
                let agent = Arc::clone(&occupied.get().zeroclaw_agent);
                occupied.get_mut().last_used = Instant::now();
                Ok(agent)
            }
            Entry::Vacant(vacant) => {
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
        provider: Box<dyn zeroclaw::providers::traits::ModelProvider>,
        workspace_dir: &std::path::Path,
        tools: Vec<Box<dyn Tool>>,
    ) -> anyhow::Result<zeroclaw::agent::Agent> {
        let tool_dispatcher = Box::new(NativeToolDispatcher);

        let prompt_builder = SystemPromptBuilder::with_defaults().add_section(Box::new(TinyIoTHubSkillsSection));

        zeroclaw::agent::Agent::builder()
            .model_provider(provider)
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
        let row: Option<(String, String, String, String)> =
            sqlx::query_as("SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?")
                .bind(&agent_id)
                .fetch_optional(&self.db_pool)
                .await
                .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        match row {
            Some((id, _workspace, name, status)) => Ok(AgentInfo {
                id,
                name,
                status,
                created_at: None,
            }),
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
                serde_json::json!({"id": id, "name": name, "status": status, "workspaceId": _ws})
            })
            .collect();

        Ok(serde_json::json!({"agents": items}))
    }

    // ========================================================================
    // Config (delegated to ConfigService)
    // ========================================================================

    pub async fn get_agent_config(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
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

    pub async fn tools_effective(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let all_tools = {
            let runtime = self.runtime.read().await.clone();
            tool_service::load_all_tools(workspace_id, Some(self.db_pool.clone()), &runtime).await
        };
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
        let config_str = serde_json::to_string(&config).map_err(|e| AgentError::RequestFailed(e.to_string()))?;
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
        authorized_workspace: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<super::types::ChatEvent>, AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        // Empty authorized workspace = unscoped (admin) token; nothing to check against.
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }
        let agent = self.get_or_create(agent_id, &parsed.workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let enable_reflection = config.enable_reflection;
        let model = config.model.clone();
        let memory_service = self.memory_service.read().await.clone();
        let event_publisher = self.event_publisher.read().await.clone();
        chat_service::send_message(
            &agent,
            message,
            run_id,
            session_key,
            system_prompt,
            &self.chat_handles,
            memory_service,
            event_publisher,
            enable_reflection,
            &model,
            &parsed.workspace_id,
            agent_id,
            &self.db_pool,
        )
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    pub async fn chat_history(
        &self,
        _agent_id: &str,
        session_key: &str,
        limit: u32,
        authorized_workspace: &str,
    ) -> Result<serde_json::Value, AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }

        // DB-backed, session-scoped history. The zeroclaw in-memory agent
        // history is shared across all sessions of the workspace agent and
        // cannot isolate them.
        let messages = super::chat::history::list_messages(&self.db_pool, session_key, limit)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(super::chat::history::messages_to_history_json(messages, session_key))
    }
}

impl AgentPool {
    pub async fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
        authorized_workspace: &str,
    ) -> Result<(), AgentError> {
        let parsed = super::session::SessionKey::parse(session_key)?;
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }
        let _ = agent_id;
        if let Some(rid) = run_id {
            let mut handles = self.chat_handles.lock().await;
            match handles.remove(rid) {
                Some(handle) => handle.abort(),
                // An unknown run_id must not look like a successful abort —
                // the caller's run may still be streaming.
                None => {
                    return Err(AgentError::NotFound(format!(
                        "Unknown or already-finished run_id: {rid}"
                    )));
                }
            }
        }
        Ok(())
    }

    // ========================================================================
    // Run single (for cron jobs)
    // ========================================================================

    pub async fn run_single(&self, workspace_id: &str, message: &str) -> Result<String, AgentError> {
        // Per-workspace agent key prevents cross-workspace tool context leak.
        // "__heartbeat__" has no DB row, so it always falls back to
        // AgentRuntimeConfig::default() → server-level [minimax] model.
        let agent_id = format!("__heartbeat__:{}", workspace_id);
        let agent = self.get_or_create(&agent_id, workspace_id).await?;
        let mut ag = agent.lock().await;
        ag.run_single(message)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    // ========================================================================
    // Run streaming (for heartbeat with TurnEvent interception)
    // ========================================================================

    /// Run the heartbeat agent with streaming TurnEvents, enabling per-tool-call
    /// interception (trust gate, action recording).
    pub async fn run_streaming(&self, workspace_id: &str, message: &str) -> Result<StreamingRunResult, AgentError> {
        let agent_id = format!("__heartbeat__:{}", workspace_id);
        let agent = self.get_or_create(&agent_id, workspace_id).await?;

        // Set up TurnEvent channel for real-time event interception
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<zeroclaw::agent::TurnEvent>(64);

        // Spawn tool call collector
        let tool_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tool_calls_clone = std::sync::Arc::clone(&tool_calls);
        let collector = tokio::spawn(async move {
            while let Some(evt) = event_rx.recv().await {
                match evt {
                    zeroclaw::agent::TurnEvent::ToolCall { name, args, .. } => {
                        let mut calls = tool_calls_clone.lock().unwrap();
                        calls.push(StreamingToolCall {
                            name,
                            args,
                            result: None,
                            success: true,
                        });
                    }
                    zeroclaw::agent::TurnEvent::ToolResult { name, output, .. } => {
                        let mut calls = tool_calls_clone.lock().unwrap();
                        if let Some(last) = calls.iter_mut().rev().find(|c| c.name == name) {
                            last.result = Some(output.clone());
                            // NOTE: TurnEvent::ToolResult doesn't carry ToolResult.success.
                            // Trust enforcement is handled by TrustAwareTool wrapping;
                            // the LLM's response text handles error reporting via healing report.
                        }
                    }
                    _ => {}
                }
            }
        });

        // No inner timeout here: the heartbeat tick in tinyiothub-ai bounds the
        // whole run (see heartbeat::loop_ TICK_TIMEOUT). A shorter inner timeout
        // fires first every time, making the tick-level bound unreachable.
        let mut ag = agent.lock().await;
        let result = ag.turn_streamed(message, event_tx, None).await;
        drop(ag);

        // Wait for collector to finish processing remaining events
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), collector).await;

        let tool_calls = match std::sync::Arc::try_unwrap(tool_calls) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        match result {
            Ok((final_text, _conversation)) => Ok(StreamingRunResult { final_text, tool_calls }),
            Err(e) => Err(AgentError::RequestFailed(e.to_string())),
        }
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
        assert!(config.tool_denylist.contains(&"delete_thing".to_string()));
    }

    async fn test_db() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        pool
    }

    async fn test_agent_pool() -> AgentPool {
        let db = test_db().await;
        let memory_store: Arc<tinyiothub_storage::memory::MemoryStore> =
            Arc::new(tinyiothub_storage::memory::MemoryStore::new(db.clone()));
        AgentPool::new(
            db,
            memory_store,
            &tinyiothub_core::config::AgentSettings::default(),
            super::super::autonomous_factory::minimax_provider_factory(),
        )
        .expect("test AgentPool")
    }

    #[tokio::test]
    async fn chat_send_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_send("agent_main", "agent:ws_other:agent_main/s1", "hi", "r1", "", "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
        // The workspace check must run before any agent is built.
        assert_eq!(pool.pool_size(), 0);
    }

    #[tokio::test]
    async fn chat_history_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_history("agent_main", "agent:ws_other:agent_main/s1", 50, "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }

    #[tokio::test]
    async fn chat_history_with_unscoped_token_reads_persisted_messages() {
        let pool = test_agent_pool().await;
        let key = "agent:ws1:agent_main/s1";
        crate::domains::agent::host::chat::history::ensure_session(&pool.db_pool, key, "ws1", "agent_main")
            .await
            .unwrap();
        crate::domains::agent::host::chat::history::append_message(&pool.db_pool, key, "user", "hello", "r1")
            .await
            .unwrap();

        // Empty authorized_workspace = unscoped (admin) token: no workspace
        // check, history served straight from the DB.
        let out = pool.chat_history("agent_main", key, 50, "").await.unwrap();
        assert!(out.to_string().contains("hello"));
    }

    #[tokio::test]
    async fn chat_abort_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_abort("agent_main", "agent:ws_other:agent_main/s1", Some("r1"), "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }

    #[tokio::test]
    async fn chat_abort_with_unknown_run_id_errors_and_none_run_id_is_noop() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_abort("agent_main", "agent:ws1:agent_main/s1", Some("nonexistent-run"), "")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AgentError::NotFound(_)),
            "unknown run_id must not silently succeed: {err:?}"
        );
        pool.chat_abort("agent_main", "agent:ws1:agent_main/s1", None, "")
            .await
            .unwrap();
    }
}
