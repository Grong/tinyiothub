//! Documented boundary — files/concepts that stay in cloud and why
//! (SEP addendum rule 3).
//!
//! - `cloud::shared::error_handling::AuthHelper` — the user handlers' admin checks route through
//!   the *event* security plane (`SecureEventService` via
//!   `AppState::initialize_secure_event_service`), not the role domain. The crate consumes it
//!   through the `RoleChecker` seam; the adapter (`EventSecurityRoleChecker`) lives in
//!   `cloud/src/shared/app_state.rs`. Reclaim with Task 18 (event) or Task 24 (admin/system).
//! - `cloud::shared::pagination` — `PaginationQuery` moved to `tinyiothub_web::pagination` (shared
//!   with device/monitoring/event/system modules); cloud keeps a re-export shim.
//! - Auth seam note: `tinyiothub_auth::user_store::AuthUserStore` stays implemented in cloud
//!   (`app_state.rs`) as an adapter on `tinyiothub_user::UserService` — the user crate must not
//!   depend on the auth crate (wrong dependency direction).
