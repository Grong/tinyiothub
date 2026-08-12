//! Documented boundary — entangled files that stayed in cloud (SEP addendum
//! rule 3). Each entry names the reclaim task.
//!
//! - `cloud/src/shared/sse_token.rs` (`SseTokenManager`) — shared with the event plane's SSE
//!   handlers; auth consumes it via [`crate::domains::auth::sse::SseTokenIssuer`]. Reclaim: Task 18 (event crate).
//! - `cloud/src/shared/error_handling.rs` (`AuthHelper`) — admin-role checks backed by
//!   `AppState::initialize_secure_event_service` (event security service); consumers are
//!   user/monitoring/marketplace/event handlers, not the auth module. Reclaim: Task 24 (admin
//!   crate) or Task 18.
//! - `cloud/src/modules/user/` (`User`, `UserService`, `CreateUserRequest`) — auth consumes it via
//!   [`crate::domains::auth::user_store::AuthUserStore`]; cloud maps types until the user domain extraction.
//!   Reclaim: Task 17.
//! - `cloud/src/modules/system/handler/initialization.rs` (`ensure_user_has_workspace`) — takes
//!   `&AppState` and scaffolds the agent plane; auth consumes it via
//!   [`crate::domains::auth::bootstrap::WorkspaceBootstrap`]. Reclaim: Task 17 or Task 24.
//! - `crates/web` `AuthClaims`/`WorkspaceScope` + JWT-validator/tenant-resolver registration —
//!   one-time Task 15 infrastructure; consumed, never re-registered (SEP addendum rule 4).
