//! TinyIoTHub error types
//!
//! This crate extends `tinyiothub-core::error` with `thiserror` derives
//! and `From` conversions for external framework errors (sqlx, serde_json, io).
//!
//! Usage in application code:
//! ```ignore
//! use tinyiothub_error::{Error, Result};
//! ```

pub mod context;
pub mod kind;

pub use context::ErrorContext;
pub use kind::ErrorKind;

use thiserror::Error as ThisError;
use tinyiothub_core::error::Error as CoreError;

/// Framework-level error enum with automatic `From` conversions.
#[derive(Debug, ThisError)]
pub enum Error {
    #[error("{0}")]
    Core(#[from] CoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

impl Error {
    /// Convert to core error (lossy — framework detail dropped)
    pub fn into_core(self) -> CoreError {
        match self {
            Error::Core(e) => e,
            Error::Io(e) => CoreError::IOError(e.to_string()),
            Error::Sqlx(e) => CoreError::DatabaseError(e.to_string()),
            Error::SerdeJson(e) => CoreError::SerializationError(e.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
