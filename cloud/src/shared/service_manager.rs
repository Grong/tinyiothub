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
    orchestrator: Option<Arc<tinyiothub_ai::orchestrator::Orchestrator>>,

    /// AI heartbeat runner (set during start_all)
    heartbeat_runner: Option<Arc<tinyiothub_ai::heartbeat::runner::HeartbeatRunner>>,

    /// Shared AI event publisher (set during start_all, drained on shutdown)
    event_publisher: Option<Arc<tinyiothub_ai::event::bus::AiEventPublisher>>,
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
            heartbeat_runner: None,
            event_publisher: None,
        }
    }

    /// 启动所有服务
    pub async fn start_all(
        &mut self,
        app_state: &mut crate::shared::app_state::AppState,
    ) -> Result<(), Error> {
        info!("🚀 Starting all background services...");

        // 更新状态为启动中
        *self.status.write().await = ServiceStatus::Starting;

        // 1. 创建并启动数据服务器
        let data_server =
            Arc::new(DataServer::new(app_state.device_cache.clone(), app_state.event_bus.clone()));

        // 启动数据服务器
        let shutdown_rx = self.shutdown_tx.subscribe();
        data_server.run(shutdown_rx).await?;

        // 注册为事件处理器
        app_state.event_bus.register_handler(data_server.clone());

        // 注册 SSE 事件处理器 - 将事件实时推送到前端
        let sse_handler = Arc::new(crate::shared::event::handlers::SseEventHandler::new(
            app_state.sse_manager.clone(),
        ));
        app_state.event_bus.register_handler(sse_handler);
        info!("✅ SseEventHandler registered");

        // 注册报警事件处理器 - 评估报警规则并创建报警
        let notification_dispatcher =
            Arc::new(crate::modules::alarm::notification::NotificationDispatcher::new(
                app_state.database.clone(),
            ));
        let alarm_handler = Arc::new(crate::modules::alarm::AlarmEventHandler::new(
            app_state.alarm_service.clone(),
            notification_dispatcher,
        ));
        app_state.event_bus.register_handler(alarm_handler);
        info!("✅ AlarmEventHandler registered");

        // 注册实时状态事件处理器 - 状态类事件 upsert 到 events 当前态视图
        // (occurrence_count 累加 + 去重; eng-review T2)
        let real_time_status_handler =
            Arc::new(crate::shared::event::handlers::RealTimeStatusHandler::new(
                app_state.real_time_event_repository.clone(),
            ));
        app_state.event_bus.register_handler(real_time_status_handler);
        info!("✅ RealTimeStatusHandler registered");

        // 保存到 AppState
        app_state.set_data_server(data_server.clone());

        info!("✅ DataServer started and registered");

        // 2. 启动 Cron 调度器
        #[cfg(not(feature = "harmonyos"))]
        {
            // Wire db-bound executors into the scheduler registry
            let mut registry = tinyiothub_scheduler::ExecutorRegistry::new();
            registry.register(Box::new(tinyiothub_runtime::DeviceCommandExecutor::new(
                data_server.clone(),
                (*app_state.database).clone(),
            )));
            registry.register(Box::new(tinyiothub_runtime::EventRetentionExecutor::new(
                (*app_state.database).clone(),
            )));
            let cron_scheduler = tinyiothub_scheduler::CronSchedulerService::new(
                app_state.cron_job_repo.clone(),
                app_state.cron_run_repo.clone(),
                registry,
            );
            let cron_handle = cron_scheduler.start();
            self.service_handles.write().await.push(cron_handle);
            *self.cron_scheduler.write().await = Some(cron_scheduler);
            info!("✅ CronSchedulerService started");
        }

        // 3. 启动健康检查服务
        #[cfg(not(feature = "harmonyos"))]
        self.start_health_monitor(data_server.clone(), app_state.database.clone()).await?;

        // 4. Build and start AI subsystem (tinyiothub-ai Orchestrator)
        #[cfg(not(feature = "harmonyos"))]
        {
            let heartbeat_task_repo = Arc::new(
                crate::modules::agent::heartbeat_repo::SqliteHeartbeatTaskRepository::new(
                    app_state.database.pool().clone(),
                ),
            );

            let heartbeat_config = tinyiothub_ai::heartbeat::types::HeartbeatConfig {
                enabled: true,
                interval_minutes: 15,
            };
            let event_publisher = Arc::new(
                tinyiothub_ai::event::bus::AiEventPublisher::new(app_state.event_bus.clone())
                    .with_drop_notifier(Arc::new(tinyiothub_ai::event::bus::LoggingDropNotifier)),
            );
            let heartbeat_runner =
                Arc::new(tinyiothub_ai::heartbeat::runner::HeartbeatRunner::new(
                    heartbeat_task_repo.clone(),
                    event_publisher.clone(),
                    heartbeat_config,
                ));

            // Wire event publisher to services that need cross-domain dispatching
            app_state.alarm_service.set_event_publisher(event_publisher.clone());
            app_state.workspace_service.set_event_publisher(event_publisher.clone());
            app_state.workspace_service.set_heartbeat_task_repo(heartbeat_task_repo.clone());

            // Wire agent pool via adapter
            let ai_adapter = Arc::new(crate::shared::ai_adapter::CloudAgentPoolAdapter::new(
                app_state.agent_pool.clone(),
            ));
            heartbeat_runner.set_agent_pool(ai_adapter).await;

            let memory_service = Arc::new(
                tinyiothub_ai::memory::service::MemoryService::new(
                    Arc::new(crate::shared::llm_provider::MinimaxLlmProvider::new()),
                    app_state.memory_store.clone(),
                )
                // Share the runner's Metrics so reflection stats land in the same snapshot.
                .with_metrics(heartbeat_runner.metrics.clone()),
            );

            // Wire MemoryService into AgentPool for reflection from chat path
            app_state.agent_pool.set_memory_service(memory_service.clone()).await;
            app_state.agent_pool.set_event_publisher(event_publisher.clone()).await;

            // T15 thing-agent loop: per-workspace 自治 Loop 注册表。
            // Orchestrator 的 WorkspaceCreated/Deleted 事件流驱动其启停；
            // 三触发器（物事件/定时巡检/用户指令）汇入 per-workspace 调度器。
            let thing_agent_manager = {
                let pool = app_state.database.pool().clone();
                let policy_repo = Arc::new(
                    crate::modules::agent::policy_repo::SqlitePolicyRepository::new(pool.clone()),
                );
                let runs_repo = Arc::new(
                    crate::modules::agent::agent_runs_repo::SqliteAgentRunsRepository::new(
                        pool.clone(),
                    ),
                );
                let manager = Arc::new(tinyiothub_ai::thing_agent::ThingAgentManager::new(
                    Arc::new(crate::modules::agent::thing_agent_host::CloudThingAgentHost::new(
                        pool.clone(),
                        app_state.thing_event_bus.clone(),
                    )),
                    policy_repo.clone(),
                    Arc::new(
                        crate::modules::agent::autonomous_factory::AutonomousAgentFactory::new(
                            pool.clone(),
                            policy_repo,
                            app_state.thing_event_bus.clone(),
                            Arc::new(crate::modules::event::router::ThrottleState::new(60)),
                            app_state.agent_pool.shared_memory.clone(),
                            app_state.agent_pool.observer.clone(),
                            crate::modules::agent::autonomous_factory::minimax_provider_factory(),
                            crate::shared::agent::config::AgentRuntimeConfig::default().model,
                        ),
                    ),
                    runs_repo.clone(),
                    Arc::new(tinyiothub_ai::thing_agent::Runner::new()),
                    tinyiothub_ai::thing_agent::ThingAgentManagerConfig::default(),
                ));
                // T18 X6 心跳桥：HeartbeatCompleted 的结构化 proposals 经 O11
                // dedup 后投递 UserDirective 进 thing-agent loop。
                let bridge =
                    Arc::new(tinyiothub_ai::orchestrator::callbacks::HeartbeatBridge::new(
                        runs_repo,
                        manager.clone(),
                    ));
                (manager, bridge)
            };
            let (thing_agent_manager, heartbeat_bridge) = thing_agent_manager;

            let orchestrator = Arc::new(tinyiothub_ai::orchestrator::Orchestrator::new(
                app_state.event_bus.clone(),
                heartbeat_runner.clone(),
                heartbeat_task_repo,
                memory_service,
                Some(Arc::new(tinyiothub_ai::event::bus::LoggingDropNotifier)),
                Some(Arc::new(crate::modules::agent::dlq_repo::SqliteDeadLetterQueue::new(
                    app_state.database.pool().clone(),
                ))),
                Some(thing_agent_manager.clone()),
                Some(heartbeat_bridge),
            ));
            orchestrator.start();

            // T14 用户指令入口：chat 工具 / HTTP 端点经 directive_sink 投递到
            // 对应工作区的 thing-agent 调度器。
            app_state.set_directive_sink(thing_agent_manager.clone());

            // Start heartbeat loops for existing workspaces
            match app_state.workspace_service.list_all_ids().await {
                Ok(ws_ids) => {
                    for ws_id in &ws_ids {
                        let workspace_dir = crate::shared::paths::workspace_dir(ws_id);
                        if let Err(e) = crate::modules::agent::heartbeat::migrate_file_tasks_to_db(
                            heartbeat_runner.task_repo().as_ref(),
                            ws_id,
                            &workspace_dir,
                        )
                        .await
                        {
                            warn!(%ws_id, "⚠️ Heartbeat task migration failed: {}", e);
                        }
                        heartbeat_runner.start(ws_id).await;
                        thing_agent_manager.start(ws_id);
                    }
                    info!("✅ AI Orchestrator started ({} workspaces)", ws_ids.len());
                }
                Err(e) => {
                    warn!("⚠️ Failed to list workspaces for AI subsystem: {}", e);
                }
            }

            // Store in ServiceManager for shutdown
            self.orchestrator = Some(orchestrator);
            self.heartbeat_runner = Some(heartbeat_runner);
            self.event_publisher = Some(event_publisher);

            // Also store in AppState for potential access by other subsystems
            app_state.orchestrator = self.orchestrator.clone();
            app_state.heartbeat_runner = self.heartbeat_runner.clone();
        }

        // 5. Start SSE token cleanup to prevent expired tokens from accumulating in memory.
        self.start_sse_token_cleanup(app_state.sse_token_manager.clone()).await?;

        // 更新状态为运行中
        *self.status.write().await = ServiceStatus::Running;

        info!("✅ All background services started successfully");
        Ok(())
    }

    /// 启动健康检查服务
    async fn start_health_monitor(
        &self,
        data_server: Arc<DataServer>,
        database: Arc<tinyiothub_storage::Database>,
    ) -> Result<(), Error> {
        info!("Starting Health Monitor...");

        let _status = self.status.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::perform_health_check(&data_server, &database).await {
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

    async fn perform_health_check(
        data_server: &DataServer,
        database: &Arc<tinyiothub_storage::Database>,
    ) -> Result<(), Error> {
        match sqlx::query("SELECT 1").fetch_optional(database.pool()).await {
            Ok(_) => {
                tracing::debug!("Database health check passed");
            }
            Err(e) => {
                return Err(Error::IOError(format!("Database health check failed: {}", e)));
            }
        }

        tracing::debug!("Cache stats: {} devices cached", data_server.get_devices().len());

        Ok(())
    }

    /// Start a background task that periodically removes expired SSE tokens.
    /// Tokens expire after 5 minutes but are only consumed on successful use;
    /// without cleanup the DashMap would grow unbounded.
    async fn start_sse_token_cleanup(
        &self,
        manager: Arc<crate::shared::sse_token::SseTokenManager>,
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

        // 关闭 AI subsystem — Orchestrator 先关闭停止接收事件，
        // 再关闭 HeartbeatRunner 停止循环，避免中间窗口事件丢失。
        if let Some(ref orchestrator) = self.orchestrator {
            orchestrator.shutdown().await;
            info!("Orchestrator shut down");
        }
        if let Some(ref heartbeat_runner) = self.heartbeat_runner {
            heartbeat_runner.shutdown().await;
            info!("HeartbeatRunner shut down");
        }
        // Drain queued events after all producers have stopped.
        if let Some(ref event_publisher) = self.event_publisher {
            event_publisher.shutdown().await;
            info!("AiEventPublisher drained");
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
        _app_state: &mut crate::shared::app_state::AppState,
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

        let mut sigterm =
            signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
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
