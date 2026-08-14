//! Composition ports — the seams where the composition layer (cloud) plugs
//! capabilities into the agent host without the agent crate depending on
//! not-yet-extracted or downstream domains.
//!
//! Three ports today:
//!
//! - [`ExternalToolRegistry`] — MCP-registered tools. The MCP plane lives in
//!   the mcp crate (P4-Task23) and depends on the agent
//!   crate, never vice versa. The mcp crate adapts its `HandlerRegistry` to
//!   this port (`agent_bridge`); [`external_tool_registry`] derives the adapter
//!   on demand from the MCP registry (G3 — no registration static).
//! - [`WorkspaceAccess`] — workspace existence + tenant ownership lookup used
//!   by HTTP handlers. Implemented in cloud over
//!   `crate::domains::tenant::WorkspaceService` (tenant → agent edge stays one-way;
//!   agent must not depend on the tenant crate).
//! - [`set_default_model`] — the fallback model id for agent runtime configs,
//!   previously read from cloud's global `[minimax]` config section.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::domains::agent::types::{ToolSafety, classify_tool_safety};

// ---------------------------------------------------------------------------
// External tool registry (MCP seam)
// ---------------------------------------------------------------------------

/// Metadata for an externally-registered tool (mirrors MCP `ToolMetadata`).
#[derive(Debug, Clone)]
pub struct ExternalToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Execution context for an external tool call: the workspace scope plus the
/// actor identity the composition layer should attribute the call to
/// (e.g. `"agent"` for chat turns, `"__heartbeat__:{ws}"` for heartbeat
/// approvals).
#[derive(Debug, Clone)]
pub struct ExternalToolContext {
    pub workspace_id: String,
    pub actor: String,
}

/// An externally-registered tool handler (mirrors the MCP `ToolHandler`
/// surface the agent host consumes).
#[async_trait]
pub trait ExternalToolHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, ctx: &ExternalToolContext, args: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Declared safety classification — authoritative for trust evaluation.
    fn safety(&self) -> ToolSafety {
        classify_tool_safety(self.name())
    }
}

/// Read-side registry of external tools available to agents.
#[async_trait]
pub trait ExternalToolRegistry: Send + Sync {
    async fn list_tools(&self) -> Vec<ExternalToolMeta>;
    async fn get_handler(&self, name: &str) -> Option<Arc<dyn ExternalToolHandler>>;
}

/// The external tool registry, derived on demand from the MCP tool registry
/// (G3 — OnceLock eliminated; MCP_REGISTRY is the single source of truth).
pub fn external_tool_registry() -> Option<Arc<dyn ExternalToolRegistry>> {
    crate::domains::mcp::get_mcp_registry().map(|registry| {
        Arc::new(crate::domains::mcp::agent_bridge::McpExternalToolRegistry::new(
            registry,
        )) as Arc<dyn ExternalToolRegistry>
    })
}

// ---------------------------------------------------------------------------
// Workspace access (tenant seam)
// ---------------------------------------------------------------------------

/// Workspace existence + tenant ownership lookup for HTTP handlers.
#[async_trait]
pub trait WorkspaceAccess: Send + Sync {
    /// The owning tenant_id of the workspace, `Ok(None)` when it does not
    /// exist, `Err` on lookup failure.
    async fn workspace_tenant_id(&self, workspace_id: &str) -> Result<Option<String>, String>;
}

/// Verify the workspace exists and belongs to the current tenant. Mirrors the
/// semantics of the tenant crate's `verify_workspace_access!` macro (which the
/// agent crate cannot use without a tenant dependency edge).
///
/// Expands to an early `return` with the same error responses as the original.
#[macro_export]
macro_rules! verify_workspace_access_port {
    ($state:expr, $claims:expr, $id:expr) => {{
        match $state.workspace_access.workspace_tenant_id(&$id).await {
            Ok(Some(tenant_id)) => {
                if tenant_id != $claims.tenant_id {
                    return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
                }
            }
            Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
            Err(e) => {
                tracing::error!("Failed to get workspace: {}", e);
                return ApiResponseBuilder::error("获取工作空间失败");
            }
        }
    }};
}

// ---------------------------------------------------------------------------
// Default model id (config seam)
// ---------------------------------------------------------------------------

static DEFAULT_MODEL: RwLock<Option<String>> = RwLock::new(None);

/// Register the fallback model id (composition layer, from `[minimax].model`).
pub fn set_default_model(model: String) {
    *DEFAULT_MODEL.write().expect("default model lock poisoned") = Some(model);
}

/// The fallback model id for agent runtime configs.
pub fn default_model() -> String {
    DEFAULT_MODEL
        .read()
        .expect("default model lock poisoned")
        .clone()
        .unwrap_or_else(|| "minimax-m2".into())
}

// ---------------------------------------------------------------------------
// Minimax provider settings (LLM seam)
// ---------------------------------------------------------------------------

/// `[minimax]` provider settings, registered by the composition layer from
/// its config at startup (previously read via cloud's global config).
#[derive(Debug, Clone)]
pub struct MinimaxSettings {
    pub base_url: String,
    pub auth_token: String,
    pub model: String,
}

static MINIMAX_SETTINGS: RwLock<Option<MinimaxSettings>> = RwLock::new(None);

/// Register the minimax provider settings (composition layer at startup).
/// Also seeds [`set_default_model`] from the same section.
pub fn set_minimax_settings(settings: MinimaxSettings) {
    set_default_model(settings.model.clone());
    *MINIMAX_SETTINGS.write().expect("minimax settings lock poisoned") = Some(settings);
}

/// The registered minimax provider settings, if any.
pub fn minimax_settings() -> Option<MinimaxSettings> {
    MINIMAX_SETTINGS.read().expect("minimax settings lock poisoned").clone()
}

/// Create a MiniMax model provider from the registered settings.
pub fn create_minimax_provider() -> anyhow::Result<Box<dyn zeroclaw::providers::traits::ModelProvider>> {
    let cfg =
        minimax_settings().ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
    zeroclaw::providers::create_model_provider_with_url("minimaxi", Some(&cfg.auth_token), Some(&cfg.base_url))
}
