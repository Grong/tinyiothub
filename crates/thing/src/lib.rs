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
//! - 禁止依赖 agent/mcp —— 动作确认经 core::thing_hooks 反向注入
//! - devices 表即 things 表；名称查询按 workspace 作用域


pub mod errors;
pub mod handler;
pub mod legacy;
pub mod repo;
pub mod service;
pub mod summary;
pub mod tag;
pub mod template;
pub mod types;

use std::sync::Arc;

pub use types::*;

/// Axum sub-state for the thing domain.
///
/// The composition layer owns the global `AppState`; this struct is the
/// slice the thing domain needs, derived via `FromRef`. No globals, no
/// app-state singleton.
#[derive(Clone)]
pub struct ThingState {
    /// Database handle — handlers take the sqlx pool from here.
    pub database: Arc<tinyiothub_storage::Database>,

    /// Thing action hooks (P4.0b) — agent-provided param validation,
    /// confirmation token store and policy confirm gate, behind the
    /// `tinyiothub_core::thing_hooks::ThingActionHooks` seam.
    pub hooks: Arc<dyn tinyiothub_core::thing_hooks::ThingActionHooks>,

    /// Command dispatch channel for action invoke/confirm. `None` before the
    /// runtime is wired up; execution is then reported as simulated.
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,

    /// Template engine — device template management.
    pub template_engine: Arc<template::TemplateEngine>,

    /// Tag service — tags and tag bindings.
    pub tag_service: Arc<tag::TagService>,
}

/// Thing API router, mounted by the composition layer at /api/v1/things.
///
/// Generic over the composition layer's state `S`; the only requirement is
/// `ThingState: FromRef<S>`, so the domain crate never names `AppState`.
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    ThingState: axum::extract::FromRef<S>,
{
    handler::create_router()
}
