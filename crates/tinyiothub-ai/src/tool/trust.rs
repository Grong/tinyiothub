//! Trust engine -- evaluates tool trust at execution time.

use std::sync::Arc;

use crate::patrol::types::{resolve_trust, TrustConfig, TrustLevel};

/// Engine for evaluating tool trust at execution time.
pub struct TrustEngine {
    config: TrustConfig,
    workspace_id: String,
}

impl TrustEngine {
    pub fn new(config: TrustConfig, workspace_id: String) -> Self {
        Self { config, workspace_id }
    }

    /// Check if a tool is allowed to auto-execute given its category.
    pub fn check(&self, tool_category: &str) -> TrustLevel {
        resolve_trust(&self.config, tool_category)
    }

    pub fn trust_level(&self) -> TrustLevel {
        self.config.trust_level
    }

    pub fn max_auto_actions(&self) -> u32 {
        self.config.max_auto_actions_per_tick
    }

    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.config.blocked_tools.iter().any(|t| t == tool_name)
    }

    #[allow(dead_code)]
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

/// Wrapper that enforces trust before tool execution.
pub struct TrustAwareTool<T> {
    inner: T,
    engine: Arc<TrustEngine>,
    category: String,
}

impl<T> TrustAwareTool<T> {
    pub fn new(inner: T, engine: Arc<TrustEngine>, category: String) -> Self {
        Self {
            inner,
            engine,
            category,
        }
    }

    pub fn check_trust(&self) -> TrustLevel {
        self.engine.check(&self.category)
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}
