// Configuration Module - Using config crate for zero-boilerplate config management
pub use tinyiothub_config::*;

pub mod settings {
    pub use tinyiothub_config::{
        AliyunSmsConfig, ApplicationSettings, MarketplaceConfig, SmsConfig,
    };
}

use std::sync::OnceLock;

/// Global configuration instance
static CONFIG: OnceLock<ApplicationSettings> = OnceLock::new();

/// Initialize the global configuration
pub fn initialize() -> Result<(), ConfigError> {
    let settings = load_configuration()?;
    CONFIG
        .set(settings)
        .map_err(|_| ConfigError::ValidationError("Failed to initialize config".to_string()))?;

    tracing::info!("Configuration initialized successfully");
    Ok(())
}

/// Load configuration using config crate
/// Priority: Environment variables > app_settings.toml > defaults
///
/// Environment variable format: TINYIOTHUB__SECTION__KEY
/// Example: TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db
pub fn load_configuration() -> Result<ApplicationSettings, ConfigError> {
    use config::{Config, Environment, File};

    let settings = Config::builder()
        // 1. 从 app_settings.toml 加载（如果存在）
        .add_source(File::with_name("app_settings").required(false))
        // 2. 从环境变量覆盖（自动处理 TINYIOTHUB__ 前缀，双下划线表示嵌套）
        .add_source(Environment::with_prefix("TINYIOTHUB").separator("__").try_parsing(true))
        .build()
        .map_err(|e| ConfigError::ParseError(format!("Failed to build config: {}", e)))?;

    let app_settings: ApplicationSettings = settings
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError(format!("Failed to deserialize config: {}", e)))?;

    // 打印关键配置信息
    tracing::info!("Database URL: {}", app_settings.database.url);
    tracing::info!("Server: {}:{}", app_settings.server.host, app_settings.server.port);

    app_settings.validate()?;

    Ok(app_settings)
}

/// Get the global configuration instance
pub fn get() -> &'static ApplicationSettings {
    CONFIG.get().expect("Configuration not initialized. Call config::initialize() first")
}

/// Get the global configuration if initialized, otherwise None.
pub fn try_get() -> Option<&'static ApplicationSettings> {
    CONFIG.get()
}

/// Create a MiniMax model provider using the configured base_url and auth_token.
///
/// Reads `[minimax]` section from app_settings.toml. Returns an error if the
/// section is missing or if provider construction fails.
pub fn create_minimax_provider()
-> anyhow::Result<Box<dyn zeroclaw::providers::traits::ModelProvider>> {
    let cfg = try_get()
        .and_then(|s| s.minimax.as_ref())
        .ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
    zeroclaw::providers::create_model_provider_with_url(
        "minimaxi",
        Some(&cfg.auth_token),
        Some(&cfg.base_url),
    )
}

/// Get environment name
pub fn environment() -> &'static str {
    get().environment()
}
