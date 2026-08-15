//! Policy 领域值类型：工作区自治策略 + 行动提案（自 db/policy.rs 归位，Task 1）。
//!
//! AutonomyPolicy/AutonomyMode/Proposal/ProposalStatus 为共享值类型；
//! PolicyRepository 与全部 SQL（含 agent_runs 动作频率读取）留在 db。

use serde::{Deserialize, Serialize};

/// Three-state autonomy mode for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyMode {
    Off,
    Diagnose,
    Act,
}

impl AutonomyMode {
    /// Stable string stored in `workspace_autonomy_policy.mode`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AutonomyMode::Off => "off",
            AutonomyMode::Diagnose => "diagnose",
            AutonomyMode::Act => "act",
        }
    }

    /// Inverse of `as_str`; unknown values return None (treat as fail-closed).
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "off" => Some(AutonomyMode::Off),
            "diagnose" => Some(AutonomyMode::Diagnose),
            "act" => Some(AutonomyMode::Act),
            _ => None,
        }
    }
}

/// Workspace-level autonomy policy for the thing-agent loop.
#[derive(Debug, Clone)]
pub struct AutonomyPolicy {
    pub mode: AutonomyMode,
    /// Allowed action names; `["*"]` means all actions.
    pub allowed_actions: Vec<String>,
    /// Denied action names (exact match); checked before the allowlist.
    pub denied_actions: Vec<String>,
    pub max_actions_per_run: u32,
    pub max_actions_per_hour: u32,
}

// ── 行动提案 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    /// Tool or action name being proposed.
    pub tool_name: String,
    /// Target device or resource.
    pub device_id: Option<String>,
    /// Human-readable summary of what this will do.
    pub summary: String,
    /// Why the agent wants to take this action.
    pub reason: String,
    /// Risk assessment (low/medium/high).
    pub risk: String,
    /// Proposed parameters (tool-specific).
    pub parameters: Option<serde_json::Value>,
    /// ISO 8601 timestamp.
    pub created_at: String,
    /// Status lifecycle: Pending → Approved / Rejected.
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

impl std::fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalStatus::Pending => write!(f, "pending"),
            ProposalStatus::Approved => write!(f, "approved"),
            ProposalStatus::Rejected => write!(f, "rejected"),
        }
    }
}
