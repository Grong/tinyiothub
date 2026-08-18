// AgentPool — pool lifecycle: construction, cache lookup, creation from
// caller-injected parts, invalidation, idle cleanup, and the zeroclaw agent
// builder.
//
// Task 14 自 apps/cloud `host/agent/pool.rs` 迁入（该文件 Task 7 起即存储无关）。

use std::{sync::Arc, time::Instant};

use anyhow::anyhow;
use dashmap::DashMap;
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

use crate::config::AgentRuntimeConfig;
use crate::error::AgentError;
use crate::memory::workspace_memory::WorkspaceScopedMemory;
use crate::tools::{ToolRegistry, ToolRuntimeContext};

use super::provider::ProviderFactory;

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
pub fn load_workspace_skills(workspace_dir: &std::path::Path) -> Vec<tinyiothub_skills::LoadedSkill> {
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

/// Storage-free by design (Task 7): the pool holds no db/memory storage
/// handles and no method signature mentions them. Callers (the composition
/// layer) resolve per-request data — agent config, tool list, memory
/// service — and inject the results into the pool's pure methods.
pub struct AgentPool {
    pub(crate) agents: Arc<DashMap<String, PoolEntry>>,
    pub(crate) shared_memory: Arc<dyn Memory>,
    pub(crate) observer: Arc<dyn Observer>,
    pub(crate) response_cache: Option<Arc<zeroclaw::memory::ResponseCache>>,
    #[allow(dead_code)]
    pub(crate) agent_settings: tinyiothub_core::config::AgentSettings,
    pub chat_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub trust_configs: DashMap<String, tinyiothub_core::heartbeat::TrustConfig>,
    pub event_publisher: tokio::sync::RwLock<Option<Arc<crate::runtime::event::bus::AiEventPublisher>>>,
    /// Builds a fresh model provider per agent (injected by the composition
    /// layer — providers are per-agent in zeroclaw).
    provider_factory: ProviderFactory,
    /// Late-bound runtime handles for tool construction (data server /
    /// directive sink), set once at startup after the composition state
    /// exists (P4-Task22; replaces the AppState backdoor).
    pub(crate) runtime: tokio::sync::RwLock<ToolRuntimeContext>,
    /// Built-in tool providers + external tool factory, registered by the
    /// composition layer at startup (Task 14 — 数据工具实现住组合层，经此
    /// 注册进框架；见 `tools::ToolRegistry`)。
    tool_registry: ToolRegistry,
}

impl AgentPool {
    /// Create a new AgentPool with shared memory and observer backends.
    ///
    /// No storage handles: agent config and tools are resolved by the cloud
    /// caller and injected per creation (see [`Self::create`]).
    pub fn new(
        agent_settings: &tinyiothub_core::config::AgentSettings,
        provider_factory: ProviderFactory,
    ) -> anyhow::Result<Self> {
        let workspace_dir = crate::prompt::paths::default_workspace_dir();
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
            agents: Arc::new(DashMap::new()),
            shared_memory,
            observer,
            response_cache,
            agent_settings: agent_settings.clone(),
            chat_handles: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            trust_configs: DashMap::new(),
            event_publisher: tokio::sync::RwLock::new(None),
            provider_factory,
            runtime: tokio::sync::RwLock::new(Default::default()),
            tool_registry: ToolRegistry::default(),
        })
    }

    /// Bind runtime handles for tool construction (chat thing tools read
    /// `data_server` / `directive_sink` through them).
    pub async fn set_runtime_context(&self, ctx: ToolRuntimeContext) {
        let mut guard = self.runtime.write().await;
        *guard = ctx;
    }

    pub async fn set_event_publisher(&self, publisher: Arc<crate::runtime::event::bus::AiEventPublisher>) {
        let mut guard = self.event_publisher.write().await;
        *guard = Some(publisher);
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

    pub fn set_trust_config(&self, workspace_id: &str, config: tinyiothub_core::heartbeat::TrustConfig) {
        self.trust_configs.insert(workspace_id.to_string(), config);
    }

    /// Trust config for a workspace (the composition layer resolves tools with it).
    pub fn trust_config(&self, workspace_id: &str) -> Option<Arc<tinyiothub_core::heartbeat::TrustConfig>> {
        self.trust_configs
            .get(workspace_id)
            .map(|e| Arc::new(e.value().clone()))
    }

    /// Snapshot of the late-bound runtime handles (the composition layer
    /// resolves tools with it).
    pub async fn runtime_context(&self) -> ToolRuntimeContext {
        self.runtime.read().await.clone()
    }

    /// The pool's tool registry — 组合层在启动时注册内建工具 provider 与外部
    /// 工具工厂；调用方经它加载/解析工具。
    pub fn tool_registry(&self) -> ToolRegistry {
        self.tool_registry.clone()
    }

    // ========================================================================
    // Agent lifecycle
    // ========================================================================

    /// Fast-path cache lookup; refreshes `last_used`. Never holds a DashMap
    /// entry across an await — callers resolve tools/config on a miss and
    /// come back through [`Self::create`].
    pub fn get_cached(&self, agent_id: &str) -> Option<Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>> {
        self.agents.get_mut(agent_id).map(|mut entry| {
            let agent = Arc::clone(&entry.zeroclaw_agent);
            entry.last_used = Instant::now();
            agent
        })
    }

    /// Build and insert a per-agent zeroclaw Agent with NamespacedMemory
    /// isolation, from caller-injected parts.
    ///
    /// Pool key: `agent_id`. The runtime config and the fully resolved tool
    /// list come from the composition-layer caller; this method performs no
    /// storage I/O itself.
    pub fn create(
        &self,
        agent_id: &str,
        workspace_id: &str,
        config: &AgentRuntimeConfig,
        tools: Vec<Box<dyn Tool>>,
    ) -> Result<Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>, AgentError> {
        let namespaced: Arc<dyn Memory> = Arc::new(WorkspaceScopedMemory::new(
            Arc::clone(&self.shared_memory),
            workspace_id.to_string(),
        ));

        let provider = (self.provider_factory)()
            .map_err(|e| AgentError::BuildError(format!("Failed to create provider: {}", e)))?;

        let ws_dir = crate::prompt::paths::workspace_dir(workspace_id);

        let agent = Self::build_agent(
            &namespaced,
            &self.observer,
            config,
            self.response_cache.clone(),
            provider,
            &ws_dir,
            tools,
        )
        .map_err(|e| AgentError::BuildError(e.to_string()))?;

        let metadata = Agent {
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            config: config.clone(),
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

    // ── scripted provider (no network, no ports registration) ────────────

    struct ScriptedModelProvider;

    #[async_trait::async_trait]
    impl zeroclaw::providers::traits::ModelProvider for ScriptedModelProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<String> {
            Ok("done".into())
        }

        async fn chat(
            &self,
            _request: zeroclaw::providers::ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<zeroclaw::providers::ChatResponse> {
            Ok(zeroclaw::providers::ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl zeroclaw_api::attribution::Attributable for ScriptedModelProvider {
        fn role(&self) -> zeroclaw_api::attribution::Role {
            zeroclaw_api::attribution::Role::Provider(zeroclaw_api::attribution::ProviderKind::Model(
                zeroclaw_api::attribution::ModelProviderKind::Custom,
            ))
        }
        fn alias(&self) -> &str {
            "ScriptedModelProvider"
        }
    }

    fn scripted_provider_factory() -> ProviderFactory {
        Arc::new(|| Ok(Box::new(ScriptedModelProvider)))
    }

    #[test]
    fn test_pool_entry_creation() {
        // PoolEntry::new is tested indirectly via AgentPool::create
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

    #[test]
    fn pool_constructs_without_storage_handles() {
        // Task 7: no db_pool / memory_store / memory_service — construction
        // takes only settings + provider factory.
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            scripted_provider_factory(),
        )
        .expect("pool builds without storage");
        assert_eq!(pool.pool_size(), 0);
    }

    #[test]
    fn create_uses_injected_config_and_caches() {
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            scripted_provider_factory(),
        )
        .expect("pool builds without storage");
        let config = AgentRuntimeConfig {
            model: "injected-model".to_string(),
            ..Default::default()
        };
        // Tools are caller-resolved and injected; the test passes an empty
        // list and the agent is built purely from the injected config.
        let a1 = pool
            .create("a1", "ws-test", &config, vec![])
            .expect("create with injected config");
        let entry = pool.agents.get("a1").expect("cached entry");
        assert_eq!(entry.metadata.config.model, "injected-model");
        drop(entry);

        // get_cached hits the same instance without rebuilding.
        let a2 = pool.get_cached("a1").expect("cached");
        assert!(Arc::ptr_eq(&a1, &a2));
        assert_eq!(pool.pool_size(), 1);
        assert!(pool.get_cached("unknown").is_none());
    }
}
