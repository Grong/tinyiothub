//! Post-registration tenant/workspace bootstrap seam.
//!
//! Backed by cloud's `modules::system::handler::ensure_user_has_workspace`,
//! which takes `&AppState` and is entangled with the agent plane
//! (workspace scaffolding + agent creation). It stays in cloud; the
//! composition layer injects an adapter here. Reclaim: Task 17 (user/tenant)
//! or Task 24 (admin).

use async_trait::async_trait;

/// Ensures a freshly registered user is linked to the default tenant and
/// owns a personal workspace (idempotent).
#[async_trait]
pub trait WorkspaceBootstrap: Send + Sync {
    async fn ensure_user_has_workspace(&self, user_id: &str) -> Result<(), String>;
}
