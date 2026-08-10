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
pub mod security;
pub mod sse;
pub mod types;
pub mod user_store;

use std::sync::Arc;

/// Axum sub-state for the auth domain.
///
/// The composition layer owns the global `AppState`; this struct is the
/// slice the auth domain needs, derived via `FromRef`. No globals, no
/// app-state singleton. Config slices (sms/social/harmonyos) are cloned from
/// the process-global `ApplicationSettings` at extraction time — identical
/// semantics to the former per-request `config::get()` reads, since the
/// global config is set once at startup and never reloaded.
#[derive(Clone)]
pub struct AuthState {
    /// Database handle — token blacklist, SMS code fallback, social bindings.
    pub database: Arc<tinyiothub_storage::Database>,

    /// Identity store (user lookup/authenticate/create) — backed by cloud's
    /// `modules::user::UserService` until Task 17 extracts the user domain.
    pub users: Arc<dyn user_store::AuthUserStore>,

    /// Post-registration tenant/workspace bootstrap — backed by cloud's
    /// `modules::system::handler::ensure_user_has_workspace` (entangled with
    /// the agent plane; reclaimed by Task 17/24).
    pub workspace_bootstrap: Arc<dyn bootstrap::WorkspaceBootstrap>,

    /// Redis client — SMS rate limiting and OAuth state store. Optional:
    /// when absent, SMS falls back to DB storage and rate checks are skipped
    /// (identical to the former `state.redis` behavior).
    pub redis: Option<redis::RedisClient>,

    /// Short-lived SSE token issuer — backed by cloud's
    /// `shared::sse_token::SseTokenManager` (shared with the event plane).
    pub sse_token_issuer: Arc<dyn sse::SseTokenIssuer>,

    /// SMS config slice (enabled/rate_limit/captcha/aliyun).
    pub sms_config: tinyiothub_core::config::SmsConfig,

    /// Social login config slice (wechat).
    pub social_config: tinyiothub_core::config::SocialConfig,

    /// HarmonyOS mode flag — skips last-login DB writes (Signal 11 workaround).
    pub harmonyos_enabled: bool,
}

/// Protected auth API router (session profile/refresh/validate + SSE token),
/// mounted by the composition layer at /api/v1/auth under the JWT middleware.
///
/// Generic over the composition layer's state `S`; the only requirement is
/// `AuthState: FromRef<S>`, so the domain crate never names `AppState`.
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    AuthState: axum::extract::FromRef<S>,
{
    axum::Router::new()
        .merge(handler::session::create_router())
        .merge(handler::token::create_protected_router())
}
