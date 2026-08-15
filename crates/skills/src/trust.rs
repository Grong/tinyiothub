//! Trust engine — evaluates tool trust at execution time.
//!
//! Trust is layered:
//! 1. Intrinsic safety (read-only tools auto-execute, destructive always blocked)
//! 2. TrustConfig overrides (block specific tools, allow specific write tools)
//! 3. Global trust_level fallback (ReadOnlyAuto / FullAuto / ApprovalRequired)

use tinyiothub_core::heartbeat::{TrustConfig, TrustLevel};

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

    // known read-only tools that don't follow verb_noun convention
    // canvas: A2UI rendering — pushes UI components, does not modify data
    if tool_name == "canvas" {
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

/// Evaluate whether a tool can auto-execute given the workspace trust config,
/// classifying safety from the tool name. Prefer
/// [`evaluate_tool_trust_with_safety`] when the registry's declared safety is
/// available — name patterns are only a fallback.
///
/// Rules (in priority order):
/// 1. Explicitly blocked tool → Block
/// 2. Category not in allowed_tool_categories (non-empty) → Propose
/// 3. Read-only tool → Allow (safe by definition)
/// 4. Destructive tool + FullAuto + explicit allowlist → Allow, else Propose
/// 5. Write tool + FullAuto → Allow
/// 6. Write tool + ReadOnlyAuto / ApprovalRequired → Propose
pub fn evaluate_tool_trust(config: &TrustConfig, tool_name: &str) -> TrustDecision {
    evaluate_tool_trust_with_safety(config, tool_name, classify_tool_safety(tool_name))
}

/// Same as [`evaluate_tool_trust`] but with an explicitly declared safety.
/// The declared safety is authoritative; the tool name is only used for
/// block-list matching and messages.
pub fn evaluate_tool_trust_with_safety(config: &TrustConfig, tool_name: &str, safety: ToolSafety) -> TrustDecision {
    // 1. Explicit block list
    if config.blocked_tools.iter().any(|t| t == tool_name) {
        return TrustDecision::Block {
            reason: format!(
                "Tool '{}' is explicitly blocked by workspace trust config. Do not retry.",
                tool_name
            ),
        };
    }

    // 2. Category gate for read/write tools: a non-empty
    // allowed_tool_categories list must contain the tool's safety category.
    // "query" is accepted as a legacy alias of "read"; an empty list means no
    // restriction. Destructive tools are exempt — they are governed by the
    // stricter per-tool allowed_destructive_tools allowlist below.
    let category = safety_category(safety);
    if !matches!(safety, ToolSafety::Destructive)
        && !config.allowed_tool_categories.is_empty()
        && !config
            .allowed_tool_categories
            .iter()
            .any(|c| c == category || (category == "read" && c == "query"))
    {
        return TrustDecision::Propose {
            reason: format!(
                "Tool '{}' is in category '{}', which is not in the workspace's \
                 allowed_tool_categories. Propose this action in pending_proposals instead.",
                tool_name, category
            ),
        };
    }

    // 3. Read-only tools are intrinsically safe — always allow
    if matches!(safety, ToolSafety::ReadOnly) {
        return TrustDecision::Allow;
    }

    // 4. Destructive tools require explicit allowlisting even under FullAuto
    if matches!(safety, ToolSafety::Destructive) {
        if config.trust_level == TrustLevel::FullAuto && config.allowed_destructive_tools.iter().any(|t| t == tool_name)
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

    // 5. Write tools: check global trust level
    match config.trust_level {
        TrustLevel::FullAuto => TrustDecision::Allow,
        TrustLevel::ReadOnlyAuto | TrustLevel::ApprovalRequired => TrustDecision::Propose {
            reason: format!(
                "Tool '{}' requires human approval under current trust level ({:?}). \
                 Propose this action in pending_proposals with tool_name, device_id, \
                 summary, reason, and risk.",
                tool_name, config.trust_level,
            ),
        },
    }
}

/// Safety category string used for allowed_tool_categories matching.
pub fn safety_category(safety: ToolSafety) -> &'static str {
    match safety {
        ToolSafety::ReadOnly => "read",
        ToolSafety::Write => "write",
        ToolSafety::Destructive => "destructive",
    }
}

/// Risk label for a proposal, computed from tool safety — never trust the
/// LLM's self-reported risk, which it can understate.
pub fn risk_for_tool(tool_name: &str) -> &'static str {
    match classify_tool_safety(tool_name) {
        ToolSafety::Destructive => "high",
        ToolSafety::Write => "medium",
        ToolSafety::ReadOnly => "low",
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
        // known read-only tools that don't follow verb_noun convention
        assert_eq!(classify_tool_safety("canvas"), ToolSafety::ReadOnly);
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
        assert_eq!(evaluate_tool_trust(&config, "get_device"), TrustDecision::Allow);
        assert_eq!(evaluate_tool_trust(&config, "search_devices"), TrustDecision::Allow);
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
        assert_eq!(evaluate_tool_trust(&config, "write_properties"), TrustDecision::Allow);
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
        assert_eq!(evaluate_tool_trust(&config, "delete_device"), TrustDecision::Allow);
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

    #[test]
    fn test_declared_safety_overrides_name_pattern() {
        let config = default_config();
        // Name says "delete_" (destructive by pattern), but the registry
        // declares the tool read-only — declaration wins.
        assert_eq!(
            evaluate_tool_trust_with_safety(&config, "delete_device", ToolSafety::ReadOnly),
            TrustDecision::Allow
        );
        // Inverse: innocent name, declared destructive.
        assert!(matches!(
            evaluate_tool_trust_with_safety(&config, "get_device", ToolSafety::Destructive),
            TrustDecision::Propose { .. }
        ));
    }

    #[test]
    fn test_allowed_tool_categories_gate_auto_execution() {
        // Write tool under FullAuto, but "write" not in allowed categories.
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            allowed_tool_categories: vec!["read".into()],
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "write_properties"),
            TrustDecision::Propose { .. }
        ));
        // Read tool allowed via "query" legacy alias.
        let config = TrustConfig {
            trust_level: TrustLevel::ReadOnlyAuto,
            allowed_tool_categories: vec!["query".into()],
            ..default_config()
        };
        assert_eq!(evaluate_tool_trust(&config, "get_device"), TrustDecision::Allow);
        // Read tool blocked when neither "read" nor "query" is allowed.
        let config = TrustConfig {
            trust_level: TrustLevel::ReadOnlyAuto,
            allowed_tool_categories: vec!["write".into()],
            ..default_config()
        };
        assert!(matches!(
            evaluate_tool_trust(&config, "get_device"),
            TrustDecision::Propose { .. }
        ));
    }

    #[test]
    fn test_empty_allowed_tool_categories_means_no_restriction() {
        let config = TrustConfig {
            trust_level: TrustLevel::FullAuto,
            allowed_tool_categories: vec![],
            ..default_config()
        };
        assert_eq!(evaluate_tool_trust(&config, "write_properties"), TrustDecision::Allow);
        assert_eq!(evaluate_tool_trust(&config, "get_device"), TrustDecision::Allow);
    }

    #[test]
    fn test_risk_for_tool_computed_from_safety() {
        assert_eq!(risk_for_tool("firmware_update"), "high");
        assert_eq!(risk_for_tool("delete_device"), "high");
        assert_eq!(risk_for_tool("write_properties"), "medium");
        assert_eq!(risk_for_tool("get_device"), "low");
    }
}
