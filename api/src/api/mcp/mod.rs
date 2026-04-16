// MCP API Module
// Embedded MCP server for AI Agent integration

use std::sync::Arc;

use tokio::sync::OnceCell as TokioOnceCell;
use tokio::sync::RwLock;

pub mod handlers;
pub mod tool_metadata;
pub mod tool_registry;
pub mod tools;

#[cfg(test)]
mod tests; // Integration tests in tests/ directory

// Re-export types for use in other modules
pub use tool_metadata::{IoTToolMetadata, PermissionLevel};
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

    // Register device tools
    reg.register(tools::device::ListDevicesHandler);
    reg.register(tools::device::DeviceProfileHandler);
    reg.register(tools::device::DevicePropertyGetHandler);
    reg.register(tools::device::CreateDeviceHandler);
    reg.register(tools::device::DeviceCommandHandler);
    reg.register(tools::device::DeviceTemplateListHandler);

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

    // Register workspace tools (Task 11)
    reg.register(tools::workspace::ListWorkspacesHandler);
    reg.register(tools::workspace::GetWorkspaceHandler);
    reg.register(tools::workspace::CreateWorkspaceHandler);
    reg.register(tools::workspace::UpdateWorkspaceHandler);
    reg.register(tools::workspace::DeleteWorkspaceHandler);

    // Register job tools (Task 13)
    reg.register(tools::job::ListSchedulesHandler);
    reg.register(tools::job::CreateScheduleHandler);
    reg.register(tools::job::DeleteScheduleHandler);

    // Register batch tools (Task 14)
    reg.register(tools::batch::BatchCommandHandler);
    reg.register(tools::batch::GetBatchStatusHandler);

    // Register alarm tools (Task 18)
    reg.register(tools::alarm_mcp::AlarmListHandler);
    reg.register(tools::alarm_mcp::AlarmStatisticsHandler);
    reg.register(tools::alarm_mcp::AlarmAcknowledgeHandler);
    reg.register(tools::alarm_mcp::AlarmRuleAddHandler);

    // Register device enhanced tools (Task 19)
    reg.register(tools::device_enhanced::CompareDevicesHandler);
    reg.register(tools::device_enhanced::DiagnoseDeviceHandler);
    reg.register(tools::device_enhanced::ScanSerialHandler);

    tracing::info!("Registered {} device MCP tools, {} driver MCP tools, {} heartbeat MCP tools, {} self-heal MCP tools, {} knowledge MCP tools, {} workspace MCP tools, {} job MCP tools, {} batch MCP tools, {} alarm MCP tools, {} device_enhanced MCP tools",
        12, 7, 3, 3, 3, 5, 3, 2, 4, 3);
}
