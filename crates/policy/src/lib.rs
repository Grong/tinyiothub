#![allow(clippy::double_must_use)] // async_trait 展开的 BoxFuture 自带 must_use，属已知误报类
//! Policy Engine — workspace-level guardrails for Agent actions.
//!
//! Policies layer on top of the TrustEngine:
//! - TrustEngine: intrinsic tool safety (read/write/destructive)
//! - PolicyEngine: workspace-specific rules (rate limits, allowlists, content filters)
//!
//! ## 设计不变量
//! - 策略裁决纯逻辑 + SQLite 持久化；不感知 HTTP/agent 细节
//! - 禁止依赖 web/runtime；skills（信任引擎）与 core 为仅有的 workspace 依赖

pub mod adapters;
pub mod autonomy;
pub mod proposal;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Categories of Agent actions subject to policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PolicyCategory {
    ToolExecution,
    LlmInput,
    LlmOutput,
    AgentAction,
}

impl std::fmt::Display for PolicyCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PolicyCategory {
    /// Stable string stored in `policy_rules.category`.
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyCategory::ToolExecution => "tool_execution",
            PolicyCategory::LlmInput => "llm_input",
            PolicyCategory::LlmOutput => "llm_output",
            PolicyCategory::AgentAction => "agent_action",
        }
    }

    /// Inverse of `as_str`; unknown values return None (caller skips the row).
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "tool_execution" => Some(PolicyCategory::ToolExecution),
            "llm_input" => Some(PolicyCategory::LlmInput),
            "llm_output" => Some(PolicyCategory::LlmOutput),
            "agent_action" => Some(PolicyCategory::AgentAction),
            _ => None,
        }
    }
}

/// A concrete policy rule scoped to a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub workspace_id: String,
    pub category: PolicyCategory,
    /// What to do when this rule matches.
    pub action: PolicyAction,
    /// Glob pattern or exact match target (tool name, action name, etc.).
    pub target: String,
    /// Higher priority rules override lower ones on conflict.
    pub priority: u32,
    /// Human-readable rationale.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyAction {
    Allow,
    Block,
    RequireApproval,
}

impl PolicyAction {
    /// Stable string stored in `policy_rules.action`.
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyAction::Allow => "allow",
            PolicyAction::Block => "block",
            PolicyAction::RequireApproval => "require_approval",
        }
    }

    /// Inverse of `as_str`; unknown values return None (caller skips the row).
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(PolicyAction::Allow),
            "block" => Some(PolicyAction::Block),
            "require_approval" => Some(PolicyAction::RequireApproval),
            _ => None,
        }
    }
}

/// Result of evaluating policies against an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Action is allowed to proceed.
    Allow,
    /// Action is blocked — do not execute, do not retry.
    Block { reason: String },
    /// Action is flagged — execute but log/report.
    Flag { reason: String },
    /// Action needs explicit user approval before it may proceed.
    RequireApproval { reason: String },
}

/// Match a rule's target pattern against a concrete target string.
/// `"*"` matches everything; a trailing `"*"` is a prefix glob
/// (`"delete_*"`); anything else is an exact match.
pub fn target_matches(pattern: &str, target: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return target.starts_with(prefix);
    }
    pattern == target
}

/// Fail-safe rank for priority ties: Block > RequireApproval > Allow.
fn action_rank(action: &PolicyAction) -> u8 {
    match action {
        PolicyAction::Block => 2,
        PolicyAction::RequireApproval => 1,
        PolicyAction::Allow => 0,
    }
}

/// Unified rule arbitration core shared by every [`PolicyEngine`]
/// implementation and the X3 surface adapters.
///
/// Resolution order:
/// 1. Keep rules matching workspace + category + target ([`target_matches`]).
/// 2. Sort by priority desc (ties fail safe: Block > RequireApproval > Allow).
/// 3. First match wins.
/// 4. No match → Allow (permissive default; surfaces layer their own fail-closed defaults on top,
///    e.g. the chat confirm toggle).
pub fn evaluate_rules(
    rules: &[PolicyRule],
    workspace_id: &str,
    category: PolicyCategory,
    target: &str,
) -> PolicyDecision {
    let mut matches: Vec<&PolicyRule> = rules
        .iter()
        .filter(|r| r.workspace_id == workspace_id && r.category == category && target_matches(&r.target, target))
        .collect();
    matches.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| action_rank(&b.action).cmp(&action_rank(&a.action)))
    });
    match matches.first() {
        None => PolicyDecision::Allow,
        Some(rule) => match rule.action {
            PolicyAction::Allow => PolicyDecision::Allow,
            PolicyAction::Block => PolicyDecision::Block {
                reason: rule.reason.clone(),
            },
            PolicyAction::RequireApproval => PolicyDecision::RequireApproval {
                reason: rule.reason.clone(),
            },
        },
    }
}

/// Engine for evaluating workspace-level policies.
///
/// All implementations arbitrate through [`evaluate_rules`]: matching rules
/// sorted by priority desc, first match wins (ties fail safe), default Allow.
#[async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluate all applicable policies for an action.
    async fn evaluate(&self, workspace_id: &str, category: PolicyCategory, target: &str) -> PolicyDecision;

    /// Add a policy rule.
    async fn add_rule(&self, rule: PolicyRule) -> anyhow::Result<()>;

    /// Remove a policy rule by id.
    async fn remove_rule(&self, rule_id: &str) -> anyhow::Result<()>;

    /// List all rules for a workspace, sorted by priority desc.
    async fn list_rules(&self, workspace_id: &str) -> Vec<PolicyRule>;
}

/// No-op implementation for testing / when policies aren't configured.
pub struct NoopPolicyEngine;

#[async_trait]
impl PolicyEngine for NoopPolicyEngine {
    async fn evaluate(&self, _workspace_id: &str, _category: PolicyCategory, _target: &str) -> PolicyDecision {
        PolicyDecision::Allow
    }

    async fn add_rule(&self, _rule: PolicyRule) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove_rule(&self, _rule_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_rules(&self, _workspace_id: &str) -> Vec<PolicyRule> {
        vec![]
    }
}

/// Input guardrail — validates/sanitizes LLM input before sending.
pub fn sanitize_llm_input(input: &str) -> String {
    // Strip null bytes which can confuse some models
    let cleaned = input.replace('\0', "");
    // Trim to reasonable max length (most models cap at 8k–128k tokens)
    let max_chars = 100_000;
    if cleaned.len() > max_chars {
        cleaned.chars().take(max_chars).collect()
    } else {
        cleaned
    }
}

/// Output guardrail — validates LLM output for common issues.
pub fn validate_llm_output(output: &str) -> Result<&str, &'static str> {
    if output.is_empty() {
        return Err("LLM output is empty");
    }
    if output.len() > 1_000_000 {
        return Err("LLM output exceeds max size (1MB)");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        id: &str,
        ws: &str,
        category: PolicyCategory,
        action: PolicyAction,
        target: &str,
        priority: u32,
    ) -> PolicyRule {
        PolicyRule {
            id: id.to_string(),
            workspace_id: ws.to_string(),
            category,
            action,
            target: target.to_string(),
            priority,
            reason: format!("reason-{id}"),
        }
    }

    #[test]
    fn category_db_roundtrip() {
        for (s, c) in [
            ("tool_execution", PolicyCategory::ToolExecution),
            ("llm_input", PolicyCategory::LlmInput),
            ("llm_output", PolicyCategory::LlmOutput),
            ("agent_action", PolicyCategory::AgentAction),
        ] {
            assert_eq!(PolicyCategory::from_db(s), Some(c));
            assert_eq!(c.as_str(), s);
        }
        assert_eq!(PolicyCategory::from_db("bogus"), None);
    }

    #[test]
    fn action_db_roundtrip() {
        for (s, a) in [
            ("allow", PolicyAction::Allow),
            ("block", PolicyAction::Block),
            ("require_approval", PolicyAction::RequireApproval),
        ] {
            assert_eq!(PolicyAction::from_db(s), Some(a.clone()));
            assert_eq!(a.as_str(), s);
        }
        assert_eq!(PolicyAction::from_db("bogus"), None);
    }

    #[test]
    fn target_matching_rules() {
        assert!(target_matches("*", "anything"));
        assert!(target_matches("reboot", "reboot"));
        assert!(!target_matches("reboot", "shutdown"));
        assert!(target_matches("delete_*", "delete_device"));
        assert!(!target_matches("delete_*", "get_device"));
    }

    #[test]
    fn no_matching_rule_defaults_allow() {
        let rules = vec![rule(
            "r1",
            "ws1",
            PolicyCategory::AgentAction,
            PolicyAction::Block,
            "reboot",
            10,
        )];
        // different workspace
        assert_eq!(
            evaluate_rules(&rules, "ws2", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::Allow
        );
        // different category
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::ToolExecution, "reboot"),
            PolicyDecision::Allow
        );
        // different target
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "shutdown"),
            PolicyDecision::Allow
        );
        // empty rule set
        assert_eq!(
            evaluate_rules(&[], "ws1", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn higher_priority_wins_across_actions() {
        // A low-priority Block must NOT override a high-priority Allow:
        // priority decides, not the action kind.
        let rules = vec![
            rule(
                "block-low",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Block,
                "reboot",
                1,
            ),
            rule(
                "allow-high",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Allow,
                "reboot",
                10,
            ),
        ];
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn first_match_carries_reason() {
        let rules = vec![
            rule(
                "ra",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::RequireApproval,
                "*",
                5,
            ),
            rule(
                "block",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Block,
                "reboot",
                10,
            ),
        ];
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::Block {
                reason: "reason-block".to_string()
            }
        );
        // Lower-priority RequireApproval still fires for other targets.
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "shutdown"),
            PolicyDecision::RequireApproval {
                reason: "reason-ra".to_string()
            }
        );
    }

    #[test]
    fn priority_tie_breaks_fail_safe() {
        // Same priority: Block > RequireApproval > Allow.
        let rules = vec![
            rule(
                "allow",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Allow,
                "reboot",
                10,
            ),
            rule(
                "ra",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::RequireApproval,
                "reboot",
                10,
            ),
            rule(
                "block",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Block,
                "reboot",
                10,
            ),
        ];
        assert!(matches!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::Block { .. }
        ));

        let rules = vec![
            rule(
                "allow",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::Allow,
                "reboot",
                10,
            ),
            rule(
                "ra",
                "ws1",
                PolicyCategory::AgentAction,
                PolicyAction::RequireApproval,
                "reboot",
                10,
            ),
        ];
        assert!(matches!(
            evaluate_rules(&rules, "ws1", PolicyCategory::AgentAction, "reboot"),
            PolicyDecision::RequireApproval { .. }
        ));
    }

    #[test]
    fn glob_rule_matches_prefix() {
        let rules = vec![rule(
            "g",
            "ws1",
            PolicyCategory::ToolExecution,
            PolicyAction::Block,
            "delete_*",
            10,
        )];
        assert!(matches!(
            evaluate_rules(&rules, "ws1", PolicyCategory::ToolExecution, "delete_device"),
            PolicyDecision::Block { .. }
        ));
        assert_eq!(
            evaluate_rules(&rules, "ws1", PolicyCategory::ToolExecution, "get_device"),
            PolicyDecision::Allow
        );
    }
}
