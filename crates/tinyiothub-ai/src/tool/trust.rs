//! Trust engine — evaluates tool trust at execution time.
//!
//! Trust is layered:
//! 1. Intrinsic safety (read-only tools auto-execute, destructive always blocked)
//! 2. TrustConfig overrides (block specific tools, allow specific write tools)
//! 3. Global trust_level fallback (ReadOnlyAuto / FullAuto / ApprovalRequired)

use serde::{Deserialize, Serialize};

/// Trust level for automatic tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// All tools require human approval.
    ApprovalRequired,
    /// Read-only tools auto-execute; write tools require approval.
    ReadOnlyAuto,
    /// All tools auto-execute.
    FullAuto,
}

/// Per-workspace trust configuration for tool auto-execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    pub trust_level: TrustLevel,
    pub max_auto_actions_per_tick: u32,
    pub allowed_tool_categories: Vec<String>,
    pub blocked_tools: Vec<String>,
    /// Destructive tools explicitly allowlisted by workspace admin.
    /// Only takes effect under FullAuto; all other levels still require approval.
    #[serde(default)]
    pub allowed_destructive_tools: Vec<String>,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::ReadOnlyAuto,
            max_auto_actions_per_tick: 10,
            allowed_tool_categories: vec!["read".into(), "query".into()],
            blocked_tools: vec![],
            allowed_destructive_tools: vec![],
        }
    }
}

impl TrustConfig {
    /// Load from DB JSON column, falling back to safe default.
    pub fn from_db_json(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }

    /// Serialize to JSON for DB storage.
    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Intrinsic safety classification derived from tool naming conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSafety {
    /// Read-only: list_, get_, read_, search_, query_, *_status, *_statistics
    ReadOnly,
    /// Write: everything that doesn't match read/destructive patterns
    Write,
    /// Destructive: delete_, remove_, unload_, firmware, reset, reboot, factory
    Destructive,
}

/// Classify a tool by its name. Pattern-based, not hardcoded lists —
/// any future tool following naming conventions gets the right safety level.
pub fn classify_tool_safety(tool_name: &str) -> ToolSafety {
    // destructive patterns — comprehensive coverage for data-loss and irreversible ops
    if tool_name.starts_with("delete_")
        || tool_name.starts_with("remove_")
        || tool_name.starts_with("unload_")
        || tool_name.starts_with("purge_")
        || tool_name.starts_with("wipe_")
        || tool_name.starts_with("clear_all_")
        || tool_name.starts_with("destroy_")
        || tool_name.starts_with("format_")
        || tool_name.starts_with("erase_")
        || tool_name.starts_with("overwrite_")
        || tool_name.starts_with("drop_")
        || tool_name.starts_with("truncate_")
        || tool_name.contains("firmware")
        || tool_name.contains("reset")
        || tool_name.contains("reboot")
        || tool_name.contains("factory")
    {
        return ToolSafety::Destructive;
    }

    // read-only patterns (prefix: action_domain, suffix: domain_action)
    if tool_name.starts_with("list_")
        || tool_name.starts_with("get_")
        || tool_name.starts_with("read_")
        || tool_name.starts_with("search_")
        || tool_name.ends_with("_list")
        || tool_name.ends_with("_get")
        || tool_name.ends_with("_read")
        || tool_name.ends_with("_query")
        || tool_name.ends_with("_search")
        || tool_name.ends_with("_statistics")
        || tool_name.ends_with("_status")
    {
        return ToolSafety::ReadOnly;
    }

    ToolSafety::Write
}

/// Outcome of a trust evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// Execute immediately.
    Allow,
    /// Block with reason — tool should not execute.
    Block { reason: String },
    /// Block, but the LLM can propose this action for human approval.
    Propose { reason: String },
}

/// Evaluate whether a tool can auto-execute given the workspace trust config.
///
/// Rules (in priority order):
/// 1. Explicitly blocked tool → Block
/// 2. Read-only tool → Allow (safe by definition)
/// 3. Destructive tool + not FullAuto → Propose
/// 4. Write tool + FullAuto → Allow
/// 5. Write tool + ReadOnlyAuto → Propose
/// 6. Write tool + ApprovalRequired → Propose
pub fn evaluate_tool_trust(config: &TrustConfig, tool_name: &str) -> TrustDecision {
    // 1. Explicit block list
    if config.blocked_tools.iter().any(|t| t == tool_name) {
        return TrustDecision::Block {
            reason: format!(
                "Tool '{}' is explicitly blocked by workspace trust config. Do not retry.",
                tool_name
            ),
        };
    }

    let safety = classify_tool_safety(tool_name);

    // 2. Read-only tools are intrinsically safe — always allow
    if matches!(safety, ToolSafety::ReadOnly) {
        return TrustDecision::Allow;
    }

    // 3. Destructive tools require explicit allowlisting even under FullAuto
    if matches!(safety, ToolSafety::Destructive) {
        if config.trust_level == TrustLevel::FullAuto
            && config.allowed_destructive_tools.iter().any(|t| t == tool_name)
        {
            return TrustDecision::Allow;
        }
        return TrustDecision::Propose {
            reason: format!(
                "Tool '{}' is destructive. It must be explicitly listed in \
                 allowed_destructive_tools under FullAuto trust level. \
                 Propose this action in pending_proposals instead.",
                tool_name
            ),
        };
    }

    // 4. Write tools: check global trust level
    match config.trust_level {
        TrustLevel::FullAuto => TrustDecision::Allow,
        TrustLevel::ReadOnlyAuto | TrustLevel::ApprovalRequired => TrustDecision::Propose {
            reason: format!(
                "Tool '{}' requires human approval under current trust level ({:?}). \
                 Propose this action in pending_proposals with tool_name, device_id, \
                 summary, reason, and risk.",
                tool_name,
                config.trust_level,
            ),
        },
    }
}

/// Engine for evaluating tool trust at execution time.
pub struct TrustEngine {
    config: TrustConfig,
    workspace_id: String,
}

impl TrustEngine {
    pub fn new(config: TrustConfig, workspace_id: String) -> Self {
        Self { config, workspace_id }
    }

    /// Evaluate whether a tool can auto-execute.
    pub fn evaluate(&self, tool_name: &str) -> TrustDecision {
        let decision = evaluate_tool_trust(&self.config, tool_name);
        match &decision {
            TrustDecision::Block { reason } => {
                tracing::warn!(
                    workspace_id = %self.workspace_id,
                    tool = %tool_name,
                    reason = %reason,
                    "Tool blocked by trust engine"
                );
            }
            TrustDecision::Propose { reason } => {
                tracing::debug!(
                    workspace_id = %self.workspace_id,
                    tool = %tool_name,
                    reason = %reason,
                    "Tool requires human approval"
                );
            }
            TrustDecision::Allow => {}
        }
        decision
    }

    pub fn trust_level(&self) -> TrustLevel {
        self.config.trust_level
    }

    pub fn max_auto_actions(&self) -> u32 {
        self.config.max_auto_actions_per_tick
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

/// Wrapper that enforces trust before tool execution.
pub struct TrustAwareTool<T> {
    inner: T,
    engine: std::sync::Arc<TrustEngine>,
    category: String,
}

impl<T> TrustAwareTool<T> {
    pub fn new(inner: T, engine: std::sync::Arc<TrustEngine>, category: String) -> Self {
        Self {
            inner,
            engine,
            category,
        }
    }

    pub fn check_trust(&self) -> TrustDecision {
        self.engine.evaluate(&self.category)
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TrustConfig {
        TrustConfig::default()
    }

    #[test]
    fn test_classify_read_only_tools() {
        assert_eq!(classify_tool_safety("search_devices"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_device"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_properties"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("list_schedules"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("alarm_list"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("config_get"), ToolSafety::ReadOnly);
    }

    #[test]
    fn test_classify_destructive_tools() {
        assert_eq!(classify_tool_safety("delete_device"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("remove_workspace"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("firmware_update"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("reset_device"), ToolSafety::Destructive);
    }

    #[test]
    fn test_classify_write_tools() {
        assert_eq!(classify_tool_safety("write_properties"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("send_command"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("create_device"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("alarm_acknowledge"), ToolSafety::Write);
    }

    #[test]
    fn test_read_only_always_allowed() {
        let config = TrustConfig {
            trust_level: TrustLevel::ApprovalRequired,
            ..default_config()
        };
        // Even with strictest config, read tools auto-execute
        assert_eq!(
            evaluate_tool_trust(&config, "get_device"),
            TrustDecision::Allow
        );
        assert_eq!(
            evaluate_tool_trust(&config, "search_devices"),
            TrustDecision::Allow
        );
    }

    #[test]
    fn test_write_requires_approval_in_read_only_auto() {
        let config = TrustConfig {
            trust_level: TrustLevel::ReadOnlyAuto,
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "write_properties"),
            TrustDecision::Propose { .. }
        ));
    }

    #[test]
    fn test_write_allowed_in_full_auto() {
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            ..default_config()
        };
        assert_eq!(
            evaluate_tool_trust(&config, "write_properties"),
            TrustDecision::Allow
        );
    }

    #[test]
    fn test_destructive_requires_full_auto() {
        let config = TrustConfig {
            trust_level: TrustLevel::ReadOnlyAuto,
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "delete_device"),
            TrustDecision::Propose { .. }
        ));
    }

    #[test]
    fn test_destructive_allowed_when_explicitly_allowlisted() {
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            allowed_destructive_tools: vec!["delete_device".into()],
            ..default_config()
        };
        assert_eq!(
            evaluate_tool_trust(&config, "delete_device"),
            TrustDecision::Allow
        );
    }

    #[test]
    fn test_destructive_blocked_without_allowlist_even_in_full_auto() {
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "delete_device"),
            TrustDecision::Propose { .. }
        ));
    }

    #[test]
    fn test_new_destructive_patterns_caught() {
        assert_eq!(classify_tool_safety("purge_data"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("wipe_device"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("clear_all_caches"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("destroy_workspace"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("format_disk"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("erase_logs"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("overwrite_config"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("drop_table"), ToolSafety::Destructive);
        assert_eq!(classify_tool_safety("truncate_logs"), ToolSafety::Destructive);
    }

    #[test]
    fn test_explicit_block_overrides() {
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            blocked_tools: vec!["get_device".into()],
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "get_device"),
            TrustDecision::Block { .. }
        ));
    }
}

