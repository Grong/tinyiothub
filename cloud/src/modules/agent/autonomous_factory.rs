//! Autonomous agent factory (T11, O8/O20).
//!
//! One autonomous zeroclaw Agent per workspace (DashMap cache), built with
//! the 9 thing-ontology tools where `invoke_action` is the autonomous
//! variant (policy gate + RunContext injection) instead of the chat
//! confirmation-token one. The thing-agent runner (T9) consumes the
//! returned [`AgentHandle`].
//!
//! Construction mirrors [`super::agent::AgentPool::get_or_create`]: fast-path
//! cache hit, slow-path build, double-checked insert; never hold a DashMap
//! guard across `.await`. Differences from the chat pool, deliberately:
//! - no response cache (an autonomous control loop must never replay a
//!   stale cached decision);
//! - run context is swapped per run through a [`RunContextSlot`] (O8: one
//!   agent per workspace, contents replaced every run; the serial scheduler
//!   guarantees no concurrent runs for a workspace);
//! - no TrustAwareTool wrapping — the autonomy policy gate (T4) is the
//!   authorization mechanism here, not the chat trust config.

use std::sync::Arc;

use anyhow::anyhow;
use dashmap::DashMap;
use sqlx::SqlitePool;
use tinyiothub_ai::thing_agent::{AgentHandle, RunContextInner, manager::AutonomousAgentProvider};
use tinyiothub_memory::workspace_memory::WorkspaceScopedMemory;
use tinyiothub_policy::autonomy::PolicyRepository;
use tinyiothub_thing::service::ThingService;
use tokio::sync::RwLock;
use zeroclaw::{
    agent::{dispatcher::NativeToolDispatcher, prompt::SystemPromptBuilder},
    memory::Memory,
    observability::Observer,
    providers::traits::ModelProvider,
    security::AutonomyLevel,
    tools::Tool,
};

use super::tools::{
    AutonomousInvokeActionTool, RunContextSlot, create_thing_tools, new_run_context_slot,
    thing::InvokeActionTool,
};
use crate::modules::event::{bus::ThingEventBus, router::ThrottleState};

/// Builds a fresh model provider per agent (providers are per-agent in
/// zeroclaw). Production wires [`minimax_provider_factory`]; tests inject a
/// scripted provider.
pub type ProviderFactory = Arc<dyn Fn() -> anyhow::Result<Box<dyn ModelProvider>> + Send + Sync>;

/// Production provider factory — `[minimax]` section of app_settings.toml.
pub fn minimax_provider_factory() -> ProviderFactory {
    Arc::new(crate::shared::config::create_minimax_provider)
}

pub(crate) struct AutonomousEntry {
    pub handle: AgentHandle,
    pub run_ctx_slot: RunContextSlot,
}

pub struct AutonomousAgentFactory {
    db_pool: SqlitePool,
    policy_repo: Arc<dyn PolicyRepository>,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    provider_factory: ProviderFactory,
    model: String,
    app_state: Option<Arc<crate::shared::app_state::AppState>>,
    pub(crate) agents: DashMap<String, AutonomousEntry>,
}

impl AutonomousAgentFactory {
    pub fn new(
        db_pool: SqlitePool,
        policy_repo: Arc<dyn PolicyRepository>,
        event_bus: Arc<ThingEventBus>,
        throttle: Arc<ThrottleState>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        provider_factory: ProviderFactory,
        model: String,
        app_state: Option<Arc<crate::shared::app_state::AppState>>,
    ) -> Self {
        Self {
            db_pool,
            policy_repo,
            event_bus,
            throttle,
            memory,
            observer,
            provider_factory,
            model,
            app_state,
            agents: DashMap::new(),
        }
    }

    /// Get or lazily create the per-workspace autonomous agent, binding the
    /// current run context into its tools (O8 "每 Run 换内容").
    pub async fn get_or_create(
        &self,
        workspace_id: &str,
        ctx: Arc<RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        // Fast path: cached agent — swap the run context and return. Clone
        // under a brief shard lock; never hold a DashMap guard across .await
        // (AgentPool precedent).
        let cached = self
            .agents
            .get(workspace_id)
            .map(|entry| (Arc::clone(&entry.handle), Arc::clone(&entry.run_ctx_slot)));
        if let Some((handle, slot)) = cached {
            *slot.write().await = Some(ctx);
            return Ok(handle);
        }

        let slot = new_run_context_slot(Arc::clone(&ctx));
        let tools = build_autonomous_tools(
            &self.db_pool,
            workspace_id,
            Arc::clone(&self.policy_repo),
            Arc::clone(&slot),
            Arc::clone(&self.event_bus),
            Arc::clone(&self.throttle),
            self.app_state.clone(),
        );

        let provider = (self.provider_factory)()?;
        let memory: Arc<dyn Memory> = Arc::new(WorkspaceScopedMemory::new(
            Arc::clone(&self.memory),
            workspace_id.to_string(),
        ));

        // Mirrors AgentPool::build_agent, minus the response cache (an
        // autonomous control loop must never replay a stale decision).
        let agent = zeroclaw::agent::Agent::builder()
            .model_provider(provider)
            .tools(tools)
            .memory(memory)
            .observer(Arc::clone(&self.observer))
            .tool_dispatcher(Box::new(NativeToolDispatcher))
            .model_name(self.model.clone())
            .security_summary(Some(
                "Autonomous thing-agent loop: every action is gated by the workspace autonomy policy (mode/allowlist/denylist/rate fuses)."
                    .into(),
            ))
            .autonomy_level(AutonomyLevel::Supervised)
            .response_cache(None)
            .prompt_builder(SystemPromptBuilder::with_defaults())
            .workspace_dir(crate::shared::paths::workspace_dir(workspace_id))
            .build()
            .map_err(|e| anyhow!("Autonomous agent build failed: {}", e))?;

        // Double-checked insert: a concurrent creator may have won the race
        // while we were building. The loser's agent is dropped unstarted —
        // but the winner's slot must get OUR (latest) run context.
        use dashmap::mapref::entry::Entry;
        match self.agents.entry(workspace_id.to_string()) {
            Entry::Occupied(occupied) => {
                let handle = Arc::clone(&occupied.get().handle);
                let winner_slot = Arc::clone(&occupied.get().run_ctx_slot);
                drop(occupied);
                *winner_slot.write().await = Some(ctx);
                Ok(handle)
            }
            Entry::Vacant(vacant) => {
                let handle: AgentHandle = Arc::new(tokio::sync::Mutex::new(agent));
                vacant.insert(AutonomousEntry { handle: Arc::clone(&handle), run_ctx_slot: slot });
                tracing::info!(
                    workspace_id = workspace_id,
                    pool_size = self.agents.len(),
                    "Autonomous agent created"
                );
                Ok(handle)
            }
        }
    }

    /// Remove the cached agent (e.g. on workspace policy model change).
    pub fn invalidate(&self, workspace_id: &str) {
        if self.agents.remove(workspace_id).is_some() {
            tracing::info!(workspace_id = workspace_id, "Autonomous agent invalidated");
        }
    }

    pub fn pool_size(&self) -> usize {
        self.agents.len()
    }
}

/// T15: the ThingAgentManager drives runs through this trait.
#[async_trait::async_trait]
impl AutonomousAgentProvider for AutonomousAgentFactory {
    async fn get_or_create(
        &self,
        workspace_id: &str,
        ctx: Arc<RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        AutonomousAgentFactory::get_or_create(self, workspace_id, ctx).await
    }

    fn invalidate(&self, workspace_id: &str) {
        AutonomousAgentFactory::invalidate(self, workspace_id);
    }
}

/// The 9 thing-ontology tools with `invoke_action` swapped for the
/// autonomous variant (policy-gated, RunContext-injected).
pub(crate) fn build_autonomous_tools(
    pool: &SqlitePool,
    workspace_id: &str,
    policy_repo: Arc<dyn PolicyRepository>,
    run_ctx: RunContextSlot,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
    app_state: Option<Arc<crate::shared::app_state::AppState>>,
) -> Vec<Box<dyn Tool>> {
    // The 9 ontology tools minus the chat invoke_action (confirmation-token
    // flow), plus the autonomous invoke_action (policy-gated).
    let mut tools: Vec<Box<dyn Tool>> =
        create_thing_tools(pool.clone(), workspace_id, app_state.clone())
            .into_iter()
            .filter(|(tool, _)| tool.name() != "invoke_action")
            .map(|(tool, _)| tool)
            .collect();

    let inner = InvokeActionTool {
        thing_service: Arc::new(ThingService::new(pool.clone())),
        pool: pool.clone(),
        workspace_id: workspace_id.to_string(),
        app_state,
    };
    tools.push(Box::new(AutonomousInvokeActionTool::new(
        inner,
        policy_repo,
        run_ctx,
        pool.clone(),
        workspace_id.to_string(),
        event_bus,
        throttle,
    )));
    tools
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use zeroclaw::providers::{ChatRequest, ChatResponse};
    use zeroclaw_api::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};

    use super::*;

    // ── scripted provider (no network) ─────────────────────────

    struct ScriptedModelProvider {
        responses: Mutex<Vec<ChatResponse>>,
    }

    impl ScriptedModelProvider {
        fn new() -> Self {
            Self { responses: Mutex::new(vec![]) }
        }
    }

    #[async_trait]
    impl ModelProvider for ScriptedModelProvider {
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
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Ok(ChatResponse {
                    text: Some("done".into()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                });
            }
            Ok(guard.remove(0))
        }
    }

    impl Attributable for ScriptedModelProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }
        fn alias(&self) -> &str {
            "ScriptedModelProvider"
        }
    }

    fn scripted_provider_factory() -> ProviderFactory {
        Arc::new(|| Ok(Box::new(ScriptedModelProvider::new())))
    }

    // ── fixture ────────────────────────────────────────────────

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        tinyiothub_storage::migrations::run_migrations(&pool).await.expect("migrations");
        pool
    }

    fn test_factory(pool: SqlitePool) -> AutonomousAgentFactory {
        let observer: Arc<dyn Observer> = Arc::from(zeroclaw::observability::create_observer(
            &zeroclaw::config::schema::ObservabilityConfig {
                backend: zeroclaw::config::schema::ObservabilityBackend::None,
                ..Default::default()
            },
        ));
        AutonomousAgentFactory::new(
            pool.clone(),
            Arc::new(crate::modules::agent::policy_repo::SqlitePolicyRepository::new(pool)),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            Arc::new(zeroclaw::memory::NoneMemory::new("test")),
            observer,
            scripted_provider_factory(),
            "minimax-m2".to_string(),
            None,
        )
    }

    fn run_ctx() -> Arc<RwLock<RunContextInner>> {
        Arc::new(RwLock::new(RunContextInner::default()))
    }

    // ── tool composition ───────────────────────────────────────

    #[tokio::test]
    async fn build_autonomous_tools_has_9_unique_tools_with_autonomous_invoke() {
        let pool = test_pool().await;
        let tools = build_autonomous_tools(
            &pool,
            "ws-1",
            Arc::new(crate::modules::agent::policy_repo::SqlitePolicyRepository::new(pool.clone())),
            new_run_context_slot(run_ctx()),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            None,
        );

        let mut names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "get_thing",
                "get_thing_profile",
                "get_thing_tree",
                "invoke_action",
                "list_things",
                "query_events",
                "read_document",
                "read_property",
                "search_knowledge",
            ],
            "autonomous agent must carry exactly the 9 ontology tools"
        );
        let invoke = tools.iter().find(|t| t.name() == "invoke_action").expect("invoke_action");
        assert!(
            invoke.description().contains("自治"),
            "invoke_action must be the autonomous variant, got: {}",
            invoke.description()
        );
    }

    // ── caching / run-context swap ─────────────────────────────

    #[tokio::test]
    async fn get_or_create_caches_one_agent_per_workspace() {
        let pool = test_pool().await;
        let factory = test_factory(pool);

        let a1 = factory.get_or_create("ws-1", run_ctx()).await.expect("create ws-1");
        let a2 = factory.get_or_create("ws-1", run_ctx()).await.expect("cached ws-1");
        assert!(Arc::ptr_eq(&a1, &a2), "same workspace must reuse the cached agent");
        assert_eq!(factory.pool_size(), 1);

        let b1 = factory.get_or_create("ws-2", run_ctx()).await.expect("create ws-2");
        assert!(!Arc::ptr_eq(&a1, &b1), "different workspaces get different agents");
        assert_eq!(factory.pool_size(), 2);
    }

    #[tokio::test]
    async fn get_or_create_swaps_run_context_on_each_run() {
        let pool = test_pool().await;
        let factory = test_factory(pool);

        let ctx_run1 = run_ctx();
        factory.get_or_create("ws-1", Arc::clone(&ctx_run1)).await.expect("run 1");
        {
            let entry = factory.agents.get("ws-1").expect("cached");
            let slot = entry.run_ctx_slot.read().await;
            let bound = slot.as_ref().expect("slot bound");
            assert!(Arc::ptr_eq(bound, &ctx_run1), "slot must hold run 1 context");
        }

        let ctx_run2 = run_ctx();
        factory.get_or_create("ws-1", Arc::clone(&ctx_run2)).await.expect("run 2");
        {
            let entry = factory.agents.get("ws-1").expect("cached");
            let slot = entry.run_ctx_slot.read().await;
            let bound = slot.as_ref().expect("slot bound");
            assert!(Arc::ptr_eq(bound, &ctx_run2), "slot must be swapped to run 2 context");
            assert!(!Arc::ptr_eq(bound, &ctx_run1));
        }
        assert_eq!(factory.pool_size(), 1, "context swap must not rebuild the agent");
    }

    #[tokio::test]
    async fn invalidate_drops_cached_agent() {
        let pool = test_pool().await;
        let factory = test_factory(pool);

        let a1 = factory.get_or_create("ws-1", run_ctx()).await.expect("create");
        factory.invalidate("ws-1");
        assert_eq!(factory.pool_size(), 0);
        let a2 = factory.get_or_create("ws-1", run_ctx()).await.expect("rebuild");
        assert!(!Arc::ptr_eq(&a1, &a2), "rebuilt agent must be a fresh instance");
    }
}
