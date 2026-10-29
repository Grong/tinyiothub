//! Autonomous agent factory (T11, O8/O20).
//!
//! One autonomous zeroclaw Agent per workspace (DashMap cache), built with
//! the 9 thing-ontology tools where `invoke_action` is the autonomous
//! variant (policy gate + RunContext injection) instead of the chat
//! confirmation-token one. The thing-agent runner (T9) consumes the
//! returned [`AgentHandle`].
//!
//! Construction mirrors the chat `AgentPool` (now in `tinyiothub_agent::pool`): fast-path
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

use crate::domains::thing::service::ThingService;
use anyhow::anyhow;
use dashmap::DashMap;
use sqlx::SqlitePool;
use tinyiothub_agent::adapters::rig::loop_::rig_loop_factory;
use tinyiothub_agent::memory::workspace_memory::WorkspaceScopedMemory;
use tinyiothub_agent::pool::ProviderFactory;
use tinyiothub_agent::port::memory::Memory;
use tinyiothub_agent::port::observer::Observer;
use tinyiothub_agent::port::prompt::SystemPromptBuilder;
use tinyiothub_agent::port::runtime::AgentLoopConfig;
use tinyiothub_agent::port::tool::Tool;
use tinyiothub_agent::runtime::thing_agent::{AgentHandle, RunContextInner, manager::AutonomousAgentProvider};
use tinyiothub_storage::Db;
use tokio::sync::RwLock;

use super::tools::{
    AutonomousInvokeActionTool, RunContextSlot, ThingToolContext, create_thing_tools, new_run_context_slot,
    thing::InvokeActionTool,
};
use crate::domains::event::{bus::ThingEventBus, router::ThrottleState};

pub(crate) struct AutonomousEntry {
    pub handle: AgentHandle,
    pub run_ctx_slot: RunContextSlot,
}

pub struct AutonomousAgentFactory {
    db_pool: SqlitePool,
    policy_repo: Arc<Db>,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    provider_factory: ProviderFactory,
    model: String,
    runtime: ThingToolContext,
    /// E2 ToolExec 接缝（默认直通；组合根可替换证明可换）。
    tool_exec: Arc<dyn tinyiothub_agent::runtime::thing_agent::seams::tool_exec::ToolExecSeam>,
    pub(crate) agents: DashMap<String, AutonomousEntry>,
}

impl AutonomousAgentFactory {
    #[allow(clippy::too_many_arguments)] // 组合根工厂：参数即依赖清单，收敛于此一处
    pub fn new(
        db_pool: SqlitePool,
        policy_repo: Arc<Db>,
        event_bus: Arc<ThingEventBus>,
        throttle: Arc<ThrottleState>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        provider_factory: ProviderFactory,
        model: String,
        runtime: ThingToolContext,
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
            runtime,
            tool_exec: Arc::new(tinyiothub_agent::runtime::thing_agent::seams::tool_exec::DefaultToolExec),
            agents: DashMap::new(),
        }
    }

    /// E2：注入 ToolExec 接缝替换实现（组合根调用）。
    pub fn with_tool_exec(
        mut self,
        seam: Arc<dyn tinyiothub_agent::runtime::thing_agent::seams::tool_exec::ToolExecSeam>,
    ) -> Self {
        self.tool_exec = seam;
        self
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
        let tools: Vec<Box<dyn tinyiothub_agent::port::tool::Tool>> = build_autonomous_tools(
            &self.db_pool,
            workspace_id,
            Arc::clone(&self.policy_repo),
            Arc::clone(&slot),
            Arc::clone(&self.event_bus),
            Arc::clone(&self.throttle),
            &self.runtime,
        )
        .into_iter()
        .map(|t| {
            Box::new(tinyiothub_agent::runtime::thing_agent::seams::tool_exec::SeamTool::new(
                t,
                Arc::clone(&self.tool_exec),
                workspace_id,
            )) as Box<dyn tinyiothub_agent::port::tool::Tool>
        })
        .collect();

        let memory: Arc<dyn Memory> = Arc::new(WorkspaceScopedMemory::new(
            Arc::clone(&self.memory),
            workspace_id.to_string(),
        ));

        // Mirrors AgentPool::create (same rig_loop_factory), minus the
        // skills section and response cache (an autonomous control loop must
        // never replay a stale decision).
        let cfg = AgentLoopConfig {
            model_name: self.model.clone(),
            prompt_builder: SystemPromptBuilder::with_defaults(),
            tools,
            memory,
            observer: Arc::clone(&self.observer),
            workspace_dir: tinyiothub_agent::prompt::paths::workspace_dir(workspace_id),
            security_summary: Some(
                "Autonomous thing-agent loop: every action is gated by the workspace autonomy policy (mode/allowlist/denylist/rate fuses)."
                    .into(),
            ),
            provider_factory: Arc::clone(&self.provider_factory),
            // thing_agent 自治路径：保留 zeroclaw 时代的跨轮 recall 行为。
            conversation_memory: true,
        };
        let agent = rig_loop_factory(cfg).map_err(|e| anyhow!("Autonomous agent build failed: {}", e))?;

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
                let handle: AgentHandle = agent;
                vacant.insert(AutonomousEntry {
                    handle: Arc::clone(&handle),
                    run_ctx_slot: slot,
                });
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
    policy_repo: Arc<Db>,
    run_ctx: RunContextSlot,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
    runtime: &ThingToolContext,
) -> Vec<Box<dyn Tool>> {
    // The 9 ontology tools minus the chat invoke_action (confirmation-token
    // flow), plus the autonomous invoke_action (policy-gated).
    let mut task_runtime = runtime.clone();
    task_runtime.run_ctx = Some(run_ctx.clone());
    let mut tools: Vec<Box<dyn Tool>> = create_thing_tools(pool.clone(), workspace_id, &task_runtime)
        .into_iter()
        .filter(|(tool, _)| tool.name() != "call_device")
        .map(|(tool, _)| tool)
        .collect();

    let inner = InvokeActionTool {
        thing_service: Arc::new(ThingService::new(pool.clone())),
        pool: pool.clone(),
        workspace_id: workspace_id.to_string(),
        data_server: runtime.data_server.clone(),
        pending_actions: runtime
            .pending_actions
            .clone()
            .expect("pending_actions must be wired in ThingToolContext"),
        // E1 仲裁（Task 9）：与 chat 路径共享同一注册表；串行调度保证
        // 同 workspace 无并发 run，持有者名以 workspace 粒度即可读。
        arbitration: runtime.arbitration.clone(),
        arbitration_holder: format!("autonomous:{workspace_id}"),
        approval: None,
        // E4a：上下文已接线则沿用（自治 run 后续单独接线时只需注入 context）。
        access: runtime.access.clone(),
        // E4c：与 chat 路径共享声明资源解析。
        resources: runtime.resources.clone(),
        // Task-owned calls become durable Jobs; non-Task runs keep direct dispatch.
        jobs: runtime.jobs.clone(),
        run_ctx: Some(run_ctx.clone()),
    };
    tools.push(Box::new(
        AutonomousInvokeActionTool::new(
            inner,
            policy_repo,
            run_ctx,
            pool.clone(),
            workspace_id.to_string(),
            event_bus,
            throttle,
        )
        .with_task_approvals(runtime.task_approvals.clone()),
    ));
    tools
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};

    use super::*;

    // ── scripted provider (no network) ─────────────────────────

    struct ScriptedModelProvider {
        responses: Mutex<Vec<ChatResponse>>,
    }

    impl ScriptedModelProvider {
        fn new() -> Self {
            Self {
                responses: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ModelProvider for ScriptedModelProvider {
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
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn test_factory(pool: SqlitePool) -> AutonomousAgentFactory {
        AutonomousAgentFactory::new(
            pool.clone(),
            Arc::new(tinyiothub_storage::Db::new(pool)),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            scripted_provider_factory(),
            "minimax-m2".to_string(),
            ThingToolContext {
                pending_actions: Some(Arc::new(dashmap::DashMap::new())),
                ..Default::default()
            },
        )
    }

    fn run_ctx() -> Arc<RwLock<RunContextInner>> {
        Arc::new(RwLock::new(RunContextInner::default()))
    }

    // ── tool composition ───────────────────────────────────────

    #[tokio::test]
    async fn build_autonomous_tools_exposes_fixed_set_and_keeps_hidden_aliases() {
        let pool = test_pool().await;
        let tools = build_autonomous_tools(
            &pool,
            "ws-1",
            Arc::new(tinyiothub_storage::Db::new(pool.clone())),
            new_run_context_slot(run_ctx()),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            &ThingToolContext {
                pending_actions: Some(Arc::new(dashmap::DashMap::new())),
                ..Default::default()
            },
        );

        // E2：模型可见 = 7 设备工具 + 4 非设备工具（固定集合）。
        let mut exposed: Vec<&str> = tools.iter().filter(|t| t.exposed()).map(|t| t.name()).collect();
        exposed.sort_unstable();
        assert_eq!(
            exposed,
            vec![
                "call_device",
                "get_thing_tree",
                "list_composites",
                "list_devices",
                "query_events",
                "read_device",
                "read_document",
                "run_composite",
                "search_knowledge",
                "set_device",
                "stop",
                "unwatch",
                "watch",
            ],
            "exposed tool list must be fixed (device tools never grow per device)"
        );

        // 旧名仍注册为隐藏别名（批准通道兼容），但绝不进模型列表。
        for legacy in [
            "list_things",
            "get_thing",
            "get_thing_profile",
            "read_property",
            "invoke_action",
        ] {
            let tool = tools
                .iter()
                .find(|t| t.name() == legacy)
                .unwrap_or_else(|| panic!("legacy alias {legacy} must stay registered"));
            assert!(!tool.exposed(), "legacy alias {legacy} must be hidden");
        }

        let invoke = tools.iter().find(|t| t.name() == "call_device").expect("call_device");
        assert!(
            invoke.description().contains("自治"),
            "call_device must be the autonomous variant, got: {}",
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
        factory
            .get_or_create("ws-1", Arc::clone(&ctx_run1))
            .await
            .expect("run 1");
        {
            let entry = factory.agents.get("ws-1").expect("cached");
            let slot = entry.run_ctx_slot.read().await;
            let bound = slot.as_ref().expect("slot bound");
            assert!(Arc::ptr_eq(bound, &ctx_run1), "slot must hold run 1 context");
        }

        let ctx_run2 = run_ctx();
        factory
            .get_or_create("ws-1", Arc::clone(&ctx_run2))
            .await
            .expect("run 2");
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

    // ── Task-owned durable Job composition ─────────────────────
    //
    // These exercise the REAL production tool composition
    // (`build_autonomous_tools` → `AutonomousInvokeActionTool` →
    // `InvokeActionTool`) against a real `JobService` + DB store. The device
    // worker and declaration validator/resolver are the only fakes; the Job
    // facts, wake outbox rows and idempotency all go through SQLite.

    use tinyiothub_agent::runtime::thing_agent::RunContextInner;
    use tinyiothub_agent::runtime::thing_agent::task::approval::{
        ApprovalExecutor, ApprovalTasks, ApprovedJobs, ExecutionOutcome,
    };
    use tinyiothub_agent::runtime::thing_agent::task::coordinator::{Advance, TaskError};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::DeviceJobState;
    use tinyiothub_core::driver::manifest::{ManifestTool, ToolKindDecl, ToolSafetyDecl, ToolTimeout};
    use tinyiothub_core::models::thing_command::ThingCommand;
    use tinyiothub_core::task_approval::ApprovalDecision;
    use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
    use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
    use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
    use tinyiothub_runtime::job::service::{JobFinal, JobService};
    use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobResultSink, JobWorkerLocator};

    #[derive(Default)]
    struct SubmittingWorker {
        submitted: Mutex<Vec<String>>,
    }

    impl JobDeviceWorker for SubmittingWorker {
        fn submit(
            &self,
            _command: ThingCommand,
            job_id: &str,
            _lease: Option<ResourceLease>,
        ) -> Result<(), tinyiothub_core::error::Error> {
            self.submitted.lock().expect("submitted").push(job_id.to_string());
            Ok(())
        }

        fn cancel_job(&self, _job_id: &str) -> bool {
            false
        }

        fn has_recoverable_call(&self, _job_id: &str) -> bool {
            true
        }

        fn complete_job(&self, _job_id: &str) {}
    }

    struct OneWorker {
        worker: Arc<SubmittingWorker>,
    }

    impl JobWorkerLocator for OneWorker {
        fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
            Some(self.worker.clone())
        }
    }

    struct AcceptAllValidator;

    #[async_trait]
    impl AccessCallValidator for AcceptAllValidator {
        async fn validate(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            tool: &str,
            _params: Option<&serde_json::Value>,
        ) -> Result<ValidatedCall, CallError> {
            Ok(ValidatedCall {
                tool: ManifestTool {
                    name: tool.to_string(),
                    kind: ToolKindDecl::Action,
                    target_field: None,
                    parameters: serde_json::json!({"type":"object"}),
                    timeout: ToolTimeout {
                        default_ms: 5_000,
                        max_ms: 5_000,
                    },
                    safety: ToolSafetyDecl::Control,
                    resources: vec![],
                    cancellable: true,
                },
                timeout_ms: 5_000,
                clamped: false,
            })
        }
    }

    struct DeviceResources;

    #[async_trait]
    impl ResourceResolver for DeviceResources {
        async fn resolve_action(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _tool_name: &str,
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }

        async fn resolve_properties(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _properties: &[String],
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }
    }

    fn act_policy() -> tinyiothub_policy::autonomy::AutonomyPolicy {
        tinyiothub_policy::autonomy::AutonomyPolicy {
            mode: tinyiothub_policy::autonomy::AutonomyMode::Act,
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec![],
            max_actions_per_run: 5,
            max_actions_per_hour: 30,
        }
    }

    fn job_service(db: &tinyiothub_storage::Db, worker: Arc<SubmittingWorker>) -> JobService {
        JobService::new(
            Arc::new(crate::shared::runtime_ports::DbDeviceJobStore::new(db.clone())),
            Arc::new(OneWorker { worker }),
            Arc::new(AcceptAllValidator),
            Arc::new(DeviceResources),
            Arc::new(ArbitrationRegistry::default()),
        )
    }

    async fn seed_task_owned_device(
        pool: &SqlitePool,
        db: &tinyiothub_storage::Db,
        workspace_id: &str,
        thing_id: &str,
        task_id: &str,
        run_id: &str,
        require_confirm: bool,
    ) {
        crate::domains::agent::host::test_utils::seed_test_workspace(pool, "tenant-1", workspace_id).await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')")
            .bind(thing_id)
            .bind(format!("Thing {thing_id}"))
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("insert device");
        sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, ?, 'set_level')")
            .bind(format!("act-set-level-{thing_id}"))
            .bind(thing_id)
            .execute(pool)
            .await
            .expect("register action");
        sqlx::query("UPDATE workspaces SET require_action_confirm = ? WHERE id = ?")
            .bind(i64::from(require_confirm))
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("confirm gate");
        let now = chrono::Utc::now();
        let task = AgentTask {
            id: task_id.into(),
            workspace_id: workspace_id.into(),
            idempotency_key: format!("{task_id}-key"),
            objective: "exercise Task-owned Jobs".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: TaskStatus::Active,
            current_run_id: Some(run_id.into()),
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = TaskWake {
            task_id: task_id.into(),
            event_id: format!("task:{task_id}:created"),
            signal_json: serde_json::json!({"workspace_id": workspace_id}).to_string(),
            dispatch_state: WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        db.create_task_with_wake(&task, &wake).await.expect("Task");
        db.save_autonomy_policy(workspace_id, &act_policy(), "test")
            .await
            .expect("autonomy policy");
    }

    async fn task_run_context(workspace_id: &str, task_id: &str, run_id: &str) -> Arc<RwLock<RunContextInner>> {
        let ctx = Arc::new(RwLock::new(RunContextInner {
            run_id: run_id.into(),
            workspace_id: workspace_id.into(),
            task_id: Some(task_id.into()),
            ..Default::default()
        }));
        // The runner counts this call at dispatch time (in-flight inclusive).
        ctx.write().await.action_counts.insert("dev-1".to_string(), 1);
        ctx
    }

    /// File-backed pool for the approval composition test: the multi-statement
    /// approval decision + Job submission sequence needs a durable schema
    /// across pool connections (mirrors the approvals handler tests).
    async fn file_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().expect("factory test directory");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.path().join("factory.db"))
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5))
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .expect("file pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("migrations");
        (dir, pool)
    }

    async fn job_rows(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs")
            .fetch_one(pool)
            .await
            .expect("job count")
    }

    async fn wake_rows(pool: &SqlitePool, event_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = ?")
            .bind(event_id)
            .fetch_one(pool)
            .await
            .expect("wake count")
    }

    fn call_device(tools: &[Box<dyn Tool>]) -> &dyn Tool {
        tools
            .iter()
            .find(|tool| tool.name() == "call_device")
            .map(|tool| tool.as_ref())
            .expect("autonomous call_device")
    }

    fn call_args() -> serde_json::Value {
        serde_json::json!({"thingId":"dev-1","action":"set_level","params":{"level":7}})
    }

    #[tokio::test]
    async fn task_owned_autonomous_call_creates_one_durable_job_and_one_completion_wake() {
        let pool = test_pool().await;
        let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        seed_task_owned_device(&pool, &db, "ws-jobs", "dev-1", "task-1", "run-1", false).await;
        let worker = Arc::new(SubmittingWorker::default());
        let jobs = job_service(&db, worker.clone());
        let tools = build_autonomous_tools(
            &pool,
            "ws-jobs",
            db.clone(),
            new_run_context_slot(task_run_context("ws-jobs", "task-1", "run-1").await),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            &ThingToolContext {
                pending_actions: Some(Arc::new(dashmap::DashMap::new())),
                jobs: Some(jobs.clone()),
                ..Default::default()
            },
        );
        let call = call_device(&tools);

        let first = call.execute(call_args()).await.expect("tool call");
        let output: serde_json::Value = serde_json::from_str(&first.output).expect("tool output");
        assert_eq!(output["status"], "accepted", "{output}");
        assert_eq!(output["jobId"], "job:run-1:dev-1:1");
        assert_eq!(output["jobState"], "accepted");
        assert_eq!(
            worker.submitted.lock().expect("submitted").as_slice(),
            ["job:run-1:dev-1:1".to_string()],
            "exactly one worker submission"
        );
        let job = db
            .get_job("ws-jobs", "job:run-1:dev-1:1")
            .await
            .expect("job read")
            .expect("durable Job row");
        assert_eq!(job.task_id.as_deref(), Some("task-1"));
        assert_eq!(job.run_id.as_deref(), Some("run-1"));
        assert_eq!(job.tool_call_id.as_deref(), Some("run-1:dev-1:1"));
        assert_eq!(job.tool, "set_level");
        assert_eq!(job.args_json, serde_json::json!({"level":7}));
        assert_eq!(job.state, DeviceJobState::Accepted);
        assert_eq!(job_rows(&pool).await, 1);

        // Retry of the same tool call resolves to the same durable Job.
        let retry = call.execute(call_args()).await.expect("retry");
        let retry_output: serde_json::Value = serde_json::from_str(&retry.output).expect("retry output");
        assert_eq!(retry_output["jobId"], "job:run-1:dev-1:1");
        assert_eq!(retry_output["deduplicated"], true, "{retry_output}");
        assert_eq!(job_rows(&pool).await, 1, "retry must not create a second Job");
        assert_eq!(
            worker.submitted.lock().expect("submitted").len(),
            1,
            "retry must not submit the device command twice"
        );

        // The worker's terminal handoff follows Accepted → Running → Done and
        // creates exactly one Task wake; replay is a no-op.
        JobResultSink::command_finished(&jobs, "job:run-1:dev-1:1", true, None).await;
        let done = db
            .get_job("ws-jobs", "job:run-1:dev-1:1")
            .await
            .expect("job read")
            .expect("terminal Job");
        assert_eq!(done.state, DeviceJobState::Done);
        let wake_event = "device_job:job:run-1:dev-1:1:done";
        assert_eq!(wake_rows(&pool, wake_event).await, 1);
        let signal: String = sqlx::query_scalar("SELECT signal_json FROM task_wake_outbox WHERE event_id = ?")
            .bind(wake_event)
            .fetch_one(&pool)
            .await
            .expect("wake signal");
        let signal: serde_json::Value = serde_json::from_str(&signal).expect("wake signal JSON");
        assert_eq!(signal["kind"], "device_job_terminal");
        assert_eq!(signal["job_id"], "job:run-1:dev-1:1");
        assert_eq!(signal["workspace_id"], "ws-jobs");

        // A duplicate terminal handoff and a direct duplicate finish are both
        // no-ops: one Job, one wake.
        JobResultSink::command_finished(&jobs, "job:run-1:dev-1:1", true, None).await;
        let replay = jobs
            .finish("job:run-1:dev-1:1", JobFinal::Done(serde_json::json!({"ok":true})))
            .await
            .expect("duplicate finish");
        assert!(!replay.transitioned);
        assert!(!replay.wake_created);
        assert_eq!(wake_rows(&pool, wake_event).await, 1);
        assert_eq!(job_rows(&pool).await, 1);

        // JobService only enqueues the wake; the Task status is still owned by
        // the coordinator/router contract.
        let status: String = sqlx::query_scalar("SELECT status FROM agent_tasks WHERE id = 'task-1'")
            .fetch_one(&pool)
            .await
            .expect("task status");
        assert_eq!(status, "active");
    }

    #[tokio::test]
    async fn approval_gated_task_call_creates_zero_jobs_until_approval_then_exactly_one() {
        let (_dir, pool) = file_pool().await;
        let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        seed_task_owned_device(&pool, &db, "ws-approve", "dev-1", "task-1", "run-1", true).await;
        let worker = Arc::new(SubmittingWorker::default());
        let jobs = job_service(&db, worker.clone());
        let approvals: Arc<crate::domains::agent::runtime::approvals::DbApprovalStore> = Arc::new(
            crate::domains::agent::runtime::approvals::DbApprovalStore::new(db.clone()),
        );
        let tools = build_autonomous_tools(
            &pool,
            "ws-approve",
            db.clone(),
            new_run_context_slot(task_run_context("ws-approve", "task-1", "run-1").await),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            &ThingToolContext {
                pending_actions: Some(Arc::new(dashmap::DashMap::new())),
                jobs: Some(jobs.clone()),
                task_approvals: Some(approvals.clone()),
                ..Default::default()
            },
        );
        let call = call_device(&tools);

        let pending = call.execute(call_args()).await.expect("tool call");
        let pending_output: serde_json::Value = serde_json::from_str(&pending.output).expect("tool output");
        assert_eq!(pending_output["status"], "approval_pending", "{pending_output}");
        assert_eq!(job_rows(&pool).await, 0, "no Job before human approval");
        assert!(worker.submitted.lock().expect("submitted").is_empty());
        let mut approvals_list = db
            .list_task_approvals("ws-approve", "task-1")
            .await
            .expect("approval list");
        assert_eq!(approvals_list.len(), 1);
        let approval = approvals_list.pop().expect("approval");
        assert_eq!(approval.run_id, "run-1");
        assert_eq!(approval.device_id, "dev-1");
        assert_eq!(approval.tool, "set_level");
        assert_eq!(approval.args_json, serde_json::json!({"level":7}));

        let now = chrono::Utc::now();
        let decision_wake = TaskWake {
            task_id: "task-1".into(),
            event_id: approval.decision_event_id.clone(),
            signal_json: serde_json::json!({
                "kind":"approval_decided",
                "workspace_id":"ws-approve",
                "task_id":"task-1",
                "approval_id":approval.id,
            })
            .to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        tinyiothub_agent::port::approval::TaskApprovalStore::decide(
            approvals.as_ref(),
            "ws-approve",
            "task-1",
            &approval.id,
            ApprovalDecision::Approve,
            "user-1",
            now,
            &decision_wake,
        )
        .await
        .expect("approve");

        struct RecordedTasks;

        #[async_trait]
        impl ApprovalTasks for RecordedTasks {
            async fn advance(&self, _task_id: &str, _event_id: &str) -> Result<Advance, TaskError> {
                Ok(Advance::Recorded)
            }
        }

        let approved_jobs: Arc<dyn ApprovedJobs> = Arc::new(jobs.clone());
        let executor = ApprovalExecutor::new(approvals.clone(), approved_jobs, Arc::new(RecordedTasks));
        let outcome = executor
            .execute_decision(&approval.decision_event_id)
            .await
            .expect("approved execution");
        let ExecutionOutcome::Job(job) = outcome else {
            panic!("approved call must create exactly one Job");
        };
        assert_eq!(job.task_id.as_deref(), Some("task-1"));
        assert_eq!(job.run_id.as_deref(), Some("run-1"));
        assert_eq!(job.tool_call_id.as_deref(), Some(approval.tool_call_id.as_str()));
        assert_eq!(job.state, DeviceJobState::Accepted);
        assert_eq!(job_rows(&pool).await, 1);
        assert_eq!(
            worker.submitted.lock().expect("submitted").as_slice(),
            std::slice::from_ref(&job.id)
        );

        // Replaying the approved decision re-uses the same frozen Job instead
        // of submitting the side effect again.
        let replay = executor
            .execute_decision(&approval.decision_event_id)
            .await
            .expect("approved replay");
        let ExecutionOutcome::Job(replayed) = replay else {
            panic!("replay must return the existing Job");
        };
        assert_eq!(replayed.id, job.id);
        assert_eq!(job_rows(&pool).await, 1);
        assert_eq!(worker.submitted.lock().expect("submitted").len(), 1);

        JobResultSink::command_finished(&jobs, &job.id, true, None).await;
        let wake_event = format!("device_job:{}:done", job.id);
        assert_eq!(wake_rows(&pool, &wake_event).await, 1);
        JobResultSink::command_finished(&jobs, &job.id, true, None).await;
        assert_eq!(wake_rows(&pool, &wake_event).await, 1, "exactly-once completion wake");
        let done = db
            .get_job("ws-approve", &job.id)
            .await
            .expect("job read")
            .expect("Job row");
        assert_eq!(done.state, DeviceJobState::Done);
    }
}
