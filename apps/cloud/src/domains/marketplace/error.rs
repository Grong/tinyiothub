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

    #[error("参数校验失败: {0}")]
    Validation(String),

    /// 场景包展开失败（结构化的 ExpandError，400；display 与 Validation 一致）。
    #[error("参数校验失败: {0}")]
    Expand(#[from] tinyiothub_storage::scene_template::ExpandError),

    #[error("Driver error: {0}")]
    Driver(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Publish failed: {0}")]
    PublishFailed(String),

    /// SQLite 锁竞争（BUSY/LOCKED 扩展码）：场景实例化整事务回滚重试的信号；
    /// 重试耗尽后按 500 处理。
    #[error("数据库锁竞争: {0}")]
    LockContention(String),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;
