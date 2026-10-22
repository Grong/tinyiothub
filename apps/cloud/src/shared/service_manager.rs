use std::sync::Arc;

use tinyiothub_runtime::DataServer;
use tokio::{
    sync::{RwLock, broadcast},
    task::JoinHandle,
};
use tracing::{error, info, warn};

use crate::shared::error::Error;

/// 服务状态枚举
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed(String),
}

/// 服务管理器 - 统一管理所有后台服务
pub struct ServiceManager {
    /// 服务状态
    status: Arc<RwLock<ServiceStatus>>,

    /// 关闭信号发送器
    shutdown_tx: broadcast::Sender<()>,

    /// 服务句柄
    service_handles: Arc<RwLock<Vec<JoinHandle<Result<(), Error>>>>>,

    /// Cron 调度器（可选，用于优雅关闭）
    cron_scheduler: Arc<RwLock<Option<tinyiothub_scheduler::CronSchedulerService>>>,

    /// AI orchestrator (set during start_all)
    orchestrator: Option<Arc<tinyiothub_agent::runtime::orchestrator::Orchestrator>>,

    /// Shared AI event publisher (set during start_all, drained on shutdown)
    event_publisher: Option<Arc<tinyiothub_agent::runtime::event::bus::AiEventPublisher>>,

    /// Agent 持久化订阅者关停令牌（Task 9）：cancel 后主循环退出、
    /// 落库重试任务中止；句柄在 service_handles 中随关停排空。
    persistence_shutdown: Option<tokio_util::sync::CancellationToken>,
}

impl ServiceManager {
    /// 创建新的服务管理器
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            status: Arc::new(RwLock::new(ServiceStatus::Stopped)),
            shutdown_tx,
            service_handles: Arc::new(RwLock::new(Vec::new())),
            cron_scheduler: Arc::new(RwLock::new(None)),
            orchestrator: None,
            event_publisher: None,
            persistence_shutdown: None,
        }
    }

    /// 启动所有服务
    pub async fn start_all(&mut self, app_state: &mut crate::state::AppState) -> Result<(), Error> {
        info!("🚀 Starting all background services...");

        // 更新状态为启动中
        *self.status.write().await = ServiceStatus::Starting;

        // 1. E4a 驱动访问服务（声明协议）：内置 manifest 播种 + Db 端口
        // 适配器注入（runtime 不依赖 db/sqlx）。必须先于 DataServer::new：
        // 后者在构造时按缓存物 create_driver，E4a 动态驱动只有 rehydrate
        // 之后才在注册表里，否则重启后这些物要再重启一次才开始轮询。
        let manifests = Arc::new(tinyiothub_runtime::access::registry::ManifestRegistry::new());
        for (_, json) in tinyiothub_runtime::driver::describe::builtin_manifest_jsons() {
            manifests
                .seed_builtin(json)
                .map_err(|e| Error::IOError(e.to_string()))?;
        }
        // Demand tracker 先于 AccessService 建立：读工具经 AccessService 写入、
        // sampler（下方）与 Hub 订阅共享同一实例。
        let demand_tracker = Arc::new(tinyiothub_runtime::access::demand::DemandTracker::new());
        let access = Arc::new(tinyiothub_runtime::access::service::AccessService::new(
            crate::shared::paths::api_data_dir(),
            manifests,
            Arc::new(crate::shared::access_ports::DbDriverStore(app_state.db.clone())),
            Arc::new(crate::shared::access_ports::DbDeviceStore(app_state.db.clone())),
            Arc::new(crate::shared::access_ports::DbProjectionStore(app_state.db.clone())),
            Arc::new(crate::shared::access_ports::DbThingSource(app_state.db.clone())),
            Arc::new(crate::shared::access_ports::DbAcceptanceStore(app_state.db.clone())),
            demand_tracker.clone(),
        ));
        // Installed packages live on disk + DB; restore them before serving
        // so a restart does not silently lose everything but the built-ins.
        if let Err(e) = access.rehydrate().await {
            tracing::warn!(error = %e, "E4a driver package rehydrate failed");
        }
        app_state.set_access(access.clone());
        info!("✅ AccessService started");

        // 2. 创建并启动数据服务器（device_cache 经 D15 端口适配器注入）
        let device_cache: Arc<dyn tinyiothub_runtime::ports::ThingCacheSource> = Arc::new(
            crate::shared::runtime_ports::ThingCacheAdapter(app_state.device_cache.clone()),
        );
        let data_server = Arc::new(DataServer::new(device_cache.clone(), app_state.event_bus.clone()));

        // 2.1 中央 sampler：catalog = device_cache ∪ 已安装 manifest；
        // 需求 = 本地订阅/近期读取（DemandTracker）+ 持久规则快照
        // （DbDemandSource：watch/派生/告警，每秒刷新一次）。
        let sampling_catalog = Arc::new(tinyiothub_runtime::access::catalog::CachedManifestCatalog::new(
            device_cache,
            access.manifests.clone(),
        ));
        let sampler = Arc::new(tinyiothub_runtime::access::sampler::Sampler::new(
            sampling_catalog,
            demand_tracker.clone(),
            data_server.sampling_workers(),
            Some(Arc::new(crate::shared::access_ports::DbDemandSource(
                app_state.db.clone(),
            ))),
        ));
        data_server.set_sampler(sampler);

        // 启动数据服务器
        let shutdown_rx = self.shutdown_tx.subscribe();
        data_server.run(shutdown_rx).await?;

        // 注册为事件处理器
        app_state.event_bus.register_handler(data_server.clone());

        // 注册 SSE 事件处理器 - 将事件实时推送到前端
        let sse_handler = Arc::new(crate::domains::event::subscribers::SseEventHandler::new(
            app_state.sse_manager.clone(),
        ));
        app_state.event_bus.register_handler(sse_handler);
        info!("✅ SseEventHandler registered");

        // 注册报警事件处理器 - 评估报警规则并创建报警
        let notification_dispatcher = Arc::new(crate::domains::alarm::notification::NotificationDispatcher::new(
            app_state.db.clone(),
        ));
        let alarm_handler = Arc::new(crate::domains::alarm::AlarmEventHandler::new(
            app_state.alarm_service.clone(),
            notification_dispatcher,
        ));
        app_state.event_bus.register_handler(alarm_handler);
        info!("✅ AlarmEventHandler registered");

        // 注册实时状态事件处理器 - 状态类事件 upsert 到 events 当前态视图
        // (occurrence_count 累加 + 去重; eng-review T2)
        let real_time_status_handler = Arc::new(crate::domains::event::subscribers::RealTimeStatusHandler::new(
            app_state.db.clone(),
        ));
        app_state.event_bus.register_handler(real_time_status_handler);
        info!("✅ RealTimeStatusHandler registered");

        // 注册持久化事件处理器 - 领域事件落 events 表（Warning+ 全量、
        // 非属性 Info 全量、属性变化仅在报警时；批量缓冲 100 条/5s 刷新）
        let persistence_handler = Arc::new(crate::domains::event::subscribers::PersistenceEventHandler::new(
            app_state.db.clone(),
            crate::domains::event::subscribers::PersistenceConfig::default(),
        ));
        app_state.event_bus.register_handler(persistence_handler);
        info!("✅ PersistenceEventHandler registered");

        // 保存到 AppState
        app_state.set_data_server(data_server.clone());

        info!("✅ DataServer started and registered");

        // 2.5 派生引擎管理器（Task 7 + X4 单引擎化，spec §三）：启动索引重建 +
        // 全 workspace 单 EventBus 订阅 + 首值补偿。读数经 data_server.value_sink()
        // → 共享管线回流（consume_driver_reading 的 device_cache 回退写 owner 物，
        // 不再注册任何 driver wrapper）。list_derived 失败 = 启动期 DB 读失败，
        // 中止启动（与 T21 fail-closed 同源）。三类触发源
        // （notify_config_changed / notify_tree_changed / notify_thing_deleted）
        // 由 Task 8 物管理面钩子接线。
        let derived_manager = Arc::new(tinyiothub_runtime::driver::derived_manager::DerivedDriverManager::new(
            Arc::new(crate::shared::runtime_ports::DbDerivedPropertyQueries(
                (*app_state.db).clone(),
            )),
            Arc::new(crate::shared::runtime_ports::ThingCacheAdapter(
                app_state.device_cache.clone(),
            )),
            data_server.value_sink(),
            app_state.event_bus.clone(),
        ));
        derived_manager.start().await.map_err(Error::IOError)?;
        app_state.derived_manager = Some(derived_manager);
        info!("✅ DerivedDriverManager started");

        // 2. 启动 Cron 调度器
        #[cfg(not(feature = "harmonyos"))]
        {
            // Wire db-bound executors into the scheduler registry（db 访问经
            // D15 端口适配器注入，runtime 不直接依赖 db）
            let mut registry = tinyiothub_scheduler::ExecutorRegistry::new();
            registry.register(Box::new(tinyiothub_runtime::ThingCommandExecutor::new(
                data_server.clone(),
                Arc::new(crate::shared::runtime_ports::ThingCommandQueriesAdapter(
                    (*app_state.db).clone(),
                )),
            )));
            registry.register(Box::new(tinyiothub_runtime::EventRetentionExecutor::new(Arc::new(
                crate::shared::runtime_ports::EventRetentionAdapter((*app_state.db).clone()),
            ))));
            // T6：审批超时升级（24h 未响应的判断自动转工单）
            registry.register(Box::new(tinyiothub_runtime::ApprovalTimeoutExecutor::new(Arc::new(
                crate::shared::runtime_ports::ApprovalTimeoutAdapter {
                    db: (*app_state.db).clone(),
                    sse: app_state.sse_manager.clone(),
                    alarm_service: app_state.alarm_service.clone(),
                    notify: app_state.notification_manager.clone(),
                },
            ))));
            let cron_scheduler = tinyiothub_scheduler::CronSchedulerService::new(app_state.db.clone(), registry);
            let cron_handle = cron_scheduler.start();
            self.service_handles.write().await.push(cron_handle);
            *self.cron_scheduler.write().await = Some(cron_scheduler);
            info!("✅ CronSchedulerService started");
        }

        // 3. 启动健康检查服务
        #[cfg(not(feature = "harmonyos"))]
        self.start_health_monitor(data_server.clone(), app_state.db.clone())
            .await?;

        // 4. Build and start AI subsystem (AgentRuntime 门面，Task 9 启动顺序)
        #[cfg(not(feature = "harmonyos"))]
        {
            let heartbeat_task_db = app_state.db.clone();

            // 统一循环心跳默认间隔（HeartbeatConfig::default 同源）；每工作区
            // 覆盖走统一循环 intervals 共享表。
            let heartbeat_default_interval = std::time::Duration::from_secs(
                u64::from(tinyiothub_agent::runtime::heartbeat::types::HeartbeatConfig::default().interval_minutes)
                    * 60,
            );
            let event_publisher = Arc::new(
                tinyiothub_agent::runtime::event::bus::AiEventPublisher::new(app_state.event_bus.clone())
                    .with_drop_notifier(Arc::new(tinyiothub_agent::runtime::event::bus::LoggingDropNotifier)),
            );

            // 遗留文件任务 → DB 一次性迁移必须先于快照装配，否则迁移落库
            // 的任务不在 restore 预热的内存真源中（start 会跳过）。
            let ws_ids = match app_state.workspace_service.list_all_ids().await {
                Ok(ids) => ids,
                Err(e) => {
                    warn!("⚠️ Failed to list workspaces for AI subsystem: {}", e);
                    Vec::new()
                }
            };
            for ws_id in &ws_ids {
                let workspace_dir = crate::shared::paths::workspace_dir(ws_id);
                if let Err(e) = crate::domains::agent::host::heartbeat::migrate_file_tasks_to_db(
                    heartbeat_task_db.as_ref(),
                    ws_id,
                    &workspace_dir,
                )
                .await
                {
                    warn!(%ws_id, "⚠️ Heartbeat task migration failed: {}", e);
                }
            }

            // ── Task 9 启动顺序（D11-①③，错序即丢事件）──
            // 1. 从 DB 构造 RestoreSnapshot（活跃 heartbeat 配置/任务 +
            //    每 ws 最近 50 条 run + O11 dedup 元数据）。CEO review T21：
            //    fail-closed——启动期 DB 读失败中止启动，不再降级为空快照
            //    （宽松默认 trust config 会被对账固化 / 零心跳永久静默）。
            let snapshot = crate::bootstrap::build_agent_snapshot(&app_state.db)
                .await
                .map_err(Error::IOError)?;
            // 2. bus 先建并经 RuntimeDeps 注入 restore（Task 3 评审指针
            //    选项 a）；持久化 receiver 在 restore 之前取得 —— restore
            //    期间及之后的事件不丢。
            // ── Task 14 工具注册点（composition root）──────────────────
            // chat agent 的内建数据工具实现（thing 9 工具 / canvas /
            // get_skill / dispatch_thing_task）留 cloud（D2），在此显式
            // 注册进 agent crate 的 ToolRegistry；数据句柄由 provider 闭包
            // 捕获。外部（MCP）工具工厂按需从 MCP_REGISTRY 派生（G3 —
            // 单一事实源，无第二注册静态）。
            let tool_registry = app_state.agent_pool.tool_registry();
            // E1 Task 10：LiveState 读面迟绑定槽——此处注入 chat/自治两侧工具
            // 上下文，manager 就绪（runtime.thing_agents() 之后）set；未 set 时
            // get_thing_profile 的 live=null。
            let live_summary_slot = crate::domains::agent::host::tools::thing::LiveSummarySlot::default();
            let live_summary: Arc<dyn crate::domains::agent::host::tools::thing::LiveSummarySource> =
                Arc::new(live_summary_slot.clone());
            // E2 watch/unwatch：manager 迟绑定槽（与 live_summary_slot 同点 set）。
            let manager_slot = crate::domains::agent::host::tools::thing::ManagerSlot::default();
            tool_registry.register_provider(crate::domains::agent::host::tools::chat_builtin_tools_provider(
                app_state.db.pool().clone(),
                Some(app_state.device_cache.clone()),
                app_state.pending_actions.clone(),
                app_state.arbitration.clone(),
                live_summary.clone(),
                // E2 ToolExec 接缝默认实现（直通内层工具）。
                std::sync::Arc::new(tinyiothub_agent::runtime::thing_agent::seams::tool_exec::DefaultToolExec),
                Some(app_state.event_bus.clone()),
                manager_slot.clone(),
                // E4a：调用边界校验（声明缺失/未声明操作/参数越界 → 拒绝）。
                app_state
                    .access
                    .clone()
                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>),
                // E4c：call/set 按声明 resources 解析并 claim。
                app_state
                    .access
                    .clone()
                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::resources::ResourceResolver>),
                // E4b：读取需求写入 sampler 共享的 DemandTracker。
                app_state
                    .access
                    .clone()
                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::DemandRecorder>),
            ));
            tool_registry.set_external_tool_factory(Arc::new(|| {
                crate::domains::agent::host::ports::external_tool_registry()
            }));

            // CEO review F11：容量对齐 spec D4 的 4096——256 在中等负载
            // （多 agent 同时 tick + DB 抖动）即可逼入 Lagged→全量重投影循环。
            let agent_events = Arc::new(tinyiothub_agent::runtime::events::AgentEventBus::new(4096));
            let persist_rx = agent_events.subscribe();
            // 工单订阅者（T5）：与 persist 同一 bus，restore 前取 receiver。
            let ticket_rx = agent_events.subscribe();
            let agent_events_ticket = agent_events.clone();
            // T5 judgment 订阅者：同一 bus，同样 restore 前取 receiver。
            let judgment_rx = agent_events.subscribe();
            let agent_events_judgment = agent_events.clone();
            // CEO review T1：监管循环重启订阅时需要 bus 句柄（deps 收走所有权前克隆）。
            let agent_events_supervisor = agent_events.clone();
            let pool = app_state.db.pool().clone();
            let policy_repo = app_state.db.clone();
            let runtime = Arc::new(tinyiothub_agent::runtime::runtime::AgentRuntime::restore(
                snapshot,
                tinyiothub_agent::runtime::runtime::RuntimeDeps {
                    thing_agent_host: Arc::new(
                        crate::domains::agent::host::thing_agent_host::CloudThingAgentHost::new(
                            pool.clone(),
                            app_state.thing_event_bus.clone(),
                        ),
                    ),
                    policy_repo: Arc::new(crate::domains::agent::host::ports::StorageAutonomyPolicyReader::new(
                        policy_repo.clone(),
                    )),
                    agent_provider: Arc::new(
                        crate::domains::agent::host::autonomous_factory::AutonomousAgentFactory::new(
                            pool.clone(),
                            policy_repo,
                            app_state.thing_event_bus.clone(),
                            Arc::new(crate::domains::event::router::ThrottleState::new(60)),
                            app_state.agent_pool.shared_memory(),
                            app_state.agent_pool.observer(),
                            tinyiothub_agent::pool::minimax_provider_factory(),
                            tinyiothub_agent::config::AgentRuntimeConfig::default().model,
                            crate::domains::agent::host::tools::ThingToolContext {
                                device_cache: Some(app_state.device_cache.clone()),
                                data_server: app_state.data_server.clone(),
                                pending_actions: Some(app_state.pending_actions.clone()),
                                arbitration: Some(app_state.arbitration.clone()),
                                live: Some(live_summary.clone()),
                                event_bus: Some(app_state.event_bus.clone()),
                                manager: Some(manager_slot.clone()),
                                access: app_state
                                    .access
                                    .clone()
                                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>),
                                resources: app_state
                                    .access
                                    .clone()
                                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::resources::ResourceResolver>),
                                demand: app_state
                                    .access
                                    .clone()
                                    .map(|a| a as Arc<dyn tinyiothub_runtime::access::DemandRecorder>),
                            },
                        ),
                    ),
                    thing_agent_config: tinyiothub_agent::runtime::thing_agent::ThingAgentManagerConfig {
                        heartbeat_default_interval,
                        ..Default::default()
                    },
                    event_bus: app_state.event_bus.clone(),
                    drop_notifier: Some(Arc::new(tinyiothub_agent::runtime::event::bus::LoggingDropNotifier)),
                    agent_events,
                    ticket_resolutions: Arc::new(crate::domains::agent::host::ports::DbTicketResolutionProvider::new(
                        app_state.db.clone(),
                    )),
                },
            ));
            // 3. 僵尸 reconcile：DB 里 status='running' 但 registry 无主的
            //    run → 'interrupted'。
            crate::bootstrap::reconcile_zombie_runs(&app_state.db, &runtime).await;
            // 4. 持久化订阅者（restore 前取得的 receiver；shutdown token
            //    编排主循环与心跳重试任务退出）。CEO review T1：改经监管
            //    循环启动——任务 panic/异常退出不再静默，error! + 新 receiver
            //    + 立即全量 resync + 退避重启；句柄注册进 service_handles
            //    随关停排空。
            let persist_shutdown = tokio_util::sync::CancellationToken::new();
            {
                let handle = tokio::spawn({
                    let runtime = runtime.clone();
                    let db = app_state.db.clone();
                    let token = persist_shutdown.clone();
                    async move {
                        crate::domains::agent::host::persist::persist::supervise_persistence_subscriber(
                            runtime,
                            db,
                            agent_events_supervisor,
                            persist_rx,
                            token,
                        )
                        .await;
                        Ok(())
                    }
                });
                self.service_handles.write().await.push(handle);
                self.persistence_shutdown = Some(persist_shutdown);
                info!("✅ Agent persistence subscriber started (supervised)");
            }

            // 工单订阅者（T5/T6）：RunRecorded → tickets 投影 + SSE 通知。
            // 与 persist 同构的监管循环；共享同一 shutdown 编排。
            {
                let ticket_shutdown = tokio_util::sync::CancellationToken::new();
                let db = app_state.db.clone();
                let sse = app_state.sse_manager.clone();
                let bus = agent_events_ticket.clone();
                let token = ticket_shutdown.clone();
                let handle = tokio::spawn(async move {
                    crate::domains::agent::host::notify::ticket_subscriber::supervise_ticket_subscriber(
                        ticket_rx,
                        move || bus.subscribe(),
                        db,
                        sse,
                        token,
                    )
                    .await;
                    Ok(())
                });
                self.service_handles.write().await.push(handle);
                info!("✅ Ticket subscriber started (supervised)");
            }

            // T5 judgment 订阅者：报警调查 run（problem_key="alarm:…"）→
            // judgments 投影 + 三出口路由（噪声抑制/待审批/转工单）。
            {
                let judgment_shutdown = tokio_util::sync::CancellationToken::new();
                let db = app_state.db.clone();
                let sse = app_state.sse_manager.clone();
                let alarm_service = app_state.alarm_service.clone();
                let notify = app_state.notification_manager.clone();
                let bus = agent_events_judgment.clone();
                let token = judgment_shutdown.clone();
                let handle = tokio::spawn(async move {
                    crate::domains::agent::host::judge::judgment_subscriber::supervise_judgment_subscriber(
                        judgment_rx,
                        move || bus.subscribe(),
                        db,
                        sse,
                        alarm_service,
                        notify,
                        token,
                    )
                    .await;
                    Ok(())
                });
                self.service_handles.write().await.push(handle);
                info!("✅ Judgment subscriber started (supervised)");
            }

            let thing_agent_manager = runtime.thing_agents().clone();
            // E4b §3.2：turn 开头工作区快照的声明属性读取也计入需求。
            thing_agent_manager.set_snapshot_demand_sink(Arc::new(
                crate::shared::access_ports::AccessSnapshotDemandSink(access.clone()),
            ));
            // E1 Task 10：manager 就绪，LiveState 读面槽封口——此后工具查询
            // 经 watch 命令通道生效（此前 live=null）。
            live_summary_slot.set(thing_agent_manager.clone());
            manager_slot.set(thing_agent_manager.clone());
            let orchestrator = runtime.orchestrator().clone();

            // Wire event publisher to services that need cross-domain dispatching
            app_state.alarm_service.set_event_publisher(Arc::new(
                crate::shared::ai_adapter::AlarmAiPublisherAdapter::new(event_publisher.clone()),
            ));
            app_state.workspace_service.set_event_publisher(Arc::new(
                crate::shared::ai_adapter::WorkspaceAiPublisherAdapter::new(event_publisher.clone()),
            ));
            app_state
                .workspace_service
                .set_heartbeat_task_db(heartbeat_task_db.clone());
            // Task 9：runtime 注入 agent hooks —— WorkspaceCreated 种子任务
            // 在发布事件前同步推入统一循环共享表（D11-⑤：DB 已先写），随后
            // 的 trigger tick 才能读到任务集。
            app_state.workspace_service.set_agent_hooks(Arc::new(
                crate::domains::agent::host::trigger::agent_hooks::AgentHooksImpl::new().with_runtime(runtime.clone()),
            ));

            // T15 退役接线：Metrics 所有权自旧 runner 迁出——MemoryService
            // 的反射统计继续落同一类型实例（由此处持有并注入）。
            let agent_metrics = Arc::new(tinyiothub_agent::memory::metrics::Metrics::new());
            let memory_service = Arc::new(
                tinyiothub_memory::service::MemoryService::new(
                    Arc::new(crate::shared::llm_provider::MinimaxLlmProvider::new(
                        app_state.minimax.clone(),
                    )),
                    app_state.memory_store.clone(),
                )
                .with_metrics(agent_metrics),
            );

            // Task 7 起 AgentPool 不再持有 MemoryService：chat 反射路径由
            // proxy handler 从 AgentState.memory_service 注入。
            app_state.agent_pool.set_event_publisher(event_publisher.clone()).await;

            // Task 6 起 orchestrator 不再持有 repo/MemoryService：统一循环的
            // run/心跳提案经 AgentEventBus(RunRecorded) 出口（持久化订阅者
            // 落库，T15 起唯一通道）；MemoryService 由 cloud 侧自持
            // （AgentState.memory_service，memory profile compile/weekly
            // digest handler 使用）。
            app_state.memory_service = Some(memory_service);

            orchestrator.start();

            // T14 用户指令入口：chat 工具 / HTTP 端点经 directive_sink 投递到
            // 对应工作区的 thing-agent 调度器。
            app_state.set_directive_sink(thing_agent_manager.clone());
            // AI 运行透明化 T5：/agent-runs/{id}/trace 实时分支数据源——与
            // manager 内 PipelineDeps 同一实例（克隆共享 DashMap）。
            app_state.set_active_runs(thing_agent_manager.active_run_registry());

            // 启动补种（2026-09-16）：老工作区（早于 WorkspaceCreated 播种
            // 功能）heartbeat_tasks 为 0，心跳永不 tick（统一循环 trigger
            // 空任务抑制）——AI 巡检完全空转。0 任务时补种默认任务：先写
            // DB，再 reload 统一循环共享表，trigger 随即能读到任务集。
            for ws_id in &ws_ids {
                match app_state.db.list_heartbeat_tasks(ws_id).await {
                    Ok(tasks) if tasks.is_empty() => {
                        let defaults: Vec<tinyiothub_core::heartbeat::NewHeartbeatTask> =
                            crate::domains::agent::host::heartbeat::get_default_tasks()
                                .into_iter()
                                .map(|t| tinyiothub_core::heartbeat::NewHeartbeatTask {
                                    priority: t.priority,
                                    text: t.text,
                                    paused: t.paused,
                                })
                                .collect();
                        match app_state.db.replace_heartbeat_tasks(ws_id, &defaults).await {
                            Ok(()) => match app_state.db.list_heartbeat_tasks(ws_id).await {
                                Ok(seeded) => {
                                    runtime.reload_heartbeat_tasks(ws_id, seeded);
                                    info!(%ws_id, "seeded default heartbeat tasks at boot");
                                }
                                Err(e) => {
                                    tracing::warn!(%ws_id, error = %e, "read back seeded heartbeat tasks failed")
                                }
                            },
                            Err(e) => {
                                tracing::warn!(%ws_id, error = %e, "seed default heartbeat tasks failed")
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%ws_id, error = %e, "list heartbeat tasks failed"),
                }
            }

            // Start unified agent loops for existing workspaces（heartbeat
            // 共享表已由 restore 预热：tasks 非空，trigger 不抑制）。
            //
            // E1 Task 8 bootstrap 硬顺序：set_wake_ledger / set_watch_deps /
            // 限速预热全部先于 start——三者都只影响其后 start 的工作区。
            let hub_ledger = Arc::new(crate::shared::runtime_ports::HubLedgerAdapter::new(
                (*app_state.db).clone(),
            ));
            thing_agent_manager.set_wake_ledger(hub_ledger.clone());
            thing_agent_manager.set_watch_deps(tinyiothub_agent::runtime::thing_agent::WatchDeps {
                cache: Arc::new(crate::shared::runtime_ports::ThingCacheAdapter(
                    app_state.device_cache.clone(),
                )),
                ledger: hub_ledger,
                rules: Arc::new(crate::shared::runtime_ports::WatchRuleStoreAdapter(
                    (*app_state.db).clone(),
                )),
                events: app_state.event_bus.clone(),
            });
            // E1 同键限速预热：DB heartbeat_config JSON 的
            // rate_limit_per_key_per_min → scheduler 共享表（spec §3 热改
            // 生效；读失败只记日志，缺省 3/min 兜底）。
            for ws_id in &ws_ids {
                match app_state.db.load_heartbeat_config(ws_id).await {
                    Ok(Some(cfg)) => {
                        if let Some(per_min) = cfg.rate_limit_per_key_per_min {
                            thing_agent_manager.set_wake_rate_limit_per_min(ws_id, per_min);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!(%ws_id, error = %e, "load heartbeat config for rate-limit prewarm failed"),
                }
            }
            for ws_id in &ws_ids {
                thing_agent_manager.start(ws_id);
            }
            info!("✅ AI Orchestrator started ({} workspaces)", ws_ids.len());

            // Boot race 恢复：AI 接线完成前创建的报警（publisher 未接线时
            // AlarmCreated 被丢弃）留下的 investigating judgment 重派调查。
            for ws_id in &ws_ids {
                app_state.alarm_service.redispatch_pending_investigations(ws_id).await;
            }

            // Store in ServiceManager for shutdown
            self.orchestrator = Some(orchestrator);
            self.event_publisher = Some(event_publisher);

            // Also store in AppState for potential access by other subsystems
            app_state.orchestrator = self.orchestrator.clone();
            // T13 bootstrap 装配：统一循环管理器 + last_ticks 共享表注入
            // （T15 起 handler 的唯一读源）。
            app_state.thing_agents = Some(thing_agent_manager.clone());
            app_state.heartbeat_last_ticks = Some(thing_agent_manager.heartbeat_last_ticks());
        }

        // 5. Start SSE token cleanup to prevent expired tokens from accumulating in memory.
        self.start_sse_token_cleanup(app_state.sse_token_manager.clone())
            .await?;

        // 更新状态为运行中
        *self.status.write().await = ServiceStatus::Running;

        info!("✅ All background services started successfully");
        Ok(())
    }

    /// 启动健康检查服务
    async fn start_health_monitor(
        &self,
        data_server: Arc<DataServer>,
        db: Arc<tinyiothub_storage::Db>,
    ) -> Result<(), Error> {
        info!("Starting Health Monitor...");

        let _status = self.status.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::perform_health_check(&data_server, &db).await {
                            warn!("Health check failed: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Health Monitor received shutdown signal");
                        break;
                    }
                }
            }

            Ok(())
        });

        self.service_handles.write().await.push(handle);
        info!("✅ Health Monitor started");

        Ok(())
    }

    async fn perform_health_check(data_server: &DataServer, db: &Arc<tinyiothub_storage::Db>) -> Result<(), Error> {
        match db.ping().await {
            Ok(_) => {
                tracing::debug!("Database health check passed");
            }
            Err(e) => {
                return Err(Error::IOError(format!("Database health check failed: {}", e)));
            }
        }

        tracing::debug!("Cache stats: {} things cached", data_server.get_things().len());

        Ok(())
    }

    /// Start a background task that periodically removes expired SSE tokens.
    /// Tokens expire after 5 minutes but are only consumed on successful use;
    /// without cleanup the DashMap would grow unbounded.
    async fn start_sse_token_cleanup(
        &self,
        manager: Arc<tinyiothub_authn::sse_token::SseTokenManager>,
    ) -> Result<(), Error> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let before = manager.token_count();
                        manager.cleanup_expired();
                        let after = manager.token_count();
                        if before != after {
                            tracing::debug!("Cleaned up {} expired SSE tokens", before - after);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("SSE token cleanup received shutdown signal");
                        break;
                    }
                }
            }

            Ok(())
        });

        self.service_handles.write().await.push(handle);
        info!("✅ SSE token cleanup started");

        Ok(())
    }

    /// 优雅关闭所有服务
    pub async fn shutdown(&self) -> Result<(), Error> {
        info!("🛑 Shutting down all background services...");

        // 更新状态为关闭中
        *self.status.write().await = ServiceStatus::Stopping;

        // 关闭 Cron 调度器
        if let Some(cron_scheduler) = self.cron_scheduler.write().await.take() {
            cron_scheduler.shutdown();
            info!("CronSchedulerService shutdown signal sent");
        }

        // 关闭 AI subsystem — Orchestrator 先关闭停止接收事件。
        if let Some(ref orchestrator) = self.orchestrator {
            orchestrator.shutdown().await;
            info!("Orchestrator shut down");
        }
        // Drain queued events after all producers have stopped.
        if let Some(ref event_publisher) = self.event_publisher {
            event_publisher.shutdown().await;
            info!("AiEventPublisher drained");
        }
        // 持久化订阅者退出：生产者已全部关停，cancel 后主循环退出、
        // 在飞落库重试任务中止（句柄在下方 service_handles 排空中等待）。
        if let Some(ref token) = self.persistence_shutdown {
            token.cancel();
            info!("Agent persistence subscriber shutdown signal sent");
        }

        // 发送关闭信号
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // 等待所有服务句柄完成（带超时，防止无限循环的服务阻塞退出）
        let handles = std::mem::take(&mut *self.service_handles.write().await);

        for handle in handles {
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), handle).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    error!("Service shutdown error: {}", e);
                }
                Err(_) => {
                    warn!("Service shutdown timed out after 10s");
                }
            }
        }

        // 更新状态为已停止
        *self.status.write().await = ServiceStatus::Stopped;

        info!("✅ All background services shut down gracefully");
        Ok(())
    }

    /// 获取服务状态
    pub async fn get_status(&self) -> ServiceStatus {
        self.status.read().await.clone()
    }

    /// 重启特定服务
    pub async fn restart_service(
        &mut self,
        _service_name: &str,
        _app_state: &mut crate::state::AppState,
    ) -> Result<(), Error> {
        Err(Error::IOError("Service restart not implemented".to_string()))
    }
}

/// 优雅关闭处理器
pub async fn setup_graceful_shutdown() {
    // 等待关闭信号
    #[cfg(feature = "harmonyos")]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for ctrl_c signal: {}", e);
        } else {
            info!("Received Ctrl+C, initiating graceful shutdown...");
        }
    }

    #[cfg(all(unix, not(feature = "harmonyos")))]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
        }
    }

    #[cfg(windows)]
    {
        use tokio::signal::windows;

        let mut ctrl_c = windows::ctrl_c().expect("Failed to create Ctrl+C handler");
        let mut ctrl_break = windows::ctrl_break().expect("Failed to create Ctrl+Break handler");

        tokio::select! {
            _ = ctrl_c.recv() => {
                info!("Received Ctrl+C, initiating graceful shutdown...");
            }
            _ = ctrl_break.recv() => {
                info!("Received Ctrl+Break, initiating graceful shutdown...");
            }
        }
    }

    #[cfg(not(any(unix, windows, feature = "harmonyos")))]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for ctrl_c signal: {}", e);
        } else {
            info!("Received Ctrl+C, initiating graceful shutdown...");
        }
    }

    info!("Graceful shutdown signal received");
}
