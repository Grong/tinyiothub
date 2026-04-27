//! 驱动错误类型定义

use std::fmt;

/// 驱动错误类型
#[derive(Debug, Clone)]
pub enum DriverError {
    /// 网络错误
    NetworkError(String),
    /// IO错误
    IOError(String),
    /// 配置错误
    ConfigError(String),
    /// 验证错误
    ValidationError(String),
    /// 不支持的操作
    Unsupported(String),
    /// 内部错误
    Internal(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            DriverError::IOError(msg) => write!(f, "IO error: {}", msg),
            DriverError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            DriverError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            DriverError::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
            DriverError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for DriverError {}

#[cfg(feature = "core-interop")]
impl From<tinyiothub_core::error::Error> for DriverError {
    fn from(err: tinyiothub_core::error::Error) -> Self {
        match err {
            tinyiothub_core::error::Error::NetworkError(msg) => DriverError::NetworkError(msg),
            tinyiothub_core::error::Error::IOError(msg) => DriverError::IOError(msg),
            tinyiothub_core::error::Error::ConfigError(msg) => DriverError::ConfigError(msg),
            tinyiothub_core::error::Error::ValidationError(msg) => DriverError::ValidationError(msg),
            tinyiothub_core::error::Error::Unsupported(msg) => DriverError::Unsupported(msg),
            other => DriverError::Internal(other.to_string()),
        }
    }
}

/// 驱动结果类型
pub type Result<T> = std::result::Result<T, DriverError>;
