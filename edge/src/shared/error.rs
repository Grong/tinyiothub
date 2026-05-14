use thiserror::Error;

#[derive(Debug, Error)]
pub enum EdgeError {
    #[error("MQTT authentication rejected")]
    AuthRejected,

    #[error("Disk full: {0}")]
    DiskFull(String),

    #[error("Payload too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: usize },

    #[error("Config parse error: {0}")]
    ConfigParse(String),

    #[error("Version conflict: local={local}, cloud={cloud}")]
    VersionConflict { local: String, cloud: String },

    #[error("Rule compile error: {0}")]
    RuleCompile(String),

    #[error("Rule timeout: {0}")]
    RuleTimeout(String),

    #[error("Probe panicked: {0}")]
    ProbePanic(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Scan already in progress")]
    ScanBusy,

    #[error("Too many requests")]
    TooManyRequests,

    #[error(transparent)]
    Core(#[from] tinyiothub_error::Error),

    #[error("{0}")]
    Internal(String),
}

impl From<sqlx::Error> for EdgeError {
    fn from(e: sqlx::Error) -> Self {
        EdgeError::Internal(e.to_string())
    }
}

impl From<std::io::Error> for EdgeError {
    fn from(e: std::io::Error) -> Self {
        EdgeError::Internal(e.to_string())
    }
}

impl From<serde_json::Error> for EdgeError {
    fn from(e: serde_json::Error) -> Self {
        EdgeError::Internal(e.to_string())
    }
}

impl From<serde_yaml::Error> for EdgeError {
    fn from(e: serde_yaml::Error) -> Self {
        EdgeError::ConfigParse(e.to_string())
    }
}

impl From<String> for EdgeError {
    fn from(s: String) -> Self {
        EdgeError::Internal(s)
    }
}

impl From<&str> for EdgeError {
    fn from(s: &str) -> Self {
        EdgeError::Internal(s.to_string())
    }
}

pub type EdgeResult<T> = Result<T, EdgeError>;
