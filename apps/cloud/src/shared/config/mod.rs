// Configuration Module - Using config crate for zero-boilerplate config management
//
// 禁止新增进程级配置全局（G6 裁决）；配置经 AppState 注入。
// main.rs 启动时调用 `load_configuration()` 得到局部 `ApplicationSettings`，
// 随后传入 `AppState::new`；handler 从 `State` 取切片，service 经构造参数获取。
pub use tinyiothub_core::config::*;

pub mod settings {
    pub use tinyiothub_core::config::{AliyunSmsConfig, ApplicationSettings, MarketplaceConfig, SmsConfig};
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

/// Create a MiniMax model provider using the configured base_url and auth_token.
///
/// Takes the `[minimax]` config slice from the caller (G6 — injected, not global).
pub fn create_minimax_provider(
    cfg: &MinimaxConfig,
) -> anyhow::Result<Box<dyn zeroclaw::providers::traits::ModelProvider>> {
    zeroclaw::providers::create_model_provider_with_url("minimaxi", Some(&cfg.auth_token), Some(&cfg.base_url))
}
