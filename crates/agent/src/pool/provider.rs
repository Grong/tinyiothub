//! Provider 缝 — `ProviderFactory` 类型与 minimax provider 设置注册
//!（Task 14 自 apps/cloud `host/autonomous_factory.rs` / `host/ports.rs` 迁入）。
//!
//! 组合层启动时自配置 `[minimax]` 段注册设置；provider 按 agent 构建
//! （provider 是 per-agent 的）。引擎 provider 的具体构建住
//! adapters/zeroclaw（zeroclaw 是 adapter 内部细节）。

use std::sync::Arc;

use crate::port::provider::ModelProvider;

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

static MINIMAX_SETTINGS: parking_lot::RwLock<Option<MinimaxSettings>> = parking_lot::RwLock::new(None);

/// Register the minimax provider settings (composition layer at startup).
/// Also seeds [`crate::config::set_default_model`] from the same section.
pub fn set_minimax_settings(settings: MinimaxSettings) {
    crate::config::set_default_model(settings.model.clone());
    *MINIMAX_SETTINGS.write() = Some(settings);
}

/// The registered minimax provider settings, if any.
pub fn minimax_settings() -> Option<MinimaxSettings> {
    MINIMAX_SETTINGS.read().clone()
}

/// Create a MiniMax model provider from the registered settings.
pub fn create_minimax_provider() -> anyhow::Result<Box<dyn ModelProvider>> {
    let cfg =
        minimax_settings().ok_or_else(|| anyhow::anyhow!("[minimax] config section is required but not found"))?;
    create_minimax_provider_with(&cfg)
}

/// Create a MiniMax model provider from explicit settings (cloud composition
/// layer passes its `[minimax]` config slice here).
pub fn create_minimax_provider_with(cfg: &MinimaxSettings) -> anyhow::Result<Box<dyn ModelProvider>> {
    // rig minimax provider：OpenAI 兼容协议，base_url/auth_token 直接对应。
    use rig_core::client::CompletionClient;
    let client = rig_core::providers::minimax::Client::builder()
        .api_key(cfg.auth_token.clone())
        .base_url(&cfg.base_url)
        .build()
        .map_err(|e| anyhow::anyhow!("[minimax] rig client build failed: {e}"))?;
    Ok(Box::new(crate::adapters::rig::provider::RigMinimaxAsPort::new(
        client.completion_model(&cfg.model),
    )))
}

/// Production provider factory — `[minimax]` settings registered by the
/// composition layer (see [`set_minimax_settings`]).
pub fn minimax_provider_factory() -> ProviderFactory {
    Arc::new(create_minimax_provider)
}
