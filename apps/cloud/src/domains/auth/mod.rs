//! Auth domain crate — authentication API (login/logout/session/token/SMS/social)
//! plus the cloud JWT service and tenant-carrying `Claims` extractor.
//!
//! Extracted from `cloud::modules::auth` + `cloud::shared::security::jwt`
//! (P4-Task16, SEP application #1, following the Task 15 pilot).
//!
//! Composition contract: the cloud binary mounts the routers below with
//! its `AppState`; handlers read their config slices (sms/social/harmonyos)
//! off `AppState` fields. The web-layer `AuthClaims`/`WorkspaceScope` seam
//! in `tinyiothub_web` stays registered by the binary as before; this
//! module never re-registers it.
//!
//! ## 设计不变量
//! - security::jwt::Claims 可供其他领域 crate 复用（web 萃取器经组合层注入 validator）

pub mod bootstrap;
pub mod handler;
pub mod legacy;
pub mod redis;
pub mod sse;
pub mod types;
pub mod user_store;

/// Protected auth API router (session profile/refresh/validate + SSE token),
/// mounted by the composition layer at /api/v1/auth under the JWT middleware.
///
/// Handlers extract `State<AppState>`; config slices (sms/social/harmonyos)
/// are copied onto `AppState` from `ApplicationSettings` at startup —
/// identical semantics to the former per-request `config::get()` reads,
/// since the settings are loaded once and never reloaded (G6).
pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .merge(handler::session::create_router())
        .merge(handler::token::create_protected_router())
}
