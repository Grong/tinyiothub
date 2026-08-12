//! Admin domain crate — system, monitoring, batch, jobs, open API.
//!
//! ## 设计不变量
//! - 系统/监控/批处理/开放 API；调度接 scheduler crate（admin→scheduler）

// Admin domain crate (P4-Task24) — the final modules/ extraction.
//
// Covers the platform-administration API surface formerly under
// `cloud/src/modules/{system,monitoring,batch,jobs,open}` plus the device
// management-plane handlers (`modules::device::handler`):
//   device/     — /devices management-plane handlers (profile, properties,
//                 commands, traces, monitoring, dashboard, 410 stubs)
//   system/     — /system configuration + features + time tasks
//   monitoring/ — /monitoring dashboard stats, health, logs, metrics
//   batch/      — /batch batch command operations
//   jobs/       — /jobs task-management API over tinyiothub_scheduler
//   open/       — /open third-party integration surface (X-API-Key auth)
//
// The crate never names cloud's `AppState`: handlers extract
// `State<AdminState>` and every exported router is generic over the
// composition state `S` with `AdminState: FromRef<S>` (SEP contract,
// P4-Task15 pilot).

pub mod batch;
pub mod device;
pub mod jobs;
pub mod legacy;
pub mod monitoring;
pub mod open;
pub mod system;

/// Admin role-check port — the admin handlers' privileged-operation guard
/// routes through cloud's event-security plane (`AuthHelper` →
/// `SecureEventService`), which stays in the composition layer. Cloud
/// injects the adapter via `FromRef<AppState> for AdminState`
/// (same seam shape as `tinyiothub_user::RoleChecker`, P4-Task17a).
#[async_trait::async_trait]
pub trait AdminRoleChecker: Send + Sync {
    async fn require_admin_role(&self, user_id: &str, operation: &str) -> Result<(), String>;
}
