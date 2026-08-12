//! TinyIoTHub alarm domain crate — alarm rules, alarm lifecycle, recent-alarm
//! queries, and notification dispatch (P4-Task19).
//!
//! Extracted from `cloud::modules::alarm` per the Task 15 SEP. The crate never
//! names the composition layer's `AppState`: handlers take `State<AlarmState>`
//! and every router is generic over the composition state `S` with an
//! `AlarmState: FromRef<S>` bound.
//!
//! One-way edges:
//! - alarm → event: consumes `tinyiothub_event` types and implements
//!   `router::EventAlarmHook` for `AlarmService` (`event_hook`); the event
//!   ingest pipeline fires the hook after persisting a thing event.
//! - alarm ↔ agent (no direct edge): the `AlarmEvent` payload lives in the
//!   event crate (re-homed in P4-Task22 so the agent crate needs no alarm
//!   dependency); the alarm crate never names agent types — `AlarmService`
//!   publishes through the `types_ai::AlarmAiPublisher` port and cloud's
//!   composition layer adapts `AiEventPublisher` to it
//!   (`cloud::shared::ai_adapter::AlarmAiPublisherAdapter`).
//!
//! Boundary notes:
//! - `NotificationChannelType` etc. come from `crate::domains::event::aggregates`
//!   (sunk to core in P4.0-Task13) — there is NO alarm → notify edge; the
//!   notification domain was extracted in P4-Task21 (`tinyiothub_notify`)
//!   and alarm's notification dispatch stays independent of it.
//! - `RecentAlarm` moved here from `cloud::modules::monitoring::types`
//!   (the `/alarms/recent` handler was its only consumer).
//!
//! ## 设计不变量
//! - 只许 alarm→event 单向边；AI 发布经组合层适配器（AlarmAiPublisher）

pub mod alarm;
pub mod dto;
pub mod event_hook;
pub mod event_matcher;
pub mod handler;
pub mod notification;
pub mod service;
pub mod types_ai;

// Note: `alarm::BatchOperationResult` duplicates `types::BatchOperationResult`
// (pre-existing); only the types one is glob-exported, as before.
pub use event_matcher::*;
// Repositories live in the db crate (E2 集中化); re-exported for compatibility.
pub use dto::*;
pub use service::*;
pub use tinyiothub_storage::alarm::{AlarmRepository, AlarmRuleRepository};
pub use types_ai::{AlarmAiPublisher, AlarmEvent};

/// Alarms API router (`/alarms`), generic over the composition state `S`.
pub fn router() -> axum::Router<crate::shared::app_state::AppState> {
    handler::create_alarm_router()
}

/// Alarm rules API router (`/alarm-rules`), generic over the composition
/// state `S`.
pub fn rule_router() -> axum::Router<crate::shared::app_state::AppState> {
    handler::create_alarm_rule_router()
}
