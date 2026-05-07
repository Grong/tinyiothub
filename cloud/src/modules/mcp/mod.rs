// MCP API Module
// Embedded MCP server for AI Agent integration

use std::sync::Arc;

use tokio::sync::{OnceCell as TokioOnceCell, RwLock};

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

/// Global MCP tool registry (shared across requests)
static MCP_REGISTRY: std::sync::OnceLock<Arc<RwLock<HandlerRegistry>>> = std::sync::OnceLock::new();

/// Global AppState for MCP tool handlers (initialized before tool registration)
static APP_STATE: TokioOnceCell<Arc<crate::shared::app_state::AppState>> =
    TokioOnceCell::const_new();

/// Initialize the global MCP registry
pub fn init_mcp_registry() -> Arc<RwLock<HandlerRegistry>> {
    MCP_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HandlerRegistry::new()))).clone()
}

/// Get the global MCP registry (returns None if not yet initialized)
pub fn get_mcp_registry() -> Option<Arc<RwLock<HandlerRegistry>>> {
    MCP_REGISTRY.get().cloned()
}

/// Initialize the global AppState for MCP tool handlers
pub fn init_app_state(state: Arc<crate::shared::app_state::AppState>) {
    let _ = APP_STATE.set(state);
}

/// Get the global AppState (returns None if not yet initialized)
pub fn get_app_state() -> Option<Arc<crate::shared::app_state::AppState>> {
    APP_STATE.get().cloned()
}

/// Register tools to the global registry
pub async fn register_tools() {
    let registry = init_mcp_registry();
    let mut reg = registry.write().await;

    // Initialize heartbeat state (used by REST API handler)
    crate::modules::heartbeat::init_heartbeat_state();

    // Device tools (7)
    reg.register(tools::device::DeviceProfileHandler);
    reg.register(tools::device::SearchDevicesHandler);
    reg.register(tools::device::DevicePropertyGetHandler);
    reg.register(tools::device::WritePropertiesHandler);
    reg.register(tools::device::DeviceCommandHandler);
    reg.register(tools::device::CreateDeviceHandler);
    reg.register(tools::device::DeleteDeviceHandler);

    // Driver tools (2)
    reg.register(tools::driver::ListDriversHandler);
    reg.register(tools::driver::TestDriverHandler);

    // Job tools (4)
    reg.register(tools::job::ListSchedulesHandler);
    reg.register(tools::job::CreateScheduleHandler);
    reg.register(tools::job::UpdateScheduleHandler);
    reg.register(tools::job::DeleteScheduleHandler);

    // Alarm tools (3)
    reg.register(tools::alarm_mcp::AlarmListHandler);
    reg.register(tools::alarm_mcp::AlarmAcknowledgeHandler);
    reg.register(tools::alarm_mcp::AlarmRuleAddHandler);

    tracing::info!("Registered {} MCP tools: 7 device, 2 driver, 4 job, 3 alarm", 16);
}
