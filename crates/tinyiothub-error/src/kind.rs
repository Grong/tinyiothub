//! Error kind classification.

/// Classification of errors for programmatic handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Internal bug or invariant violation.
    Internal,
    /// Resource not found.
    NotFound,
    /// Invalid argument or bad input.
    InvalidArgument,
    /// Unsupported operation.
    Unsupported,
    /// I/O failure.
    Io,
    /// Network failure.
    Network,
    /// Configuration error.
    Config,
    /// Validation failure.
    Validation,
    /// Database error.
    Database,
    /// Serialization/deserialization error.
    Serialization,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ErrorKind::Internal => "internal",
            ErrorKind::NotFound => "not_found",
            ErrorKind::InvalidArgument => "invalid_argument",
            ErrorKind::Unsupported => "unsupported",
            ErrorKind::Io => "io",
            ErrorKind::Network => "network",
            ErrorKind::Config => "config",
            ErrorKind::Validation => "validation",
            ErrorKind::Database => "database",
            ErrorKind::Serialization => "serialization",
        };
        write!(f, "{}", name)
    }
}
