// AgentPool — pool lifecycle: construction, cache lookup, creation from
// caller-injected parts, invalidation, idle cleanup, and the agent loop
// factory wiring.
//
// Task 14 自 apps/cloud `host/agent/pool.rs` 迁入（该文件 Task 7 起即存储无关）。

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use dashmap::DashMap;

use crate::domains::agent::prompt::assistant::PromptPackage;
use crate::domains::agent::tools::{ToolRegistry, ToolRuntimeContext};
use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::error::AgentError;
use tinyiothub_agent::memory::workspace_memory::WorkspaceScopedMemory;
use tinyiothub_agent::port::memory::Memory;
use tinyiothub_agent::port::observer::Observer;
use tinyiothub_agent::port::prompt::SystemPromptBuilder;
use tinyiothub_agent::port::runtime::{AgentLoopConfig, AgentLoopHandle};
use tinyiothub_agent::port::tool::Tool;

use tinyiothub_agent::provider::ProviderFactory;

// ============================================================================
// Skills loading
// ============================================================================

/// Load skills for a workspace, workspace-specific dir overriding the global one.
pub fn load_workspace_skills(workspace_dir: &std::path::Path) -> Vec<tinyiothub_skills::LoadedSkill> {
    load_workspace_skills_from(workspace_dir, std::path::Path::new("data/skills"))
}

fn load_workspace_skills_from(
    workspace_dir: &std::path::Path,
    global_dir: &std::path::Path,
) -> Vec<tinyiothub_skills::LoadedSkill> {
    let dirs = vec![workspace_dir.join("skills"), global_dir.to_path_buf()];
    let mut skills = tinyiothub_skills::load_skills_from_dirs(&dirs);
    if !skills.iter().any(|skill| skill.name == "visualization")
        && let Some(skill) = tinyiothub_skills::parse_skill(
            "visualization",
            crate::domains::agent::prompt::templates::VISUALIZATION_SKILL_MD,
        )
    {
        skills.push(skill);
    }
    skills
}

// ============================================================================
// PoolEntry
// ============================================================================

pub(crate) struct PoolEntry {
    pub agent: AgentLoopHandle,
    #[allow(dead_code)]
    pub metadata: Agent,
    pub last_used: Instant,
}

impl PoolEntry {
    fn new(agent: AgentLoopHandle, metadata: Agent) -> Self {
        Self {
            agent,
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
    /// Tool-set flavor fixed at creation: whether the session primitives
    /// (`todo_write` / `ask_user` / `spawn_subagent`) were exposed.
    pub session_tools_enabled: bool,
    pub prompt_hash: Option<u64>,
}

// ============================================================================
// AgentPool
// ============================================================================

/// Storage-free by design (Task 7): the pool holds no db/memory storage
/// handles and no method signature mentions them. Callers (the composition
/// layer) resolve per-request data — agent config, tool list, memory
/// service — and inject the results into the pool's pure methods.
pub type ChatHandles = Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>;

pub struct AgentPool {
    pub(crate) agents: Arc<DashMap<String, PoolEntry>>,
    pub(crate) shared_memory: Arc<dyn Memory>,
    pub(crate) observer: Arc<dyn Observer>,
    #[allow(dead_code)]
    pub(crate) agent_settings: tinyiothub_core::config::AgentSettings,
    pub chat_handles: ChatHandles,
    pub trust_configs: DashMap<String, tinyiothub_core::heartbeat::TrustConfig>,
    pub event_publisher: tokio::sync::RwLock<Option<Arc<crate::domains::agent::event::bus::AiEventPublisher>>>,
    /// Builds a fresh model provider per agent (injected by the composition
    /// layer — providers are per-agent).
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
    /// Create a new AgentPool with caller-injected memory and observer backends
    /// (the composition layer builds them — crates/agent keeps zero storage
    /// implementations).
    ///
    /// No storage handles: agent config and tools are resolved by the cloud
    /// caller and injected per creation (see [`Self::create`]).
    pub fn new(
        agent_settings: &tinyiothub_core::config::AgentSettings,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        provider_factory: ProviderFactory,
    ) -> anyhow::Result<Self> {
        let workspace_dir = crate::domains::agent::prompt::paths::default_workspace_dir();
        std::fs::create_dir_all(&workspace_dir).ok();

        Ok(Self {
            agents: Arc::new(DashMap::new()),
            shared_memory: memory,
            observer,
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

    pub async fn set_event_publisher(&self, publisher: Arc<crate::domains::agent::event::bus::AiEventPublisher>) {
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
    pub fn get_cached(&self, agent_id: &str) -> Option<AgentLoopHandle> {
        self.agents.get_mut(agent_id).map(|mut entry| {
            let agent = Arc::clone(&entry.agent);
            entry.last_used = Instant::now();
            agent
        })
    }

    /// Build and insert a per-agent loop with WorkspaceScopedMemory
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
        session_tools_enabled: bool,
        prompt: &PromptPackage,
    ) -> Result<AgentLoopHandle, AgentError> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        prompt.system_prompt.hash(&mut hasher);
        self.create_with_prompt_builder(
            agent_id,
            workspace_id,
            config,
            tools,
            session_tools_enabled,
            SystemPromptBuilder::from_static(prompt.system_prompt.clone()),
            Some(hasher.finish()),
        )
    }

    fn create_with_prompt_builder(
        &self,
        agent_id: &str,
        workspace_id: &str,
        config: &AgentRuntimeConfig,
        tools: Vec<Box<dyn Tool>>,
        session_tools_enabled: bool,
        prompt_builder: SystemPromptBuilder,
        prompt_hash: Option<u64>,
    ) -> Result<AgentLoopHandle, AgentError> {
        let namespaced: Arc<dyn Memory> = Arc::new(WorkspaceScopedMemory::new(
            Arc::clone(&self.shared_memory),
            workspace_id.to_string(),
        ));

        let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(workspace_id);

        let cfg = AgentLoopConfig {
            model_name: config.model.clone(),
            prompt_builder,
            tools,
            memory: namespaced,
            observer: Arc::clone(&self.observer),
            workspace_dir: ws_dir,
            security_summary: Some(
                "IoT device operations: destructive actions (delete, write) require user approval. Read-only operations are auto-approved."
                    .into(),
            ),
            provider_factory: Arc::clone(&self.provider_factory),
            require_context_budget: config.require_context_budget,
            // chat/heartbeat：历史由 cloud DB 每轮重建，不经 agent 内存。
            conversation_memory: false,
            available_capabilities: vec![],
            tool_call_ids: None,
            checkpoint_before_tool: true,
        };

        let agent = tinyiothub_agent::adapters::rig::loop_::rig_loop_factory(cfg)
            .map_err(|e| AgentError::BuildError(e.to_string()))?;

        let metadata = Agent {
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            config: config.clone(),
            session_tools_enabled,
            prompt_hash,
        };

        // Double-checked insert: a concurrent creator may have won the race
        // while we were building. The loser's agent is dropped unstarted.
        use dashmap::mapref::entry::Entry;
        match self.agents.entry(agent_id.to_string()) {
            Entry::Occupied(mut occupied) => {
                let matches = occupied.get().metadata.session_tools_enabled == session_tools_enabled
                    && occupied.get().metadata.prompt_hash == prompt_hash;
                if matches {
                    let agent = Arc::clone(&occupied.get().agent);
                    occupied.get_mut().last_used = Instant::now();
                    return Ok(agent);
                }
                // A concurrent or direct create may hold an entry built from a
                // different package. Never return that stale prompt to this caller.
                occupied.remove();
                let entry = PoolEntry::new(agent, metadata);
                let agent = Arc::clone(&entry.agent);
                self.agents.insert(agent_id.to_string(), entry);
                Ok(agent)
            }
            Entry::Vacant(vacant) => {
                let entry = PoolEntry::new(agent, metadata);
                let agent_arc = Arc::clone(&entry.agent);
                vacant.insert(entry);
                tracing::info!(agent_id = agent_id, pool_size = self.agents.len(), "Agent created");
                Ok(agent_arc)
            }
        }
    }

    /// 读取已缓存 Agent 的运行时配置（子代理继承父配置）。
    pub fn agent_config(&self, agent_id: &str) -> Option<AgentRuntimeConfig> {
        self.agents.get(agent_id).map(|entry| entry.metadata.config.clone())
    }

    /// Whether the cached agent was built with session primitives enabled.
    /// `None` = not cached; callers use the mismatch to rebuild on id reuse.
    pub fn agent_session_tools(&self, agent_id: &str) -> Option<bool> {
        self.agents
            .get(agent_id)
            .map(|entry| entry.metadata.session_tools_enabled)
    }

    pub fn agent_prompt_hash(&self, agent_id: &str) -> Option<u64> {
        self.agents.get(agent_id).and_then(|entry| entry.metadata.prompt_hash)
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
    impl tinyiothub_agent::port::provider::ModelProvider for ScriptedModelProvider {
        async fn chat(
            &self,
            _request: tinyiothub_agent::port::provider::ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<tinyiothub_agent::port::provider::ChatResponse> {
            Ok(tinyiothub_agent::port::provider::ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl tinyiothub_agent::port::attribution::Attributable for ScriptedModelProvider {
        fn role(&self) -> tinyiothub_agent::port::attribution::Role {
            tinyiothub_agent::port::attribution::Role::Provider(
                tinyiothub_agent::port::attribution::ProviderKind::Model(
                    tinyiothub_agent::port::attribution::ModelProviderKind::Custom,
                ),
            )
        }
        fn alias(&self) -> &str {
            "ScriptedModelProvider"
        }
    }

    fn scripted_provider_factory() -> ProviderFactory {
        Arc::new(|| Ok(Box::new(ScriptedModelProvider)))
    }

    fn test_pool() -> AgentPool {
        AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            scripted_provider_factory(),
        )
        .expect("pool builds without storage")
    }

    #[test]
    fn test_pool_entry_creation() {
        // PoolEntry::new is tested indirectly via AgentPool::create
        // This test validates the metadata field layout
        let metadata = Agent {
            agent_id: "a1".to_string(),
            workspace_id: "ws1".to_string(),
            config: AgentRuntimeConfig {
                model: "test-model".to_string(),
                ..Default::default()
            },
            session_tools_enabled: false,
            prompt_hash: None,
        };
        assert_eq!(metadata.agent_id, "a1");
        assert_eq!(metadata.workspace_id, "ws1");
        assert_eq!(metadata.config.model, "test-model");
    }

    #[test]
    fn test_agent_metadata_defaults() {
        let config = AgentRuntimeConfig::default();
        assert!(!config.model.is_empty());
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 4096);
        assert!(config.tool_denylist.contains(&"delete_thing".to_string()));
    }

    #[test]
    fn pool_constructs_without_storage_handles() {
        // Task 7: no db_pool / memory_store / memory_service — construction
        // takes only settings + provider factory (memory/observer injected).
        let pool = test_pool();
        assert_eq!(pool.pool_size(), 0);
    }

    #[test]
    fn create_uses_injected_config_and_caches() {
        let pool = test_pool();
        let config = AgentRuntimeConfig {
            model: "injected-model".to_string(),
            ..Default::default()
        };
        // Tools are caller-resolved and injected; the test passes an empty
        // list and the agent is built purely from the injected config.
        let prompt = PromptPackage {
            system_prompt: "test-prompt".into(),
            turn_input: String::new(),
        };
        let a1 = pool
            .create("a1", "ws-test", &config, vec![], false, &prompt)
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

    #[test]
    fn occupied_entry_with_different_prompt_is_replaced() {
        let pool = test_pool();
        let config = AgentRuntimeConfig::default();
        let prompt_a = PromptPackage {
            system_prompt: "prompt-a".into(),
            turn_input: String::new(),
        };
        let prompt_b = PromptPackage {
            system_prompt: "prompt-b".into(),
            turn_input: String::new(),
        };
        let first = pool.create("a1", "ws-test", &config, vec![], false, &prompt_a).unwrap();
        let second = pool.create("a1", "ws-test", &config, vec![], false, &prompt_b).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));

        let cached = pool.create("a1", "ws-test", &config, vec![], false, &prompt_b).unwrap();
        assert!(Arc::ptr_eq(&second, &cached));
    }

    #[test]
    fn visualization_skill_is_indexed_when_runtime_dirs_have_no_file() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("ws");
        let global = root.path().join("global");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&global).unwrap();

        let skills = load_workspace_skills_from(&workspace, &global);
        assert!(skills.iter().any(|skill| skill.name == "visualization"));
    }
}
