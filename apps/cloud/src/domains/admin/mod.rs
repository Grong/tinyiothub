//! Admin domain module — system, monitoring, batch, jobs, open API.
//!
//! ## 设计不变量
//! - 系统/监控/批处理/开放 API；调度接 scheduler crate（admin→scheduler）

// Admin domain module (P4-Task24) — the final modules/ extraction, now a
// domain module of cloud (G series).
//
// Covers the platform-administration API surface formerly under
// `cloud/src/modules/{system,monitoring,batch,jobs,open}` plus the device
// management-plane handlers (`modules::device::handler`):
//   device/     — /devices management-plane handlers (profile, properties,
//                 commands, traces, monitoring, dashboard, 410 stubs)
//   system/     — /system configuration + features + time tasks
//   monitoring/ — /monitoring dashboard stats, health, logs, metrics
//   batch/      — /batch batch command operations
//   jobs/       — /jobs task-management API over tinyiothub_scheduler
//   open/       — /open third-party integration surface (X-API-Key auth)
//
// Handlers extract `State<AdminState>` and every exported router is generic
// over the composition state `S` with `AdminState: FromRef<S>` (SEP
// contract, P4-Task15 pilot).

use std::sync::Arc;

use crate::domains::driver::legacy::{
    DeviceMonitoringService, DevicePerformanceService, DeviceQueryService, DeviceService,
};
use crate::domains::thing::legacy::trace::DeviceTraceService;
use crate::shared::error::Error;
use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device_property::DeviceProperty;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_storage::event::EventRepository;
use tinyiothub_storage::{Db, DeviceRepository, cache::DeviceCache};

pub mod batch;
pub mod device;
pub mod jobs;
pub mod legacy;
pub mod monitoring;
pub mod open;
pub mod system;

/// Admin role-check port — the admin handlers' privileged-operation guard
/// routes through cloud's event-security plane (`AuthHelper` →
/// `SecureEventService`), which stays in the composition layer. Cloud
/// injects the adapter via `FromRef<AppState> for AdminState`
/// (same seam shape as `crate::domains::user::RoleChecker`, P4-Task17a).
#[async_trait::async_trait]
pub trait AdminRoleChecker: Send + Sync {
    async fn require_admin_role(&self, user_id: &str, operation: &str) -> Result<(), String>;
}

/// Admin domain state slice (G7) — the fields of cloud's `AppState` the
/// admin handlers actually consume. The composition layer (cloud) derives it
/// via `FromRef<AppState>`; this crate never names `AppState`.
#[derive(Clone)]
pub struct AdminState {
    /// 数据库连接池
    pub db: Arc<Db>,
    /// 设备内存缓存
    pub device_cache: Arc<DeviceCache>,
    /// 标签仓库 - 用于设备服务的标签关联
    pub tag_repository: Arc<crate::domains::thing::tag::TagRepository>,
    /// 标签服务 - 设备 profile 的标签加载
    pub tag_service: Arc<crate::domains::thing::tag::TagService>,
    /// 事件总线 - 属性变更事件发布（update_device_property_value）
    pub event_bus: Arc<EventBus>,
    /// 事件历史仓库 - 设备 profile 的最近事件查询
    pub event_repository: Arc<EventRepository>,
    /// 数据服务器 - 设备命令执行
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    /// 设备查询服务 - dashboard 报表和只读查询
    pub device_query_service: Arc<dyn DeviceQueryService>,
    /// 设备监控服务 - 状态监控和指标
    pub monitoring_service: Arc<DeviceMonitoringService>,
    /// 设备性能服务 - 性能分析和告警
    pub performance_service: Arc<DevicePerformanceService>,
    /// 设备追踪服务 - 操作日志和审计
    pub trace_service: Arc<DeviceTraceService>,
    /// 工作空间服务 - workspace 解析（resolve_workspace）与 open API
    pub workspace_service: Arc<crate::domains::tenant::WorkspaceService>,
    /// 租户服务 - open API 的 API Key 校验与配额
    pub tenant_service: Arc<crate::domains::tenant::TenantService>,
    /// 缓存的系统信息对象，避免每次请求重新扫描
    pub sysinfo_system: Arc<std::sync::Mutex<sysinfo::System>>,
    /// 管理员角色检查（event-security seam）
    pub role_checker: Arc<dyn AdminRoleChecker>,
    /// 网络默认配置切片（/system/network）
    pub network_defaults: tinyiothub_core::config::NetworkDefaultsConfig,
    /// MQTT 主 broker 配置切片（/system/mqtt）
    pub mqtt_primary: tinyiothub_core::config::MqttBrokerConfig,
    /// 进程启动时间（G3，health/metrics uptime）
    pub started_at: std::time::SystemTime,
}

impl AdminState {
    /// 获取数据库实例
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// 获取数据库连接池
    pub fn db_pool(&self) -> sqlx::SqlitePool {
        self.db.pool().clone()
    }

    /// 获取数据服务器
    pub fn data_server(&self) -> Option<&tinyiothub_runtime::DataServer> {
        self.data_server.as_ref().map(|ds| ds.as_ref())
    }

    /// 获取设备（从缓存读取实时状态）
    pub fn get_device(&self, device_id: &str) -> Option<Device> {
        self.device_cache.get(device_id)
    }

    /// 通过设备名称和属性名称获取属性
    pub fn get_device_prop_by_name(&self, device_name: &str, property_name: &str) -> Option<DeviceProperty> {
        self.device_cache.get_by_name(device_name).and_then(|d| {
            d.properties
                .as_ref()
                .and_then(|props| props.iter().find(|p| p.name == property_name).cloned())
        })
    }

    /// 租户作用域设备仓储（AppState::device_repo_for 的域内移植）
    fn device_repo_for(&self, workspace_id: String) -> Arc<DeviceRepository> {
        Arc::new(DeviceRepository::new(self.db.as_ref().clone()).for_workspace(workspace_id))
    }

    /// Returns a tenant-scoped device service.
    ///
    /// AppState 同名方法的域内移植：workspace_id 为 None 时记录安全警告并
    /// 使用空 workspace（查不到任何设备），绝不回退到未隔离的原始仓库。
    pub fn tenant_device_service(&self, workspace_id: &Option<String>) -> Arc<DeviceService> {
        let ws_id = workspace_id.clone().unwrap_or_else(|| {
            tracing::warn!(
                "[SECURITY] tenant_device_service called with workspace_id=None — \
                 using empty workspace (no devices will be returned). \
                 This indicates a bug: WorkspaceScope should always resolve to a workspace_id."
            );
            String::new()
        });
        let repository = self.device_repo_for(ws_id);

        Arc::new(
            DeviceService::with_event_bus(repository, self.db.clone(), self.event_bus.clone())
                .with_tag_repository(self.tag_repository.clone()),
        )
    }

    /// Resolve workspace ID for a tenant.
    ///
    /// AppState 同名方法的域内移植：显式 workspace_id 直接返回，否则查询该
    /// 租户的默认工作空间。
    pub async fn resolve_workspace(&self, tenant_id: &str, explicit: Option<String>) -> Result<String, (i32, String)> {
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

    /// 更新设备属性值
    ///
    /// AppState 同名方法的域内移植：验证 + 发布 PropertyChange 事件解耦，
    /// DataServer 作为 EventHandler 接收事件并更新 DeviceCache。
    pub async fn update_device_property_value(
        &self,
        workspace_id: &str,
        device_id: &str,
        property_id: &str,
        value: &str,
    ) -> Result<(), Error> {
        use tinyiothub_core::models::event::{ContentElement, EventSource, RichContent, TextFormat};

        // 1. 验证设备存在且属于指定的workspace
        let tenant_device_service = self.tenant_device_service(&Some(workspace_id.to_string()));
        let device = match tenant_device_service.get_device_by_id(device_id).await? {
            Some(d) => d,
            None => return Err(Error::NotFound),
        };

        // 2. 验证属性存在且属于该设备
        let property = match tinyiothub_storage::find_device_property_by_id(self.db(), property_id).await {
            Ok(Some(p)) if p.device_id == device_id => p,
            Ok(Some(_)) => {
                return Err(Error::ValidationError("Property does not belong to device".to_string()));
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

        self.event_bus
            .publish(event)
            .await
            .map_err(|e| Error::IOError(e.to_string()))?;

        Ok(())
    }
}
