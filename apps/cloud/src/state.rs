//! 应用组合态（AppState）与各域状态切片（`<Domain>State`）的唯一派生点。
//!
//! G7 裁决：新增域必须走 `<Domain>State + FromRef` 切片。已切片的域
//! （admin/mcp/agent）禁止 handler 直接吃 `State<AppState>`；其余遗留域
//! 尚未切片，待后续迁移。

use std::sync::Arc;

use crate::domains::agent::host::agent::AgentPool;
use crate::domains::auth::redis::RedisClient;
use crate::domains::driver::legacy::{
    DeviceMonitoringService, DevicePerformanceService, DeviceQueryService, DeviceService,
};
use tinyiothub_storage::event::{EventRepository, RealTimeEventRepository};
use tinyiothub_storage::notify::{NotificationHistoryRepository, NotificationRuleRepository};

use crate::domains::notify::channels::NotificationChannelFactory;
use crate::domains::notify::service::NotificationManager;
use crate::domains::thing::{
    legacy::{trace::DeviceTraceService, trace_repository::DeviceTraceRepository},
    template::{TemplateEngine, TemplateRepository, TemplateValidator},
};
use tinyiothub_storage::memory::MemoryStore;
use tinyiothub_storage::{Database, cache::DeviceCache};
use tokio::sync::OnceCell;

use crate::domains::event::security::{EventSecurityFactory, SecureEventService};
use crate::domains::event::sse_manager::SseConnectionManager;
use tinyiothub_runtime::event_bus::EventBus;

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
    pub sse_token_manager: Arc<tinyiothub_authn::sse_token::SseTokenManager>,

    /// 安全事件服务 - 带权限控制和加密的事件服务（懒加载）
    pub secure_event_service: OnceCell<Arc<SecureEventService>>,

    /// === 事件系统 ===
    /// 事件总线 - 事件发布和订阅
    pub event_bus: Arc<EventBus>,

    /// === 事件系统仓库 ===
    /// 事件历史仓库 - 事件持久化存储
    pub event_repository: Arc<EventRepository>,

    /// 实时事件状态仓库 - 当前活跃事件管理
    pub real_time_event_repository: Arc<RealTimeEventRepository>,

    /// 报警服务 - 报警规则和报警管理
    pub alarm_service: Arc<crate::domains::alarm::service::AlarmService>,

    /// Agent Pool — central agent lifecycle manager
    pub agent_pool: Arc<AgentPool>,

    /// 用户服务 - CRUD 操作
    pub user_service: Arc<crate::domains::user::UserService>,

    /// 租户服务 - CRUD 操作
    pub tenant_service: Arc<crate::domains::tenant::TenantService>,

    /// 工作空间服务 - CRUD 操作
    pub workspace_service: Arc<crate::domains::tenant::WorkspaceService>,

    /// 标签服务 - CRUD 操作
    pub tag_service: Arc<crate::domains::thing::tag::TagService>,

    /// AI subsystem orchestrator (set during async startup)
    pub orchestrator: Option<Arc<crate::domains::agent::loop_::orchestrator::Orchestrator>>,

    /// AI subsystem heartbeat runner (set during async startup)
    pub heartbeat_runner: Option<Arc<crate::domains::agent::loop_::heartbeat::runner::HeartbeatRunner>>,

    /// Agent MemoryService（set during async startup）—— Task 6 起由 cloud 侧
    /// 自持（memory profile compile / weekly digest handler），不再经
    /// Orchestrator 中转。
    pub memory_service: Option<Arc<tinyiothub_memory::service::MemoryService>>,

    /// 标签仓库 - 用于设备服务的标签关联
    pub tag_repository: Arc<crate::domains::thing::tag::TagRepository>,

    /// 角色服务 - CRUD 操作
    pub role_service: Arc<crate::domains::user::role::RoleService>,

    /// 权限服务 - CRUD 操作
    pub permission_service: Arc<crate::domains::user::permission::PermissionService>,

    /// Cron 任务仓库
    pub cron_job_repo: Arc<tinyiothub_storage::CronJobRepository>,

    /// Cron 执行记录仓库
    pub cron_run_repo: Arc<tinyiothub_storage::CronRunRepository>,

    /// 会话服务 - Agent 聊天会话管理
    pub session_service: Arc<crate::domains::agent::host::SessionService>,

    /// 缓存的系统信息对象，避免每次请求重新扫描
    pub sysinfo_system: Arc<std::sync::Mutex<sysinfo::System>>,

    /// 网关服务 - MQTT 网关配对
    pub gateway_service: Arc<crate::domains::driver::gateway::service::GatewayService>,

    /// MQTT 客户端（可选，未配置时为空）
    pub mqtt_client: Option<Arc<crate::shared::mqtt_client::PlatformMqttClient>>,

    /// 全局物事件广播总线（T6）—— thing-agent loop 经此订阅事件信号
    pub thing_event_bus: Arc<crate::domains::event::bus::ThingEventBus>,

    /// 用户指令投递入口（T14）—— HTTP 端点 / chat 工具经此向
    /// thing-agent loop 投递 WakeSignal。T15 用 ThingAgentManager 实现
    /// 并注入；None 时指令入口返回 503。
    pub directive_sink: Option<Arc<dyn crate::domains::agent::loop_::thing_agent::DirectiveSink>>,

    /// Agent 记忆存储 - 持久化 agent 记忆到 SQLite
    pub memory_store: Arc<MemoryStore>,

    /// Thing action hooks（G5a）—— thing handler 经此调用 agent 侧的
    /// 参数校验 / 确认令牌存储 / 策略裁决，斩断 thing→agent 依赖边。
    /// 由组合层（此处）注入 agent 实现；thing 域只依赖自有 trait。
    pub thing_action_hooks: Arc<dyn crate::domains::thing::hooks::ThingActionHooks>,

    /// Agent hooks（G5b）—— tenant 域 workspace 服务经此使用 agent 侧的
    /// 默认心跳任务集，斩断 tenant→agent 依赖边。由组合层（此处）注入
    /// agent 实现；tenant 域只依赖自有 trait。
    pub agent_hooks: Arc<dyn crate::domains::tenant::hooks::AgentHooks>,
    /// 工作空间访问校验（agent 域 seam）
    pub workspace_access: Arc<TenantWorkspaceAccess>,
    /// System prompts config（chat proxy 构造 full prompt）
    pub system_prompts: Arc<crate::shared::config::SystemPromptsConfig>,
    /// Workspace 创建/删除时的 agent 生命周期 seam（tenant 域）
    pub agent_lifecycle: Arc<AgentPoolLifecycle>,
    /// AI 标签建议（无 minimax 配置时 None）
    pub tag_suggester: Option<Arc<dyn crate::domains::tenant::TagSuggester>>,
    /// 租户 tj_* token 密钥（启动时从 config 克隆）
    pub jwt_secret: String,
    /// 每工作区文件数据根目录
    pub agents_base_dir: std::path::PathBuf,
    /// 网络默认值配置切片
    pub network_defaults: tinyiothub_core::config::NetworkDefaultsConfig,
    /// 主 MQTT 配置切片
    pub mqtt_primary: tinyiothub_core::config::MqttBrokerConfig,
    /// Marketplace 配置切片
    pub marketplace: tinyiothub_core::config::MarketplaceConfig,
    /// 动态驱动安装目录（device.drivers.dynamic_drivers_dir）
    pub dynamic_drivers_dir: String,
    /// CORS 允许来源（server.cors_origins）
    pub cors_origins: Vec<String>,
    /// 事件安全配置切片（secure event service 懒加载用）
    pub event_security: tinyiothub_core::config::EventSecurityConfig,
    /// MiniMax 配置切片（llm provider / tag suggester 用；未配置时 None）
    pub minimax: Option<tinyiothub_core::config::MinimaxConfig>,
    /// SMS 配置切片
    pub sms_config: tinyiothub_core::config::SmsConfig,
    /// 社交登录配置切片
    pub social_config: tinyiothub_core::config::SocialConfig,
    /// HarmonyOS 开关
    pub harmonyos_enabled: bool,
    /// JWT 机制服务（G2 构造注入，消灭 OnceLock 全局态）
    pub jwt_service: std::sync::Arc<tinyiothub_authn::jwt::JwtService>,
    /// 进程启动时间（G3，替代 health::START_TIME 全局静态）
    pub started_at: std::time::SystemTime,
    /// 待确认动作暂存（G3，替代 PENDING_ACTIONS 全局静态）
    pub pending_actions: std::sync::Arc<crate::domains::agent::host::tools::thing::PendingActionStore>,
    /// 驱动心跳状态/配置（G3，替代 HEARTBEAT_STATUS/CONFIG 双全局静态）
    pub driver_heartbeat_status:
        std::sync::Arc<tokio::sync::RwLock<crate::domains::driver::heartbeat::types::HeartbeatStatus>>,
    pub driver_heartbeat_config:
        std::sync::Arc<tokio::sync::RwLock<crate::domains::driver::heartbeat::types::HeartbeatConfig>>,
}

impl axum::extract::FromRef<AppState> for std::sync::Arc<tinyiothub_authn::jwt::JwtService> {
    fn from_ref(state: &AppState) -> Self {
        state.jwt_service.clone()
    }
}

impl AppState {
    /// user 域角色校验适配（原 UserState.role_checker，每次萃取新建语义保持）
    pub fn role_checker(&self) -> Arc<dyn crate::domains::user::RoleChecker> {
        Arc::new(EventSecurityRoleChecker { state: self.clone() })
    }

    /// Auth 域身份存储适配（原 AuthState.users，FromRef 每次萃取新建语义保持）
    pub fn auth_users(&self) -> Arc<dyn crate::domains::auth::user_store::AuthUserStore> {
        Arc::new(UserServiceAuthAdapter {
            service: self.user_service.clone(),
        })
    }

    /// 注册后 tenant/workspace 引导适配（原 AuthState.workspace_bootstrap）
    pub fn workspace_bootstrap(&self) -> Arc<dyn crate::domains::auth::bootstrap::WorkspaceBootstrap> {
        Arc::new(SystemWorkspaceBootstrap { state: self.clone() })
    }

    /// 创建应用程序状态
    ///
    /// 采用依赖注入容器模式，在应用启动时一次性创建所有服务
    /// 这样做的好处：
    /// 1. 性能优化 - 避免每次请求创建服务
    /// 2. 依赖管理 - 清晰的服务依赖关系
    /// 3. 测试友好 - 便于单元测试和集成测试
    /// 4. 类型安全 - 编译时检查所有依赖
    pub fn new(
        device_cache: Arc<DeviceCache>,
        db_pool: sqlx::SqlitePool,
        settings: &tinyiothub_core::config::ApplicationSettings,
    ) -> Self {
        // 创建共享的数据库连接
        let database = Arc::new(Database::new(db_pool));

        // 创建设备仓库工厂

        // === 创建领域服务 ===
        // 按照依赖关系顺序创建，避免循环依赖

        // === 创建事件系统仓库 ===
        let event_repository: Arc<EventRepository> = Arc::new(tinyiothub_storage::event::EventRepository::new(
            database.as_ref().clone(),
        ));
        let real_time_event_repository: Arc<RealTimeEventRepository> = Arc::new(
            tinyiothub_storage::event::RealTimeEventRepository::new(database.as_ref().clone()),
        );

        // 通知管理器 - 可选服务，依赖数据库
        let notification_manager = Self::create_notification_manager(database.clone()).ok();

        // 创建事件总线
        let event_bus = Arc::new(EventBus::new());

        // 创建报警服务
        let alarm_repository = Arc::new(crate::domains::alarm::AlarmRepository::new(database.clone()));
        let alarm_rule_repository = Arc::new(crate::domains::alarm::AlarmRuleRepository::new(database.clone()));
        let alarm_service = Arc::new(crate::domains::alarm::service::AlarmService::new(
            alarm_repository.clone(),
            alarm_rule_repository,
        ));

        // 创建SSE管理器（带 DeviceCache 用于设备 workspace 查找）
        let sse_manager = Arc::new(SseConnectionManager::new());

        // SSE Token 管理器 — 生成短期令牌用于 SSE 连接认证（替代 URL 中的 JWT）
        let sse_token_manager = Arc::new(tinyiothub_authn::sse_token::SseTokenManager::default());

        // 注册事件处理器将在异步初始化中完成
        // 这里只创建事件总线，处理器注册推迟到 register_event_handlers() 方法

        // 标签仓库（提前创建，供 DeviceService 使用）
        let tag_repository: Arc<crate::domains::thing::tag::TagRepository> =
            Arc::new(tinyiothub_storage::tag::TagRepository::new(database.as_ref().clone()));
        let tag_binding_repository: Arc<crate::domains::thing::tag::TagBindingRepository> = Arc::new(
            tinyiothub_storage::tag::TagBindingRepository::new(database.as_ref().clone()),
        );

        // 基础服务 - 使用事件总线
        let device_repository: Arc<tinyiothub_storage::device::DeviceRepository> =
            Arc::new(tinyiothub_storage::DeviceRepository::new(database.as_ref().clone()));
        let device_service = Arc::new(
            DeviceService::with_event_bus(device_repository, database.clone(), event_bus.clone())
                .with_tag_repository(tag_repository.clone()),
        );
        let device_query_service: Arc<dyn DeviceQueryService> = Arc::new(
            crate::domains::driver::legacy::SqliteDeviceQueryService::new(database.as_ref().clone()),
        );

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
        let template_engine = Arc::new(TemplateEngine::new(template_repository, template_validator));

        // 创建安全事件服务 - 可选服务，依赖配置
        // Note: Secure event service requires async initialization, so we'll create it lazily
        let secure_event_service = OnceCell::new();

        // Redis 客户端 - 可选服务，依赖配置
        let redis = settings.redis.as_ref().and_then(|config| RedisClient::new(&config.url).ok());

        // Agent Runtime - 使用 zeroclaw 内置的 OpenAiCompatibleProvider (MiniMax)
        // Validate minimax config exists (used by get_or_create_agent at provider creation time)
        let minimax_config = settings
            .minimax
            .clone()
            .expect("minimax config is required - set [minimax] in app_settings.toml");
        // Register the agent crate's config ports (minimax provider settings
        // + default model) — the agent crate no longer reads cloud's global
        // config directly (P4-Task22).
        crate::domains::agent::host::ports::set_minimax_settings(crate::domains::agent::host::ports::MinimaxSettings {
            base_url: minimax_config.base_url.clone(),
            auth_token: minimax_config.auth_token.clone(),
            model: minimax_config.model.clone(),
        });
        let agent_settings = settings.agent.clone();
        tracing::info!(
            "TinyIoTHub Agent runtime initialized (memory_backend={}, observer_backend={})",
            agent_settings.memory_backend,
            agent_settings.observer_backend
        );
        // Agent Memory Store
        let memory_store: Arc<MemoryStore> =
            Arc::new(tinyiothub_storage::memory::MemoryStore::new(database.pool().clone()));

        // Task 7 起 AgentPool 不再持有存储句柄（db_pool/memory_store/
        // memory_service）；调用方按请求注入。
        let agent_pool: Arc<AgentPool> = Arc::new(
            AgentPool::new(
                &agent_settings,
                crate::domains::agent::host::autonomous_factory::minimax_provider_factory(),
            )
            .expect("failed to build AgentPool"),
        );

        alarm_service.set_device_cache(device_cache.clone());

        // 用户服务
        let user_repository: Arc<tinyiothub_storage::user::UserRepository> =
            Arc::new(tinyiothub_storage::user::UserRepository::new(database.as_ref().clone()));
        let user_service = Arc::new(crate::domains::user::UserService::new(user_repository));

        // 租户服务
        let tenant_repository: Arc<tinyiothub_storage::tenant::TenantRepository> = Arc::new(
            tinyiothub_storage::tenant::TenantRepository::new(database.as_ref().clone()),
        );
        let tenant_service = Arc::new(crate::domains::tenant::TenantService::new(tenant_repository));

        // 工作空间服务
        let workspace_repository: Arc<tinyiothub_storage::workspace::WorkspaceRepository> = Arc::new(
            tinyiothub_storage::workspace::WorkspaceRepository::new(database.as_ref().clone()),
        );
        let workspace_service = Arc::new(crate::domains::tenant::WorkspaceService::new(workspace_repository));

        // 标签服务
        let tag_service = Arc::new(crate::domains::thing::tag::TagService::new(
            tag_repository.clone(),
            tag_binding_repository,
        ));

        // 角色服务
        let role_repository: Arc<tinyiothub_storage::role::RoleRepository> =
            Arc::new(tinyiothub_storage::role::RoleRepository::new(database.as_ref().clone()));
        let role_service = Arc::new(crate::domains::user::role::RoleService::new(role_repository));

        // 权限服务
        let permission_repository: Arc<tinyiothub_storage::permission::PermissionRepository> = Arc::new(
            tinyiothub_storage::permission::PermissionRepository::new(database.as_ref().clone()),
        );
        let permission_group_repository: Arc<tinyiothub_storage::permission::PermissionGroupRepository> = Arc::new(
            tinyiothub_storage::permission::PermissionGroupRepository::new(database.as_ref().clone()),
        );
        let permission_service = Arc::new(crate::domains::user::permission::PermissionService::new(
            permission_repository,
            permission_group_repository,
        ));

        // Cron 仓库
        let cron_job_repo: Arc<tinyiothub_storage::CronJobRepository> =
            Arc::new(tinyiothub_storage::CronJobRepository::new(database.as_ref().clone()));
        let cron_run_repo: Arc<tinyiothub_storage::CronRunRepository> =
            Arc::new(tinyiothub_storage::CronRunRepository::new(database.as_ref().clone()));

        // 会话服务 - 用于 Agent 聊天会话管理
        let session_repository: Arc<tinyiothub_storage::session::SessionRepository> = Arc::new(
            tinyiothub_storage::session::SessionRepository::new(database.as_ref().clone()),
        );
        let session_service = Arc::new(crate::domains::agent::host::SessionService::new(Arc::clone(
            &session_repository,
        )));

        // === 网关配对服务 ===
        let (mqtt_tx, mqtt_rx) =
            tokio::sync::mpsc::channel::<crate::domains::driver::gateway::service::MqttPublish>(100);
        let (announce_tx, mut announce_rx) =
            tokio::sync::mpsc::channel::<crate::domains::driver::gateway::types::PairingAnnounce>(1000);
        let (data_tx, mut data_rx) =
            tokio::sync::mpsc::channel::<crate::domains::driver::gateway::types::GatewayDataMessage>(1000);
        let pairing_cache = Arc::new(crate::domains::driver::gateway::pairing::PairingCache::new(10000));
        let gateway_service = Arc::new(crate::domains::driver::gateway::service::GatewayService::new(
            database.clone(),
            event_repository.clone(),
            pairing_cache,
            mqtt_tx,
        ));

        // MQTT 客户端
        let mqtt_broker = settings.mqtt.primary.host.clone();
        let mqtt_port = settings.mqtt.primary.port;
        let mqtt_username = settings.mqtt.primary.username.clone().unwrap_or_default();
        let mqtt_password = settings.mqtt.primary.password.clone().unwrap_or_default();
        let throttle_state = Arc::new(crate::domains::event::router::ThrottleState::new(60));
        let thing_event_bus = Arc::new(crate::domains::event::bus::ThingEventBus::new());
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

        let pending_actions: std::sync::Arc<crate::domains::agent::host::tools::thing::PendingActionStore> =
            std::sync::Arc::new(dashmap::DashMap::new());

        // Thing action hooks（G5a）—— agent 侧实现 thing 域 trait，注入给 thing handler
        let thing_action_hooks: Arc<dyn crate::domains::thing::hooks::ThingActionHooks> = Arc::new(
            crate::domains::agent::host::thing_action_hooks::AgentThingActionHooks::new(
                database.pool().clone(),
                pending_actions.clone(),
            ),
        );

        // Agent hooks（G5b）—— agent 侧实现 tenant 域 trait，注入给 workspace 服务
        let agent_hooks: Arc<dyn crate::domains::tenant::hooks::AgentHooks> =
            Arc::new(crate::domains::agent::host::agent_hooks::AgentHooksImpl::new());

        let agent_lifecycle: Arc<AgentPoolLifecycle> = Arc::new(AgentPoolLifecycle {
            pool: agent_pool.clone(),
            db_pool: database.pool().clone(),
        });
        let tag_suggester: Option<Arc<dyn crate::domains::tenant::TagSuggester>> = settings
            .minimax
            .clone()
            .map(|minimax| Arc::new(MinimaxTagSuggester { minimax }) as Arc<dyn crate::domains::tenant::TagSuggester>);
        let jwt_secret = settings.security.jwt.secret.clone();
        let agents_base_dir = crate::shared::paths::agents_base_dir();
        let network_defaults = settings.network.defaults.clone();
        let mqtt_primary = settings.mqtt.primary.clone();
        let sms_config = settings.sms.clone();
        let social_config = settings.social.clone();
        let harmonyos_enabled = settings.harmonyos.enabled;
        let jwt_service = std::sync::Arc::new(tinyiothub_authn::jwt::JwtService::new(
            tinyiothub_authn::jwt::JwtSettings {
                secret: settings.security.jwt.secret.clone(),
                harmonyos_enabled: settings.harmonyos.enabled,
            },
        ));

        Self {
            device_cache,
            agent_lifecycle,
            tag_suggester,
            jwt_secret,
            agents_base_dir,
            network_defaults,
            mqtt_primary,
            marketplace: settings.marketplace.clone(),
            dynamic_drivers_dir: settings.device.drivers.dynamic_drivers_dir.clone(),
            cors_origins: settings.server.cors_origins.clone(),
            event_security: settings.event.security.clone(),
            minimax: settings.minimax.clone(),
            sms_config,
            social_config,
            harmonyos_enabled,
            jwt_service,
            started_at: std::time::SystemTime::now(),
            pending_actions,
            driver_heartbeat_status: std::sync::Arc::new(tokio::sync::RwLock::new(Default::default())),
            driver_heartbeat_config: std::sync::Arc::new(tokio::sync::RwLock::new(Default::default())),
            workspace_access: Arc::new(TenantWorkspaceAccess {
                workspace_service: workspace_service.clone(),
            }),
            system_prompts: Arc::new(settings.agent.system_prompts.clone()),
            database,
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
            memory_service: None,
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
    pub fn set_directive_sink(&mut self, sink: Arc<dyn crate::domains::agent::loop_::thing_agent::DirectiveSink>) {
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
    pub fn get_sse_token_manager(&self) -> &tinyiothub_authn::sse_token::SseTokenManager {
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

        // Get security configuration from AppState config slice (G6)
        let config = self.event_security.clone();

        // Create security factory
        let security_factory = EventSecurityFactory::new(self.database.clone(), config)?;

        // Create secure event service
        let secure_service = security_factory
            .create_secure_event_service(self.event_repository.clone())
            .await?;

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
        let _history_store = Arc::new(NotificationHistoryRepository::new(database.clone()));

        // Create notification rule repository
        let rule_repo = Arc::new(NotificationRuleRepository::new(database));

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
    pub async fn new_for_testing(settings: &tinyiothub_core::config::ApplicationSettings) -> Self {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database_url = format!("sqlite://{}", db_path.to_str().unwrap());
        let pool = sqlx::SqlitePool::connect(&database_url).await.unwrap();

        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();

        let device_cache = Arc::new(DeviceCache::new());

        Self::new(device_cache, pool, settings)
    }
}
// ============================================================================
// P4-Task16: auth domain slice + seam adapters
// ============================================================================

/// Map the cloud user entity to the auth crate's byte-identical mirror.
fn auth_user_from_user(user: tinyiothub_storage::user::User) -> crate::domains::auth::user_store::AuthUser {
    crate::domains::auth::user_store::AuthUser {
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
    pub service: Arc<crate::domains::user::UserService>,
}

#[async_trait::async_trait]
impl crate::domains::auth::user_store::AuthUserStore for UserServiceAuthAdapter {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<crate::domains::auth::user_store::AuthUser>, String> {
        self.service
            .authenticate(username, password)
            .await
            .map_err(|e| e.to_string())
            .map(|o| o.map(auth_user_from_user))
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<crate::domains::auth::user_store::AuthUser>, String> {
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
        self.service
            .exists_by_username(username)
            .await
            .map_err(|e| e.to_string())
    }

    async fn exists_by_phone(&self, phone: &str) -> Result<bool, String> {
        self.service.exists_by_phone(phone).await.map_err(|e| e.to_string())
    }

    async fn exists_by_email(&self, email: &str) -> Result<bool, String> {
        self.service.exists_by_email(email).await.map_err(|e| e.to_string())
    }

    async fn create_user(
        &self,
        request: &crate::domains::auth::user_store::AuthCreateUserRequest,
    ) -> Result<crate::domains::auth::user_store::AuthUser, String> {
        let create_request = tinyiothub_core::models::user::CreateUserRequest {
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
/// stays in cloud (`shared::initialization`, entangled with the agent
/// plane; P4-Task24 boundary — see `tinyiothub_admin::legacy`); this
/// adapter carries the AppState the function needs.
pub struct SystemWorkspaceBootstrap {
    pub state: AppState,
}

#[async_trait::async_trait]
impl crate::domains::auth::bootstrap::WorkspaceBootstrap for SystemWorkspaceBootstrap {
    async fn ensure_user_has_workspace(&self, user_id: &str) -> Result<(), String> {
        crate::shared::initialization::ensure_user_has_workspace(&self.state, user_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// SSE token issuer seam: the manager stays in cloud (shared with the event
/// plane's SSE handlers).
impl crate::domains::auth::sse::SseTokenIssuer for tinyiothub_authn::sse_token::SseTokenManager {
    fn generate_token(&self, user_id: &str, workspace_id: &str) -> String {
        tinyiothub_authn::sse_token::SseTokenManager::generate_token(self, user_id, workspace_id)
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
impl crate::domains::user::RoleChecker for EventSecurityRoleChecker {
    async fn check_role(&self, user_id: &str, role: &str) -> Result<bool, String> {
        crate::shared::error_handling::AuthHelper::check_role(&self.state, user_id, role).await
    }
}
// ============================================================================
// P4-Task17b: tenant domain slice + seam adapters
// ============================================================================

/// Agent-lifecycle seam adapter: workspace create/delete provisions and
/// tears down the per-workspace Agent via cloud's `AgentPool` (agent plane,
/// not yet extracted).
pub struct AgentPoolLifecycle {
    pub pool: Arc<crate::domains::agent::host::agent::AgentPool>,
    /// AgentPool 不持有存储句柄（Task 7）—— 生命周期 CRUD 的 db 由 cloud 注入。
    pub db_pool: sqlx::SqlitePool,
}

impl AgentPoolLifecycle {
    pub async fn create_agent(&self, workspace_id: &str, name: &str) -> Result<String, String> {
        self.pool
            .create_agent(
                &self.db_pool,
                &crate::domains::agent::host::shared::AgentConfig {
                    workspace_id: workspace_id.to_string(),
                    name: name.to_string(),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<(), String> {
        self.pool
            .delete_agent(&self.db_pool, agent_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Tag-suggester seam adapter: prompt construction, provider creation and
/// response parsing, byte-identical to the former inline workspace handler
/// code. Keeps the zeroclaw provider type out of the tenant crate.
/// Holds its MiniMax config slice (G6 — no process-global config reads).
pub struct MinimaxTagSuggester {
    pub minimax: tinyiothub_core::config::MinimaxConfig,
}

#[async_trait::async_trait]
impl crate::domains::tenant::TagSuggester for MinimaxTagSuggester {
    async fn suggest(
        &self,
        name: &str,
        resource_type_label: &str,
        description: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let model = self.minimax.model.clone();

        let prompt = format!(
            "你是一个资源标签生成助手。根据用户提供的资源信息，生成 3-5 个简洁的中文标签。\n\
             严格只返回逗号分隔的标签，不要任何解释或额外文字。\n\n\
             示例输出：3D模型, 工厂, 设备, 车间\n\n\
             资源信息：\n- 文件名：{}\n- 资源类型：{}{}",
            name,
            resource_type_label,
            description.map_or(String::new(), |d| format!("\n- 描述：{}", d)),
        );

        let provider = crate::shared::config::create_minimax_provider(&self.minimax).map_err(|e| {
            tracing::error!("Failed to create AI provider: {}", e);
            "AI 服务初始化失败".to_string()
        })?;

        let response = provider
            .chat_with_system(None, &prompt, &model, Some(0.3))
            .await
            .map_err(|e| {
                tracing::error!("AI tag generation failed: {}", e);
                "AI 生成标签失败，请稍后重试".to_string()
            })?;

        let tags: Vec<String> = response
            .split([',', '，', '、', '\n'])
            .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|t| !t.is_empty() && t.len() < 20)
            .collect();

        if tags.is_empty() {
            Err("AI 未生成有效标签".to_string())
        } else {
            Ok(tags)
        }
    }
}

/// Workspace-access seam adapter: the agent crate's `WorkspaceAccess` port
/// over the tenant crate's `WorkspaceService`.
pub struct TenantWorkspaceAccess {
    pub workspace_service: Arc<crate::domains::tenant::WorkspaceService>,
}

impl TenantWorkspaceAccess {
    pub async fn workspace_tenant_id(&self, workspace_id: &str) -> Result<Option<String>, String> {
        self.workspace_service
            .find_by_id(workspace_id)
            .await
            .map(|ws| ws.map(|w| w.tenant_id))
            .map_err(|e| e.to_string())
    }
}
// ============================================================================
// P4-Task23: mcp domain slice
// ============================================================================

/// Mcp 域状态切片注入（G7）—— mcp handlers 萃取 `State<McpState>`，路由器
/// 对组合态 `S` 泛型（`McpState: FromRef<S>`）；此处是唯一的
/// AppState → McpState 派生点。全局 MCP_REGISTRY 持有的 `Arc<McpState>`
/// 也经此派生（router.rs / main.rs 启动路径）。
impl axum::extract::FromRef<AppState> for crate::domains::mcp::McpState {
    fn from_ref(state: &AppState) -> Self {
        crate::domains::mcp::McpState {
            database: state.database.clone(),
            device_cache: state.device_cache.clone(),
            tag_repository: state.tag_repository.clone(),
            event_bus: state.event_bus.clone(),
            data_server: state.data_server.clone(),
            template_engine: state.template_engine.clone(),
            cron_job_repo: state.cron_job_repo.clone(),
            cron_run_repo: state.cron_run_repo.clone(),
            alarm_service: state.alarm_service.clone(),
            tenant_service: state.tenant_service.clone(),
        }
    }
}

// ============================================================================
// P4-Task24: admin domain slice + admin-role seam adapter
// ============================================================================

/// Admin-role seam adapter: the admin crate's privileged-operation guard
/// routes through cloud's event-security plane (`AuthHelper` →
/// `SecureEventService`), which stays in cloud. Same shape as
/// `EventSecurityRoleChecker` (P4-Task17a).
pub struct EventSecurityAdminRoleChecker {
    pub state: AppState,
}

#[async_trait::async_trait]
impl crate::domains::admin::AdminRoleChecker for EventSecurityAdminRoleChecker {
    async fn require_admin_role(&self, user_id: &str, operation: &str) -> Result<(), String> {
        crate::shared::error_handling::AuthHelper::require_admin_role(&self.state, user_id, operation)
            .await
            .map_err(|_| "Access denied: admin role required".to_string())
    }
}

/// Admin 域状态切片注入（G7）—— admin handlers 萃取 `State<AdminState>`，
/// 路由器对组合态 `S` 泛型（`AdminState: FromRef<S>`）；此处是唯一的
/// AppState → AdminState 派生点。`role_checker` 每次萃取新建，与原先
/// handler 内按需构造 `EventSecurityAdminRoleChecker` 的语义一致。
impl axum::extract::FromRef<AppState> for crate::domains::admin::AdminState {
    fn from_ref(state: &AppState) -> Self {
        crate::domains::admin::AdminState {
            database: state.database.clone(),
            device_cache: state.device_cache.clone(),
            tag_repository: state.tag_repository.clone(),
            tag_service: state.tag_service.clone(),
            event_bus: state.event_bus.clone(),
            event_repository: state.event_repository.clone(),
            data_server: state.data_server.clone(),
            device_query_service: state.device_query_service.clone(),
            monitoring_service: state.monitoring_service.clone(),
            performance_service: state.performance_service.clone(),
            trace_service: state.trace_service.clone(),
            workspace_service: state.workspace_service.clone(),
            tenant_service: state.tenant_service.clone(),
            cron_job_repo: state.cron_job_repo.clone(),
            cron_run_repo: state.cron_run_repo.clone(),
            sysinfo_system: state.sysinfo_system.clone(),
            role_checker: Arc::new(EventSecurityAdminRoleChecker { state: state.clone() }),
            network_defaults: state.network_defaults.clone(),
            mqtt_primary: state.mqtt_primary.clone(),
            started_at: state.started_at,
        }
    }
}

// ============================================================================
// P4-Task25: agent domain slice
// ============================================================================

/// Agent 域状态切片注入（G7）—— agent host/chat handlers 萃取
/// `State<AgentState>`，路由器对组合态 `S` 泛型（`AgentState: FromRef<S>`）；
/// 此处是唯一的 AppState → AgentState 派生点。
impl axum::extract::FromRef<AppState> for crate::domains::agent::AgentState {
    fn from_ref(state: &AppState) -> Self {
        crate::domains::agent::AgentState {
            database: state.database.clone(),
            workspace_service: state.workspace_service.clone(),
            workspace_access: state.workspace_access.clone(),
            directive_sink: state.directive_sink.clone(),
            heartbeat_runner: state.heartbeat_runner.clone(),
            orchestrator: state.orchestrator.clone(),
            memory_service: state.memory_service.clone(),
            memory_store: state.memory_store.clone(),
            agent_pool: state.agent_pool.clone(),
            session_service: state.session_service.clone(),
            system_prompts: state.system_prompts.clone(),
        }
    }
}
