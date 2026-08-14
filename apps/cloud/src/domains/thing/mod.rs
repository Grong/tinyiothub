//! Thing domain crate — the universal entity model (物) management API.
//!
//! 3-layer architecture (Handler → Service → Repo). Extracted from
//! `cloud::modules::{thing, template, tag}` plus the legacy device
//! management plane (P4 pilot, SEP #0).
//!
//! Composition contract: the cloud binary derives [`ThingState`] from its
//! global `AppState` via `impl FromRef<AppState> for ThingState` and mounts
//! [`router()`] (things), [`template::handler::create_router()`] (device
//! templates) and [`tag::create_router()`] (tags).
//!
//! ## 设计不变量
//! - 禁止依赖 agent/mcp —— 动作确认经 [`hooks::ThingActionHooks`] 反向注入
//! - devices 表即 things 表；名称查询按 workspace 作用域

pub mod errors;
pub mod handler;
pub mod hooks;
pub mod legacy;
pub mod repo;
pub mod service;
pub mod summary;
pub mod tag;
pub mod template;
pub mod types;

pub use types::*;

/// Thing API router, mounted by the composition layer at /api/v1/things.
///
/// Generic over the composition layer's state `S`; the only requirement is
/// `ThingState: FromRef<S>`, so the domain crate never names `AppState`.
pub fn router() -> axum::Router<crate::shared::app_state::AppState> {
    handler::create_router()
}
