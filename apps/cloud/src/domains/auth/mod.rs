//! Auth domain crate — authentication API (login/logout/session/token/SMS/social)
//! plus the cloud JWT service and tenant-carrying `Claims` extractor.
//!
//! Extracted from `cloud::modules::auth` + `cloud::shared::security::jwt`
//! (P4-Task16, SEP application #1, following the Task 15 pilot).
//!
//! Composition contract: the cloud binary derives [`AuthState`] from its
//! global `AppState` via `impl FromRef<AppState> for AuthState` and mounts
//! the routers below. The binary must also call
//! [`security::jwt::init_jwt_settings`] once at startup (mirrors the former
//! global-config read) — the web-layer `AuthClaims`/`WorkspaceScope` seam in
//! `tinyiothub_web` stays registered by the binary as before; this crate
//! never re-registers it.
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

/// Axum sub-state for the auth domain.
///
/// The composition layer owns the global `AppState`; this struct is the
/// slice the auth domain needs, derived via `FromRef`. No globals, no
/// app-state singleton. Config slices (sms/social/harmonyos) are cloned from
/// the process-global `ApplicationSettings` at extraction time — identical
/// semantics to the former per-request `config::get()` reads, since the
/// global config is set once at startup and never reloaded.

/// Protected auth API router (session profile/refresh/validate + SSE token),
/// mounted by the composition layer at /api/v1/auth under the JWT middleware.
///
/// Generic over the composition layer's state `S`; the only requirement is
/// `AuthState: FromRef<S>`, so the domain crate never names `AppState`.
pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .merge(handler::session::create_router())
        .merge(handler::token::create_protected_router())
}
