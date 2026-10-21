//! AgentRuntime — agent loop 子系统的门面（Task 3）。
//!
//! 聚合 `ThingAgentManager` / `Orchestrator`（不重写其逻辑），持有
//! `AgentEventBus`（Task 2）与恢复快照状态。启动顺序（Task 9）：bus 由
//! 调用方先建并经 [`RuntimeDeps::agent_events`] 注入 restore（Task 3
//! 评审指针：restore 内部建 bus 则"先 subscribe 再 restore"不可能实现——
//! 采用选项 a，bus 外建注入），调用方在 restore 前 `bus.subscribe()` 取得
//! receiver，保证 restore 期间及之后的事件不丢。
//!
//! 状态真源：heartbeat 段（tasks/trust/interval/enabled）由统一 Run 循环的
//! `HeartbeatDeps` 共享表持有（T15 起唯一真源，旧 runner 已退役）——
//! restore 预热注入，命令方法同步更新共享表并发射事件（DB 写由 cloud 侧
//! service 在调命令前完成，D11-⑤ 写序）；recent_runs 段自 Task 4 起由
//! `RunRegistry` 承接；O11 dedup 元数据段（problem_meta）自 Task 9 起由
//! 快照预热恢复（行级 ack 保真）。

use std::sync::Arc;

use tokio::sync::broadcast;

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};
use tinyiothub_runtime::EventBus;

use super::event::bus::DropNotifier;
use super::events::{AgentEvent, AgentEventBus, AgentEventKind};
use super::heartbeat::types::HeartbeatConfig;
use super::orchestrator::Orchestrator;
use super::orchestrator::callbacks::HeartbeatBridge;
use super::snapshot::{RestoreSnapshot, WorkspaceHeartbeatState};
use super::thing_agent::manager::{AutonomousAgentProvider, HeartbeatDeps, ThingAgentManager, ThingAgentManagerConfig};
use super::thing_agent::registry::RunRegistry;
use super::thing_agent::runner::Runner;
use super::thing_agent::traits::{AutonomyPolicyReader, ThingAgentHost};

/// RuntimeDeps — 聚合组件现有构造所需依赖（收集自 service_manager.rs
/// 现有接线；本任务只聚已有件，不造新依赖）。
///
/// Task 6 起不聚合任何 storage/memory 句柄：心跳结果落库经统一循环的
/// RunRecorded 事件（持久化订阅者心跳支路，T15 起唯一通道），O11 dedup
/// 走 RunRegistry 内存，MemoryService 由 cloud 侧自持
/// （AgentState.memory_service）。
pub struct RuntimeDeps {
    // ThingAgentManager 构造件
    pub thing_agent_host: Arc<dyn ThingAgentHost>,
    pub policy_repo: Arc<dyn AutonomyPolicyReader>,
    pub agent_provider: Arc<dyn AutonomousAgentProvider>,
    pub thing_agent_config: ThingAgentManagerConfig,
    // Orchestrator 构造件
    pub event_bus: Arc<EventBus>,
    pub drop_notifier: Option<Arc<dyn DropNotifier>>,
    /// AgentEventBus（Task 9：bus 外建注入，调用方在 restore 前订阅——
    /// "先 subscribe 再 restore"的可实现形式）。lagged 订阅者经
    /// dump_state 对账恢复。
    pub agent_events: Arc<AgentEventBus>,
    /// 工单人工解法注入源（T7：cloud 经 db 门面实现；测试用 Noop）。
    pub ticket_resolutions: Arc<dyn super::thing_agent::traits::TicketResolutionProvider>,
}

/// Agent 子系统门面。命令入站（D3）；调用约定（D11-⑤）：cloud 先写 DB
/// 成功，再调命令；命令失败告警。
pub struct AgentRuntime {
    thing_agents: Arc<ThingAgentManager>,
    /// heartbeat 共享表（与 ThingAgentManager 同一实例）：内存真源——
    /// restore 预热、命令写穿、dump_state 导出都走这组表。
    heartbeat_deps: HeartbeatDeps,
    orchestrator: Arc<Orchestrator>,
    events: Arc<AgentEventBus>,
    /// run 记录内存真相源（Task 4 RunRegistry）：restore 时由快照预热，
    /// 与 ThingAgentManager 共享同一实例；dump_state/active_runs 由此导出。
    run_registry: RunRegistry,
}

impl AgentRuntime {
    /// 用快照构造运行时：聚合组件（接线同 service_manager.rs），并将
    /// heartbeat 段预热进统一循环共享表（内存真源）。
    /// 只做构造与预热，不启动任何 loop —— 启动顺序由 Task 11 编排。
    pub fn restore(snapshot: RestoreSnapshot, deps: RuntimeDeps) -> Self {
        let events = deps.agent_events;
        let run_registry = RunRegistry::new();
        // recent_runs 段输入契约：旧→新（见 RestoreSnapshot::recent_runs）。
        run_registry.prewarm(snapshot.recent_runs);
        // O11 dedup 元数据段（Task 9）：行级 ack 随预热恢复。
        run_registry.prewarm_problem_meta(snapshot.problem_meta);
        // 预热：快照 heartbeat 段注入统一循环共享表（命令路径之外的唯一
        // 注入口）；last_ticks/governor 空态起步（连败治理不持久化）。
        let heartbeat_deps = super::thing_agent::HeartbeatDeps::default();
        for state in &snapshot.heartbeat {
            heartbeat_deps
                .tasks
                .insert(state.workspace_id.clone(), state.tasks.clone());
            heartbeat_deps
                .trust_configs
                .insert(state.workspace_id.clone(), state.trust_config.clone());
            heartbeat_deps
                .intervals
                .insert(state.workspace_id.clone(), state.interval_minutes);
            // 终审 I-1：enabled 开关随快照预热进统一循环共享表——禁用工作区
            // 重启后 trigger 仍不 tick。
            heartbeat_deps.enabled.insert(state.workspace_id.clone(), state.enabled);
        }
        // Task 11：桥与统一循环共享同一 governor——告警信号 resume 的暂停态
        // 必须作用于 trigger 读的那一份（克隆共享同一 DashMap 真源）。
        let heartbeat_governor = heartbeat_deps.governor.clone();
        let thing_agents = Arc::new(ThingAgentManager::new(
            deps.thing_agent_host,
            deps.policy_repo,
            deps.agent_provider,
            run_registry.clone(),
            events.clone(),
            Arc::new(Runner::new()),
            deps.thing_agent_config,
            deps.ticket_resolutions,
            heartbeat_deps.clone(),
        ));
        // 告警桥（原 T18 X6 心跳桥）：AlarmCreated 转 Heartbeat 变体入统一
        // 循环。X6 自动派发已退役（2026-10-16 裁定：提案走审批队列）；
        // registry 仍共享同一实例（ack 回写链 + 回推窗口读）。
        let bridge = Arc::new(HeartbeatBridge::new(
            run_registry.clone(),
            thing_agents.clone(),
            heartbeat_governor,
        ));
        let orchestrator = Arc::new(Orchestrator::new(
            deps.event_bus,
            deps.drop_notifier,
            Some(thing_agents.clone()),
            Some(bridge),
        ));

        Self {
            thing_agents,
            heartbeat_deps,
            orchestrator,
            events,
            run_registry,
        }
    }

    /// 订阅 AgentEvent 流。Task 9 启动顺序：bus 经 RuntimeDeps 注入，
    /// 调用方在 restore 前经 `deps.agent_events.subscribe()` 取得
    /// receiver；restore 后的 `runtime.subscribe()` 与其等价（同一 bus）。
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }

    /// 导出当前状态快照（Lagged resync + 周期对账出口）。
    /// heartbeat 段读统一循环共享表真源；recent_runs 段读 RunRegistry
    /// 内存真源（Task 4）。
    pub fn dump_state(&self) -> RestoreSnapshot {
        let heartbeat = self.heartbeat_snapshot_states();
        // 排序保证导出确定性（DashMap 迭代序不稳定），便于对账 diff。
        let mut heartbeat = heartbeat;
        heartbeat.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        let mut recent_runs = self.run_registry.active();
        // 同理排序（RunReport 无时间戳，run_id 字典序保证确定性）。
        recent_runs.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        // problem_meta 段导出为空：周期对账不投影 dedup 元数据（DB 即其
        // 真相源，重启时经快照构建器重建）。
        // recent_run_meta 段（CEO review T3）：窗口内 run 的发射期 dedup 键
        // 旁路，resync 补插缺失行时恢复 problem_key/dedup_key。
        RestoreSnapshot {
            heartbeat,
            recent_runs,
            problem_meta: Vec::new(),
            recent_run_meta: self.run_registry.all_run_keys(),
        }
    }

    /// heartbeat 段导出：tasks/trust/intervals/enabled 四表 key 的并集，
    /// 缺省字段回退 default（interval 缺省=HeartbeatConfig::default，
    /// enabled 缺省=true——与 trigger 判定集同口径）。
    fn heartbeat_snapshot_states(&self) -> Vec<WorkspaceHeartbeatState> {
        let mut ws_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in self.heartbeat_deps.tasks.iter() {
            ws_ids.insert(r.key().clone());
        }
        for r in self.heartbeat_deps.trust_configs.iter() {
            ws_ids.insert(r.key().clone());
        }
        for r in self.heartbeat_deps.intervals.iter() {
            ws_ids.insert(r.key().clone());
        }
        for r in self.heartbeat_deps.enabled.iter() {
            ws_ids.insert(r.key().clone());
        }
        ws_ids
            .into_iter()
            .map(|ws| WorkspaceHeartbeatState {
                tasks: self.heartbeat_tasks(&ws),
                trust_config: self
                    .heartbeat_deps
                    .trust_configs
                    .get(&ws)
                    .map(|r| r.value().clone())
                    .unwrap_or_default(),
                interval_minutes: self
                    .heartbeat_deps
                    .intervals
                    .get(&ws)
                    .map(|r| *r.value())
                    .unwrap_or(HeartbeatConfig::default().interval_minutes),
                enabled: self.heartbeat_deps.enabled.get(&ws).map(|r| *r.value()).unwrap_or(true),
                workspace_id: ws,
            })
            .collect()
    }

    /// trust config 变更命令：更新共享表 + 发射事件（Task 5）。
    /// DB 写由 cloud 侧 service 在调本命令前完成（D11-⑤ 写序）。
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        self.heartbeat_deps
            .trust_configs
            .insert(workspace_id.to_string(), config.clone());
        self.events.emit(AgentEventKind::TrustConfigChanged {
            workspace_id: workspace_id.to_string(),
            config: Box::new(config),
        });
    }

    /// 心跳间隔变更命令：更新共享表并唤醒 trigger 立即重读（统一循环
    /// trigger 每轮读 intervals 表，notify 闭合生效延迟）。入站校验与
    /// DB 写（含 enabled 归并）由 cloud 侧 handler 先做。
    pub fn update_heartbeat_interval(&self, workspace_id: &str, interval_minutes: u32) {
        self.heartbeat_deps
            .intervals
            .insert(workspace_id.to_string(), interval_minutes);
        self.thing_agents.heartbeat_reload_notify(workspace_id).notify_one();
    }

    /// 心跳任务全量替换命令：更新共享表 + 唤醒 trigger 立即重读 + 发射
    /// 事件。调用方已写 DB（D11-⑤）。
    pub fn reload_heartbeat_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        self.heartbeat_deps.tasks.insert(workspace_id.to_string(), tasks);
        self.thing_agents.heartbeat_reload_notify(workspace_id).notify_one();
        self.events.emit(AgentEventKind::HeartbeatTasksChanged {
            workspace_id: workspace_id.to_string(),
        });
    }

    /// 实时读 API（D13）：读 RunRegistry 窗口（Task 4 起为内存真源）。
    pub fn active_runs(&self) -> Vec<RunReport> {
        self.run_registry.active()
    }

    /// 工作区心跳任务：读统一循环共享表真源。
    pub fn heartbeat_tasks(&self, workspace_id: &str) -> Vec<HeartbeatTask> {
        self.heartbeat_deps
            .tasks
            .get(workspace_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// 工作区删除清理命令：清心跳共享表（tasks/trust/intervals/enabled/
    /// last_ticks）并解除 governor 暂停态，防止已删工作区在内存与
    /// dump_state 快照中残留（Task 5 fix round 1 语义）。loop 停止由
    /// orchestrator 的 WorkspaceDeleted 臂承担。
    pub fn remove_workspace(&self, workspace_id: &str) {
        self.heartbeat_deps.tasks.remove(workspace_id);
        self.heartbeat_deps.trust_configs.remove(workspace_id);
        self.heartbeat_deps.intervals.remove(workspace_id);
        self.heartbeat_deps.enabled.remove(workspace_id);
        self.heartbeat_deps.last_ticks.remove(workspace_id);
        self.heartbeat_deps.governor.resume(workspace_id);
    }

    /// 事件总线句柄 —— 测试经 bus().emit(...) 注入事件（Task 8）。
    pub fn bus(&self) -> &AgentEventBus {
        &self.events
    }

    pub fn thing_agents(&self) -> &Arc<ThingAgentManager> {
        &self.thing_agents
    }

    pub fn orchestrator(&self) -> &Arc<Orchestrator> {
        &self.orchestrator
    }

    /// RunRegistry 内存真源句柄 —— 测试/对账探针（Task 9 验证 dedup
    /// 元数据预热；clone 与门面共享同一实例）。
    pub fn run_registry(&self) -> &RunRegistry {
        &self.run_registry
    }
}

#[cfg(any(test, feature = "testing"))]
mod stubs {
    //! 无 I/O 测试夹具桩：所有 async 方法不被触达（restore 只构造不启动）。

    use super::*;

    use super::super::thing_agent::runner::{AgentHandle, RunContextInner};
    use super::super::thing_agent::traits::ThingEventSignal;

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

#[cfg(any(test, feature = "testing"))]
impl RuntimeDeps {
    /// 无 I/O 测试夹具：空内存策略桩 + noop 桩 + 小容量 AgentEventBus。
    /// 调用方需提供 tokio 上下文（`#[tokio::test]`）：`Orchestrator::new`
    /// 内部构造 `AiEventPublisher` 时 spawn worker。
    ///
    /// `testing` feature 下对 crate 外可见 —— apps/cloud 集成测试复用本夹具。
    pub fn test_stub() -> Self {
        let event_bus = Arc::new(EventBus::new());
        Self {
            thing_agent_host: Arc::new(stubs::NoopThingAgentHost),
            policy_repo: Arc::new(super::thing_agent::traits::test_stubs::StubAutonomyPolicyReader::new()),
            agent_provider: Arc::new(stubs::NoopAgentProvider),
            thing_agent_config: ThingAgentManagerConfig::default(),
            event_bus,
            drop_notifier: None,
            agent_events: Arc::new(AgentEventBus::new(16)),
            ticket_resolutions: Arc::new(super::thing_agent::traits::NoopTicketResolutions),
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
                enabled: true,
            }],
            recent_runs: vec![],
            problem_meta: vec![],
            recent_run_meta: Default::default(),
        }
    }

    /// 显式空快照夹具（不用 Default，字段逐个列出）。
    fn empty_snapshot() -> RestoreSnapshot {
        RestoreSnapshot {
            heartbeat: vec![],
            recent_runs: vec![],
            problem_meta: vec![],
            recent_run_meta: Default::default(),
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

        let config = TrustConfig {
            max_auto_actions_per_tick: 3,
            ..Default::default()
        };
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

    /// 终审 I-1：restore 预热把快照 enabled 写入统一循环共享表——禁用工作区
    /// 重启后 trigger 仍不 tick（此前 enabled 只在旧 runner 生效，重启后统一
    /// 循环无视持久化的开关）。缺省语义（快照 true）预热后读出口为 true。
    #[tokio::test]
    async fn restore_prewarms_heartbeat_enabled_into_unified_loop() {
        let mut snap = snapshot_with_ws1();
        snap.heartbeat[0].enabled = false;
        let rt = AgentRuntime::restore(snap, RuntimeDeps::test_stub());
        assert!(
            !rt.thing_agents().heartbeat_enabled("ws1"),
            "快照 enabled=false 必须预热进统一循环共享表"
        );

        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        assert!(rt.thing_agents().heartbeat_enabled("ws1"));
    }

    #[tokio::test]
    async fn bus_emits_to_subscriber() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        let mut rx = rt.subscribe();
        rt.bus().emit(AgentEventKind::HeartbeatTasksChanged {
            workspace_id: "ws1".into(),
        });
        let event = rx.try_recv().expect("subscriber receives emitted event");
        assert!(matches!(event.kind, AgentEventKind::HeartbeatTasksChanged { .. }));
    }

    #[tokio::test]
    async fn reload_tasks_command_updates_in_memory_tasks() {
        let rt = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
        rt.reload_heartbeat_tasks("ws1", vec![task_fixture()]);
        assert_eq!(rt.heartbeat_tasks("ws1").len(), 1);
    }

    #[tokio::test]
    async fn update_trust_config_emits_event() {
        let rt = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
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
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
        }];
        let rt = AgentRuntime::restore(snap, RuntimeDeps::test_stub());

        let active = rt.active_runs();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].run_id, "run_1");

        let dumped = rt.dump_state();
        assert_eq!(dumped.recent_runs.len(), 1);
        assert_eq!(dumped.recent_runs[0].run_id, "run_1");
    }

    /// testing specialist T5：dump_state → recent_run_meta 接线端到端——
    /// record + set_run_keys 后导出必须携带键；回归掉 runtime.rs:146 的
    /// 导出会在全部测试绿的情况下静默复活 T3 的 NULL 元数据 bug。
    #[tokio::test]
    async fn dump_state_exports_recent_run_meta_from_registry() {
        let rt = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
        rt.run_registry().record(tinyiothub_core::agent_runs::RunReport {
            run_id: "run_keys".into(),
            workspace_id: "ws1".into(),
            trigger: "timer:ws1".into(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "巡检".into(),
            actions: vec![],
            verified: true,
            duration_ms: 10,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
        });
        rt.run_registry().set_run_keys(
            "run_keys",
            crate::runtime::snapshot::RunDedupKeys {
                problem_key: Some("漏水".into()),
                dedup_key: Some("t:1".into()),
            },
        );

        let dumped = rt.dump_state();
        let keys = dumped
            .recent_run_meta
            .get("run_keys")
            .expect("dump_state must export run keys");
        assert_eq!(keys.problem_key.as_deref(), Some("漏水"));
        assert_eq!(keys.dedup_key.as_deref(), Some("t:1"));
    }

    // Value card
    // - 覆盖路径：remove_workspace 清五张心跳共享表（tasks/trust/intervals/
    //   enabled/last_ticks）+ governor.resume + dump_state 不残留。
    // - 真实回归场景：T15 把清表从旧 runner 迁到门面时漏清某张表——工作区
    //   删除后重建，残留的 enabled=false / 旧任务表 / 旧 last_tick 直接生效，
    //   dump_state 对账还把幽灵工作区写回 DB。
    // - 失败信号：任一表未清即断言失败；enabled 读出口回缺省 true 证明表已清。
    // - 覆盖理由：orchestrator 生命周期测试只断言 manager.start/stop，门面
    //   清表命令无任何直接断言。
    #[tokio::test]
    async fn remove_workspace_clears_heartbeat_shared_tables() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        // 前置：预热态非空 + last_ticks 有值。
        assert_eq!(rt.heartbeat_tasks("ws1").len(), 1);
        rt.thing_agents()
            .heartbeat_last_ticks()
            .insert("ws1".to_string(), chrono::Utc::now());

        rt.remove_workspace("ws1");

        assert!(rt.heartbeat_tasks("ws1").is_empty(), "tasks 表须清");
        assert!(
            rt.thing_agents().heartbeat_trust_config("ws1").is_none(),
            "trust 表须清"
        );
        assert_eq!(
            rt.thing_agents().heartbeat_interval_minutes("ws1"),
            None,
            "intervals 表须清"
        );
        assert!(
            rt.thing_agents().heartbeat_enabled("ws1"),
            "enabled 表清后读出口回缺省 true"
        );
        assert!(
            !rt.thing_agents().heartbeat_last_ticks().contains_key("ws1"),
            "last_ticks 表须清"
        );
        assert!(rt.dump_state().heartbeat.is_empty(), "dump_state 不得残留已删工作区");
    }

    // Value card
    // - 覆盖路径：update_heartbeat_interval / reload_heartbeat_tasks 命令在
    //   更新共享表后调用 heartbeat_reload_notify().notify_one()（行为保持
    //   清单第 3 行：配置/任务变更立即 tick，不等下个周期间隔）。
    // - 真实回归场景：有人删掉 notify_one 调用——间隔热改与任务变更退化为
    //   「下一个周期才生效」，最长延迟一个 interval（默认 15min），trigger
    //   侧 reload_notify_forces_immediate_tick 测试依旧绿（它直接 notify）。
    // - 失败信号：notified() 超时即 notify 断链。
    // - 覆盖理由：既有 commands_update_facade_state 只断言表值，不覆盖
    //   notify 副作用。
    #[tokio::test]
    async fn interval_and_task_commands_notify_unified_trigger() {
        let rt = AgentRuntime::restore(snapshot_with_ws1(), RuntimeDeps::test_stub());
        let notify = rt.thing_agents().heartbeat_reload_notify("ws1");

        let waited = notify.notified();
        rt.update_heartbeat_interval("ws1", 45);
        tokio::time::timeout(std::time::Duration::from_millis(100), waited)
            .await
            .expect("interval 命令必须 notify 统一 trigger");

        let waited = notify.notified();
        rt.reload_heartbeat_tasks("ws1", vec![]);
        tokio::time::timeout(std::time::Duration::from_millis(100), waited)
            .await
            .expect("任务替换命令必须 notify 统一 trigger");
    }
}
