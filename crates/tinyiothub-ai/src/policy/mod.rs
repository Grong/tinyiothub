//! Policy Engine — workspace-level guardrails for Agent actions.
//!
//! Policies layer on top of the TrustEngine:
//! - TrustEngine: intrinsic tool safety (read/write/destructive)
//! - PolicyEngine: workspace-specific rules (rate limits, allowlists, content filters)

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
        match self {
            PolicyCategory::ToolExecution => write!(f, "tool_execution"),
            PolicyCategory::LlmInput => write!(f, "llm_input"),
            PolicyCategory::LlmOutput => write!(f, "llm_output"),
            PolicyCategory::AgentAction => write!(f, "agent_action"),
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

/// Result of evaluating policies against an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Action is allowed to proceed.
    Allow,
    /// Action is blocked — do not execute, do not retry.
    Block { reason: String },
    /// Action is flagged — execute but log/report.
    Flag { reason: String },
}

/// Engine for evaluating workspace-level policies.
///
/// Resolution order (first match wins):
/// 1. Explicit Block rules (highest priority sort)
/// 2. Explicit Allow rules
/// 3. RequireApproval rules
/// 4. Default: Allow (permissive by default)
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
