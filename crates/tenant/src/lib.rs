//! TinyIoTHub tenant domain crate — tenants, workspaces, API keys (P4-Task17b).
//!
//! Extracted from `cloud::modules::{tenant, workspace}` per the Task 15 SEP.
//! The crate never names the composition layer's `AppState`: handlers take
//! `State<TenantState>` and every router is generic over the composition
//! state `S` with a `TenantState: FromRef<S>` bound.

use std::sync::Arc;

pub mod handler;
pub mod legacy;
pub mod repo;
pub mod service;
pub mod sql_security;
pub mod types;
pub mod workspace;

pub use repo::*;
pub use service::TenantService;
pub use types::*;
pub use workspace::{SqliteWorkspaceRepository, WorkspaceRepository, WorkspaceService};

/// Tenant domain state slice — Arc'd services + config slices, derived from
/// the composition layer's `AppState` via `FromRef`
/// (cloud/src/shared/app_state.rs).
#[derive(Clone)]
pub struct TenantState {
    pub database: Arc<tinyiothub_storage::Database>,
    pub tenant_service: Arc<TenantService>,
    pub workspace_service: Arc<WorkspaceService>,
    /// Workspace create/delete synchronously manages the per-workspace
    /// Agent — the AgentPool lives in the agent crate, consumed through
    /// this seam (see `legacy/mod.rs`).
    pub agent_lifecycle: Arc<dyn WorkspaceAgentLifecycle>,
    /// AI tag suggestions for workspace resources. `None` when no minimax
    /// config is present — the handler then answers "AI 服务未配置",
    /// byte-identical to the former `config::get().minimax.is_none()` check.
    pub tag_suggester: Option<Arc<dyn TagSuggester>>,
    /// Secret for the tenant auth token scheme (`tj_*` HMAC tokens) —
    /// cloned from the process-global config at `FromRef` extraction, same
    /// semantics as the former per-request `config::get()` read.
    pub jwt_secret: String,
    /// Base directory for per-workspace filesystem data
    /// (`<agents_base>/{workspace_id}/...`) — the path helpers stay in
    /// cloud (`shared::paths`) because `env!("CARGO_MANIFEST_DIR")` would
    /// resolve against this crate; the composition layer hands in the
    /// computed base.
    pub agents_base_dir: std::path::PathBuf,
}

/// Agent lifecycle seam: workspace creation/deletion provisions and tears
/// down the workspace's Agent. Cloud implements this over
/// `tinyiothub_agent::host::agent::AgentPool` (agent crate, P4-Task22).
#[async_trait::async_trait]
pub trait WorkspaceAgentLifecycle: Send + Sync {
    async fn create_agent(&self, workspace_id: &str, name: &str) -> Result<String, String>;
    async fn delete_agent(&self, agent_id: &str) -> Result<(), String>;
}

/// AI tag-suggestion seam for workspace resources. Cloud implements this
/// over the minimax model provider (`shared::config::create_minimax_provider`
/// — a zeroclaw type the tenant crate must not depend on). Error strings
/// are user-facing messages, byte-identical to the former inline handler.
#[async_trait::async_trait]
pub trait TagSuggester: Send + Sync {
    async fn suggest(
        &self,
        name: &str,
        resource_type_label: &str,
        description: Option<&str>,
    ) -> Result<Vec<String>, String>;
}

/// Tenants router (tenant CRUD + usage; mounted at `/tenants`).
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    TenantState: axum::extract::FromRef<S>,
{
    handler::create_router()
}

/// API keys router (mounted at `/api-keys`).
pub fn api_key_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    TenantState: axum::extract::FromRef<S>,
{
    handler::create_api_key_router()
}

/// Public tenant auth router (register/login/verify/plans; no JWT).
pub fn auth_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    TenantState: axum::extract::FromRef<S>,
{
    handler::create_auth_router()
}

/// Workspaces router (CRUD + resources; mounted at `/workspaces`).
pub fn workspace_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    TenantState: axum::extract::FromRef<S>,
{
    workspace::create_router()
}
