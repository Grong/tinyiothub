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

/// In-memory rate-limiting policy engine with block-list support.
///
/// Tracks tool calls per tick per workspace and enforces a call limit.
/// For Phase 1 this is in-memory; Phase 2 adds DB-backed rule persistence.
pub struct RateLimitingPolicyEngine {
    /// Per-workspace tick call counter. Key: workspace_id → count.
    tick_counts: std::sync::RwLock<std::collections::HashMap<String, u32>>,
    /// Blocked tools per workspace. Key: workspace_id → tool names.
    blocked_tools: std::sync::RwLock<std::collections::HashMap<String, Vec<String>>>,
    /// Max calls per tick fallback (overridden by TrustConfig when available).
    default_max_per_tick: u32,
}

impl RateLimitingPolicyEngine {
    pub fn new(default_max_per_tick: u32) -> Self {
        Self {
            tick_counts: std::sync::RwLock::new(std::collections::HashMap::new()),
            blocked_tools: std::sync::RwLock::new(std::collections::HashMap::new()),
            default_max_per_tick,
        }
    }

    /// Reset tick counters at the start of a new tick.
    pub fn reset_tick(&self, workspace_id: &str) {
        self.tick_counts.write().unwrap().insert(workspace_id.to_string(), 0);
    }

    /// Record a tool call for the current tick. Returns false if limit exceeded.
    pub fn record_call(&self, workspace_id: &str, max_per_tick: u32) -> bool {
        let max = if max_per_tick > 0 { max_per_tick } else { self.default_max_per_tick };
        let mut counts = self.tick_counts.write().unwrap();
        let count = counts.entry(workspace_id.to_string()).or_insert(0);
        if *count >= max {
            return false;
        }
        *count += 1;
        true
    }

    /// Current tick count for a workspace.
    pub fn tick_count(&self, workspace_id: &str) -> u32 {
        self.tick_counts
            .read()
            .unwrap()
            .get(workspace_id)
            .copied()
            .unwrap_or(0)
    }
}

#[async_trait]
impl PolicyEngine for RateLimitingPolicyEngine {
    async fn evaluate(
        &self,
        workspace_id: &str,
        category: PolicyCategory,
        target: &str,
    ) -> PolicyDecision {
        if category != PolicyCategory::ToolExecution {
            return PolicyDecision::Allow;
        }

        // Check explicit block list
        let blocked = self.blocked_tools.read().unwrap();
        if let Some(tools) = blocked.get(workspace_id)
            && tools.iter().any(|t| t == target)
        {
            return PolicyDecision::Block {
                reason: format!("Tool '{}' is blocked by workspace policy", target),
            };
        }

        PolicyDecision::Allow
    }

    async fn add_rule(&self, rule: PolicyRule) -> anyhow::Result<()> {
        if rule.category == PolicyCategory::ToolExecution && rule.action == PolicyAction::Block {
            let mut blocked = self.blocked_tools.write().unwrap();
            blocked
                .entry(rule.workspace_id)
                .or_default()
                .push(rule.target);
        }
        Ok(())
    }

    async fn remove_rule(&self, rule_id: &str) -> anyhow::Result<()> {
        // Phase 2: implement rule ID tracking and removal
        let _ = rule_id;
        Ok(())
    }

    async fn list_rules(&self, workspace_id: &str) -> Vec<PolicyRule> {
        let blocked = self.blocked_tools.read().unwrap();
        blocked
            .get(workspace_id)
            .map(|tools| {
                tools
                    .iter()
                    .enumerate()
                    .map(|(i, t)| PolicyRule {
                        id: format!("block-{}", i),
                        workspace_id: workspace_id.to_string(),
                        category: PolicyCategory::ToolExecution,
                        action: PolicyAction::Block,
                        target: t.clone(),
                        priority: 100,
                        reason: String::new(),
                    })
                    .collect()
            })
            .unwrap_or_default()
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
