use std::sync::Arc;

use tokio::{
    sync::{broadcast, RwLock},
    task::JoinHandle,
};
use tracing::{error, info, warn};

use crate::{
    application::{scheduler::TimeTask, DataContext, DataServer},
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
}

impl ServiceManager {
    /// 创建新的服务管理器
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            status: Arc::new(RwLock::new(ServiceStatus::Stopped)),
            shutdown_tx,
            service_handles: Arc::new(RwLock::new(Vec::new())),
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
            Arc::new(DataServer::new(app_state.data_context.clone(), app_state.event_bus.clone()));

        // 启动数据服务器
        let shutdown_rx = self.shutdown_tx.subscribe();
        data_server.run(shutdown_rx).await?;

        // 注册为事件处理器
        app_state.event_bus.register_handler(data_server.clone()).await;

        // 注册 SSE 事件处理器 - 将事件实时推送到前端
        let sse_handler = Arc::new(
            crate::infrastructure::event::handlers::SseEventHandler::new(app_state.sse_manager.clone()),
        );
        app_state.event_bus.register_handler(sse_handler).await;
        info!("✅ SseEventHandler registered");

        // 保存到 AppState
        app_state.set_data_server(data_server.clone());

        info!("✅ DataServer started and registered");

        // 2. 启动定时任务调度器
        #[cfg(not(feature = "harmonyos"))]
        {
            let time_task = TimeTask::new()
                .with_services(
                    app_state.job_service.clone(),
                    app_state.job_execution_service.clone(),
                );
            // 在后台启动调度器
            tokio::spawn(async move {
                time_task.run().await;
            });
            info!("✅ TimeTask Scheduler started");
        }

        // 3. 启动健康检查服务
        #[cfg(not(feature = "harmonyos"))]
        self.start_health_monitor(app_state.data_context.clone()).await?;

        // 4. 启动 Heartbeat 服务
        #[cfg(not(feature = "harmonyos"))]
        {
            let agent_settings = crate::infrastructure::config::get().agent.clone();
            let workspace_dir = std::path::PathBuf::from(&agent_settings.workspace_dir);

            // Initialize workspace with template files
            let scaffold_result = crate::infrastructure::agent::scaffold_service::scaffold_workspace(&workspace_dir).await;
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
            let heartbeat_service = crate::infrastructure::agent::HeartbeatService::new(
                workspace_dir,
                heartbeat_config,
                heartbeat_observer,
                app_state.chat_service.clone(),
                "default".to_string(),
                "default".to_string(),
                agent_settings.system_prompts.heartbeat.clone(),
            );
            tokio::spawn(async move {
                heartbeat_service.run().await;
            });
            info!("✅ HeartbeatService started");
        }

        // 更新状态为运行中
        *self.status.write().await = ServiceStatus::Running;

        info!("✅ All background services started successfully");
        Ok(())
    }

    /// 启动健康检查服务
    async fn start_health_monitor(&self, context: Arc<DataContext>) -> Result<(), Error> {
        info!("� SStarting Health Monitor...");

        let _status = self.status.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 执行健康检查
                        if let Err(e) = Self::perform_health_check(&context).await {
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

    /// 执行健康检查
    async fn perform_health_check(context: &Arc<DataContext>) -> Result<(), Error> {
        // 检查数据库连接
        let db = context.database();
        match sqlx::query("SELECT 1").fetch_optional(db.pool()).await {
            Ok(_) => {
                tracing::debug!("Database health check passed");
            }
            Err(e) => {
                return Err(Error::IOError(format!("Database health check failed: {}", e)));
            }
        }

        // 检查缓存状态
        let cache_stats = context.get_cache_stats();
        tracing::debug!("Cache stats: {} devices cached", cache_stats.device_count);

        Ok(())
    }

    /// 优雅关闭所有服务
    pub async fn shutdown(&self) -> Result<(), Error> {
        info!("🛑 Shutting down all background services...");

        // 更新状态为关闭中
        *self.status.write().await = ServiceStatus::Stopping;

        // 发送关闭信号
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // 等待所有服务句柄完成
        let handles = std::mem::take(&mut *self.service_handles.write().await);

        for handle in handles {
            if let Err(e) = handle.await {
                error!("Service shutdown error: {}", e);
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
