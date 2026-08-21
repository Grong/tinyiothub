//! Agent bridge — adapts the MCP `HandlerRegistry` to the agent crate's
//! `ExternalToolRegistry` port (P4-Task22).
//!
//! The dependency rule is mcp → agent, never agent → mcp: the agent crate
//! consumes externally-registered tools only through its port. This module
//! is the mcp-side adapter (it implements the agent crate's
//! `ExternalToolHandler`/`ExternalToolRegistry` traits and is registered at
//! startup by the composition layer); it moved into the mcp crate in
//! P4-Task23.
//!
//! The auth-context guard that used to wrap handler execution inside the
//! agent crate (`IoTToolAdapter` / heartbeat proposal approval) is applied
//! here instead: [`BridgedToolHandler::execute`] enters the
//! `McpContextGuard` built from the call's workspace + actor before
//! delegating to the real handler, preserving fail-closed scoping.

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_agent::tools::{ExternalToolContext, ExternalToolHandler, ExternalToolMeta, ExternalToolRegistry};
use tokio::sync::RwLock;

use super::handlers::{McpAuthContext, McpContextGuard};
use super::tool_registry::{HandlerRegistry, ToolHandler};

struct BridgedToolHandler {
    inner: Arc<dyn ToolHandler>,
}

#[async_trait]
impl ExternalToolHandler for BridgedToolHandler {
    fn name(&self) -> &str {
        // `ToolHandler::name` borrows from the handler; the bridge is only
        // ever used behind Arc, so the borrow lives as long as `self`.
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.inner.input_schema().to_json()
    }

    async fn execute(&self, ctx: &ExternalToolContext, args: serde_json::Value) -> Result<serde_json::Value, String> {
        let _guard = McpContextGuard::new(McpAuthContext::for_heartbeat(
            ctx.workspace_id.clone(),
            ctx.actor.clone(),
        ));
        self.inner.execute(args).await.map_err(|e| e.to_string())
    }

    fn safety(&self) -> tinyiothub_skills::trust::ToolSafety {
        self.inner.safety()
    }
}

/// MCP-backed external tool registry (bridge adapter over the MCP registry).
pub struct McpExternalToolRegistry {
    registry: Arc<RwLock<HandlerRegistry>>,
}

impl McpExternalToolRegistry {
    /// Wrap an existing MCP handler registry.
    pub fn new(registry: Arc<RwLock<HandlerRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ExternalToolRegistry for McpExternalToolRegistry {
    async fn list_tools(&self) -> Vec<ExternalToolMeta> {
        let reg = self.registry.read().await;
        reg.list_tools()
            .into_iter()
            .map(|m| ExternalToolMeta {
                name: m.name,
                description: m.description,
                input_schema: m.input_schema,
            })
            .collect()
    }

    async fn get_handler(&self, name: &str) -> Option<Arc<dyn ExternalToolHandler>> {
        let reg = self.registry.read().await;
        reg.get_owned(name)
            .map(|h| Arc::new(BridgedToolHandler { inner: h }) as _)
    }
}
