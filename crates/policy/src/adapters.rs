//! X3 surface adapters — converge the chat-confirm and heartbeat-trust
//! surfaces onto the unified policy engine (O23 arbitration semantics).
//!
//! - [`ChatConfirmAdapter`]: the chat `invoke_action` confirmation flow evaluates through the
//!   engine first; `RequireApproval` maps to minting a confirmation token (existing behavior),
//!   `Allow` to direct dispatch, `Block` to rejection. The workspace `require_action_confirm`
//!   toggle is preserved: with no matching engine rule the toggle decides exactly as before.
//! - [`HeartbeatTrustAdapter`]: translates a [`TrustConfig`] (loaded from the legacy
//!   `workspaces.heartbeat_trust_config` column by the caller) into engine rules and arbitrates
//!   them through [`evaluate_rules`]. O23 equivalence: for the same TrustConfig input the adapter's
//!   verdict equals the legacy heartbeat path (`evaluate_tool_trust_with_safety`) — this is
//!   per-surface equivalence, NOT cross-surface parity.

use std::sync::Arc;

use crate::{PolicyAction, PolicyCategory, PolicyDecision, PolicyEngine, PolicyRule, evaluate_rules};
use tinyiothub_skills::trust::{ToolSafety, TrustDecision, safety_category};
use tinyiothub_storage::heartbeat::{TrustConfig, TrustLevel};

// ── ChatConfirmAdapter ──────────────────────────────────────────

/// Verdict of the chat confirm surface for one `invoke_action` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatConfirmVerdict {
    /// Dispatch immediately.
    Execute,
    /// Mint a confirmation token (the pre-X3 behavior when the workspace
    /// `require_action_confirm` toggle is ON).
    RequireToken,
    /// Refuse the action (engine Block rule).
    Deny { reason: String },
}

/// Chat 接入面适配器：把"要不要确认"的裁决搬进统一引擎。
///
/// The engine is consulted first; the legacy workspace toggle is the
/// fail-closed default when no engine rule matches, so the user-visible
/// interaction is unchanged (toggle ON → token, OFF → direct dispatch).
/// Note: an explicit engine Allow rule does NOT bypass the toggle — the
/// toggle keeps its fail-closed semantics until an admin turns it off.
pub struct ChatConfirmAdapter {
    engine: Arc<dyn PolicyEngine>,
}

impl ChatConfirmAdapter {
    pub fn new(engine: Arc<dyn PolicyEngine>) -> Self {
        Self { engine }
    }

    /// 裁决一次 chat 链路的 invoke_action：Block → 拒绝；RequireApproval →
    /// 发确认令牌；Allow/Flag → 回落到工作区开关。
    pub async fn decide(&self, workspace_id: &str, action_name: &str, require_confirm: bool) -> ChatConfirmVerdict {
        match self
            .engine
            .evaluate(workspace_id, PolicyCategory::AgentAction, action_name)
            .await
        {
            PolicyDecision::Block { reason } => ChatConfirmVerdict::Deny { reason },
            PolicyDecision::RequireApproval { .. } => ChatConfirmVerdict::RequireToken,
            PolicyDecision::Allow | PolicyDecision::Flag { .. } => {
                if require_confirm {
                    ChatConfirmVerdict::RequireToken
                } else {
                    ChatConfirmVerdict::Execute
                }
            }
        }
    }
}

// ── HeartbeatTrustAdapter ───────────────────────────────────────

/// Rule priorities encoding the legacy trust.rs evaluation ORDER, so the
/// unified engine's priority-desc arbitration reproduces it exactly:
/// explicit block > category gate > read-only allow > destructive >
/// write-by-trust-level.
const PRIO_BLOCKED: u32 = 100;
const PRIO_CATEGORY_GATE: u32 = 90;
const PRIO_READ_ONLY: u32 = 80;
const PRIO_DESTRUCTIVE: u32 = 70;
const PRIO_WRITE_LEVEL: u32 = 60;

/// Synthetic workspace id for translated rules — the translation is pure
/// (the caller already loaded the TrustConfig from the legacy column).
const TRUST_WS: &str = "__heartbeat_trust__";

/// Heartbeat 接入面适配器：把 TrustConfig 翻译为统一引擎输入。
///
/// The translation mirrors `evaluate_tool_trust_with_safety` rule-for-rule
/// (including reason text); the thinness is deliberate — all arbitration
/// lives in [`evaluate_rules`].
pub struct HeartbeatTrustAdapter;

impl HeartbeatTrustAdapter {
    fn rule(id: &str, action: PolicyAction, target: &str, priority: u32, reason: String) -> PolicyRule {
        PolicyRule {
            id: format!("trust-{id}"),
            workspace_id: TRUST_WS.to_string(),
            category: PolicyCategory::ToolExecution,
            action,
            target: target.to_string(),
            priority,
            reason,
        }
    }

    /// Translate a TrustConfig + tool into engine rules. Every applicable
    /// legacy branch contributes a rule (branches 1/2 can coexist with 3–5);
    /// the engine's priority ordering resolves conflicts exactly like the
    /// legacy first-match order.
    pub fn rules_for(config: &TrustConfig, tool_name: &str, safety: ToolSafety) -> Vec<PolicyRule> {
        let mut rules = Vec::new();

        // 1. Explicit block list.
        if config.blocked_tools.iter().any(|t| t == tool_name) {
            rules.push(Self::rule(
                "blocked",
                PolicyAction::Block,
                tool_name,
                PRIO_BLOCKED,
                format!("Tool '{tool_name}' is explicitly blocked by workspace trust config. Do not retry."),
            ));
        }

        // 2. Category gate (destructive tools exempt; "query" is a legacy
        // alias of "read"; empty list means no restriction).
        let category = safety_category(safety);
        if !matches!(safety, ToolSafety::Destructive)
            && !config.allowed_tool_categories.is_empty()
            && !config
                .allowed_tool_categories
                .iter()
                .any(|c| c == category || (category == "read" && c == "query"))
        {
            rules.push(Self::rule(
                "category-gate",
                PolicyAction::RequireApproval,
                tool_name,
                PRIO_CATEGORY_GATE,
                format!(
                    "Tool '{}' is in category '{}', which is not in the workspace's \
                     allowed_tool_categories. Propose this action in pending_proposals instead.",
                    tool_name, category
                ),
            ));
        }

        // 3–5. Safety branch (mutually exclusive per tool).
        match safety {
            ToolSafety::ReadOnly => {
                rules.push(Self::rule(
                    "read-only",
                    PolicyAction::Allow,
                    tool_name,
                    PRIO_READ_ONLY,
                    String::new(),
                ));
            }
            ToolSafety::Destructive => {
                if config.trust_level == TrustLevel::FullAuto
                    && config.allowed_destructive_tools.iter().any(|t| t == tool_name)
                {
                    rules.push(Self::rule(
                        "destructive-allowlisted",
                        PolicyAction::Allow,
                        tool_name,
                        PRIO_DESTRUCTIVE,
                        String::new(),
                    ));
                } else {
                    rules.push(Self::rule(
                        "destructive",
                        PolicyAction::RequireApproval,
                        tool_name,
                        PRIO_DESTRUCTIVE,
                        format!(
                            "Tool '{}' is destructive. It must be explicitly listed in \
                             allowed_destructive_tools under FullAuto trust level. \
                             Propose this action in pending_proposals instead.",
                            tool_name
                        ),
                    ));
                }
            }
            ToolSafety::Write => match config.trust_level {
                TrustLevel::FullAuto => {
                    rules.push(Self::rule(
                        "write-full-auto",
                        PolicyAction::Allow,
                        tool_name,
                        PRIO_WRITE_LEVEL,
                        String::new(),
                    ));
                }
                TrustLevel::ReadOnlyAuto | TrustLevel::ApprovalRequired => {
                    rules.push(Self::rule(
                        "write-by-level",
                        PolicyAction::RequireApproval,
                        tool_name,
                        PRIO_WRITE_LEVEL,
                        format!(
                            "Tool '{}' requires human approval under current trust level ({:?}). \
                             Propose this action in pending_proposals with tool_name, device_id, \
                             summary, reason, and risk.",
                            tool_name, config.trust_level
                        ),
                    ));
                }
            },
        }

        rules
    }

    /// O23 equivalence: same TrustConfig input → same verdict as the legacy
    /// heartbeat path (`evaluate_tool_trust_with_safety`).
    pub fn evaluate(config: &TrustConfig, tool_name: &str, safety: ToolSafety) -> TrustDecision {
        let rules = Self::rules_for(config, tool_name, safety);
        match evaluate_rules(&rules, TRUST_WS, PolicyCategory::ToolExecution, tool_name) {
            PolicyDecision::Allow => TrustDecision::Allow,
            PolicyDecision::Block { reason } => TrustDecision::Block { reason },
            PolicyDecision::RequireApproval { reason } | PolicyDecision::Flag { reason } => {
                TrustDecision::Propose { reason }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{NoopPolicyEngine, PolicyAction, PolicyCategory, PolicyRule};
    use tinyiothub_skills::trust::{
        ToolSafety, classify_tool_safety, evaluate_tool_trust, evaluate_tool_trust_with_safety,
    };
    use tinyiothub_storage::heartbeat::{TrustConfig, TrustLevel};

    // ── HeartbeatTrustAdapter equivalence (O23) ─────────────────

    fn configs() -> Vec<TrustConfig> {
        let mut out = vec![];
        for trust_level in [
            TrustLevel::ApprovalRequired,
            TrustLevel::ReadOnlyAuto,
            TrustLevel::FullAuto,
        ] {
            for allowed_tool_categories in [
                vec!["read".to_string(), "query".to_string(), "write".to_string()],
                vec![],
                vec!["read".to_string()],
                vec!["query".to_string()],
            ] {
                out.push(TrustConfig {
                    trust_level,
                    max_auto_actions_per_tick: 10,
                    allowed_tool_categories,
                    blocked_tools: vec![],
                    allowed_destructive_tools: vec![],
                });
            }
        }
        out
    }

    fn tools() -> Vec<(&'static str, ToolSafety)> {
        vec![
            ("get_device", ToolSafety::ReadOnly),
            ("write_properties", ToolSafety::Write),
            ("delete_device", ToolSafety::Destructive),
        ]
    }

    /// Parameterized equivalence: for every (config, tool, blocked, allowlisted)
    /// combination the adapter's engine-arbitrated verdict — including the
    /// reason text — must equal the legacy heartbeat path.
    #[test]
    fn heartbeat_adapter_matches_legacy_trust_path() {
        let mut cases = 0usize;
        for base in configs() {
            for (tool, safety) in tools() {
                for blocked in [false, true] {
                    for allowlisted in [false, true] {
                        let mut config = base.clone();
                        if blocked {
                            config.blocked_tools = vec![tool.to_string()];
                        }
                        if allowlisted {
                            config.allowed_destructive_tools = vec![tool.to_string()];
                        }
                        let legacy = evaluate_tool_trust_with_safety(&config, tool, safety);
                        let adapter = HeartbeatTrustAdapter::evaluate(&config, tool, safety);
                        assert_eq!(
                            legacy, adapter,
                            "verdict mismatch: level={:?} categories={:?} tool={} safety={:?} blocked={} allowlisted={}",
                            config.trust_level, config.allowed_tool_categories, tool, safety, blocked, allowlisted
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert!(cases >= 100, "equivalence matrix must be broad, got {cases} cases");
    }

    /// Name-classified variant: adapter agrees with `evaluate_tool_trust` when
    /// safety is derived from the tool name.
    #[test]
    fn heartbeat_adapter_matches_name_classified_path() {
        for config in configs() {
            for (tool, _) in tools() {
                assert_eq!(
                    evaluate_tool_trust(&config, tool),
                    HeartbeatTrustAdapter::evaluate(&config, tool, classify_tool_safety(tool)),
                    "name-classified mismatch for {tool} under {:?}",
                    config.trust_level
                );
            }
        }
    }

    /// The translation must be non-trivial: conflicting synthesized rules are
    /// arbitrated by the engine (e.g. a blocked read-only tool yields both a
    /// Block and an Allow rule; the engine must pick Block like the legacy
    /// first-match order does).
    #[test]
    fn heartbeat_adapter_synthesizes_conflicting_rules_for_engine_arbitration() {
        let config = TrustConfig {
            blocked_tools: vec!["get_device".to_string()],
            ..TrustConfig::default()
        };
        let rules = HeartbeatTrustAdapter::rules_for(&config, "get_device", ToolSafety::ReadOnly);
        assert!(
            rules.iter().any(|r| r.action == PolicyAction::Block)
                && rules.iter().any(|r| r.action == PolicyAction::Allow),
            "blocked read-only tool must synthesize both Block and Allow rules: {rules:?}"
        );
    }

    // ── ChatConfirmAdapter ──────────────────────────────────────

    /// Vec-backed engine for adapter tests: arbitrates through the same
    /// `evaluate_rules` core the SQLite engine uses.
    struct VecEngine(Vec<PolicyRule>);

    #[async_trait::async_trait]
    impl crate::PolicyEngine for VecEngine {
        async fn evaluate(&self, workspace_id: &str, category: PolicyCategory, target: &str) -> crate::PolicyDecision {
            crate::evaluate_rules(&self.0, workspace_id, category, target)
        }
        async fn add_rule(&self, _rule: PolicyRule) -> anyhow::Result<()> {
            Ok(())
        }
        async fn remove_rule(&self, _rule_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn list_rules(&self, _workspace_id: &str) -> Vec<PolicyRule> {
            self.0.clone()
        }
    }

    fn rule(action: PolicyAction, target: &str) -> PolicyRule {
        PolicyRule {
            id: format!("{action:?}-{target}"),
            workspace_id: "ws1".to_string(),
            category: PolicyCategory::AgentAction,
            action,
            target: target.to_string(),
            priority: 10,
            reason: "test rule".to_string(),
        }
    }

    #[tokio::test]
    async fn chat_toggle_preserved_without_rules() {
        let adapter = ChatConfirmAdapter::new(Arc::new(NoopPolicyEngine));
        // 开关开 → 令牌
        assert_eq!(
            adapter.decide("ws1", "reboot", true).await,
            ChatConfirmVerdict::RequireToken
        );
        // 开关关 → 直发
        assert_eq!(
            adapter.decide("ws1", "reboot", false).await,
            ChatConfirmVerdict::Execute
        );
    }

    #[tokio::test]
    async fn chat_block_rule_denies_even_with_toggle_off() {
        let adapter = ChatConfirmAdapter::new(Arc::new(VecEngine(vec![rule(PolicyAction::Block, "reboot")])));
        match adapter.decide("ws1", "reboot", false).await {
            ChatConfirmVerdict::Deny { reason } => assert_eq!(reason, "test rule"),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_require_approval_rule_mints_token_even_with_toggle_off() {
        let adapter = ChatConfirmAdapter::new(Arc::new(VecEngine(vec![rule(PolicyAction::RequireApproval, "reboot")])));
        assert_eq!(
            adapter.decide("ws1", "reboot", false).await,
            ChatConfirmVerdict::RequireToken
        );
    }

    #[tokio::test]
    async fn chat_rules_are_workspace_and_target_scoped() {
        let adapter = ChatConfirmAdapter::new(Arc::new(VecEngine(vec![rule(PolicyAction::Block, "reboot")])));
        // Other action unaffected.
        assert_eq!(
            adapter.decide("ws1", "shutdown", false).await,
            ChatConfirmVerdict::Execute
        );
        // Other workspace unaffected.
        assert_eq!(
            adapter.decide("ws2", "reboot", false).await,
            ChatConfirmVerdict::Execute
        );
    }
}
