//! Unified error types for TinyIoTHub.
//!
//! All crates in the workspace should use `tinyiothub_error::Error` as their
//! primary error type. Domain-specific error enums should implement
//! `From<DomainError> for Error` to integrate with the unified system.

/// Unified error type for the entire TinyIoTHub workspace.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("internal error: {0}")]
    Internal(String),

    #[error("not found")]
    NotFound,

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("io error: {0}")]
    IOError(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("validation error: {0}")]
    ValidationError(String),

    #[error("database error: {0}")]
    DatabaseError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),
}

/// Unified Result type alias.
pub type Result<T> = std::result::Result<T, Error>;

// --- From conversions for common external errors ---

impl From<std::num::ParseFloatError> for Error {
    fn from(err: std::num::ParseFloatError) -> Self {
        Error::InvalidArgument(err.to_string())
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(err: std::num::ParseIntError) -> Self {
        Error::InvalidArgument(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IOError(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerializationError(err.to_string())
    }
}

#[cfg(feature = "sqlx-dep")]
impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::DatabaseError(err.to_string())
    }
}

