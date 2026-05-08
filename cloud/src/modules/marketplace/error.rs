use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid checksum: expected {expected}, got {actual}")]
    InvalidChecksum { expected: String, actual: String },

    #[error("Platform not supported: {0}")]
    PlatformNotSupported(String),

    #[error("Marketplace is disabled")]
    Disabled,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Driver error: {0}")]
    Driver(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Publish failed: {0}")]
    PublishFailed(String),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;
