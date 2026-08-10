//! TinyIoTHub user domain crate — users, roles, permissions (P4-Task17a).
//!
//! Extracted from `cloud::modules::{user, role, permission}` per the Task 15
//! SEP. The crate never names the composition layer's `AppState`: handlers
//! take `State<UserState>` and every router is generic over the composition
//! state `S` with a `UserState: FromRef<S>` bound.
//!
//! ## 设计不变量
//! - 用户/角色/权限领域；不依赖其他领域 crate


use std::sync::Arc;

pub mod handler;
pub mod legacy;
pub mod password;
pub mod permission;
pub mod repo;
pub mod role;
pub mod service;
pub mod types;

pub use repo::*;
pub use service::UserService;
pub use types::*;

/// User domain state slice — Arc'd services only, derived from the
/// composition layer's `AppState` via `FromRef` (cloud/src/shared/app_state.rs).
#[derive(Clone)]
pub struct UserState {
    pub user_service: Arc<UserService>,
    pub role_service: Arc<role::RoleService>,
    pub permission_service: Arc<permission::PermissionService>,
    /// Admin-role checks route through the composition layer's event
    /// security plane (`AuthHelper` → `SecureEventService`), which stays in
    /// cloud until Tasks 18/24 — see `legacy/mod.rs`.
    pub role_checker: Arc<dyn RoleChecker>,
}

/// Role-check seam: the user handlers' admin checks historically call
/// cloud's `AuthHelper::check_role`, which depends on the event security
/// plane (not the role domain). Cloud implements this trait; the crate
/// consumes it without naming `AppState`.
#[async_trait::async_trait]
pub trait RoleChecker: Send + Sync {
    async fn check_role(&self, user_id: &str, role: &str) -> Result<bool, String>;
}

/// Users router (`/users`).
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    UserState: axum::extract::FromRef<S>,
{
    handler::create_router()
}
