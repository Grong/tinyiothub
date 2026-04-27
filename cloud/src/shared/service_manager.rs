use std::sync::Arc;

use tokio::{
    sync::{broadcast, RwLock},
    task::JoinHandle,
};
use tracing::{error, info, warn};

use tinyiothub_runtime::DataServer;
use crate::{
    shared::error::Error,
};

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
    cron_scheduler: Arc<RwLock<Option<crate::shared::cron_scheduler::CronSchedulerService>>>,
}

impl ServiceManager {
    /// 创建新的服务管理器
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            status: Arc::new(RwLock::new(ServiceStatus::Stopped)),
            shutdown_tx,
            service_handles: Arc::new(RwLock::new(Vec::new())),
            cron_scheduler: Arc::new(RwLock::new(None)),
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
        let sse_handler = Arc::new(
            crate::shared::event::handlers::SseEventHandler::new(app_state.sse_manager.clone()),
        );
        app_state.event_bus.register_handler(sse_handler);
        info!("✅ SseEventHandler registered");

        // 保存到 AppState
        app_state.set_data_server(data_server.clone());

        info!("✅ DataServer started and registered");

        // 2. 启动 Cron 调度器
        #[cfg(not(feature = "harmonyos"))]
        {
            let cron_scheduler = crate::shared::cron_scheduler::CronSchedulerService::new(
                app_state.cron_job_repo.clone(),
                app_state.cron_run_repo.clone(),
                Some(data_server.clone()),
                Some((*app_state.database).clone()),
            );
            let cron_handle = cron_scheduler.start();
            self.service_handles.write().await.push(cron_handle);
            *self.cron_scheduler.write().await = Some(cron_scheduler);
            info!("✅ CronSchedulerService started");
        }

        // 3. 启动健康检查服务
        #[cfg(not(feature = "harmonyos"))]
        self.start_health_monitor(data_server.clone(), app_state.database.clone()).await?;

        // 4. 启动 Heartbeat 服务
        #[cfg(not(feature = "harmonyos"))]
        {
            let agent_settings = crate::shared::config::get().agent.clone();
            let workspace_dir = crate::shared::paths::default_workspace_dir();

            // Initialize workspace with template files
            let scaffold_result = crate::shared::agent::scaffold_service::scaffold_workspace(&workspace_dir).await;
            match scaffold_result {
                Ok(result) => {
                    if result.created_files > 0 || result.created_dirs > 0 {
                        info!("✅ Workspace scaffolded: {}", result);
                    }
                }
                Err(e) => {
                    warn!("⚠️ Workspace scaffolding failed: {}", e);
                }
            }

            let heartbeat_config = zeroclaw::config::schema::HeartbeatConfig {
                enabled: agent_settings.heartbeat_enabled,
                interval_minutes: agent_settings.heartbeat_interval_minutes,
                two_phase: false,
                message: None,
                target: None,
                to: None,
                adaptive: false,
                min_interval_minutes: 5,
                max_interval_minutes: 120,
                deadman_timeout_minutes: 0,
                deadman_channel: None,
                deadman_to: None,
                max_run_history: 100,
                load_session_context: false,
                task_timeout_secs: 600,
            };
            let mut observer_config = zeroclaw::config::schema::ObservabilityConfig::default();
            observer_config.backend = agent_settings.observer_backend.clone();
            let heartbeat_observer: std::sync::Arc<dyn zeroclaw::observability::Observer> =
                std::sync::Arc::from(zeroclaw::observability::create_observer(&observer_config));
            let heartbeat_service = crate::shared::agent::HeartbeatService::new(
                workspace_dir.clone(),
                heartbeat_config,
                heartbeat_observer,
                app_state.chat_service.clone(),
                crate::shared::paths::DEFAULT_WORKSPACE_ID.to_string(),
                "default".to_string(),
                agent_settings.system_prompts.heartbeat.clone(),
            );
            let mut heartbeat_shutdown_rx = self.shutdown_tx.subscribe();
            let handle: tokio::task::JoinHandle<Result<(), Error>> = tokio::spawn(async move {
                heartbeat_service.run(heartbeat_shutdown_rx).await;
                Ok(())
            });
            self.service_handles.write().await.push(handle);
            info!("✅ HeartbeatService started");
        }

        // 更新状态为运行中
        *self.status.write().await = ServiceStatus::Running;

        info!("✅ All background services started successfully");
        Ok(())
    }

    /// 启动健康检查服务
    async fn start_health_monitor(
        &self,
        data_server: Arc<DataServer>,
        database: Arc<crate::shared::persistence::Database>,
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
        database: &Arc<crate::shared::persistence::Database>,
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
        // TODO: 实现服务重启逻辑
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
        use tokio::signal::unix::{signal, SignalKind};

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
