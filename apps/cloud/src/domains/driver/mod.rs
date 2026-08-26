//! TinyIoTHub driver domain crate — the device data access layer
//! (P4-Task20).
//!
//! Extracted from `cloud::modules::{drivers, driver_health, gateway, plugin,
//! heartbeat}` plus the device-legacy connection face left in
//! `cloud::modules::device` by the thing extraction (P4-Task15). The crate
//! never names the composition layer's `AppState`: handlers take
//! `State<DriverState>` and every router is generic over the composition
//! state `S` with a `DriverState: FromRef<S>` bound (or no state at all).
//!
//! One-way edges:
//! - driver → runtime: the driver framework (`tinyiothub_runtime::driver`,
//!   driver registry, plugin loader) lives in the runtime crate; this crate
//!   is the management API over it.
//! - driver → alarm (documented): `legacy::{monitoring, performance}` query
//!   read-only alarm counts on device data via the `Db` facade.
//! - driver → thing (documented): `legacy::diagnostics` reads
//!   `ThingTraceStatistics` from `crate::domains::thing::legacy::trace`.
//! - driver → event: gateway pairing persists pairing events via the
//!   `tinyiothub_storage::Db` facade (`insert_event`).
//!
//! NOT consumed: agent / mcp / notification (see `legacy/mod.rs` and
//! `plugin::registry::AppContext` docs for boundary notes).
//!
//! ## 设计不变量
//! - 只许 driver→{thing,event,alarm} 单向边；写数据经 thing，不反向

pub mod driver_health;
pub mod drivers;
pub mod gateway;
pub mod heartbeat;
pub mod legacy;
pub mod plugin;

/// Driver domain state slice — Arc'd services only, derived from the
/// composition layer's `AppState` via `FromRef` (cloud/src/state.rs).
/// Drivers metadata API router (`/drivers`). Stateless — driver metadata
/// comes from the runtime registry.
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    drivers::handler::create_router()
}

/// Driver health API router (`/driver-health`). Stateless beyond the
/// workspace scope extractor.
pub fn driver_health_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    driver_health::handler::create_router()
}

/// Thing/gateway heartbeat API router (`/heartbeat`). Stateless beyond
/// `AppState` — heartbeat status/config live in `AppState` fields (G3).
pub fn heartbeat_router() -> axum::Router<crate::state::AppState> {
    heartbeat::handler::create_router()
}
