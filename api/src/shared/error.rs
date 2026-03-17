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
    fn from(err: sqlx::Error) -> Self {
        Error::IOError(err.to_string())
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(format!("{}", Error::NotFound), "not found");
        assert_eq!(
            format!("{}", Error::Internal("something wrong".to_string())),
            "core internal error: something wrong"
        );
        assert_eq!(
            format!("{}", Error::InvalidArgument("invalid param".to_string())),
            "core invalid argument: invalid param"
        );
        assert_eq!(
            format!("{}", Error::Unsupported("feature".to_string())),
            "core unsupported error: feature"
        );
        assert_eq!(
            format!("{}", Error::IOError("file not found".to_string())),
            "core io error: file not found"
        );
        assert_eq!(
            format!("{}", Error::NetworkError("connection refused".to_string())),
            "network error: connection refused"
        );
        assert_eq!(
            format!("{}", Error::ConfigError("missing key".to_string())),
            "config error: missing key"
        );
        assert_eq!(
            format!("{}", Error::ValidationError("invalid email".to_string())),
            "validation error: invalid email"
        );
    }

    #[test]
    fn test_error_debug() {
        let err = Error::NotFound;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("NotFound"));
    }

    #[test]
    fn test_error_clone() {
        let err = Error::Internal("test".to_string());
        let cloned = err.clone();
        assert_eq!(format!("{}", cloned), "core internal error: test");
    }

    #[test]
    fn test_error_from_parse_int() {
        let err: Error = "abc".parse::<i32>().unwrap_err().into();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn test_error_from_parse_float() {
        let err: Error = "xyz".parse::<f64>().unwrap_err().into();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }
}
