// Event module — 3-layer architecture
// types: DTOs and query types
// repo:  repository traits
// service: aggregates, specifications, business logic
// handler: HTTP routes

pub mod types;
pub mod repo;
pub mod service;
pub mod errors;
pub mod handler;

// Backward compatibility: re-export core types as submodules
pub mod entities {
    pub use tinyiothub_core::models::event::Event;
}

pub mod value_objects {
    pub use tinyiothub_core::models::event::{
        ConnectionStatus, ContentElement, DeviceEventType, EventId, EventLevel, EventSource,
        EventType, LinkTarget, RichContent, SystemEventType, TextFormat,
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

impl From<String> for EventError {
    fn from(msg: String) -> Self {
        EventError::Validation { message: msg }
    }
}

impl From<&str> for EventError {
    fn from(msg: &str) -> Self {
        EventError::Validation { message: msg.to_string() }
    }
}

impl From<crate::shared::error::Error> for EventError {
    fn from(err: crate::shared::error::Error) -> Self {
        EventError::Gateway(err.to_string())
    }
}

// Re-export core event types
pub use tinyiothub_core::models::event::{
    ConnectionStatus, ContentElement, DeviceEventType, Event, EventId, EventLevel, EventSource,
    EventType, LinkTarget, RichContent, SystemEventType, TextFormat,
};

pub use types::*;
pub use repo::*;
pub use service::*;
pub use handler::*;

// Backward compatibility: old module paths
pub mod repositories {
    pub use super::repo::*;
}

/// Re-export EventAggregate for backward compat (was in aggregates/ subdirectory)
pub use service::EventAggregate;

/// Backward compatibility: old aggregates::NotificationChannelType path
pub mod aggregates {
    pub use crate::modules::notification::types::NotificationChannelType;
    pub use super::service::EventAggregate;
}

// Re-export errors module types at top level for convenience
pub use errors::{DomainResult, DomainError, EventDomainError, EventServiceDomainError,
    NotificationDomainError, PerformanceDomainError, SecurityDomainError};
