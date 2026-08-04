// MCP API Module
// Embedded MCP server for AI Agent integration

use std::sync::Arc;

use tokio::sync::RwLock;

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

/// Initialize the global MCP registry with the application state.
///
/// The first call wins (OnceLock semantics); tool handlers are (re-)built
/// from the state passed to [`register_tools`].
pub fn init_mcp_registry(
    state: Option<Arc<crate::shared::app_state::AppState>>,
) -> Arc<RwLock<HandlerRegistry>> {
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
/// when the global AppState was unset ("AppState not initialized").
pub async fn register_tools(state: Option<Arc<crate::shared::app_state::AppState>>) {
    let registry = init_mcp_registry(state.clone());
    let mut reg = registry.write().await;

    // Initialize heartbeat state (used by REST API handler)
    crate::modules::heartbeat::init_heartbeat_state();

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
