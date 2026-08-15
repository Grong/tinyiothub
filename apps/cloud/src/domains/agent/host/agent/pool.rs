// AgentPool — pool lifecycle: construction, lazy get_or_create, invalidation,
// idle cleanup, and the zeroclaw agent builder.

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

use super::super::{config::service as config_service, tools::service as tool_service};
use crate::domains::agent::host::shared::config::{AgentError, AgentRuntimeConfig};

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
    provider_factory: super::super::autonomous_factory::ProviderFactory,
    /// Late-bound runtime handles for tool construction (device cache /
    /// data server / directive sink), set once at startup after the
    /// composition state exists (P4-Task22; replaces the AppState backdoor).
    pub(crate) runtime: tokio::sync::RwLock<super::super::tools::service::ToolRuntimeContext>,
}

impl AgentPool {
    /// Create a new AgentPool with shared memory and observer backends.
    pub fn new(
        db_pool: SqlitePool,
        memory_store: Arc<tinyiothub_storage::memory::MemoryStore>,
        agent_settings: &tinyiothub_core::config::AgentSettings,
        provider_factory: super::super::autonomous_factory::ProviderFactory,
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
    pub async fn set_runtime_context(&self, ctx: super::super::tools::service::ToolRuntimeContext) {
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
}
