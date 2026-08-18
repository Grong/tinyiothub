//! Provider 缝 — `ProviderFactory` 类型与 minimax provider 设置注册
//!（Task 14 自 apps/cloud `host/autonomous_factory.rs` / `host/ports.rs` 迁入）。
//!
//! 组合层启动时自配置 `[minimax]` 段注册设置；provider 按 agent 构建
//! （zeroclaw 中 provider 是 per-agent 的）。

use std::sync::Arc;

use zeroclaw::providers::traits::ModelProvider;

/// Builds a fresh model provider per agent (providers are per-agent in
/// zeroclaw). Production wires [`minimax_provider_factory`]; tests inject a
/// scripted provider.
pub type ProviderFactory = Arc<dyn Fn() -> anyhow::Result<Box<dyn ModelProvider>> + Send + Sync>;

/// `[minimax]` provider settings, registered by the composition layer from
/// its config at startup.
#[derive(Debug, Clone)]
pub struct MinimaxSettings {
    pub base_url: String,
    pub auth_token: String,
    pub model: String,
}

static MINIMAX_SETTINGS: std::sync::RwLock<Option<MinimaxSettings>> = std::sync::RwLock::new(None);

/// Register the minimax provider settings (composition layer at startup).
/// Also seeds [`crate::config::set_default_model`] from the same section.
pub fn set_minimax_settings(settings: MinimaxSettings) {
    crate::config::set_default_model(settings.model.clone());
    *MINIMAX_SETTINGS.write().expect("minimax settings lock poisoned") = Some(settings);
}

/// The registered minimax provider settings, if any.
pub fn minimax_settings() -> Option<MinimaxSettings> {
    MINIMAX_SETTINGS.read().expect("minimax settings lock poisoned").clone()
}

/// Create a MiniMax model provider from the registered settings.
pub fn create_minimax_provider() -> anyhow::Result<Box<dyn ModelProvider>> {
    let cfg =
        minimax_settings().ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
    zeroclaw::providers::create_model_provider_with_url("minimaxi", Some(&cfg.auth_token), Some(&cfg.base_url))
}

/// Production provider factory — `[minimax]` settings registered by the
/// composition layer (see [`set_minimax_settings`]).
pub fn minimax_provider_factory() -> ProviderFactory {
    Arc::new(create_minimax_provider)
}
