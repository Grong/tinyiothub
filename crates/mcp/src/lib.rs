// MCP API Module
// Embedded MCP server for AI Agent integration

use std::sync::Arc;

use tokio::sync::RwLock;

pub mod agent_bridge;
pub mod handlers;
pub mod tool_metadata;
pub mod tool_registry;
pub mod tools;

#[cfg(test)]
mod tests; // Integration tests in tests/ directory

// Re-export types for use in other modules
pub use handlers::{ToolCallParams, create_router};
pub use tool_metadata::{IoTToolMetadata, PermissionLevel};
pub use tool_registry::{HandlerRegistry, ToolError, ToolHandler, ToolMetadata};

use tinyiothub_driver::legacy::DeviceService;
use tinyiothub_storage::{Database, DeviceRepositoryFactory, cache::DeviceCache};
use tinyiothub_thing::template::TemplateEngine;

/// MCP domain state slice (P4-Task23) — the fields of cloud's `AppState`
/// the MCP registry, HTTP handlers and tool handlers actually consume.
/// The composition layer (cloud) derives it via `FromRef<AppState>`; this
/// crate never names `AppState`.
#[derive(Clone)]
pub struct McpState {
    /// 数据库连接池
    pub database: Arc<Database>,
    /// 设备内存缓存
    pub device_cache: Arc<DeviceCache>,
    /// 设备仓库工厂 - 用于创建租户感知的设备仓库
    pub device_repository_factory: Arc<DeviceRepositoryFactory>,
    /// 标签仓库 - 用于设备服务的标签关联
    pub tag_repository: Arc<dyn tinyiothub_thing::tag::TagRepository>,
    /// 模板引擎 - 设备模板管理
    pub template_engine: Arc<TemplateEngine>,
    /// 数据服务器 - 设备数据采集和命令执行
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    /// 报警服务 - 报警规则和报警管理
    pub alarm_service: Arc<tinyiothub_alarm::AlarmService>,
    /// 租户服务 - API Key 认证（X-API-Key 头校验）
    pub tenant_service: Arc<tinyiothub_tenant::TenantService>,
    /// Cron 任务仓库
    pub cron_job_repo: Arc<dyn tinyiothub_storage::traits::cron::CronJobRepository>,
    /// Cron 执行记录仓库
    pub cron_run_repo: Arc<dyn tinyiothub_storage::traits::cron::CronRunRepository>,
    /// 事件总线 - 属性变更事件发布（update_device_property_value）
    pub event_bus: Arc<tinyiothub_runtime::event_bus::EventBus>,
}

impl McpState {
    /// 获取数据库实例
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// 获取模板引擎
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
    }

    /// 获取数据服务器
    pub fn data_server(&self) -> Option<&tinyiothub_runtime::DataServer> {
        self.data_server.as_ref().map(|ds| ds.as_ref())
    }

    /// 获取租户感知的设备服务（接受字符串 workspace_id）
    ///
    /// 与 AppState 的 `_str` 变体语义一致：不挂事件总线。alarm 确认路径
    /// 只读调用 `get_device_by_id`（不发布事件），行为与原先
    /// `tenant_device_service(&Some(..))` 完全一致。
    pub fn tenant_device_service_str(&self, workspace_id: &str) -> Arc<DeviceService> {
        let repository =
            self.device_repository_factory.create_for_workspace(workspace_id.to_string());
        Arc::new(
            DeviceService::new(repository, self.database.clone())
                .with_tag_repository(self.tag_repository.clone()),
        )
    }

    /// 更新设备属性值
    ///
    /// AppState 同名方法的域内移植（P4-Task23）：验证 + 发布 PropertyChange
    /// 事件解耦，DataServer 作为 EventHandler 接收事件并更新 DeviceCache。
    /// 错误以 String 返回（唯一调用方 WritePropertiesHandler 只做格式化）。
    pub async fn update_device_property_value(
        &self,
        workspace_id: &str,
        device_id: &str,
        property_id: &str,
        value: &str,
    ) -> Result<(), String> {
        use tinyiothub_core::models::event::{
            ContentElement, EventSource, RichContent, TextFormat,
        };

        // 1. 验证设备存在且属于指定的workspace
        let tenant_device_service = self.tenant_device_service_str(workspace_id);
        let device = tenant_device_service
            .get_device_by_id(device_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        // 2. 验证属性存在且属于该设备
        let property = match tinyiothub_storage::find_device_property_by_id(
            self.database(),
            property_id,
        )
        .await
        {
            Ok(Some(p)) if p.device_id == device_id => p,
            Ok(Some(_)) => {
                return Err("Property does not belong to device".to_string());
            }
            Ok(None) => return Err("not found".to_string()),
            Err(e) => return Err(format!("DB error: {}", e)),
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
        .map_err(|e| e.to_string())?;

        self.event_bus.publish(event).await.map_err(|e| e.to_string())?;

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

/// Global MCP tool registry (shared across requests)
static MCP_REGISTRY: std::sync::OnceLock<Arc<RwLock<HandlerRegistry>>> = std::sync::OnceLock::new();

/// Initialize the global MCP registry with the domain state slice.
///
/// The first call wins (OnceLock semantics); tool handlers are (re-)built
/// from the state passed to [`register_tools`].
pub fn init_mcp_registry(state: Option<Arc<McpState>>) -> Arc<RwLock<HandlerRegistry>> {
    MCP_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HandlerRegistry::new(state)))).clone()
}

/// Get the global MCP registry (returns None if not yet initialized)
pub fn get_mcp_registry() -> Option<Arc<RwLock<HandlerRegistry>>> {
    MCP_REGISTRY.get().cloned()
}

/// Register tools to the global registry.
///
/// `state` is injected into every tool handler that needs it. Pass `None`
/// in tests: handlers then behave exactly as they did before state injection
/// when the global state was unset ("McpState not initialized").
pub async fn register_tools(state: Option<Arc<McpState>>) {
    let registry = init_mcp_registry(state.clone());
    let mut reg = registry.write().await;

    // Initialize heartbeat state (used by REST API handler)
    tinyiothub_driver::heartbeat::init_heartbeat_state();

    // Thing tools (7)
    reg.register(tools::device::DeviceProfileHandler::new(state.clone()));
    reg.register(tools::device::SearchDevicesHandler::new(state.clone()));
    reg.register(tools::device::DevicePropertyGetHandler::new(state.clone()));
    reg.register(tools::device::WritePropertiesHandler::new(state.clone()));
    reg.register(tools::device::DeviceCommandHandler::new(state.clone()));
    reg.register(tools::device::CreateDeviceHandler::new(state.clone()));
    reg.register(tools::device::DeleteDeviceHandler::new(state.clone()));

    // Driver tools (2)
    reg.register(tools::driver::ListDriversHandler);
    reg.register(tools::driver::TestDriverHandler::new(state.clone()));

    // Job tools (4)
    reg.register(tools::job::ListSchedulesHandler::new(state.clone()));
    reg.register(tools::job::CreateScheduleHandler::new(state.clone()));
    reg.register(tools::job::UpdateScheduleHandler::new(state.clone()));
    reg.register(tools::job::DeleteScheduleHandler::new(state.clone()));

    // Alarm tools (3)
    reg.register(tools::alarm_mcp::AlarmListHandler::new(state.clone()));
    reg.register(tools::alarm_mcp::AlarmAcknowledgeHandler::new(state.clone()));
    reg.register(tools::alarm_mcp::AlarmRuleAddHandler::new(state.clone()));

    tracing::info!("Registered {} MCP tools: 7 thing, 2 driver, 4 job, 3 alarm", 16);
}
