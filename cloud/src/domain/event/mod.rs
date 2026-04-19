// DDD Domain Layer - Event Domain
// This module contains pure business logic with no infrastructure dependencies

use std::fmt::Debug;

// Domain aggregates (aggregate roots)
pub mod aggregates;

// Domain entities
pub mod entities;

// Value objects
pub mod value_objects;

// Repository interfaces (defined in domain, implemented in infrastructure)
pub mod repositories;

// Domain services (pure business logic)
pub mod services;

// Domain specifications (business rules)
pub mod specifications;

// Domain errors
pub mod errors;

// Re-export DDD components

// Legacy compatibility exports (will be removed after full migration)
pub use aggregates::{
    NotificationChannelType, NotificationRecord, NotificationRule, NotificationStatus,
};

/// Event system errors (legacy - use DomainError instead)
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

// Additional From implementations for EventError
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
