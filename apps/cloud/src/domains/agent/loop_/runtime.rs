//! AgentRuntime — agent loop 子系统的门面（Task 3）。
//!
//! 聚合现有 `ThingAgentManager` / `HeartbeatRunner` / `Orchestrator`（不重写其
//! 逻辑），持有 `AgentEventBus`（Task 2）与恢复快照状态。启动顺序（Task 11）：
//! 先 `subscribe()` 再 `restore()`，保证持久化订阅者不丢事件。
//!
//! 状态真源（Task 5 起）：heartbeat 段（tasks/trust/interval）由
//! `HeartbeatRunner` 内存持有 —— restore 预热注入，命令方法同步更新内存并
//! 发射事件（DB 写由 cloud 侧 service 在调命令前完成，D11-⑤ 写序）；
//! recent_runs 段自 Task 4 起由 `RunRegistry` 承接。

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::warn;

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};
use tinyiothub_memory::service::MemoryService;
use tinyiothub_runtime::EventBus;
use tinyiothub_storage::heartbeat::HeartbeatTaskRepository;
use tinyiothub_storage::policy::PolicyRepository;

use super::event::bus::{AiEventPublisher, DropNotifier};
use super::event::dlq::DeadLetterQueue;
use super::events::{AgentEvent, AgentEventBus, AgentEventKind};
use super::heartbeat::runner::HeartbeatRunner;
use super::heartbeat::types::HeartbeatConfig;
use super::orchestrator::Orchestrator;
use super::orchestrator::callbacks::HeartbeatBridge;
use super::snapshot::RestoreSnapshot;
use super::thing_agent::manager::{AutonomousAgentProvider, ThingAgentManager, ThingAgentManagerConfig};
use super::thing_agent::registry::RunRegistry;
use super::thing_agent::report::AgentRunsRepository;
use super::thing_agent::runner::Runner;
use super::thing_agent::traits::ThingAgentHost;

/// RuntimeDeps — 聚合三大组件现有构造所需依赖（收集自 service_manager.rs
/// 现有接线；本任务只聚已有件，不造新依赖）。
pub struct RuntimeDeps {
    // HeartbeatRunner 构造件（Task 5 起 repo 不再进 runner：仅供 Orchestrator
    // 落库 insert_result / cloud 侧 service 读写）
    pub heartbeat_task_repo: Arc<HeartbeatTaskRepository>,
    pub event_publisher: Arc<AiEventPublisher>,
    pub heartbeat_config: HeartbeatConfig,
    // ThingAgentManager 构造件
    pub thing_agent_host: Arc<dyn ThingAgentHost>,
    pub policy_repo: Arc<PolicyRepository>,
    pub agent_provider: Arc<dyn AutonomousAgentProvider>,
    pub runs_repo: Arc<AgentRunsRepository>,
    pub thing_agent_config: ThingAgentManagerConfig,
    // Orchestrator 构造件
    pub event_bus: Arc<EventBus>,
    pub memory_service: Arc<MemoryService>,
    pub drop_notifier: Option<Arc<dyn DropNotifier>>,
    pub dlq: Option<Arc<dyn DeadLetterQueue>>,
    /// AgentEventBus 广播容量（lagged 订阅者经 dump_state 对账恢复）
    pub agent_event_capacity: usize,
}

/// Agent 子系统门面。命令入站（D3）；调用约定（D11-⑤）：cloud 先写 DB
/// 成功，再调命令；命令失败告警。
pub struct AgentRuntime {
    thing_agents: Arc<ThingAgentManager>,
    heartbeat: Arc<HeartbeatRunner>,
    orchestrator: Arc<Orchestrator>,
    events: Arc<AgentEventBus>,
    /// run 记录内存真相源（Task 4 RunRegistry）：restore 时由快照预热，
    /// 与 ThingAgentManager 共享同一实例；dump_state/active_runs 由此导出。
    run_registry: RunRegistry,
}

impl AgentRuntime {
    /// 用快照构造运行时：聚合三大组件（接线同 service_manager.rs），并将
    /// heartbeat 段预热进 runner 内存（Task 5 内存真源）。
    /// 只做构造与预热，不启动任何 loop —— 启动顺序由 Task 11 编排。
    pub fn restore(snapshot: RestoreSnapshot, deps: RuntimeDeps) -> Self {
        let events = Arc::new(AgentEventBus::new(deps.agent_event_capacity));
        let run_registry = RunRegistry::new();
        run_registry.prewarm(snapshot.recent_runs);
        let heartbeat = Arc::new(HeartbeatRunner::new(
            deps.event_publisher,
            deps.heartbeat_config,
        ));
        // 预热：快照 heartbeat 段注入 runner 内存（命令路径之外的唯一注入口）。
        for state in &snapshot.heartbeat {
            heartbeat.set_tasks(&state.workspace_id, state.tasks.clone());
            heartbeat.update_trust_config(&state.workspace_id, state.trust_config.clone());
            heartbeat.set_interval_minutes(&state.workspace_id, state.interval_minutes);
        }
        let thing_agents = Arc::new(ThingAgentManager::new(
            deps.thing_agent_host,
            deps.policy_repo,
            deps.agent_provider,
            run_registry.clone(),
            events.clone(),
            Arc::new(Runner::new()),
            deps.thing_agent_config,
        ));
        // T18 X6 心跳桥：HeartbeatCompleted 的结构化 proposals 投递 UserDirective。
        let bridge = Arc::new(HeartbeatBridge::new(deps.runs_repo, thing_agents.clone()));
        let orchestrator = Arc::new(Orchestrator::new(
            deps.event_bus,
            heartbeat.clone(),
            deps.heartbeat_task_repo,
            deps.memory_service,
            deps.drop_notifier,
            deps.dlq,
            Some(thing_agents.clone()),
            Some(bridge),
        ));

        Self {
            thing_agents,
            heartbeat,
            orchestrator,
            events,
            run_registry,
        }
    }

    /// 订阅 AgentEvent 流（Task 11 启动顺序：先 subscribe 再 restore）。
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }

    /// 导出当前状态快照（Lagged resync + 周期对账出口）。
    /// heartbeat 段读 runner 内存真源（Task 5）；recent_runs 段读
    /// RunRegistry 内存真源（Task 4）。
    pub fn dump_state(&self) -> RestoreSnapshot {
        let mut heartbeat = self.heartbeat.snapshot_states();
        // 排序保证导出确定性（DashMap 迭代序不稳定），便于对账 diff。
        heartbeat.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        let mut recent_runs = self.run_registry.active();
        // 同理排序（RunReport 无时间戳，run_id 字典序保证确定性）。
        recent_runs.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        RestoreSnapshot { heartbeat, recent_runs }
    }

    /// trust config 变更命令：更新 runner 内存 + 发射事件（Task 5）。
    /// DB 写由 cloud 侧 service 在调本命令前完成（D11-⑤ 写序）。
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.heartbeat.update_trust_config(workspace_id, config.clone());
        self.events.emit(AgentEventKind::TrustConfigChanged {
            workspace_id: workspace_id.to_string(),
            config: Box::new(config),
        });
    }

    /// 心跳间隔变更命令：更新 runner 内存；运行中的 loop 重启使新间隔生效
    /// （`HeartbeatRunner::start` 幂等：先 stop 再起）。入站校验与 DB 写
    /// （含 enabled 归并）由 cloud 侧 handler 先做。
    pub fn update_heartbeat_interval(&self, workspace_id: &str, interval_minutes: u32) {
        self.heartbeat.set_interval_minutes(workspace_id, interval_minutes);
        if self.heartbeat.active_workspaces().iter().any(|w| w == workspace_id) {
            let runner = self.heartbeat.clone();
            let ws = workspace_id.to_string();
            spawn_delegate(async move {
                runner.start(&ws).await;
            });
        }
    }

    /// 心跳任务全量替换命令：更新 runner 内存（运行中 loop 经 ReloadTasks
    /// 信号重读内存）+ 发射事件。调用方已写 DB（D11-⑤）。
    pub fn reload_heartbeat_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        self.heartbeat.set_tasks(workspace_id, tasks);
        self.events.emit(AgentEventKind::HeartbeatTasksChanged {
            workspace_id: workspace_id.to_string(),
        });
    }

    /// 实时读 API（D13）：读 RunRegistry 窗口（Task 4 起为内存真源）。
    pub fn active_runs(&self) -> Vec<RunReport> {
        self.run_registry.active()
    }

    /// 工作区心跳任务：读 runner 内存真源（Task 5）。
    pub fn heartbeat_tasks(&self, workspace_id: &str) -> Vec<HeartbeatTask> {
        self.heartbeat.tasks(workspace_id)
    }

    /// 事件总线句柄 —— 测试经 bus().emit(...) 注入事件（Task 8）。
    pub fn bus(&self) -> &AgentEventBus {
        &self.events
    }

    /// 组件句柄（Task 11 启动编排用：start/stop 各工作区 loop）。
    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        &self.heartbeat
    }

    pub fn thing_agents(&self) -> &Arc<ThingAgentManager> {
        &self.thing_agents
    }

    pub fn orchestrator(&self) -> &Arc<Orchestrator> {
        &self.orchestrator
    }
}

/// 命令委托派发：同步命令 API 桥到现有异步路径。无 tokio 上下文时（正常
/// 调用方均为 async handler，不应发生）记 warning 跳过 —— 门面状态已先行更新。
fn spawn_delegate<F>(fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(fut);
        }
        Err(_) => warn!("AgentRuntime command delegate skipped: no tokio runtime"),
    }
}

#[cfg(test)]
mod stubs {
    //! 无 I/O 测试夹具桩：所有 async 方法不被触达（restore 只构造不启动）。

    use super::*;
    use tinyiothub_llm::provider::{LlmProvider, LlmResponse};

    use super::super::thing_agent::runner::{AgentHandle, RunContextInner};
    use super::super::thing_agent::traits::ThingEventSignal;

    /// noop LLM provider：chat 永远失败（夹具中不会触达）。
    pub struct NoopLlmProvider;

    #[async_trait::async_trait]
    impl LlmProvider for NoopLlmProvider {
        async fn chat(
            &self,
            _system: Option<&str>,
            _prompt: &str,
            _model: &str,
            _temperature: f32,
        ) -> anyhow::Result<LlmResponse> {
            anyhow::bail!("noop llm provider (test stub)")
        }
    }

    pub struct NoopThingAgentHost;

    #[async_trait::async_trait]
    impl ThingAgentHost for NoopThingAgentHost {
        fn subscribe_events(&self) -> broadcast::Receiver<ThingEventSignal> {
            broadcast::channel(1).1
        }
        async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            Ok(vec![])
        }
        async fn push_chat_message(&self, _session_key: &str, _content: &str, _run_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn notify_alert(&self, _workspace_id: &str, _payload: serde_json::Value) -> anyhow::Result<()> {
            Ok(())
        }
        async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
    }

    pub struct NoopAgentProvider;

    #[async_trait::async_trait]
    impl AutonomousAgentProvider for NoopAgentProvider {
        async fn get_or_create(
            &self,
            _workspace_id: &str,
            _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
        ) -> anyhow::Result<AgentHandle> {
            anyhow::bail!("noop agent provider (test stub)")
        }
        fn invalidate(&self, _workspace_id: &str) {}
    }
}

#[cfg(test)]
impl RuntimeDeps {
    /// 无 I/O 测试夹具：lazy sqlite 池（首次查询前不触库）+ noop 桩 +
    /// 小容量 AgentEventBus。调用方需提供 tokio 上下文（`#[tokio::test]`）：
    /// `Orchestrator::new` 内部构造 `AiEventPublisher` 时 spawn worker。
    pub fn test_stub() -> Self {
        // sqlx 0.9 建池即 spawn 维护任务（需 tokio 上下文）；关掉 lifetime /
        // idle 超时与 min_connections 后不再 spawn。
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_lifetime(None)
            .idle_timeout(None)
            .min_connections(0)
            .connect_lazy("sqlite::memory:")
            .expect("lazy in-memory sqlite");
        let event_bus = Arc::new(EventBus::new());
        Self {
            heartbeat_task_repo: Arc::new(HeartbeatTaskRepository::new(pool.clone())),
            event_publisher: Arc::new(AiEventPublisher::new(event_bus.clone())),
            heartbeat_config: HeartbeatConfig::default(),
            thing_agent_host: Arc::new(stubs::NoopThingAgentHost),
            policy_repo: Arc::new(PolicyRepository::new(pool.clone())),
            agent_provider: Arc::new(stubs::NoopAgentProvider),
            runs_repo: Arc::new(AgentRunsRepository::new(pool.clone())),
            thing_agent_config: ThingAgentManagerConfig::default(),
            event_bus,
            memory_service: Arc::new(MemoryService::new(
                Arc::new(stubs::NoopLlmProvider),
                Arc::new(tinyiothub_storage::memory::MemoryStore::new(pool)),
            )),
            drop_notifier: None,
            dlq: None,
            agent_event_capacity: 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::events::AgentEventKind;
    use super::super::snapshot::{RestoreSnapshot, WorkspaceHeartbeatState};
    use super::{AgentRuntime, RuntimeDeps};
    use chrono::Utc;
    use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};

    fn task_fixture() -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws1".into(),
            priority: "P1".into(),
            text: "巡检设备在线率".into(),
            paused: false,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn snapshot_with_ws1() -> RestoreSnapshot {
        RestoreSnapshot {
            heartbeat: vec![WorkspaceHeartbeatState {
                workspace_id: "ws1".into(),
                tasks: vec![task_fixture()],
                trust_config: TrustConfig::default(),
                interval_minutes: 30,
            }],
            recent_runs: vec![],
        }
    }

    #[tokio::test]
    async fn restore_dump_roundtrip_preserves_heartbeat_state() {
        let snap = snapshot_with_ws1();
        let rt = AgentRuntime::restore(snap, RuntimeDeps::test_stub());
        let dumped = rt.dump_state();
        assert_eq!(dumped.heartbeat.len(), 1);
        assert_eq!(dumped.heartbeat[0].workspace_id, "ws1");
        assert_eq!(dumped.heartbeat[0].interval_minutes, 30);
        assert_eq!(dumped.heartbeat[0].tasks.len(), 1);
    }

    #[tokio::test]
    async fn commands_update_facade_state() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());

        let mut config = TrustConfig::default();
        config.max_auto_actions_per_tick = 3;
        rt.update_trust_config("ws1", config);
        assert_eq!(rt.dump_state().heartbeat[0].trust_config.max_auto_actions_per_tick, 3);

        rt.update_heartbeat_interval("ws1", 45);
        assert_eq!(rt.dump_state().heartbeat[0].interval_minutes, 45);

        rt.reload_heartbeat_tasks("ws1", vec![]);
        assert!(rt.heartbeat_tasks("ws1").is_empty());
    }

    #[tokio::test]
    async fn heartbeat_tasks_returns_snapshot_section() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        assert_eq!(rt.heartbeat_tasks("ws1").len(), 1);
        assert!(rt.heartbeat_tasks("unknown").is_empty());
    }

    #[tokio::test]
    async fn bus_emits_to_subscriber() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        let mut rx = rt.subscribe();
        rt.bus().emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: "ws1".into() });
        let event = rx.try_recv().expect("subscriber receives emitted event");
        assert!(matches!(event.kind, AgentEventKind::HeartbeatTasksChanged { .. }));
    }

    #[tokio::test]
    async fn reload_tasks_command_updates_in_memory_tasks() {
        let rt = AgentRuntime::restore(RestoreSnapshot::default(), RuntimeDeps::test_stub());
        rt.reload_heartbeat_tasks("ws1", vec![task_fixture()]);
        assert_eq!(rt.heartbeat_tasks("ws1").len(), 1);
    }

    #[tokio::test]
    async fn update_trust_config_emits_event() {
        let rt = AgentRuntime::restore(RestoreSnapshot::default(), RuntimeDeps::test_stub());
        let mut rx = rt.subscribe();
        rt.update_trust_config("ws1", TrustConfig::default());
        let ev = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("event emitted synchronously")
            .unwrap();
        assert!(matches!(ev.kind, AgentEventKind::TrustConfigChanged { .. }));
    }

    #[tokio::test]
    async fn active_runs_and_dump_read_prewarmed_registry() {
        // Task 4：restore 用 snapshot.recent_runs 预热 RunRegistry；
        // active_runs（D13）与 dump_state 均读内存真源。
        let mut snap = snapshot_with_ws1();
        snap.recent_runs = vec![tinyiothub_core::agent_runs::RunReport {
            run_id: "run_1".into(),
            workspace_id: "ws1".into(),
            trigger: "timer:ws1".into(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "巡检正常".into(),
            actions: vec![],
            verified: true,
            duration_ms: 10,
            tool_calls: 0,
            tokens: 0,
        }];
        let rt = AgentRuntime::restore(snap, RuntimeDeps::test_stub());

        let active = rt.active_runs();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].run_id, "run_1");

        let dumped = rt.dump_state();
        assert_eq!(dumped.recent_runs.len(), 1);
        assert_eq!(dumped.recent_runs[0].run_id, "run_1");
    }
}
