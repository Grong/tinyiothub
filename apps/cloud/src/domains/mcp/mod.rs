//! mcp 领域：内嵌 MCP server 与工具注册（F2 自 crates/mcp 回流，relay 范式）。
//!
//! 工具 handler 经全局 registry 持有 `Arc<McpState>`（G7 FromRef 切片），
//! 禁止任何全局 AppState 单例；本 crate 不命名 cloud 的 `AppState`。

pub mod agent_bridge;
pub mod handlers;
#[cfg(test)]
pub mod tests;
pub mod tool_metadata;
pub mod tool_registry;
pub mod tools;

use std::sync::Arc;

use tokio::sync::RwLock;

pub use handlers::{ToolCallParams, create_router};
pub use tool_registry::*;

use crate::domains::driver::legacy::DeviceService;
use crate::domains::thing::template::TemplateEngine;
use crate::shared::error::Error;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_storage::{Db, DeviceRepository, cache::DeviceCache};
use tool_registry::HandlerRegistry;

/// Mcp domain state slice (G7) — the fields of cloud's `AppState` the mcp
/// handlers and tool handlers actually consume. The composition layer (cloud)
/// derives it via `FromRef<AppState>`; this crate never names `AppState`.
#[derive(Clone)]
pub struct McpState {
    /// 数据库连接池 - thing 工具的属性/命令查询
    pub db: Arc<Db>,
    /// 设备内存缓存 - thing 工具的实时状态合并
    pub device_cache: Arc<DeviceCache>,
    /// 标签仓库 - 租户设备服务的标签关联
    pub tag_repository: Arc<crate::domains::thing::tag::TagRepository>,
    /// 事件总线 - 属性变更事件发布（update_device_property_value）
    pub event_bus: Arc<EventBus>,
    /// 数据服务器 - send_command 工具
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    /// 模板引擎 - create_thing 从模板创建设备
    pub template_engine: Arc<TemplateEngine>,
    /// Cron 任务仓库 - schedule 工具
    pub cron_job_repo: Arc<tinyiothub_storage::CronJobRepository>,
    /// Cron 执行记录仓库 - delete_schedule 级联清理
    pub cron_run_repo: Arc<tinyiothub_storage::CronRunRepository>,
    /// 报警服务 - alarm 工具（mcp→alarm 边）
    pub alarm_service: Arc<crate::domains::alarm::service::AlarmService>,
    /// 租户服务 - X-API-Key 校验
    pub tenant_service: Arc<crate::domains::tenant::TenantService>,
}

impl McpState {
    /// 获取数据库实例
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// 获取数据服务器
    pub fn data_server(&self) -> Option<&tinyiothub_runtime::DataServer> {
        self.data_server.as_ref().map(|ds| ds.as_ref())
    }

    /// 获取模板引擎
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
    }

    /// 租户作用域设备仓储（AppState::device_repo_for 的域内移植）
    pub fn device_repo_for(&self, workspace_id: String) -> Arc<DeviceRepository> {
        Arc::new(DeviceRepository::new(self.db.as_ref().clone()).for_workspace(workspace_id))
    }

    /// 获取租户感知的设备服务（接受字符串 workspace_id）
    ///
    /// AppState 同名方法的域内移植。
    pub fn tenant_device_service_str(&self, workspace_id: &str) -> Arc<DeviceService> {
        let repository = self.device_repo_for(workspace_id.to_string());
        Arc::new(DeviceService::new(repository, self.db.clone()).with_tag_repository(self.tag_repository.clone()))
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

/// Create the MCP router (mounted at `/mcp` by the composition layer).
///
/// Generic over the composition state `S` — axum 0.8 `nest()` requires
/// matching state types; `State<McpState>` extraction works for any
/// `S: FromRef<McpState>` (SEP contract, P4-Task15 pilot).
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    McpState: axum::extract::FromRef<S>,
{
    handlers::create_router()
}

/// Global MCP tool registry — process-wide, set-once at startup.
/// Deliberate global per the G-series plan (sanctioned, not an oversight).
static MCP_REGISTRY: std::sync::OnceLock<Arc<RwLock<HandlerRegistry>>> = std::sync::OnceLock::new();

/// Initialize the global MCP registry with the domain state slice.
///
/// The first call wins (OnceLock semantics); tool handlers are (re-)built
/// from the state passed to [`register_tools`].
pub fn init_mcp_registry(state: Option<Arc<McpState>>) -> Arc<RwLock<HandlerRegistry>> {
    MCP_REGISTRY
        .get_or_init(|| Arc::new(RwLock::new(HandlerRegistry::new(state))))
        .clone()
}

/// Get the global MCP registry (returns None if not yet initialized)
pub fn get_mcp_registry() -> Option<Arc<RwLock<HandlerRegistry>>> {
    MCP_REGISTRY.get().cloned()
}

/// Register tools to the global registry.
///
/// `state` is injected into every tool handler that needs it. Pass `None`
/// in tests: handlers then behave exactly as they did before state injection
/// when the global state was unset ("AppState not initialized").
pub async fn register_tools(state: Option<Arc<McpState>>) {
    let registry = init_mcp_registry(state.clone());
    let mut reg = registry.write().await;

    // Thing tools (7)
    reg.register(crate::domains::mcp::tools::device::DeviceProfileHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::SearchDevicesHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::DevicePropertyGetHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::WritePropertiesHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::DeviceCommandHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::CreateDeviceHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::device::DeleteDeviceHandler::new(
        state.clone(),
    ));

    // Driver tools (2)
    reg.register(crate::domains::mcp::tools::driver::ListDriversHandler);
    reg.register(crate::domains::mcp::tools::driver::TestDriverHandler::new(
        state.clone(),
    ));

    // Job tools (4)
    reg.register(crate::domains::mcp::tools::job::ListSchedulesHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::job::CreateScheduleHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::job::UpdateScheduleHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::job::DeleteScheduleHandler::new(
        state.clone(),
    ));

    // Alarm tools (3)
    reg.register(crate::domains::mcp::tools::alarm_mcp::AlarmListHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::alarm_mcp::AlarmAcknowledgeHandler::new(
        state.clone(),
    ));
    reg.register(crate::domains::mcp::tools::alarm_mcp::AlarmRuleAddHandler::new(
        state.clone(),
    ));

    tracing::info!("Registered {} MCP tools: 7 thing, 2 driver, 4 job, 3 alarm", 16);
}
