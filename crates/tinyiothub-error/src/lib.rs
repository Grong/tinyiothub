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

    #[error("driver error: {0}")]
    DriverError(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(Error::Internal("boom".into()).to_string(), "internal error: boom");
        assert_eq!(Error::NotFound.to_string(), "not found");
        assert_eq!(
            Error::InvalidArgument("bad".into()).to_string(),
            "invalid argument: bad"
        );
        assert_eq!(Error::Unsupported("feat".into()).to_string(), "unsupported: feat");
        assert_eq!(Error::IOError("disk".into()).to_string(), "io error: disk");
        assert_eq!(
            Error::NetworkError("timeout".into()).to_string(),
            "network error: timeout"
        );
        assert_eq!(
            Error::ConfigError("missing".into()).to_string(),
            "config error: missing"
        );
        assert_eq!(
            Error::ValidationError("required".into()).to_string(),
            "validation error: required"
        );
        assert_eq!(
            Error::DatabaseError("locked".into()).to_string(),
            "database error: locked"
        );
        assert_eq!(
            Error::SerializationError("json".into()).to_string(),
            "serialization error: json"
        );
    }

    #[test]
    fn test_from_parse_float_error() {
        let err: Error = "not_a_float".parse::<f64>().unwrap_err().into();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn test_from_parse_int_error() {
        let err: Error = "not_an_int".parse::<i32>().unwrap_err().into();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::IOError(_)));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::SerializationError(_)));
    }

    #[test]
    fn test_result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert!(matches!(ok, Ok(42)));

        let err: Result<i32> = Err(Error::NotFound);
        assert!(err.is_err());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }
}
