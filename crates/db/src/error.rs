//! Database error type (buzz-db pattern: one semantic enum per db crate).

/// Errors produced by database operations.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// A SQLx driver-level error.
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A SQLx migration error.
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A field failed validation.
    #[error("validation error: {message}")]
    Validation {
        /// What failed validation.
        message: String,
    },

    /// The requested row does not exist.
    #[error("not found: {id}")]
    NotFound {
        /// The missing row identifier.
        id: String,
    },

    /// Catch-all for invariant violations inside repositories.
    #[error("internal error: {0}")]
    Internal(String),

    /// The caller lacks permission for the requested operation.
    #[error("access denied: {0}")]
    AccessDenied(String),
}

/// db crate 统一 Result。
pub type Result<T> = std::result::Result<T, DbError>;
