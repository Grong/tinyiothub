//! Storage-specific errors.

/// Errors that can occur in the storage layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    ConnectionFailed(String),
    QueryFailed(String),
    NotFound,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::ConnectionFailed(msg) => write!(f, "connection failed: {}", msg),
            StorageError::QueryFailed(msg) => write!(f, "query failed: {}", msg),
            StorageError::NotFound => write!(f, "resource not found"),
        }
    }
}

impl std::error::Error for StorageError {}
