use std::sync::Arc;

use tinyiothub_core::models::device_property::DeviceProperty;
use tinyiothub_storage::cache::DeviceCache;
use tokio::sync::OnceCell;

use crate::{
    modules::{
        device::{
            monitoring_service::DeviceMonitoringService,
            performance_service::DevicePerformanceService, query_service::DeviceQueryService,
            service::DeviceService, trace_service::DeviceTraceService,
        },
        event::repositories::{EventRepository, RealTimeEventRepository},
        notification::NotificationManager,
        template::{TemplateEngine, TemplateRepository, TemplateValidator},
    },
    shared::{
        agent::AgentRuntime,
        error::Error,
        event::{
            EventBus, SseConnectionManager,
            channels::NotificationChannelFactory,
            security::{EventSecurityFactory, SecureEventService},
        },
        persistence::{
            Database,
            factory::DeviceRepositoryFactory,
            repositories::{
                DeviceTraceRepository, NotificationHistoryRepositoryImpl,
                NotificationRuleRepositoryImpl, SqliteDeviceMemoryRepository,
                SqliteEventRepository, SqliteRealTimeEventRepository,
            },
        },
        redis::RedisClient,
    },
};

/// 应用程序状态 - 使用 Axum 推荐的依赖注入模式
///
/// 这种设计遵循以下最佳实践：
/// 1. 单一状态类型 - Axum with_state 只支持一个状态
/// 2. 服务预创建 - 避免每次请求重复创建服务
/// 3. Arc 共享 - 多线程安全的引用计数
/// 4. 清晰的依赖关系 - 所有依赖在启动时解析
#[derive(Clone)]
pub struct AppState {
    /// 设备内存缓存
    pub device_cache: Arc<DeviceCache>,

    /// 数据库连接池
    pub database: Arc<Database>,

    /// 设备仓库工厂 - 用于创建租户感知的设备仓库
    pub device_repository_factory: Arc<DeviceRepositoryFactory>,

    /// === 应用服务层 ===
    /// 数据服务器 - 设备数据采集和命令执行
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,

    /// === 领域服务层 ===
    /// 设备基础服务 - CRUD 操作
    pub device_service: Arc<DeviceService>,

    /// 设备监控服务 - 状态监控和指标
    pub monitoring_service: Arc<DeviceMonitoringService>,

    /// 设备性能服务 - 性能分析和告警
    pub performance_service: Arc<DevicePerformanceService>,

    /// 设备追踪服务 - 操作日志和审计
    pub trace_service: Arc<DeviceTraceService>,

    /// 设备查询服务 - 报表和只读查询
    pub device_query_service: Arc<dyn DeviceQueryService>,

    /// 模板引擎 - 设备模板管理
    pub template_engine: Arc<TemplateEngine>,

    /// 通知管理器 - 事件通知和告警
    pub notification_manager: Option<Arc<NotificationManager>>,

    /// Redis 客户端 - 用于会话管理和频率限制
    pub redis: Option<RedisClient>,

    /// SSE连接管理器 - 实时事件推送
    pub sse_manager: Arc<SseConnectionManager>,

    /// 安全事件服务 - 带权限控制和加密的事件服务（懒加载）
    pub secure_event_service: OnceCell<Arc<SecureEventService>>,

    /// === 事件系统 ===
    /// 事件总线 - 事件发布和订阅
    pub event_bus: Arc<EventBus>,

    /// === 事件系统仓库 ===
    /// 事件历史仓库 - 事件持久化存储
    pub event_repository: Arc<dyn EventRepository>,

    /// 实时事件状态仓库 - 当前活跃事件管理
    pub real_time_event_repository: Arc<dyn RealTimeEventRepository>,

    /// 报警服务 - 报警规则和报警管理
    pub alarm_service: Arc<crate::modules::alarm::AlarmService>,

    /// Agent Runtime - consolidated agent interface
    pub agent_runtime: Arc<dyn AgentRuntime>,

    /// 用户服务 - CRUD 操作
    pub user_service: Arc<crate::modules::user::UserService>,

    /// 租户服务 - CRUD 操作
    pub tenant_service: Arc<crate::modules::tenant::TenantService>,

    /// 工作空间服务 - CRUD 操作
    pub workspace_service: Arc<crate::modules::workspace::WorkspaceService>,

    /// 标签服务 - CRUD 操作
    pub tag_service: Arc<crate::modules::tag::TagService>,

    /// 标签仓库 - 用于设备服务的标签关联
    pub tag_repository: Arc<dyn crate::modules::tag::TagRepository>,

    /// 角色服务 - CRUD 操作
    pub role_service: Arc<crate::modules::role::RoleService>,

    /// 权限服务 - CRUD 操作
    pub permission_service: Arc<crate::modules::permission::PermissionService>,

    /// Cron 任务仓库
    pub cron_job_repo: Arc<dyn crate::modules::cron::CronJobRepository>,

    /// Cron 执行记录仓库
    pub cron_run_repo: Arc<dyn crate::modules::cron::CronRunRepository>,

    /// 会话服务 - Agent 聊天会话管理
    pub session_service: Arc<crate::modules::agent::SessionService>,

    /// Agent 记忆服务 - 构建设备快照等上下文
    pub agent_memory_service: Arc<crate::modules::agent::AgentMemoryService>,

    /// 聊天服务 - Agent 聊天编排
    pub chat_service: Arc<crate::modules::agent::ChatService>,

    /// 缓存的系统信息对象，避免每次请求重新扫描
    pub sysinfo_system: Arc<std::sync::Mutex<sysinfo::System>>,

    /// 网关服务 - MQTT 网关配对
    pub gateway_service: Arc<crate::modules::gateway::service::GatewayService>,

    /// MQTT 客户端（可选，未配置时为空）
    pub mqtt_client: Option<Arc<crate::shared::mqtt_client::PlatformMqttClient>>,
}

impl AppState {
    /// 创建应用程序状态
    ///
    /// 采用依赖注入容器模式，在应用启动时一次性创建所有服务
    /// 这样做的好处：
    /// 1. 性能优化 - 避免每次请求创建服务
    /// 2. 依赖管理 - 清晰的服务依赖关系
    /// 3. 测试友好 - 便于单元测试和集成测试
    /// 4. 类型安全 - 编译时检查所有依赖
    pub fn new(device_cache: Arc<DeviceCache>, db_pool: sqlx::SqlitePool) -> Self {
        // 创建共享的数据库连接
        let database = Arc::new(Database::new(db_pool));

        // 创建设备仓库工厂
        let device_repository_factory = Arc::new(DeviceRepositoryFactory::new(database.clone()));

        // === 创建领域服务 ===
        // 按照依赖关系顺序创建，避免循环依赖

        // === 创建事件系统仓库 ===
        let event_repository: Arc<dyn EventRepository> =
            Arc::new(SqliteEventRepository::new(database.as_ref().clone()));
        let real_time_event_repository: Arc<dyn RealTimeEventRepository> =
            Arc::new(SqliteRealTimeEventRepository::new(database.as_ref().clone()));

        // 通知管理器 - 可选服务，依赖数据库
        let notification_manager = Self::create_notification_manager(database.clone()).ok();

        // 创建事件总线
        let event_bus = Arc::new(EventBus::new());

        // 创建报警服务
        let alarm_repository =
            Arc::new(crate::modules::alarm::SqliteAlarmRepository::new(database.clone()));
        let alarm_rule_repository =
            Arc::new(crate::modules::alarm::SqliteAlarmRuleRepository::new(database.clone()));
        let alarm_service = Arc::new(crate::modules::alarm::AlarmService::new(
            alarm_repository.clone(),
            alarm_rule_repository,
        ));

        // 旧的 AlarmRepositoryImpl（device 监控/性能服务需要特定方法）
        let legacy_alarm_repository = Arc::new(
            crate::shared::persistence::repositories::AlarmRepositoryImpl::new(database.clone()),
        );

        // 创建SSE管理器（带 DeviceCache 用于设备 workspace 查找）
        let sse_manager = Arc::new(SseConnectionManager::new());

        // 注册事件处理器将在异步初始化中完成
        // 这里只创建事件总线，处理器注册推迟到 register_event_handlers() 方法

        // 标签仓库（提前创建，供 DeviceService 使用）
        let tag_repository: Arc<dyn crate::modules::tag::TagRepository> =
            Arc::new(crate::modules::tag::SqliteTagRepository::new(database.as_ref().clone()));
        let tag_binding_repository: Arc<dyn crate::modules::tag::TagBindingRepository> = Arc::new(
            crate::modules::tag::SqliteTagBindingRepository::new(database.as_ref().clone()),
        );

        // 基础服务 - 使用事件总线
        let device_repository: Arc<dyn crate::modules::device::repository::DeviceRepository> =
            Arc::new(crate::shared::persistence::repositories::SqliteDeviceRepository::new(
                database.as_ref().clone(),
            ));
        let device_service = Arc::new(
            DeviceService::with_event_bus(device_repository, database.clone(), event_bus.clone())
                .with_tag_repository(tag_repository.clone()),
        );
        let device_query_service: Arc<dyn DeviceQueryService> =
            Arc::new(crate::shared::persistence::repositories::SqliteDeviceQueryService::new(
                database.as_ref().clone(),
            ));

        // 监控服务 - 依赖数据库、缓存和告警仓库
        let monitoring_service = Arc::new(DeviceMonitoringService::new(
            database.clone(),
            device_cache.clone(),
            legacy_alarm_repository.clone(),
        ));

        // 性能服务 - 依赖数据库、缓存和告警仓库
        let performance_service = Arc::new(DevicePerformanceService::new(
            database.clone(),
            device_cache.clone(),
            legacy_alarm_repository.clone(),
        ));

        // 追踪服务 - 依赖追踪仓库
        let trace_repository = Arc::new(DeviceTraceRepository::new((*database).clone()));
        let trace_service = Arc::new(DeviceTraceService::new(trace_repository));

        // 模板引擎 - 内置模板通过 migration seed 写入 DB
        let template_repository = Arc::new(TemplateRepository::new(database.clone()));
        let template_validator = Arc::new(TemplateValidator::new());
        let template_engine =
            Arc::new(TemplateEngine::new(template_repository, template_validator));

        // 创建安全事件服务 - 可选服务，依赖配置
        // Note: Secure event service requires async initialization, so we'll create it lazily
        let secure_event_service = OnceCell::new();

        // Redis 客户端 - 可选服务，依赖配置
        let redis = crate::shared::config::get()
            .redis
            .as_ref()
            .and_then(|config| RedisClient::new(&config.url).ok());

        // Agent Runtime - 使用 zeroclaw 内置的 OpenAiCompatibleProvider (MiniMax)
        let minimax_config = crate::shared::config::get()
            .minimax
            .clone()
            .expect("minimax config is required - set [minimax] in app_settings.toml");
        let agent_settings = crate::shared::config::get().agent.clone();
        let provider =
            zeroclaw::providers::create_provider("minimaxi", Some(&minimax_config.auth_token))
                .expect("failed to create MiniMax provider");
        tracing::info!(
            "TinyIoTHub Agent initialized with zeroclaw MiniMax provider (memory_backend={}, observer_backend={})",
            agent_settings.memory_backend,
            agent_settings.observer_backend
        );
        let agent_runtime: Arc<dyn AgentRuntime> = Arc::new(
            crate::shared::agent::AgentRuntimeImpl::new(
                database.pool().clone(),
                provider,
                minimax_config.model,
                &agent_settings,
            )
            .expect("failed to build AgentRuntimeImpl"),
        );

        // Agent Memory Service
        let memory_repo = Arc::new(SqliteDeviceMemoryRepository::new(database.pool().clone()));
        let agent_memory_service =
            Arc::new(crate::modules::agent::AgentMemoryService::new(memory_repo));

        // 用户服务
        let user_repository: Arc<dyn crate::modules::user::UserRepository> =
            Arc::new(crate::modules::user::SqliteUserRepository::new(database.as_ref().clone()));
        let user_service = Arc::new(crate::modules::user::UserService::new(user_repository));

        // 租户服务
        let tenant_repository: Arc<dyn crate::modules::tenant::TenantRepository> = Arc::new(
            crate::modules::tenant::SqliteTenantRepository::new(database.as_ref().clone()),
        );
        let tenant_service =
            Arc::new(crate::modules::tenant::TenantService::new(tenant_repository));

        // 工作空间服务
        let workspace_repository: Arc<dyn crate::modules::workspace::WorkspaceRepository> =
            Arc::new(crate::modules::workspace::SqliteWorkspaceRepository::new(
                database.as_ref().clone(),
            ));
        let workspace_service =
            Arc::new(crate::modules::workspace::WorkspaceService::new(workspace_repository));

        // 标签服务
        let tag_service = Arc::new(crate::modules::tag::TagService::new(
            tag_repository.clone(),
            tag_binding_repository,
        ));

        // 角色服务
        let role_repository: Arc<dyn crate::modules::role::RoleRepository> =
            Arc::new(crate::modules::role::SqliteRoleRepository::new(database.as_ref().clone()));
        let role_service = Arc::new(crate::modules::role::RoleService::new(role_repository));

        // 权限服务
        let permission_repository: Arc<dyn crate::modules::permission::PermissionRepository> =
            Arc::new(crate::modules::permission::SqlitePermissionRepository::new(
                database.as_ref().clone(),
            ));
        let permission_group_repository: Arc<
            dyn crate::modules::permission::PermissionGroupRepository,
        > = Arc::new(crate::modules::permission::SqlitePermissionGroupRepository::new(
            database.as_ref().clone(),
        ));
        let permission_service = Arc::new(crate::modules::permission::PermissionService::new(
            permission_repository,
            permission_group_repository,
        ));

        // Cron 仓库
        let cron_job_repo: Arc<dyn crate::modules::cron::CronJobRepository> =
            Arc::new(tinyiothub_storage::sqlite::cron_job::SqliteCronJobRepository::new(
                database.as_ref().clone(),
            ));
        let cron_run_repo: Arc<dyn crate::modules::cron::CronRunRepository> =
            Arc::new(tinyiothub_storage::sqlite::cron_run::SqliteCronRunRepository::new(
                database.as_ref().clone(),
            ));

        // 会话服务 - 用于 Agent 聊天会话管理
        let session_repository: Arc<dyn crate::modules::agent::SessionRepository> =
            Arc::new(crate::shared::persistence::repositories::SqliteSessionRepository::new(
                database.as_ref().clone(),
            ));
        let session_service =
            Arc::new(crate::modules::agent::SessionService::new(Arc::clone(&session_repository)));

        // 聊天服务 - 编排 Agent 聊天、会话、记忆上下文
        let chat_service = Arc::new(crate::modules::agent::ChatService::new(
            agent_runtime.clone(),
            session_repository,
            agent_memory_service.clone(),
            crate::modules::agent::ChatServiceConfig {
                system_prompts: agent_settings.system_prompts.clone(),
                max_messages_before_compact: agent_settings.max_messages_before_compact,
                enable_compaction: agent_settings.enable_compaction,
            },
        ));

        // === 网关配对服务 ===
        let (mqtt_tx, mqtt_rx) =
            tokio::sync::mpsc::channel::<crate::modules::gateway::service::MqttPublish>(100);
        let (announce_tx, mut announce_rx) =
            tokio::sync::mpsc::channel::<crate::modules::gateway::types::PairingAnnounce>(1000);
        let (data_tx, mut data_rx) =
            tokio::sync::mpsc::channel::<crate::modules::gateway::types::GatewayDataMessage>(1000);
        let pairing_cache = Arc::new(crate::modules::gateway::pairing::PairingCache::new(10000));
        let gateway_service = Arc::new(crate::modules::gateway::service::GatewayService::new(
            device_repository_factory.clone(),
            event_repository.clone(),
            pairing_cache,
            mqtt_tx,
        ));

        // MQTT 客户端
        let config = crate::shared::config::get();
        let mqtt_broker = config.mqtt.primary.host.clone();
        let mqtt_port = config.mqtt.primary.port;
        let mqtt_username = config.mqtt.primary.username.clone().unwrap_or_default();
        let mqtt_password = config.mqtt.primary.password.clone().unwrap_or_default();
        let mqtt_client = Arc::new(crate::shared::mqtt_client::PlatformMqttClient::new(
            &mqtt_broker,
            mqtt_port,
            &mqtt_username,
            &mqtt_password,
            announce_tx,
            mqtt_rx,
            data_tx,
        ));

        // 启动宣告处理任务
        let gs = gateway_service.clone();
        tokio::spawn(async move {
            while let Some(announce) = announce_rx.recv().await {
                if let Err(e) = gs.handle_announce(announce).await {
                    tracing::warn!(?e, "Failed to handle pairing announce");
                }
            }
        });

        // 启动网关数据消息处理任务
        let gs_data = gateway_service.clone();
        tokio::spawn(async move {
            while let Some(msg) = data_rx.recv().await {
                gs_data.handle_gateway_data(msg).await;
            }
        });

        Self {
            device_cache,
            database,
            device_repository_factory,
            data_server: None, // DataServer 由 ServiceManager 设置
            device_service,
            device_query_service,
            monitoring_service,
            performance_service,
            trace_service,
            template_engine,
            notification_manager,
            redis,
            event_bus,
            event_repository,
            real_time_event_repository,
            sse_manager,
            secure_event_service,
            alarm_service,
            agent_runtime,
            user_service,
            tenant_service,
            workspace_service,
            tag_service,
            tag_repository,
            role_service,
            permission_service,
            cron_job_repo,
            cron_run_repo,
            session_service,
            agent_memory_service,
            chat_service,
            sysinfo_system: Arc::new(std::sync::Mutex::new(sysinfo::System::new_all())),
            gateway_service,
            mqtt_client: Some(mqtt_client),
        }
    }

    /// 设置数据服务器（由 ServiceManager 调用）
    pub fn set_data_server(&mut self, data_server: Arc<tinyiothub_runtime::DataServer>) {
        self.data_server = Some(data_server);
    }

    /// 获取数据服务器
    pub fn data_server(&self) -> Option<&tinyiothub_runtime::DataServer> {
        self.data_server.as_ref().map(|ds| ds.as_ref())
    }

    /// 获取数据库实例（兼容性方法）
    ///
    /// 提供对底层数据库的访问，主要用于：
    /// 1. 遗留代码兼容
    /// 2. 直接数据库操作（谨慎使用）
    /// 3. 事务管理
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// 获取数据库连接池（兼容性方法）
    pub fn db_pool(&self) -> sqlx::SqlitePool {
        self.database.pool().clone()
    }

    /// 获取租户感知的设备服务
    ///
    /// 使用设备仓库工厂创建针对特定工作空间的租户感知设备仓库，
    /// 并基于该仓库创建设备服务。
    ///
    /// 获取租户感知的设备服务（接受字符串 workspace_id）
    pub fn tenant_device_service_str(&self, workspace_id: &str) -> Arc<DeviceService> {
        let repository =
            self.device_repository_factory.create_for_workspace(workspace_id.to_string());
        Arc::new(
            DeviceService::new(repository, self.database.clone())
                .with_tag_repository(self.tag_repository.clone()),
        )
    }

    /// Returns a tenant-scoped device service.
    /// When workspace_id is None, logs a security warning and uses an empty
    /// workspace ID (returns no devices) instead of falling back to the raw
    /// repository which would bypass all tenant isolation.
    pub fn tenant_device_service(&self, workspace_id: &Option<String>) -> Arc<DeviceService> {
        let ws_id = workspace_id.clone().unwrap_or_else(|| {
            tracing::warn!(
                "[SECURITY] tenant_device_service called with workspace_id=None — \
                 using empty workspace (no devices will be returned). \
                 This indicates a bug: WorkspaceScope should always resolve to a workspace_id."
            );
            String::new()
        });
        let repository = self.device_repository_factory.create_for_workspace(ws_id);

        // 创建设备服务（使用现有的事件总线和标签仓库）
        Arc::new(
            DeviceService::with_event_bus(
                repository,
                self.database.clone(),
                self.event_bus.clone(),
            )
            .with_tag_repository(self.tag_repository.clone()),
        )
    }

    /// Resolve workspace ID for a tenant.
    /// If an explicit workspace_id is provided, returns it directly.
    /// Otherwise queries the database for the tenant's default workspace.
    pub async fn resolve_workspace(
        &self,
        tenant_id: &str,
        explicit: Option<String>,
    ) -> Result<String, (i32, String)> {
        if let Some(ws) = explicit {
            return Ok(ws);
        }
        match self.workspace_service.find_by_tenant(tenant_id, Some(1), Some(1)).await {
            Ok(workspaces) if !workspaces.is_empty() => Ok(workspaces[0].id.clone()),
            _ => {
                tracing::warn!("No workspace found for tenant {}", tenant_id);
                Err((400, "未找到工作空间".to_string()))
            }
        }
    }

    // === 兼容性方法 ===
    // 这些方法提供对 DeviceCache 的直接访问，
    // 用于渐进式迁移，避免一次性修改所有代码

    /// 通过设备名称和属性名称获取属性
    pub fn get_device_prop_by_name(
        &self,
        device_name: &str,
        property_name: &str,
    ) -> Option<DeviceProperty> {
        self.device_cache.get_by_name(device_name).and_then(|d| {
            d.properties
                .as_ref()
                .and_then(|props| props.iter().find(|p| p.name == property_name).cloned())
        })
    }

    /// 更新设备属性值
    ///
    /// 通过发布 PropertyChange 事件解耦：
    /// 1.  cloud 层只负责验证 + 发布事件
    /// 2.  engine::DataServer 作为 EventHandler 接收事件并更新 DeviceCache
    pub async fn update_device_property_value(
        &self,
        workspace_id: &str,
        device_id: &str,
        property_id: &str,
        value: &str,
    ) -> Result<(), Error> {
        use tinyiothub_core::models::event::{
            ContentElement, EventSource, RichContent, TextFormat,
        };

        // 1. 验证设备存在且属于指定的workspace
        let tenant_device_service = self.tenant_device_service(&Some(workspace_id.to_string()));
        let device = match tenant_device_service.get_device_by_id(device_id).await? {
            Some(d) => d,
            None => return Err(Error::NotFound),
        };

        // 2. 验证属性存在且属于该设备
        let property = match crate::shared::persistence::repositories::find_device_property_by_id(
            self.database(),
            property_id,
        )
        .await
        {
            Ok(Some(p)) if p.device_id == device_id => p,
            Ok(Some(_)) => {
                return Err(Error::ValidationError(
                    "Property does not belong to device".to_string(),
                ));
            }
            Ok(None) => return Err(Error::NotFound),
            Err(e) => return Err(Error::IOError(format!("DB error: {}", e))),
        };

        // 3. 构造并发布 PropertyChange 事件
        let source = EventSource::device_property(
            device_id.to_string(),
            property_id.to_string(),
            format!("{}:{}", device_id, property_id),
        );

        let device_display_name = device.display_name.as_deref().unwrap_or(&device.name);
        let content = RichContent::new(
            format!("Property Changed: {} - {}", device_display_name, property.name),
            vec![ContentElement::Text {
                content: format!("Current value: {}", value),
                format: TextFormat::Plain,
            }],
        );

        let event = tinyiothub_core::models::event::Event::new_property_change_event(
            device_id.to_string(),
            property_id.to_string(),
            source,
            content,
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;

        self.event_bus.publish(event).await.map_err(|e| Error::IOError(e.to_string()))?;

        Ok(())
    }

    /// 获取设备（从缓存读取实时状态）
    pub fn get_device(&self, device_id: &str) -> Option<tinyiothub_core::models::device::Device> {
        self.device_cache.get(device_id)
    }

    /// 获取模板引擎
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
    }

    /// 获取通知管理器
    pub fn get_notification_manager(&self) -> Option<&NotificationManager> {
        self.notification_manager.as_ref().map(|nm| nm.as_ref())
    }

    /// 获取 Redis 客户端
    pub fn get_redis(&self) -> Option<&RedisClient> {
        self.redis.as_ref()
    }

    /// 获取SSE连接管理器
    pub fn get_sse_manager(&self) -> &SseConnectionManager {
        &self.sse_manager
    }

    /// 获取事件总线
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// 获取安全事件服务
    pub fn get_secure_event_service(&self) -> Option<&SecureEventService> {
        self.secure_event_service.get().map(|ses| ses.as_ref())
    }

    /// 初始化安全事件服务（异步）
    pub async fn initialize_secure_event_service(
        &self,
    ) -> Result<&SecureEventService, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(service) = self.secure_event_service.get() {
            return Ok(service.as_ref()); // Already initialized
        }

        // Get security configuration from unified config
        let config = crate::shared::config::get().event.security.clone();

        // Create security factory
        let security_factory = EventSecurityFactory::new(self.database.clone(), config)?;

        // Create secure event service
        let secure_service =
            security_factory.create_secure_event_service(self.event_repository.clone()).await?;

        // Store in OnceCell
        let service_arc = Arc::new(secure_service);
        match self.secure_event_service.set(service_arc) {
            Ok(_) => self
                .secure_event_service
                .get()
                .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::other("Failed to get secure event service"))
                })
                .map(|s| s.as_ref()),
            Err(_) => {
                // Another thread already initialized it
                self.secure_event_service
                    .get()
                    .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                        Box::new(std::io::Error::other("Failed to get secure event service"))
                    })
                    .map(|s| s.as_ref())
            }
        }
    }

    /// 创建通知管理器
    fn create_notification_manager(
        database: Arc<Database>,
    ) -> Result<Arc<NotificationManager>, Box<dyn std::error::Error + Send + Sync>> {
        // Create notification history store
        let _history_store = Arc::new(NotificationHistoryRepositoryImpl::new(database.clone()));

        // Create notification rule repository
        let rule_repo = Arc::new(NotificationRuleRepositoryImpl::new(database));

        // Create notification manager with rule repository
        let mut notification_manager = NotificationManager::new(rule_repo);

        // Register notification channels
        let channels = NotificationChannelFactory::create_all_channels();
        for channel in channels {
            notification_manager.register_channel(channel);
        }

        Ok(Arc::new(notification_manager))
    }

    /// Create AppState for testing
    #[cfg(test)]
    pub async fn new_for_testing() -> Self {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database_url = format!("sqlite://{}", db_path.to_str().unwrap());
        let pool = sqlx::SqlitePool::connect(&database_url).await.unwrap();

        crate::shared::persistence::test_helpers::run_all_migrations(&pool).await.unwrap();

        let device_cache = Arc::new(DeviceCache::new());

        Self::new(device_cache, pool)
    }
}

// === 为什么选择这种设计？===
//
// 1. **Axum 最佳实践**
//    - Axum 官方推荐使用单一状态类型
//    - with_state 只接受一个参数，这是框架设计
//    - 避免使用 Extension，因为它是运行时检查
//
// 2. **依赖注入模式**
//    - 在应用启动时解析所有依赖
//    - 避免服务定位器反模式
//    - 便于测试和模拟
//
// 3. **性能考虑**
//    - 服务实例在启动时创建一次
//    - Arc 提供高效的多线程共享
//    - 避免每次请求的分配开销
//
// 4. **类型安全**
//    - 编译时检查所有依赖
//    - 避免运行时的类型转换错误
//    - IDE 友好的代码补全
//
// 5. **可维护性**
//    - 清晰的服务边界
//    - 统一的依赖管理
//    - 便于添加新服务
