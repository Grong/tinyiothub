//! Event domain crate — event pipeline (router, throttle, real-time, SSE, retention).
//!
//! ## 设计不变量
//! - 单向边承载者：只许 notify→event、alarm→event、agent→event，event 不反向依赖

// Event domain crate — extracted from cloud/src/modules/event (P4-Task18).
//
// Owns: event entity/DTO types, repository traits + SQLite impls, domain
// services (aggregates, specifications), the thing-event ingest pipeline
// (`router::route_thing_event` with throttle + persist + broadcast), the
// in-process `bus::ThingEventBus`, and the query/overview/real-time HTTP API.
//
// Boundary (stays in cloud, reclaimed by the future notify/security-plane
// extraction): the event security plane and SSE manager live in
// `cloud::shared::event` (entangled with the notification module, not yet
// extracted), so the `/events/security/*` and `/events/sse*` HTTP routes stay
// there too (`cloud::shared::event::http`).
//
// One-way edges carried here: alarm → event (via `router::EventAlarmHook`,
// injected by the composition layer), notify → event (notify consumes event
// types), agent → event (`bus::ThingEventSignal` consumed by the thing-agent
// loop in crates/agent).

pub use alarm_event::AlarmEvent;

pub mod alarm_event;
pub mod bus;
pub mod errors;
pub mod handler;
pub mod http;
pub mod router;
pub mod security;
pub mod service;
pub mod sse_manager;
pub mod subscribers;
pub mod types;

// Backward compatibility: re-export core types as submodules
pub mod entities {
    pub use tinyiothub_core::models::event::Event;
}

pub mod value_objects {
    pub use tinyiothub_core::models::event::{
        ConnectionStatus, ContentElement, DeviceEventType, EventId, EventLevel, EventSource, EventType, LinkTarget,
        RichContent, SystemEventType, TextFormat,
    };
}

// Backward compatibility: EventError and Result
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Event validation error: {message}")]
    Validation { message: String },
    #[error("Event not found: {id}")]
    NotFound { id: String },
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Notification error: {0}")]
    Notification(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Gateway error: {0}")]
    Gateway(String),
}

pub type Result<T> = std::result::Result<T, EventError>;

impl From<tinyiothub_storage::DbError> for EventError {
    fn from(err: tinyiothub_storage::DbError) -> Self {
        match err {
            tinyiothub_storage::DbError::NotFound { id } => EventError::NotFound { id },
            tinyiothub_storage::DbError::Validation { message } => EventError::Validation { message },
            other => EventError::Notification(other.to_string()),
        }
    }
}

impl From<String> for EventError {
    fn from(msg: String) -> Self {
        EventError::Validation { message: msg }
    }
}

impl From<&str> for EventError {
    fn from(msg: &str) -> Self {
        EventError::Validation {
            message: msg.to_string(),
        }
    }
}

// Was `From<crate::shared::error::Error>` in cloud — that type is a re-export
// of core's error, so the impl moves here unchanged.
impl From<tinyiothub_core::error::Error> for EventError {
    fn from(err: tinyiothub_core::error::Error) -> Self {
        EventError::Gateway(err.to_string())
    }
}

// Re-export core event types
#[allow(ambiguous_glob_reexports)]
pub use handler::*;
#[allow(ambiguous_glob_reexports)]
pub use service::*;
pub use tinyiothub_core::models::event::{
    ConnectionStatus, ContentElement, DeviceEventType, Event, EventId, EventLevel, EventSource, EventType, LinkTarget,
    RichContent, SystemEventType, TextFormat,
};
pub use types::*;

// Backward compatibility: old module paths
pub mod repositories {
    pub use tinyiothub_storage::event::*;
}

/// Re-export EventAggregate for backward compat (was in aggregates/ subdirectory)
pub use service::EventAggregate;

/// Backward compatibility: old aggregates::NotificationChannelType path
pub mod aggregates {
    pub use tinyiothub_core::notification_types::NotificationChannelType;

    pub use super::service::EventAggregate;
}

// Re-export errors module types at top level for convenience
pub use errors::{
    DomainError, DomainResult, EventDomainError, EventServiceDomainError, NotificationDomainError,
    PerformanceDomainError, SecurityDomainError,
};

/// Events API router, generic over the composition state `S`.
///
/// Handlers extract `State<EventState>`, which axum derives from `S` via
/// `FromRef`. The security/SSE sub-routes are NOT here — see the crate-level
/// boundary note.
pub fn router() -> axum::Router<crate::state::AppState> {
    handler::create_router()
}
