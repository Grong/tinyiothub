//! TinyIoTHub user domain crate — users, roles, permissions (P4-Task17a).
//!
//! Extracted from `cloud::modules::{user, role, permission}` per the Task 15
//! SEP. The crate never names the composition layer's `AppState`: handlers
//! take `State<UserState>` and every router is generic over the composition
//! state `S` with a `UserState: FromRef<S>` bound.
//!
//! ## 设计不变量
//! - 用户/角色/权限领域；不依赖其他领域 crate

pub mod handler;
pub mod legacy;
pub mod password;
pub mod permission;
pub mod role;
pub mod service;

// Repositories live in the db crate (E4 集中化); re-exported for compatibility.
pub use service::UserService;

/// User domain state slice — Arc'd services only, derived from the
/// composition layer's `AppState` via `FromRef` (cloud/src/state.rs).
/// Role-check seam: the user handlers' admin checks historically call
/// cloud's `AuthHelper::check_role`, which depends on the event security
/// plane (not the role domain). Cloud implements this trait; the crate
/// consumes it without naming `AppState`.
#[async_trait::async_trait]
pub trait RoleChecker: Send + Sync {
    async fn check_role(&self, user_id: &str, role: &str) -> Result<bool, String>;
}

/// Users router (`/users`).
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_router()
}
