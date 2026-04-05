// MCP API Module
// Embedded MCP server for AI Agent integration

use std::sync::Arc;

use tokio::sync::OnceCell as TokioOnceCell;
use tokio::sync::RwLock;

pub mod handlers;
pub mod tool_registry;
pub mod tools;

#[cfg(test)]
mod tests; // Integration tests in tests/ directory

// Re-export types for use in other modules
pub use tool_registry::{HandlerRegistry, ToolError, ToolHandler, ToolMetadata};
pub use handlers::{create_router, ToolCallParams};

/// Global MCP tool registry (shared across requests)
static MCP_REGISTRY: std::sync::OnceLock<Arc<RwLock<HandlerRegistry>>> =
    std::sync::OnceLock::new();

/// Global AppState for MCP tool handlers (initialized before tool registration)
static APP_STATE: TokioOnceCell<Arc<crate::shared::app_state::AppState>> =
    TokioOnceCell::const_new();

/// Initialize the global MCP registry
pub fn init_mcp_registry() -> Arc<RwLock<HandlerRegistry>> {
    MCP_REGISTRY
        .get_or_init(|| Arc::new(RwLock::new(HandlerRegistry::new())))
        .clone()
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

    // Initialize heartbeat state (Task 4)
    tools::heartbeat::init_heartbeat_state();

    // Register device tools (Task 2)
    reg.register(tools::device::ListDevicesHandler);
    reg.register(tools::device::GetDeviceHandler);
    reg.register(tools::device::GetDeviceStatusHandler);
    reg.register(tools::device::ReadPropertiesHandler);
    reg.register(tools::device::WritePropertiesHandler);
    reg.register(tools::device::SendCommandHandler);
    reg.register(tools::device::CreateDeviceHandler);
    reg.register(tools::device::UpdateDeviceHandler);
    reg.register(tools::device::DeleteDeviceHandler);
    reg.register(tools::device::GetDeviceHistoryHandler);
    reg.register(tools::device::GetDeviceMetricsHandler);
    reg.register(tools::device::ExportDeviceReportHandler);

    // Register driver tools (Task 3)
    reg.register(tools::driver::ListDriversHandler);
    reg.register(tools::driver::GetDriverConfigSchemaHandler);
    reg.register(tools::driver::MatchDriverHandler);
    reg.register(tools::driver::GenerateDriverHandler);
    reg.register(tools::driver::LoadDriverHandler);
    reg.register(tools::driver::UnloadDriverHandler);
    reg.register(tools::driver::TestDriverHandler);

    // Register heartbeat tools (Task 4)
    reg.register(tools::heartbeat::ReportHeartbeatHandler);
    reg.register(tools::heartbeat::GetHeartbeatStatusHandler);
    reg.register(tools::heartbeat::ConfigureHeartbeatHandler);

    // Register self-heal tools (Task 5)
    tools::self_heal::register_self_heal_tools(&mut reg);

    // Register knowledge tools (Task 6)
    tools::knowledge::register_knowledge_tools(&mut reg);

    tracing::info!("Registered {} device MCP tools, {} driver MCP tools, {} heartbeat MCP tools, {} self-heal MCP tools, {} knowledge MCP tools",
        12, 7, 3, 3, 3);
}
