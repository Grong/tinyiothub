//! TinyIoTHub tenant domain crate — tenants, workspaces, API keys (P4-Task17b).
//!
//! Extracted from `cloud::modules::{tenant, workspace}` per the Task 15 SEP.
//! The crate never names the composition layer's `AppState`: handlers take
//! `State<TenantState>` and every router is generic over the composition
//! state `S` with a `TenantState: FromRef<S>` bound.
//!
//! ## 设计不变量
//! - 租户/工作区领域；对 agent 能力的调用经自有 `hooks` 端口注入（G5b），
//!   不直接依赖 agent 域（依赖方向 agent → tenant，无环）

pub mod handler;
pub mod hooks;
pub mod legacy;
pub mod service;
pub mod workspace;

// Repositories live in the db crate (E4 集中化); re-exported for compatibility.
pub use service::TenantService;
pub use tinyiothub_storage::tenant::TenantRepository;
pub use tinyiothub_storage::workspace::WorkspaceRepository;
pub use workspace::WorkspaceService;

/// Tenant domain state slice — Arc'd services + config slices, derived from
/// the composition layer's `AppState` via `FromRef`
/// (cloud/src/state.rs).

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
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_router()
}

/// API keys router (mounted at `/api-keys`).
pub fn api_key_router() -> axum::Router<crate::state::AppState> {
    handler::create_api_key_router()
}

/// Public tenant auth router (register/login/verify/plans; no JWT).
pub fn auth_router() -> axum::Router<crate::state::AppState> {
    handler::create_auth_router()
}

/// Workspaces router (CRUD + resources; mounted at `/workspaces`).
pub fn workspace_router() -> axum::Router<crate::state::AppState> {
    workspace::create_router()
}
