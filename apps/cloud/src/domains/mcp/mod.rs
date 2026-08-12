//! mcp 领域：内嵌 MCP server 与工具注册（F2 自 crates/mcp 回流，relay 范式）。
//!
//! 工具 handler 经全局 registry 持有 `Arc<AppState>`（P4.0-Task11 消灭单例后的
//! 注入形态），禁止任何全局 AppState 单例。

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

use crate::shared::app_state::AppState;
use tool_registry::HandlerRegistry;

/// Create the MCP router (mounted at `/mcp` by the composition layer).
///
/// Generic over the composition state `S` — axum 0.8 `nest()` requires
/// matching state types; `State<McpState>` extraction works for any
/// `S: FromRef<McpState>` (SEP contract, P4-Task15 pilot).
pub fn router() -> axum::Router<AppState> {
    handlers::create_router()
}

/// Global MCP tool registry (shared across requests)
static MCP_REGISTRY: std::sync::OnceLock<Arc<RwLock<HandlerRegistry>>> = std::sync::OnceLock::new();

/// Initialize the global MCP registry with the domain state slice.
///
/// The first call wins (OnceLock semantics); tool handlers are (re-)built
/// from the state passed to [`register_tools`].
pub fn init_mcp_registry(state: Option<Arc<AppState>>) -> Arc<RwLock<HandlerRegistry>> {
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
/// when the global state was unset ("McpState not initialized").
pub async fn register_tools(state: Option<Arc<AppState>>) {
    let registry = init_mcp_registry(state.clone());
    let mut reg = registry.write().await;

    // Initialize heartbeat state (used by REST API handler)
    tinyiothub_driver::heartbeat::init_heartbeat_state();

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
