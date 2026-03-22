use std::fmt;

#[derive(Debug, Clone)]
pub enum Error {
    Internal(String),
    NotFound,
    InvalidArgument(String),
    Unsupported(String),
    IOError(String),
    NetworkError(String),
    ConfigError(String),
    ValidationError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Internal(ref s) => write!(f, "core internal error: {s}"),
            Error::NotFound => write!(f, "not found"),
            Error::InvalidArgument(ref s) => write!(f, "core invalid argument: {s}"),
            Error::Unsupported(ref s) => write!(f, "core unsupported error: {s}"),
            Error::IOError(ref s) => write!(f, "core io error: {s}"),
            Error::NetworkError(ref s) => write!(f, "network error: {s}"),
            Error::ConfigError(ref s) => write!(f, "config error: {s}"),
            Error::ValidationError(ref s) => write!(f, "validation error: {s}"),
        }
    }
}

impl std::convert::From<std::num::ParseFloatError> for Error {
    fn from(err: std::num::ParseFloatError) -> Self {
        Error::InvalidArgument(err.to_string())
    }
}

impl std::convert::From<std::num::ParseIntError> for Error {
    fn from(err: std::num::ParseIntError) -> Self {
        Error::InvalidArgument(err.to_string())
    }
}

impl std::convert::From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IOError(err.to_string())
    }
}

impl From<sqlx::Error> for Error {
    fn from(_err: sqlx::Error) -> Self {
        // Map to a generic error to avoid leaking table/column names
        Error::Internal("Database operation failed".to_string())
    }
}

impl std::error::Error for Error {}
