use std::sync::Arc;

use tinyiothub_auth::redis::RedisClient;
use tinyiothub_core::{memory::MemoryStore, models::device_property::DeviceProperty};
use tinyiothub_notify::{
    NotificationHistoryRepositoryImpl, NotificationManager, NotificationRuleRepositoryImpl,
    channels::NotificationChannelFactory,
};
use tinyiothub_storage::{Database, DeviceRepositoryFactory, cache::DeviceCache};
use tinyiothub_thing::template::{TemplateEngine, TemplateRepository, TemplateValidator};
use tokio::sync::OnceCell;

use tinyiothub_event::{
    repositories::{EventRepository, RealTimeEventRepository},
    sqlite_event::SqliteEventRepository,
    sqlite_real_time_event::SqliteRealTimeEventRepository,
};
use tinyiothub_agent::host::agent::AgentPool;

use crate::{
    modules::{
        device::{
            monitoring_service::DeviceMonitoringService,
            performance_service::DevicePerformanceService, query_service::DeviceQueryService,
            service::DeviceService, trace_repository::DeviceTraceRepository,
            trace_service::DeviceTraceService,
        },
    },
    shared::{
        error::Error,
        event::{
            EventBus, SseConnectionManager,
            security::{EventSecurityFactory, SecureEventService},
        },
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

    /// SSE Token管理器 - 生成和验证短期SSE连接token
    pub sse_token_manager: Arc<crate::shared::sse_token::SseTokenManager>,

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
    pub alarm_service: Arc<tinyiothub_alarm::AlarmService>,

    /// Agent Pool — central agent lifecycle manager
    pub agent_pool: Arc<AgentPool>,

    /// 用户服务 - CRUD 操作
    pub user_service: Arc<tinyiothub_user::UserService>,

    /// 租户服务 - CRUD 操作
    pub tenant_service: Arc<tinyiothub_tenant::TenantService>,

    /// 工作空间服务 - CRUD 操作
    pub workspace_service: Arc<tinyiothub_tenant::WorkspaceService>,

    /// 标签服务 - CRUD 操作
    pub tag_service: Arc<tinyiothub_thing::tag::TagService>,

    /// AI subsystem orchestrator (set during async startup)
    pub orchestrator: Option<Arc<tinyiothub_agent::loop_::orchestrator::Orchestrator>>,

    /// AI subsystem heartbeat runner (set during async startup)
    pub heartbeat_runner: Option<Arc<tinyiothub_agent::loop_::heartbeat::runner::HeartbeatRunner>>,

    /// 标签仓库 - 用于设备服务的标签关联
    pub tag_repository: Arc<dyn tinyiothub_thing::tag::TagRepository>,

    /// 角色服务 - CRUD 操作
    pub role_service: Arc<tinyiothub_user::role::RoleService>,

    /// 权限服务 - CRUD 操作
    pub permission_service: Arc<tinyiothub_user::permission::PermissionService>,

    /// Cron 任务仓库
    pub cron_job_repo: Arc<dyn crate::modules::cron::CronJobRepository>,

    /// Cron 执行记录仓库
    pub cron_run_repo: Arc<dyn crate::modules::cron::CronRunRepository>,

    /// 会话服务 - Agent 聊天会话管理
    pub session_service: Arc<tinyiothub_agent::host::SessionService>,

    /// 缓存的系统信息对象，避免每次请求重新扫描
    pub sysinfo_system: Arc<std::sync::Mutex<sysinfo::System>>,

    /// 网关服务 - MQTT 网关配对
    pub gateway_service: Arc<tinyiothub_driver::gateway::service::GatewayService>,

    /// MQTT 客户端（可选，未配置时为空）
    pub mqtt_client: Option<Arc<crate::shared::mqtt_client::PlatformMqttClient>>,

    /// 全局物事件广播总线（T6）—— thing-agent loop 经此订阅事件信号
    pub thing_event_bus: Arc<tinyiothub_event::bus::ThingEventBus>,

    /// 用户指令投递入口（T14）—— HTTP 端点 / chat 工具经此向
    /// thing-agent loop 投递 WakeSignal。T15 用 ThingAgentManager 实现
    /// 并注入；None 时指令入口返回 503。
    pub directive_sink: Option<Arc<dyn tinyiothub_agent::loop_::thing_agent::DirectiveSink>>,

    /// Agent 记忆存储 - 持久化 agent 记忆到 SQLite
    pub memory_store: Arc<dyn MemoryStore>,

    /// Thing action hooks（P4.0b）—— thing handler 经此调用 agent 侧的
    /// 参数校验 / 确认令牌存储 / 策略裁决，斩断 thing→agent 依赖边。
    /// 由组合层（此处）注入 agent 实现；thing 域只依赖 core trait。
    pub thing_action_hooks: Arc<dyn tinyiothub_core::thing_hooks::ThingActionHooks>,

    /// Agent hooks（P4.0d）—— workspace 域经此使用 agent 侧的默认心跳
    /// 任务集 /  legacy HEARTBEAT.md 解析与迁移，斩断 workspace→agent
    /// 依赖边。由组合层（此处）注入 agent 实现；workspace 域只依赖 core trait。
    pub agent_hooks: Arc<dyn tinyiothub_core::agent_hooks::AgentHooks>,
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
            Arc::new(tinyiothub_alarm::SqliteAlarmRepository::new(database.clone()));
        let alarm_rule_repository =
            Arc::new(tinyiothub_alarm::SqliteAlarmRuleRepository::new(database.clone()));
        let alarm_service = Arc::new(tinyiothub_alarm::AlarmService::new(
            alarm_repository.clone(),
            alarm_rule_repository,
        ));

        // 创建SSE管理器（带 DeviceCache 用于设备 workspace 查找）
        let sse_manager = Arc::new(SseConnectionManager::new());

        // SSE Token 管理器 — 生成短期令牌用于 SSE 连接认证（替代 URL 中的 JWT）
        let sse_token_manager = Arc::new(crate::shared::sse_token::SseTokenManager::default());

        // 注册事件处理器将在异步初始化中完成
        // 这里只创建事件总线，处理器注册推迟到 register_event_handlers() 方法

        // 标签仓库（提前创建，供 DeviceService 使用）
        let tag_repository: Arc<dyn tinyiothub_thing::tag::TagRepository> =
            Arc::new(tinyiothub_thing::tag::SqliteTagRepository::new(database.as_ref().clone()));
        let tag_binding_repository: Arc<dyn tinyiothub_thing::tag::TagBindingRepository> = Arc::new(
            tinyiothub_thing::tag::SqliteTagBindingRepository::new(database.as_ref().clone()),
        );

        // 基础服务 - 使用事件总线
        let device_repository: Arc<dyn crate::modules::device::repository::DeviceRepository> =
            Arc::new(tinyiothub_storage::SqliteDeviceRepository::new(database.as_ref().clone()));
        let device_service = Arc::new(
            DeviceService::with_event_bus(device_repository, database.clone(), event_bus.clone())
                .with_tag_repository(tag_repository.clone()),
        );
        let device_query_service: Arc<dyn DeviceQueryService> =
            Arc::new(crate::modules::device::query_service_impl::SqliteDeviceQueryService::new(
                database.as_ref().clone(),
            ));

        // 监控服务 - 依赖数据库、缓存和告警仓库
        let monitoring_service = Arc::new(DeviceMonitoringService::new(
            database.clone(),
            device_cache.clone(),
            alarm_repository.clone(),
        ));

        // 性能服务 - 依赖数据库、缓存和告警仓库
        let performance_service = Arc::new(DevicePerformanceService::new(
            database.clone(),
            device_cache.clone(),
            alarm_repository.clone(),
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
        // Validate minimax config exists (used by get_or_create_agent at provider creation time)
        let minimax_config = crate::shared::config::get()
            .minimax
            .clone()
            .expect("minimax config is required - set [minimax] in app_settings.toml");
        // Register the agent crate's config ports (minimax provider settings
        // + default model) — the agent crate no longer reads cloud's global
        // config directly (P4-Task22).
        tinyiothub_agent::host::ports::set_minimax_settings(
            tinyiothub_agent::host::ports::MinimaxSettings {
                base_url: minimax_config.base_url.clone(),
                auth_token: minimax_config.auth_token.clone(),
                model: minimax_config.model.clone(),
            },
        );
        let agent_settings = crate::shared::config::get().agent.clone();
        tracing::info!(
            "TinyIoTHub Agent runtime initialized (memory_backend={}, observer_backend={})",
            agent_settings.memory_backend,
            agent_settings.observer_backend
        );
        // Agent Memory Store
        let memory_store: Arc<dyn MemoryStore> =
            Arc::new(tinyiothub_memory::SqliteAgentMemoryRepository::new(database.pool().clone()));

        let agent_pool: Arc<AgentPool> = Arc::new(
            AgentPool::new(
                database.pool().clone(),
                memory_store.clone(),
                &agent_settings,
                tinyiothub_agent::host::autonomous_factory::minimax_provider_factory(),
            )
            .expect("failed to build AgentPool"),
        );

        alarm_service.set_device_cache(device_cache.clone());

        // 用户服务
        let user_repository: Arc<dyn tinyiothub_user::UserRepository> =
            Arc::new(tinyiothub_user::SqliteUserRepository::new(database.as_ref().clone()));
        let user_service = Arc::new(tinyiothub_user::UserService::new(user_repository));

        // 租户服务
        let tenant_repository: Arc<dyn tinyiothub_tenant::TenantRepository> =
            Arc::new(tinyiothub_tenant::SqliteTenantRepository::new(database.as_ref().clone()));
        let tenant_service = Arc::new(tinyiothub_tenant::TenantService::new(tenant_repository));

        // 工作空间服务
        let workspace_repository: Arc<dyn tinyiothub_tenant::WorkspaceRepository> =
            Arc::new(tinyiothub_tenant::SqliteWorkspaceRepository::new(database.as_ref().clone()));
        let workspace_service =
            Arc::new(tinyiothub_tenant::WorkspaceService::new(workspace_repository));

        // 标签服务
        let tag_service = Arc::new(tinyiothub_thing::tag::TagService::new(
            tag_repository.clone(),
            tag_binding_repository,
        ));

        // 角色服务
        let role_repository: Arc<dyn tinyiothub_user::role::RoleRepository> =
            Arc::new(tinyiothub_user::role::SqliteRoleRepository::new(database.as_ref().clone()));
        let role_service = Arc::new(tinyiothub_user::role::RoleService::new(role_repository));

        // 权限服务
        let permission_repository: Arc<dyn tinyiothub_user::permission::PermissionRepository> =
            Arc::new(tinyiothub_user::permission::SqlitePermissionRepository::new(
                database.as_ref().clone(),
            ));
        let permission_group_repository: Arc<
            dyn tinyiothub_user::permission::PermissionGroupRepository,
        > = Arc::new(tinyiothub_user::permission::SqlitePermissionGroupRepository::new(
            database.as_ref().clone(),
        ));
        let permission_service = Arc::new(tinyiothub_user::permission::PermissionService::new(
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
        let session_repository: Arc<dyn tinyiothub_agent::host::SessionRepository> =
            Arc::new(tinyiothub_agent::host::session_repository::SqliteSessionRepository::new(
                database.as_ref().clone(),
            ));
        let session_service =
            Arc::new(tinyiothub_agent::host::SessionService::new(Arc::clone(&session_repository)));

        // === 网关配对服务 ===
        let (mqtt_tx, mqtt_rx) =
            tokio::sync::mpsc::channel::<tinyiothub_driver::gateway::service::MqttPublish>(100);
        let (announce_tx, mut announce_rx) =
            tokio::sync::mpsc::channel::<tinyiothub_driver::gateway::types::PairingAnnounce>(1000);
        let (data_tx, mut data_rx) =
            tokio::sync::mpsc::channel::<tinyiothub_driver::gateway::types::GatewayDataMessage>(1000);
        let pairing_cache = Arc::new(tinyiothub_driver::gateway::pairing::PairingCache::new(10000));
        let gateway_service = Arc::new(tinyiothub_driver::gateway::service::GatewayService::new(
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
        let throttle_state = Arc::new(tinyiothub_event::router::ThrottleState::new(60));
        let thing_event_bus = Arc::new(tinyiothub_event::bus::ThingEventBus::new());
        let mqtt_db_pool = database.pool().clone();
        let mqtt_client = Arc::new(crate::shared::mqtt_client::PlatformMqttClient::new(
            &mqtt_broker,
            mqtt_port,
            &mqtt_username,
            &mqtt_password,
            announce_tx,
            mqtt_rx,
            data_tx,
            throttle_state,
            thing_event_bus.clone(),
            mqtt_db_pool,
            Some(alarm_service.clone()),
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

        // Thing action hooks（P4.0b）—— agent 侧实现 core trait，注入给 thing handler
        let thing_action_hooks: Arc<dyn tinyiothub_core::thing_hooks::ThingActionHooks> =
            Arc::new(tinyiothub_agent::host::thing_action_hooks::AgentThingActionHooks::new(
                database.pool().clone(),
            ));

        // Agent hooks（P4.0d）—— agent 侧实现 core trait，注入给 workspace 域
        let agent_hooks: Arc<dyn tinyiothub_core::agent_hooks::AgentHooks> =
            Arc::new(tinyiothub_agent::host::agent_hooks::AgentHooksImpl::new(Arc::new(
                tinyiothub_agent::host::heartbeat_repo::SqliteHeartbeatTaskRepository::new(
                    database.pool().clone(),
                ),
            )));

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
            sse_token_manager,
            secure_event_service,
            alarm_service,
            agent_pool,
            orchestrator: None,
            heartbeat_runner: None,
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
            sysinfo_system: Arc::new(std::sync::Mutex::new(sysinfo::System::new_all())),
            gateway_service,
            mqtt_client: Some(mqtt_client),
            thing_event_bus,
            directive_sink: None, // T15 闭环接线时注入 ThingAgentManager

            memory_store,
            thing_action_hooks,
            agent_hooks,
        }
    }

    /// 注入用户指令投递入口（T15 闭环接线调用）
    pub fn set_directive_sink(&mut self, sink: Arc<dyn tinyiothub_agent::loop_::thing_agent::DirectiveSink>) {
        self.directive_sink = Some(sink);
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
        let property = match tinyiothub_storage::find_device_property_by_id(
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

    /// 获取 SSE 连接管理器
    pub fn get_sse_manager(&self) -> &SseConnectionManager {
        &self.sse_manager
    }

    /// 获取 SSE Token 管理器
    pub fn get_sse_token_manager(&self) -> &crate::shared::sse_token::SseTokenManager {
        &self.sse_token_manager
    }

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

        tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();

        let device_cache = Arc::new(DeviceCache::new());

        Self::new(device_cache, pool)
    }
}

/// P4-Task15 (SEP pilot): derive the thing domain's state slice from the
/// global AppState. Cloud mounts `tinyiothub_thing::router()` (things),
/// `template::handler::create_router()` and `tag::create_router()` with this
/// `FromRef` conversion.
impl axum::extract::FromRef<AppState> for tinyiothub_thing::ThingState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_thing::ThingState {
            database: state.database.clone(),
            hooks: state.thing_action_hooks.clone(),
            data_server: state.data_server.clone(),
            template_engine: state.template_engine.clone(),
            tag_service: state.tag_service.clone(),
        }
    }
}

// ============================================================================
// P4-Task16: auth domain slice + seam adapters
// ============================================================================

/// Map the cloud user entity to the auth crate's byte-identical mirror.
fn auth_user_from_user(user: tinyiothub_user::User) -> tinyiothub_auth::user_store::AuthUser {
    tinyiothub_auth::user_store::AuthUser {
        id: user.id,
        username: user.username,
        password_hash: user.password_hash,
        email: user.email,
        phone: user.phone,
        display_name: user.display_name,
        is_enabled: user.is_enabled,
        parent_id: user.parent_id,
        created_at: user.created_at,
        updated_at: user.updated_at,
        last_login_at: user.last_login_at,
    }
}

/// Identity-store seam adapter: auth handlers consume `AuthUserStore`.
/// After Task 17a both the trait (auth crate) and `UserService` (user
/// crate) are foreign to cloud, so the orphan rule requires this newtype
/// wrapper — the adapter stays in cloud because the user crate must not
/// depend on the auth crate (wrong dependency direction).
pub struct UserServiceAuthAdapter {
    pub service: Arc<tinyiothub_user::UserService>,
}

#[async_trait::async_trait]
impl tinyiothub_auth::user_store::AuthUserStore for UserServiceAuthAdapter {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<tinyiothub_auth::user_store::AuthUser>, String> {
        self.service
            .authenticate(username, password)
            .await
            .map_err(|e| e.to_string())
            .map(|o| o.map(auth_user_from_user))
    }

    async fn get_user_by_id(
        &self,
        id: &str,
    ) -> Result<Option<tinyiothub_auth::user_store::AuthUser>, String> {
        self.service
            .get_user_by_id(id)
            .await
            .map_err(|e| e.to_string())
            .map(|o| o.map(auth_user_from_user))
    }

    async fn update_last_login(&self, id: &str) -> Result<(), String> {
        self.service.update_last_login(id).await.map_err(|e| e.to_string())
    }

    async fn exists_by_username(&self, username: &str) -> Result<bool, String> {
        self.service.exists_by_username(username).await.map_err(|e| e.to_string())
    }

    async fn exists_by_phone(&self, phone: &str) -> Result<bool, String> {
        self.service.exists_by_phone(phone).await.map_err(|e| e.to_string())
    }

    async fn exists_by_email(&self, email: &str) -> Result<bool, String> {
        self.service.exists_by_email(email).await.map_err(|e| e.to_string())
    }

    async fn create_user(
        &self,
        request: &tinyiothub_auth::user_store::AuthCreateUserRequest,
    ) -> Result<tinyiothub_auth::user_store::AuthUser, String> {
        let create_request = tinyiothub_user::types::CreateUserRequest {
            username: request.username.clone(),
            password: request.password.clone(),
            email: request.email.clone(),
            phone: request.phone.clone(),
            display_name: request.display_name.clone(),
            is_enabled: request.is_enabled,
            parent_id: request.parent_id.clone(),
        };
        self.service
            .create_user(&create_request)
            .await
            .map_err(|e| e.to_string())
            .map(auth_user_from_user)
    }
}

/// Workspace-bootstrap seam: post-registration tenant/workspace scaffolding
/// stays in cloud (`modules::system::handler`, entangled with the agent
/// plane); this adapter carries the AppState the function needs.
pub struct SystemWorkspaceBootstrap {
    pub state: AppState,
}

#[async_trait::async_trait]
impl tinyiothub_auth::bootstrap::WorkspaceBootstrap for SystemWorkspaceBootstrap {
    async fn ensure_user_has_workspace(&self, user_id: &str) -> Result<(), String> {
        crate::modules::system::handler::ensure_user_has_workspace(&self.state, user_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// SSE token issuer seam: the manager stays in cloud (shared with the event
/// plane's SSE handlers).
impl tinyiothub_auth::sse::SseTokenIssuer for crate::shared::sse_token::SseTokenManager {
    fn generate_token(&self, user_id: &str, workspace_id: &str) -> String {
        crate::shared::sse_token::SseTokenManager::generate_token(self, user_id, workspace_id)
    }
}

/// P4-Task16: derive the auth domain's state slice from the global AppState.
/// Config slices are cloned from the process-global settings at extraction
/// time — identical semantics to the former per-request `config::get()`
/// reads (the global config is set once at startup and never reloaded).
impl axum::extract::FromRef<AppState> for tinyiothub_auth::AuthState {
    fn from_ref(state: &AppState) -> Self {
        let settings = crate::shared::config::get();
        tinyiothub_auth::AuthState {
            database: state.database.clone(),
            users: Arc::new(UserServiceAuthAdapter { service: state.user_service.clone() }),
            workspace_bootstrap: Arc::new(SystemWorkspaceBootstrap { state: state.clone() }),
            redis: state.redis.clone(),
            sse_token_issuer: state.sse_token_manager.clone(),
            sms_config: settings.sms.clone(),
            social_config: settings.social.clone(),
            harmonyos_enabled: settings.harmonyos.enabled,
        }
    }
}

// ============================================================================
// P4-Task17a: user domain slice + role-check seam adapter
// ============================================================================

/// Role-check seam adapter: the user handlers' admin checks route through
/// the event security plane (`AuthHelper` → `SecureEventService`), which
/// stays in cloud until Tasks 18/24. Holds an `AppState` clone like
/// `SystemWorkspaceBootstrap`.
pub struct EventSecurityRoleChecker {
    pub state: AppState,
}

#[async_trait::async_trait]
impl tinyiothub_user::RoleChecker for EventSecurityRoleChecker {
    async fn check_role(&self, user_id: &str, role: &str) -> Result<bool, String> {
        crate::shared::error_handling::AuthHelper::check_role(&self.state, user_id, role).await
    }
}

/// P4-Task17a: derive the user domain's state slice from the global
/// AppState. Cloud mounts `tinyiothub_user::router()` (users),
/// `role::create_router()` and `permission::create_router()` with this
/// `FromRef` conversion.
impl axum::extract::FromRef<AppState> for tinyiothub_user::UserState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_user::UserState {
            user_service: state.user_service.clone(),
            role_service: state.role_service.clone(),
            permission_service: state.permission_service.clone(),
            role_checker: Arc::new(EventSecurityRoleChecker { state: state.clone() }),
        }
    }
}

// ============================================================================
// P4-Task17b: tenant domain slice + seam adapters
// ============================================================================

/// Agent-lifecycle seam adapter: workspace create/delete provisions and
/// tears down the per-workspace Agent via cloud's `AgentPool` (agent plane,
/// not yet extracted).
pub struct AgentPoolLifecycle {
    pub pool: Arc<tinyiothub_agent::host::agent::AgentPool>,
}

#[async_trait::async_trait]
impl tinyiothub_tenant::WorkspaceAgentLifecycle for AgentPoolLifecycle {
    async fn create_agent(&self, workspace_id: &str, name: &str) -> Result<String, String> {
        self.pool
            .create_agent(&tinyiothub_agent::host::shared::AgentConfig {
                workspace_id: workspace_id.to_string(),
                name: name.to_string(),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_agent(&self, agent_id: &str) -> Result<(), String> {
        self.pool.delete_agent(agent_id).await.map_err(|e| e.to_string())
    }
}

/// Tag-suggester seam adapter: prompt construction, provider creation and
/// response parsing, byte-identical to the former inline workspace handler
/// code. Keeps the zeroclaw provider type out of the tenant crate.
pub struct MinimaxTagSuggester;

#[async_trait::async_trait]
impl tinyiothub_tenant::TagSuggester for MinimaxTagSuggester {
    async fn suggest(
        &self,
        name: &str,
        resource_type_label: &str,
        description: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let settings = crate::shared::config::get();
        let model = settings
            .minimax
            .as_ref()
            .map(|m| m.model.clone())
            .unwrap_or_else(|| "minimax-m2".into());

        let prompt = format!(
            "你是一个资源标签生成助手。根据用户提供的资源信息，生成 3-5 个简洁的中文标签。\n\
             严格只返回逗号分隔的标签，不要任何解释或额外文字。\n\n\
             示例输出：3D模型, 工厂, 设备, 车间\n\n\
             资源信息：\n- 文件名：{}\n- 资源类型：{}{}",
            name,
            resource_type_label,
            description.map_or(String::new(), |d| format!("\n- 描述：{}", d)),
        );

        let provider = crate::shared::config::create_minimax_provider().map_err(|e| {
            tracing::error!("Failed to create AI provider: {}", e);
            "AI 服务初始化失败".to_string()
        })?;

        let response =
            provider.chat_with_system(None, &prompt, &model, Some(0.3)).await.map_err(|e| {
                tracing::error!("AI tag generation failed: {}", e);
                "AI 生成标签失败，请稍后重试".to_string()
            })?;

        let tags: Vec<String> = response
            .split([',', '，', '、', '\n'])
            .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|t| !t.is_empty() && t.len() < 20)
            .collect();

        if tags.is_empty() { Err("AI 未生成有效标签".to_string()) } else { Ok(tags) }
    }
}

/// P4-Task17b: derive the tenant domain's state slice from the global
/// AppState. `jwt_secret` / `tag_suggester` are derived from the
/// process-global config at extraction time — identical semantics to the
/// former per-request `config::get()` reads (set once at startup).
impl axum::extract::FromRef<AppState> for tinyiothub_event::EventState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_event::EventState {
            event_repository: state.event_repository.clone(),
            real_time_event_repository: state.real_time_event_repository.clone(),
        }
    }
}

impl axum::extract::FromRef<AppState> for tinyiothub_alarm::AlarmState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_alarm::AlarmState {
            alarm_service: state.alarm_service.clone(),
            database: state.database.clone(),
        }
    }
}

impl axum::extract::FromRef<AppState> for tinyiothub_driver::DriverState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_driver::DriverState { gateway_service: state.gateway_service.clone() }
    }
}

/// P4-Task21: derive the notify domain's state slice from the global
/// AppState. Cloud mounts `tinyiothub_notify::router()` (/notifications) and
/// `tinyiothub_notify::channel_router()` (/notification-channels).
impl axum::extract::FromRef<AppState> for tinyiothub_notify::NotifyState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_notify::NotifyState {
            database: state.database.clone(),
            notification_manager: state.notification_manager.clone(),
        }
    }
}

impl axum::extract::FromRef<AppState> for tinyiothub_tenant::TenantState {
    fn from_ref(state: &AppState) -> Self {
        let settings = crate::shared::config::get();
        tinyiothub_tenant::TenantState {
            database: state.database.clone(),
            tenant_service: state.tenant_service.clone(),
            workspace_service: state.workspace_service.clone(),
            agent_lifecycle: Arc::new(AgentPoolLifecycle { pool: state.agent_pool.clone() }),
            tag_suggester: if settings.minimax.is_some() {
                Some(Arc::new(MinimaxTagSuggester))
            } else {
                None
            },
            jwt_secret: settings.security.jwt.secret.clone(),
            agents_base_dir: crate::shared::paths::agents_base_dir(),
        }
    }
}

// ============================================================================
// P4-Task22: agent domain slice + workspace-access seam adapter
// ============================================================================

/// Workspace-access seam adapter: the agent crate's `WorkspaceAccess` port
/// over the tenant crate's `WorkspaceService` (tenant → agent edge stays
/// one-way; agent never names tenant types).
pub struct TenantWorkspaceAccess {
    pub workspace_service: Arc<tinyiothub_tenant::WorkspaceService>,
}

#[async_trait::async_trait]
impl tinyiothub_agent::host::ports::WorkspaceAccess for TenantWorkspaceAccess {
    async fn workspace_tenant_id(&self, workspace_id: &str) -> Result<Option<String>, String> {
        self.workspace_service
            .find_by_id(workspace_id)
            .await
            .map(|ws| ws.map(|w| w.tenant_id))
            .map_err(|e| e.to_string())
    }
}

impl axum::extract::FromRef<AppState> for tinyiothub_agent::AgentState {
    fn from_ref(state: &AppState) -> Self {
        tinyiothub_agent::AgentState {
            database: state.database.clone(),
            agent_pool: state.agent_pool.clone(),
            session_service: state.session_service.clone(),
            memory_store: state.memory_store.clone(),
            directive_sink: state.directive_sink.clone(),
            workspace_access: Arc::new(TenantWorkspaceAccess {
                workspace_service: state.workspace_service.clone(),
            }),
            data_server: state.data_server.clone(),
            device_cache: state.device_cache.clone(),
            heartbeat_runner: state.heartbeat_runner.clone(),
            orchestrator: state.orchestrator.clone(),
            agent_hooks: state.agent_hooks.clone(),
            system_prompts: Arc::new(
                crate::shared::config::get().agent.system_prompts.clone(),
            ),
        }
    }
}
