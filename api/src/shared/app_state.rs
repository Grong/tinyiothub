use crate::{
    application::data_context::DataContext,
    domain::{
        device::{
            monitoring_service::DeviceMonitoringService,
            performance_service::DevicePerformanceService, service::DeviceService,
            trace_service::DeviceTraceService,
        },
        event::{
            repositories::{EventRepository, RealTimeEventRepository},
            services::notification_service::NotificationManager,
        },
        template::{
            engine::TemplateEngine, repository::TemplateRepository, validator::TemplateValidator,
        },
    },
    dto::entity::DeviceProperty,
    infrastructure::event::{
        channels::NotificationChannelFactory,
        handlers::{PersistenceEventHandler, RealTimeStatusHandler, SseEventHandler},
        security::{EventSecurityFactory, SecureEventService},
        EventBus, SseConnectionManager,
    },
    infrastructure::persistence::repositories::{
        NotificationHistoryRepositoryImpl, SqliteEventRepository, SqliteRealTimeEventRepository,
    },
    infrastructure::persistence::Database,
    shared::error::Error,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// 应用程序状态 - 使用 Axum 推荐的依赖注入模式
///
/// 这种设计遵循以下最佳实践：
/// 1. 单一状态类型 - Axum with_state 只支持一个状态
/// 2. 服务预创建 - 避免每次请求重复创建服务
/// 3. Arc 共享 - 多线程安全的引用计数
/// 4. 清晰的依赖关系 - 所有依赖在启动时解析
#[derive(Clone)]
pub struct AppState {
    /// 核心数据上下文 - 设备缓存和基础操作
    pub data_context: Arc<DataContext>,

    /// 数据库连接池
    pub database: Arc<Database>,

    /// === 应用服务层 ===
    /// 数据服务器 - 设备数据采集和命令执行
    pub data_server: Option<Arc<crate::application::data_server::DataServer>>,

    /// === 领域服务层 ===
    /// 设备基础服务 - CRUD 操作
    pub device_service: Arc<DeviceService>,

    /// 设备监控服务 - 状态监控和指标
    pub monitoring_service: Arc<DeviceMonitoringService>,

    /// 设备性能服务 - 性能分析和告警
    pub performance_service: Arc<DevicePerformanceService>,

    /// 设备追踪服务 - 操作日志和审计
    pub trace_service: Arc<DeviceTraceService>,

    /// 模板引擎 - 设备模板管理
    pub template_engine: Arc<TemplateEngine>,

    /// 通知管理器 - 事件通知和告警
    pub notification_manager: Option<Arc<NotificationManager>>,

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
    pub alarm_service: Arc<crate::domain::alarm::AlarmService>,
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
    pub fn new(data_context: Arc<DataContext>) -> Self {
        // 创建共享的数据库连接
        let database = Arc::new(data_context.database());

        // === 创建领域服务 ===
        // 按照依赖关系顺序创建，避免循环依赖

        // === 创建事件系统仓库 ===
        let event_repository: Arc<dyn EventRepository> =
            Arc::new(SqliteEventRepository::new(database.as_ref().clone()));
        let real_time_event_repository: Arc<dyn RealTimeEventRepository> = Arc::new(
            SqliteRealTimeEventRepository::new(database.as_ref().clone()),
        );

        // 通知管理器 - 可选服务，依赖数据库
        let notification_manager = Self::create_notification_manager(database.clone()).ok();

        // 创建事件总线
        let event_bus = Arc::new(EventBus::new());

        // 创建报警服务
        use crate::domain::alarm::AlarmService;
        use crate::infrastructure::persistence::repositories::{
            AlarmRepositoryImpl, AlarmRuleRepositoryImpl,
        };

        let alarm_repository = Arc::new(AlarmRepositoryImpl::new(database.clone()));
        let alarm_rule_repository = Arc::new(AlarmRuleRepositoryImpl::new(database.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repository, alarm_rule_repository));

        // 创建SSE管理器
        let sse_manager = Arc::new(SseConnectionManager::new());

        // 注册事件处理器将在异步初始化中完成
        // 这里只创建事件总线，处理器注册推迟到 register_event_handlers() 方法

        // 基础服务 - 使用事件总线
        let device_service = Arc::new(DeviceService::with_event_bus(
            database.clone(),
            event_bus.clone(),
        ));

        // 监控服务 - 依赖数据库和上下文
        let monitoring_service = Arc::new(DeviceMonitoringService::new(
            database.clone(),
            data_context.clone(),
        ));

        // 性能服务 - 依赖数据库和上下文
        let performance_service = Arc::new(DevicePerformanceService::new(
            database.clone(),
            data_context.clone(),
        ));

        // 追踪服务 - 依赖数据库和上下文
        let trace_service = Arc::new(DeviceTraceService::new(
            database.clone(),
            data_context.clone(),
        ));

        // 模板引擎 - 复合服务，依赖仓库和验证器
        let template_repository = Arc::new(TemplateRepository::new(
            database.clone(),
            PathBuf::from("templates"),
        ));
        let template_validator = Arc::new(TemplateValidator::new());
        let template_engine =
            Arc::new(TemplateEngine::new(template_repository, template_validator));

        // 创建SSE管理器
        let sse_manager = Arc::new(SseConnectionManager::new());

        // 创建安全事件服务 - 可选服务，依赖配置
        // Note: Secure event service requires async initialization, so we'll create it lazily
        let secure_event_service = OnceCell::new();

        Self {
            data_context,
            database,
            data_server: None, // DataServer 由 ServiceManager 设置
            device_service,
            monitoring_service,
            performance_service,
            trace_service,
            template_engine,
            notification_manager,
            event_bus,
            event_repository,
            real_time_event_repository,
            sse_manager,
            secure_event_service,
            alarm_service,
        }
    }

    /// 设置数据服务器（由 ServiceManager 调用）
    pub fn set_data_server(
        &mut self,
        data_server: Arc<crate::application::data_server::DataServer>,
    ) {
        self.data_server = Some(data_server);
    }

    /// 获取数据服务器
    pub fn data_server(&self) -> Option<&crate::application::data_server::DataServer> {
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

    // === 兼容性方法 ===
    // 这些方法提供对 DataContext 的直接访问，
    // 用于渐进式迁移，避免一次性修改所有代码

    /// 通过设备名称和属性名称获取属性
    pub fn get_device_prop_by_name(
        &self,
        device_name: &str,
        property_name: &str,
    ) -> Option<DeviceProperty> {
        self.data_context
            .get_device_prop_by_name(device_name, property_name)
    }

    /// 更新设备属性值
    pub async fn update_device_property_value(
        &self,
        device_id: &str,
        property_id: &str,
        value: &str,
    ) -> Result<(), Error> {
        self.data_context
            .as_ref()
            .update_device_property_value(device_id, property_id, value, Some(&self.event_bus))
            .await
    }

    /// 获取设备
    pub fn get_device(&self, device_id: &str) -> Option<crate::dto::entity::Device> {
        self.data_context.get_device(device_id)
    }

    /// 设置设备
    pub fn set_device(&self, device: crate::dto::entity::Device) {
        self.data_context.set_device(device)
    }

    /// 获取模板引擎
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
    }

    /// 获取通知管理器
    pub fn get_notification_manager(&self) -> Option<&NotificationManager> {
        self.notification_manager.as_ref().map(|nm| nm.as_ref())
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

        // Create default security configuration
        let config = crate::infrastructure::event::security::EventSecurityConfig::default();

        // Create security factory
        let security_factory = EventSecurityFactory::new(self.database.clone(), config)?;

        // Create secure event service
        let secure_service = security_factory
            .create_secure_event_service(self.event_repository.clone())
            .await?;

        // Store in OnceCell
        let service_arc = Arc::new(secure_service);
        match self.secure_event_service.set(service_arc) {
            Ok(_) => self.secure_event_service.get()
                .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to get secure event service"
                    ))
                })
                .map(|s| s.as_ref()),
            Err(_) => {
                // Another thread already initialized it
                self.secure_event_service.get()
                    .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to get secure event service"
                        ))
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

        // Create notification manager
        let mut notification_manager = NotificationManager::new();

        // Register notification channels
        let channels = NotificationChannelFactory::create_all_channels();
        for channel in channels {
            notification_manager.register_channel(channel);
        }

        // Load notification rules from database
        // For now, we'll start with empty rules - they can be loaded later
        // In a full implementation, we would load rules from the database here

        Ok(Arc::new(notification_manager))
    }

    /// Create AppState for testing
    #[cfg(test)]
    pub async fn new_for_testing() -> Self {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database_url = format!("sqlite:{}", db_path.to_str().unwrap());
        let pool = sqlx::SqlitePool::connect(&database_url).await.unwrap();

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let data_context = DataContext::new_with_pool(pool).await.unwrap();

        Self::new(data_context)
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
