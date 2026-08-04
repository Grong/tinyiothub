//! Thing-agent manager (T15) — per-workspace loop registry, the closed-loop
//! wiring point between triggers (T5/T7), scheduler (T8) and the run
//! pipeline (T9 runner + T10 prompt + T11 factory + T12 persistence + T13
//! pushback).
//!
//! ```text
//! WorkspaceCreated → start(): ThingEventTrigger ─┐
//!                       TimerTrigger ────────────┼─► forward ─► SchedulerHandle ─► run_pipeline
//!                       DirectiveSink (T14) ─────┘                    │
//!                                                                     ▼
//!                                              build_prompt (T10, memory/history from T12 repo)
//!                                                                     │
//!                                              factory.get_or_create (T11) → runner.execute (T9)
//!                                                                     │
//!                                              insert_run (T12) → deliver (T13)
//! WorkspaceDeleted → stop(): abort triggers, drain (O26), drop handle
//! ```
//!
//! Mode gating (off/diagnose/act) is NOT re-implemented here: both triggers
//! (T5 timer / T7 thing-event) already suppress signals when the policy mode
//! is off, and the autonomous `invoke_action` tool (T11) fail-closes on the
//! policy gate. The manager is deliberately a dumb wiring layer.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use tinyiothub_policy::autonomy::{AutonomyMode, PolicyRepository};
use crate::thing_agent::prompt::build_prompt;
use crate::thing_agent::pushback::deliver;
use crate::thing_agent::report::AgentRunsRepository;
use crate::thing_agent::runner::{AgentHandle, RunContext, RunContextInner, Runner};
use crate::thing_agent::scheduler::{EnqueueError, Scheduler, SchedulerHandle};
use crate::thing_agent::traits::{DirectiveSink, ThingAgentHost};
use crate::thing_agent::trigger::{ThingEventTrigger, TimerTrigger, Trigger};
use crate::thing_agent::types::{TriggerSource, WakeSignal};

/// Capacity of the per-workspace trigger→scheduler channel. Backpressure
/// beyond this parks the thing-event trigger, which then lags the broadcast
/// and recovers through cursor replay (O27).
const TRIGGER_CHANNEL_CAPACITY: usize = 64;

/// Factory abstraction over the T11 autonomous agent factory (implemented by
/// `AutonomousAgentFactory` in the cloud crate; stubbed in tests). One call
/// per run: binds the run context and returns the per-workspace agent.
#[async_trait::async_trait]
pub trait AutonomousAgentProvider: Send + Sync {
    async fn get_or_create(
        &self,
        workspace_id: &str,
        ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle>;

    /// 失效该工作区的缓存 agent（loop 停止时调用，避免 WorkspaceDeleted
    /// 后缓存实例泄漏）。
    fn invalidate(&self, workspace_id: &str);
}

/// Tunables for [`ThingAgentManager`].
#[derive(Debug, Clone)]
pub struct ThingAgentManagerConfig {
    /// 定时巡检间隔（spec 默认 15min）。
    pub timer_interval: Duration,
    /// 物事件唤醒最低级别（spec 默认 3 = warning）。
    pub min_wake_level: i32,
    /// 调度器合并窗口（spec 默认 30s）。集成测试用亚秒窗口在真实时间下
    /// 运行（暂停时钟 auto-advance 与 sqlx 工作线程往返不兼容）。
    pub merge_window: Duration,
}

impl Default for ThingAgentManagerConfig {
    fn default() -> Self {
        Self {
            timer_interval: Duration::from_secs(15 * 60),
            min_wake_level: 3,
            merge_window: crate::thing_agent::scheduler::MERGE_WINDOW,
        }
    }
}

/// Clonable dependency bundle for the run pipeline.
#[derive(Clone)]
struct PipelineDeps {
    host: Arc<dyn ThingAgentHost>,
    policy_repo: Arc<dyn PolicyRepository>,
    runs_repo: Arc<dyn AgentRunsRepository>,
    agent_provider: Arc<dyn AutonomousAgentProvider>,
    runner: Arc<Runner>,
}

/// One running workspace loop: the scheduler handle plus the trigger/forward
/// tasks feeding it. Dropping the handle after drain lets the scheduler's
/// merger/consumer tasks exit on closed channels.
struct WorkspaceLoop {
    handle: SchedulerHandle,
    tasks: Vec<JoinHandle<()>>,
}

/// Per-workspace thing-agent loop registry. Also the T14 [`DirectiveSink`]
/// routing point: user directives are forwarded to the target workspace's
/// scheduler.
pub struct ThingAgentManager {
    deps: PipelineDeps,
    config: ThingAgentManagerConfig,
    workspaces: DashMap<String, WorkspaceLoop>,
}

impl ThingAgentManager {
    pub fn new(
        host: Arc<dyn ThingAgentHost>,
        policy_repo: Arc<dyn PolicyRepository>,
        agent_provider: Arc<dyn AutonomousAgentProvider>,
        runs_repo: Arc<dyn AgentRunsRepository>,
        runner: Arc<Runner>,
        config: ThingAgentManagerConfig,
    ) -> Self {
        Self {
            deps: PipelineDeps {
                host,
                policy_repo,
                runs_repo,
                agent_provider,
                runner,
            },
            config,
            workspaces: DashMap::new(),
        }
    }

    /// Start the loop for a workspace: scheduler + thing-event trigger +
    /// timer trigger + forward task. Idempotent — a running workspace is
    /// left untouched (Orchestrator duplicate-start precedent).
    pub fn start(&self, workspace_id: &str) {
        use dashmap::mapref::entry::Entry;
        let Entry::Vacant(vacant) = self.workspaces.entry(workspace_id.to_string()) else {
            tracing::debug!(workspace_id, "thing-agent loop already running — start ignored");
            return;
        };

        let ws = workspace_id.to_string();
        let deps = self.deps.clone();
        let handle = Scheduler::spawn_with_merge_window(
            ws.clone(),
            move |signal| {
                let deps = deps.clone();
                Box::pin(async move { run_pipeline(deps, signal).await })
            },
            self.config.merge_window,
        );

        let (tx, rx) = mpsc::channel::<WakeSignal>(TRIGGER_CHANNEL_CAPACITY);
        let forward = tokio::spawn(forward_signals(rx, handle.clone()));

        let event_trigger = ThingEventTrigger::new(
            Arc::clone(&self.deps.host),
            Arc::clone(&self.deps.policy_repo),
            ws.clone(),
            self.config.min_wake_level,
        );
        let event_tx = tx.clone();
        let event_ws = ws.clone();
        let event_task = tokio::spawn(async move {
            if let Err(e) = event_trigger.run(event_tx).await {
                tracing::warn!(workspace_id = %event_ws, error = %e, "thing event trigger exited with error");
            }
        });

        let timer = TimerTrigger {
            workspace_id: ws.clone(),
            interval: self.config.timer_interval,
            policy_repo: Arc::clone(&self.deps.policy_repo),
        };
        let timer_task = tokio::spawn(async move {
            if let Err(e) = timer.run(tx).await {
                tracing::warn!(workspace_id = %ws, error = %e, "timer trigger exited with error");
            }
        });

        vacant.insert(WorkspaceLoop {
            handle,
            tasks: vec![forward, event_task, timer_task],
        });
        tracing::info!(workspace_id, "thing-agent loop started");
    }

    /// Stop the loop: remove from the registry, abort the trigger/forward
    /// tasks (no new signals), then drain queued signals (O26). An in-flight
    /// run is not cancelled; `drain()` returns once it finishes. The cached
    /// per-workspace agent is invalidated last, so a deleted workspace leaves
    /// no factory entry behind.
    pub async fn stop(&self, workspace_id: &str) {
        let Some((_, lp)) = self.workspaces.remove(workspace_id) else {
            return;
        };
        for task in &lp.tasks {
            task.abort();
        }
        lp.handle.drain().await;
        drop(lp.handle);
        self.deps.agent_provider.invalidate(workspace_id);
        tracing::info!(workspace_id, "thing-agent loop stopped");
    }

    pub fn is_running(&self, workspace_id: &str) -> bool {
        self.workspaces.contains_key(workspace_id)
    }

    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }
}

#[async_trait::async_trait]
impl DirectiveSink for ThingAgentManager {
    /// Route a directive to the target workspace's scheduler. Unknown
    /// workspace (loop not started / already stopped) maps to
    /// [`EnqueueError::Closed`].
    fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
        let Some(entry) = self.workspaces.get(&signal.workspace_id) else {
            return Err(EnqueueError::Closed);
        };
        entry.handle.enqueue(signal)
    }

    /// O26 kill switch: forward the drain to the target workspace's
    /// scheduler; unknown workspace is a no-op. The handle is cloned out
    /// before `.await` — never hold a DashMap guard across it.
    async fn drain(&self, workspace_id: &str) {
        let handle = self.workspaces.get(workspace_id).map(|entry| entry.handle.clone());
        if let Some(handle) = handle {
            handle.drain().await;
        }
    }
}

/// Forward trigger output into the scheduler. Scheduler-side errors already
/// log their own metrics (throttled/dropped/rejected); `Closed` means the
/// loop is gone — exit.
async fn forward_signals(mut rx: mpsc::Receiver<WakeSignal>, handle: SchedulerHandle) {
    while let Some(signal) = rx.recv().await {
        if handle.enqueue(signal) == Err(EnqueueError::Closed) {
            break;
        }
    }
}

/// Human-readable trigger label persisted as `agent_runs.trigger_context`;
/// the `xxx:` prefix doubles as `trigger_type` (T12 `trigger_type_of`).
fn trigger_label(signal: &WakeSignal) -> String {
    match &signal.source {
        TriggerSource::ThingEvent {
            thing_id, event_name, ..
        } => format!("thing:{thing_id}:event:{event_name}"),
        TriggerSource::Timer => format!("timer:{}", signal.workspace_id),
        TriggerSource::UserDirective { user_id, .. } => format!("user:{user_id}"),
        TriggerSource::Merged { .. } => {
            format!("merged:{}", signal.dedup_key.as_deref().unwrap_or("signals"))
        }
    }
}

/// The full run chain (T9–T13) driven by the scheduler for every admitted
/// wake signal.
async fn run_pipeline(deps: PipelineDeps, signal: WakeSignal) {
    let ws = signal.workspace_id.clone();
    let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
    let ctx = RunContext::new(run_id.clone(), ws.clone(), trigger_label(&signal));

    // T10 injection sources (T12 repo). Reads fail soft: a memory/history
    // outage must not kill the run — the prompt just goes without them.
    let memory = deps.runs_repo.recent_summaries(&ws, 5).await.unwrap_or_else(|e| {
        tracing::warn!(workspace_id = %ws, error = %e, "recent_summaries failed — prompt without memory");
        vec![]
    });
    let history = match &signal.dedup_key {
        Some(key) => deps
            .runs_repo
            .history_by_dedup_key(&ws, key, 3)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(workspace_id = %ws, error = %e, "history_by_dedup_key failed — prompt without history");
                vec![]
            }),
        None => vec![],
    };

    // Boundary segment: the action names the policy gate would allow. Only
    // Act mode has usable actions (Diagnose/Off deny at the gate, T4).
    let allowed = match deps.policy_repo.load_autonomy(&ws).await {
        Ok(Some(policy)) if policy.mode == AutonomyMode::Act => policy.allowed_actions,
        Ok(_) => vec![],
        Err(e) => {
            tracing::warn!(workspace_id = %ws, error = %e, "policy read failed — prompt with empty action list");
            vec![]
        }
    };

    let prompt = build_prompt(&signal, &memory, &history, &allowed);

    let agent = match deps.agent_provider.get_or_create(&ws, Arc::clone(&ctx.inner)).await {
        Ok(agent) => agent,
        Err(e) => {
            tracing::error!(workspace_id = %ws, run_id = %run_id, error = %e, "autonomous agent unavailable — run aborted");
            return;
        }
    };

    let outcome = deps.runner.execute(agent, prompt, ctx).await;
    let report = outcome.report;

    // T12 落库失败不阻断回推：结果已经发生，用户仍应收到报告（T13 镜像语义）。
    // X6：心跳桥投递的指令携带 problem_key（O11 dedup 判定依据）；其余触发
    // 源为 None。
    let problem_key = match &signal.source {
        TriggerSource::UserDirective { problem_key, .. } => problem_key.as_deref(),
        _ => None,
    };
    if let Err(e) = deps
        .runs_repo
        .insert_run(&report, problem_key, signal.dedup_key.as_deref())
        .await
    {
        tracing::error!(workspace_id = %ws, run_id = %run_id, error = %e, "run report persist failed");
    }
    deliver(&report, &signal, deps.runs_repo.as_ref(), deps.host.as_ref()).await;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    use zeroclaw::providers::{ChatRequest, ChatResponse};
    use zeroclaw_api::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};

    use tinyiothub_policy::autonomy::AutonomyPolicy;
    use crate::thing_agent::traits::ThingEventSignal;
    use crate::thing_agent::types::{Outcome, Priority, RunReport};

    const WS: &str = "ws_01";

    // ── scripted LLM provider (no network) ─────────────────────

    /// Replies a fixed text, never calls tools; records every prompt.
    /// Clones share the prompt log (the agent holds one, the test another).
    #[derive(Clone, Default)]
    pub(crate) struct ScriptedProvider {
        pub(crate) prompts: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl zeroclaw::providers::traits::ModelProvider for ScriptedProvider {
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
            request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            self.prompts
                .lock()
                .unwrap()
                .push(request.messages.iter().map(|m| m.content.as_str()).collect::<String>());
            Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }
        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    /// Stub agent provider: one shared real zeroclaw Agent driven by the
    /// scripted provider (serial scheduler ⇒ no concurrent turns).
    pub(crate) struct StubAgentProvider {
        handle: AgentHandle,
        pub(crate) llm: Arc<ScriptedProvider>,
        pub(crate) invalidated: Mutex<Vec<String>>,
        _dir: tempfile::TempDir,
    }

    impl StubAgentProvider {
        pub(crate) fn new() -> Self {
            let llm = Arc::new(ScriptedProvider::default());
            let dir = tempfile::tempdir().expect("tempdir");
            let observer: Arc<dyn zeroclaw::observability::Observer> = Arc::from(
                zeroclaw::observability::create_observer(&zeroclaw::config::schema::ObservabilityConfig {
                    backend: zeroclaw::config::schema::ObservabilityBackend::None,
                    ..Default::default()
                }),
            );
            let agent = zeroclaw::agent::Agent::builder()
                .model_provider(Box::new(llm.as_ref().clone()))
                .tools(vec![])
                .memory(Arc::new(zeroclaw::memory::NoneMemory::new("test")))
                .observer(observer)
                .tool_dispatcher(Box::new(zeroclaw::agent::dispatcher::NativeToolDispatcher))
                .model_name("stub-model".to_string())
                .prompt_builder(zeroclaw::agent::prompt::SystemPromptBuilder::with_defaults())
                .workspace_dir(dir.path().to_path_buf())
                .build()
                .expect("build stub agent");
            Self {
                handle: Arc::new(tokio::sync::Mutex::new(agent)),
                llm,
                invalidated: Mutex::new(vec![]),
                _dir: dir,
            }
        }
    }

    #[async_trait::async_trait]
    impl AutonomousAgentProvider for StubAgentProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            Ok(Arc::clone(&self.handle))
        }

        fn invalidate(&self, workspace_id: &str) {
            self.invalidated.lock().unwrap().push(workspace_id.to_string());
        }
    }

    // ── host / policy / runs stubs ─────────────────────────────

    pub(crate) struct StubHost {
        pub(crate) tx: tokio::sync::broadcast::Sender<ThingEventSignal>,
        pub(crate) pushes: Mutex<Vec<(String, String, String)>>,
        pub(crate) alerts: Mutex<Vec<(String, serde_json::Value)>>,
        pub(crate) admin_session: Option<String>,
    }

    impl StubHost {
        pub(crate) fn new() -> Self {
            let (tx, _) = tokio::sync::broadcast::channel(64);
            Self {
                tx,
                pushes: Mutex::new(vec![]),
                alerts: Mutex::new(vec![]),
                admin_session: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl ThingAgentHost for StubHost {
        fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
            self.tx.subscribe()
        }

        async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            Ok(vec![])
        }

        async fn push_chat_message(&self, session_key: &str, content: &str, run_id: &str) -> anyhow::Result<()> {
            self.pushes
                .lock()
                .unwrap()
                .push((session_key.to_string(), content.to_string(), run_id.to_string()));
            Ok(())
        }

        async fn notify_alert(&self, workspace_id: &str, payload: serde_json::Value) -> anyhow::Result<()> {
            self.alerts.lock().unwrap().push((workspace_id.to_string(), payload));
            Ok(())
        }

        async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
            Ok(self.admin_session.clone())
        }
    }

    pub(crate) struct StubPolicyRepo {
        policy: Mutex<Option<AutonomyPolicy>>,
    }

    impl StubPolicyRepo {
        pub(crate) fn act() -> Self {
            Self {
                policy: Mutex::new(Some(AutonomyPolicy {
                    mode: AutonomyMode::Act,
                    allowed_actions: vec!["*".to_string()],
                    denied_actions: vec![],
                    max_actions_per_run: 3,
                    max_actions_per_hour: 30,
                })),
            }
        }
    }

    #[async_trait::async_trait]
    impl PolicyRepository for StubPolicyRepo {
        async fn load_autonomy(&self, _workspace_id: &str) -> anyhow::Result<Option<AutonomyPolicy>> {
            Ok(self.policy.lock().unwrap().clone())
        }

        async fn save_autonomy(
            &self,
            _workspace_id: &str,
            policy: &AutonomyPolicy,
            _updated_by: &str,
        ) -> anyhow::Result<()> {
            *self.policy.lock().unwrap() = Some(policy.clone());
            Ok(())
        }

        async fn count_actions_last_hour(&self, _workspace_id: &str) -> anyhow::Result<u32> {
            Ok(0)
        }
    }

    #[derive(Debug, Clone)]
    pub(crate) struct RecordedRun {
        pub(crate) run_id: String,
        pub(crate) trigger: String,
        pub(crate) outcome: Outcome,
        pub(crate) problem_key: Option<String>,
        pub(crate) dedup_key: Option<String>,
    }

    #[derive(Default)]
    pub(crate) struct StubRunsRepo {
        pub(crate) runs: Mutex<Vec<RecordedRun>>,
    }

    impl StubRunsRepo {
        pub(crate) fn runs_with_dedup(&self, key: &str) -> usize {
            self.runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.dedup_key.as_deref() == Some(key))
                .count()
        }
    }

    #[async_trait::async_trait]
    impl AgentRunsRepository for StubRunsRepo {
        async fn insert_run(
            &self,
            report: &RunReport,
            problem_key: Option<&str>,
            dedup_key: Option<&str>,
        ) -> anyhow::Result<()> {
            self.runs.lock().unwrap().push(RecordedRun {
                run_id: report.run_id.clone(),
                trigger: report.trigger.clone(),
                outcome: report.outcome,
                problem_key: problem_key.map(str::to_string),
                dedup_key: dedup_key.map(str::to_string),
            });
            Ok(())
        }

        async fn recent_summaries(&self, _workspace_id: &str, _limit: u32) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn history_by_dedup_key(
            &self,
            _workspace_id: &str,
            _key: &str,
            _limit: u32,
        ) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn recent_runs_by_dedup_key(
            &self,
            _workspace_id: &str,
            _key: &str,
            _limit: u32,
        ) -> anyhow::Result<Vec<RunReport>> {
            Ok(vec![])
        }

        async fn ack_run(&self, _run_id: &str, _actor: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn last_problem_run(
            &self,
            _workspace_id: &str,
            _problem_key: &str,
            _since_hours: u32,
        ) -> anyhow::Result<Option<(Outcome, bool, bool)>> {
            Ok(None)
        }

        async fn count_problem_runs(
            &self,
            _workspace_id: &str,
            _problem_key: &str,
            _since_hours: u32,
        ) -> anyhow::Result<u32> {
            Ok(0)
        }
    }

    pub(crate) struct StubManagerParts {
        pub(crate) manager: Arc<ThingAgentManager>,
        pub(crate) host: Arc<StubHost>,
        pub(crate) runs: Arc<StubRunsRepo>,
        pub(crate) agents: Arc<StubAgentProvider>,
    }

    /// Manager with all-stub deps and a 24h timer interval (the timer's
    /// immediate first tick still parks one signal in a merge window;
    /// assertions filter by dedup_key).
    pub(crate) fn stub_manager() -> StubManagerParts {
        let host = Arc::new(StubHost::new());
        let runs = Arc::new(StubRunsRepo::default());
        let agents = Arc::new(StubAgentProvider::new());
        let manager = Arc::new(ThingAgentManager::new(
            host.clone(),
            Arc::new(StubPolicyRepo::act()),
            agents.clone(),
            runs.clone(),
            Arc::new(Runner::new()),
            ThingAgentManagerConfig {
                timer_interval: Duration::from_secs(24 * 3600),
                min_wake_level: 3,
                merge_window: crate::thing_agent::scheduler::MERGE_WINDOW,
            },
        ));
        StubManagerParts {
            manager,
            host,
            runs,
            agents,
        }
    }

    // ── helpers ────────────────────────────────────────────────

    fn event(event_id: i64, level: i32, actor: &str) -> ThingEventSignal {
        ThingEventSignal {
            workspace_id: WS.to_string(),
            thing_id: "t1".to_string(),
            event_name: "temp_high".to_string(),
            event_id,
            level,
            data: serde_json::json!({"value": 42}),
            is_unknown: false,
            actor: actor.to_string(),
        }
    }

    const EVENT_KEY: &str = "thing:t1:event:temp_high";

    async fn wait_subscribed(host: &StubHost) {
        for _ in 0..10_000 {
            if host.tx.receiver_count() > 0 {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("thing-event trigger did not subscribe");
    }

    /// Yield until `cond` holds; panics after a bounded number of yields so a
    /// broken pipeline fails loudly instead of hanging (paused-time safe).
    async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        for _ in 0..20_000 {
            if cond() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("timed out waiting for {what}");
    }

    /// Let the trigger→forward→merger chain pick up freshly sent events.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    // ── tests ──────────────────────────────────────────────────

    // Critical thing event → immediate wake (no merge window) → full pipeline:
    // run persisted with the event dedup key, pushback falls back to alert.
    #[tokio::test(start_paused = true)]
    async fn critical_event_runs_full_pipeline() {
        let parts = stub_manager();
        parts.manager.start(WS);
        assert!(parts.manager.is_running(WS));
        wait_subscribed(&parts.host).await;

        parts.host.tx.send(event(1, 5, "device")).expect("send critical event");

        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "run persisted").await;

        let runs = parts.runs.runs.lock().unwrap().clone();
        let run = runs.iter().find(|r| r.dedup_key.as_deref() == Some(EVENT_KEY)).unwrap();
        assert_eq!(run.trigger, EVENT_KEY, "trigger label must be the event dedup key");
        // Scripted LLM replies plain text without tool calls → no actions.
        assert_eq!(run.outcome, Outcome::NoActionNeeded);
        drop(runs);

        // T13 pushback: no user session, no admin session → alert fallback.
        wait_until(|| !parts.host.alerts.lock().unwrap().is_empty(), "pushback alert").await;
        let alerts = parts.host.alerts.lock().unwrap();
        assert_eq!(alerts[0].1["reason"], "no_active_session");

        // The run really went through prompt assembly (T10).
        let prompts = parts.agents.llm.prompts.lock().unwrap();
        assert!(
            prompts
                .iter()
                .any(|p| p.contains("自治运维 Agent") && p.contains("temp_high")),
            "LLM must receive the assembled four-segment prompt"
        );
    }

    // 5 same-key warning events inside the 30s merge window → exactly 1 run.
    #[tokio::test(start_paused = true)]
    async fn normal_events_merge_into_single_run() {
        let parts = stub_manager();
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        for id in 1..=5 {
            parts.host.tx.send(event(id, 3, "device")).expect("send event");
        }
        settle().await;

        tokio::time::advance(Duration::from_secs(30)).await;
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "merged run").await;

        tokio::time::advance(Duration::from_secs(60)).await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            1,
            "5 events in one window must collapse into exactly 1 run"
        );
    }

    // Resonance guard (O21): agent-produced events never wake the loop, even
    // at critical level.
    #[tokio::test(start_paused = true)]
    async fn agent_actor_event_does_not_wake() {
        let parts = stub_manager();
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts
            .host
            .tx
            .send(event(1, 5, "agent"))
            .expect("send agent-actor event");
        settle().await;
        // No time advance: any wake would have run by now (critical bypasses
        // the merge window); the timer's first tick is parked unflushed.
        assert_eq!(
            parts.runs.runs.lock().unwrap().len(),
            0,
            "agent-actor event must not wake"
        );

        // A device-actor event at the same level DOES wake — the block above
        // was the actor filter, not a dead loop.
        parts.host.tx.send(event(2, 5, "device")).expect("send device event");
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "device event wakes").await;
    }

    // Duplicate start() must not spawn a second loop (two triggers would
    // double every wake).
    #[tokio::test(start_paused = true)]
    async fn start_is_idempotent() {
        let parts = stub_manager();
        parts.manager.start(WS);
        parts.manager.start(WS);
        assert_eq!(parts.manager.workspace_count(), 1);
        wait_subscribed(&parts.host).await;

        parts.host.tx.send(event(1, 5, "device")).expect("send event");
        wait_until(|| parts.runs.runs_with_dedup(EVENT_KEY) == 1, "single run").await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            1,
            "duplicate start must not double runs"
        );
    }

    // stop(): triggers aborted, queued signals drained, later events ignored.
    #[tokio::test(start_paused = true)]
    async fn stop_halts_loop() {
        let parts = stub_manager();
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts.manager.stop(WS).await;
        assert!(!parts.manager.is_running(WS));
        assert_eq!(parts.manager.workspace_count(), 0);

        // Broadcast with zero receivers errors — the loop is gone, as intended.
        let _ = parts.host.tx.send(event(1, 5, "device"));
        settle().await;
        assert!(parts.runs.runs.lock().unwrap().is_empty(), "stopped loop must not run");

        // Stop is idempotent.
        parts.manager.stop(WS).await;
    }

    // stop() 必须失效工厂里缓存的 per-workspace agent（WorkspaceDeleted 不
    // 泄漏缓存实例）。
    #[tokio::test(start_paused = true)]
    async fn stop_invalidates_cached_agent() {
        let parts = stub_manager();
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        parts.manager.stop(WS).await;
        assert_eq!(
            parts.agents.invalidated.lock().unwrap().as_slice(),
            &[WS.to_string()],
            "stop must invalidate the cached agent for the workspace"
        );

        // Unknown workspace stop → no invalidate call.
        parts.manager.stop("ws_unknown").await;
        assert_eq!(parts.agents.invalidated.lock().unwrap().len(), 1);
    }

    // O26 kill switch 接线：DirectiveSink::drain 经 manager 路由到目标工作区
    // 调度器，清空合并窗口里待处理的信号；未知工作区 no-op。
    #[tokio::test(start_paused = true)]
    async fn directive_sink_drain_clears_pending_queue() {
        let parts = stub_manager();
        parts.manager.start(WS);
        wait_subscribed(&parts.host).await;

        // warning 事件进 30s 合并窗口（pending，尚未 flush）。
        parts.host.tx.send(event(1, 3, "device")).expect("send event");
        settle().await;

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.drain("ws_unknown").await; // no-op，不得 panic
        sink.drain(WS).await;

        // 越过合并窗口：被 drain 的信号不得产生 run（对照组见
        // normal_events_merge_into_single_run —— 不 drain 则 1 run）。
        tokio::time::advance(Duration::from_secs(60)).await;
        settle().await;
        assert_eq!(
            parts.runs.runs_with_dedup(EVENT_KEY),
            0,
            "drained pending signal must not run"
        );
    }

    // T14 DirectiveSink: directives route to the workspace scheduler and run
    // immediately (no merge window); the T13 pushback hits the user session.
    #[tokio::test(start_paused = true)]
    async fn directive_sink_routes_and_pushes_to_session() {
        let parts = stub_manager();
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "把车间温度降到 26 度".to_string(),
                session_key: Some("agent:ws_01:a/s1".to_string()),
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        })
        .expect("directive accepted");

        wait_until(|| !parts.host.pushes.lock().unwrap().is_empty(), "session push").await;
        let pushes = parts.host.pushes.lock().unwrap().clone();
        assert_eq!(pushes[0].0, "agent:ws_01:a/s1");
        assert!(pushes[0].1.contains("done"), "assistant message carries the summary");

        let runs = parts.runs.runs.lock().unwrap();
        let run = runs
            .iter()
            .find(|r| r.trigger == "user:u1")
            .expect("user run persisted");
        assert_eq!(run.dedup_key, None);
        assert_eq!(pushes[0].2, run.run_id, "push carries the persisted run id");
        drop(runs);

        // Unknown workspace → Closed (loop never started).
        let err = sink.enqueue(WakeSignal {
            workspace_id: "ws_unknown".to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "hi".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        });
        assert_eq!(err, Err(EnqueueError::Closed));
    }

    // X6 (T18): 心跳桥投递的 directive（source=Some("heartbeat")）携带
    // problem_key —— run 落库必须带上，否则 O11 dedup 永远查不到历史。
    #[tokio::test(start_paused = true)]
    async fn heartbeat_directive_run_records_problem_key() {
        let parts = stub_manager();
        parts.manager.start(WS);

        let sink: &dyn DirectiveSink = parts.manager.as_ref();
        sink.enqueue(WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::UserDirective {
                user_id: "heartbeat".to_string(),
                text: "心跳巡检发现待处置问题 set_hvac:dev-1，请诊断并处置。".to_string(),
                session_key: None,
                source: Some("heartbeat".to_string()),
                problem_key: Some("set_hvac:dev-1".to_string()),
            },
            dedup_key: None,
        })
        .expect("heartbeat directive accepted");

        wait_until(|| !parts.runs.runs.lock().unwrap().is_empty(), "run persisted").await;
        let runs = parts.runs.runs.lock().unwrap();
        let run = runs
            .iter()
            .find(|r| r.problem_key.as_deref() == Some("set_hvac:dev-1"))
            .expect("run must record the heartbeat problem_key");
        assert_eq!(run.trigger, "user:heartbeat");
        assert_eq!(run.dedup_key, None, "心跳 directive 不参与合并，dedup_key 为空");
    }

    #[test]
    fn trigger_labels_carry_type_prefixes() {
        let event_signal = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id: 7,
                level: 3,
                data: serde_json::Value::Null,
            },
            dedup_key: Some(EVENT_KEY.to_string()),
        };
        assert_eq!(trigger_label(&event_signal), EVENT_KEY);

        let timer = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Timer,
            dedup_key: Some("timer:ws_01".to_string()),
        };
        assert_eq!(trigger_label(&timer), "timer:ws_01");

        let directive = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "x".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        };
        assert_eq!(trigger_label(&directive), "user:u1");

        let merged = WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![event_signal],
            },
            dedup_key: Some(EVENT_KEY.to_string()),
        };
        assert_eq!(trigger_label(&merged), format!("merged:{EVENT_KEY}"));
    }
}
