// Documented boundaries (SEP addendum rule 3) — code that belongs to the
// admin domain conceptually but stays in the composition layer because it is
// entangled with planes a domain crate must not depend on.
//
// 1. `cloud/src/shared/initialization.rs` (formerly `modules::system::handler::initialization`):
//    `ensure_default_admin_user` / `ensure_user_has_workspace` / `ensure_default_tenant` scaffold
//    the default tenant + per-user workspaces and provision workspace Agents via
//    `tinyiothub_agent::host::{scaffold, agent::AgentPool}` and cloud-local `shared::paths`. admin
//    → agent is a forbidden dependency direction (SEP: domain crates never depend on agent/mcp), so
//    the bootstrap stays in cloud. Its `/system/initialize` router is mounted by the composition
//    layer alongside `tinyiothub_admin::system::create_router()`. Reclaim candidate: P5 when the
//    agent-provisioning seam is extracted behind a core trait (like P4.0b/P4.0d hooks).
//
// 2. `AuthHelper::require_admin_role` (`cloud/src/shared/error_handling.rs`): routes through
//    `SecureEventService` (event-security plane, still in cloud). The admin crate consumes it via
//    the `AdminRoleChecker` port; cloud injects the adapter in `app_state.rs`. Reclaim when the
//    event-security plane is extracted.
