//! TinyIoTHub notify domain crate — notification rules, notification history,
//! notification channel management, and the channel implementations
//! (email/sms/sse) (P4-Task21).
//!
//! Extracted from `cloud::modules::notification` +
//! `cloud::shared::event::channels` per the Task 15 SEP. The crate never
//! names the composition layer's `AppState`: handlers take
//! `State<NotifyState>` and every router is generic over the composition
//! state `S` with a `NotifyState: FromRef<S>` bound.
//!
//! One-way edge: notify → event (`tinyiothub_event` value objects/errors).
//!
//! Boundary notes:
//! - Rule matching lives SOLELY here: `NotificationFilterSpec::matches_filters`
//!   (`service.rs`) is the only production matcher (P4-Task18 F1 resolution;
//!   core keeps value types only in `core::notification_types`).
//! - `crates/alarm::notification::NotificationDispatcher` is alarm-specific
//!   dispatch (alarm rule config → channel send); it does NOT consume this
//!   crate — alarm's notification path is independent (see alarm lib.rs).
//! - `channels::sse_channel::{SseMessage, SseNotificationChannel}` is also
//!   consumed by cloud's `shared::event::sse_manager` (composition-layer SSE
//!   connection manager, not yet extracted — rides with the event plane).
//!
//! ## 设计不变量
//! - 只许 notify→event 单向边；渠道类型来自 core::notification_types


use std::sync::Arc;

pub mod channels;
pub mod handler;
pub mod repo;
pub mod service;
pub mod types;

pub use repo::*;
pub use service::*;
pub use types::*;

/// Notify domain state slice — Arc'd slices only, derived from the
/// composition layer's `AppState` via `FromRef` (cloud/src/shared/app_state.rs).
#[derive(Clone)]
pub struct NotifyState {
    pub database: Arc<tinyiothub_storage::Database>,
    /// Optional in AppState (creation failure is tolerated); handlers 500
    /// with "Notification manager not available" when absent, as before.
    pub notification_manager: Option<Arc<service::NotificationManager>>,
}

/// Notification rules API router (`/notifications`), generic over the
/// composition state `S`.
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    NotifyState: axum::extract::FromRef<S>,
{
    handler::create_router()
}

/// Notification channels API router (`/notification-channels`), generic over
/// the composition state `S`.
pub fn channel_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    NotifyState: axum::extract::FromRef<S>,
{
    handler::create_channel_router()
}
