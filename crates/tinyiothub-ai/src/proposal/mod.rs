//! Human-In-The-Loop proposals — actions requiring human approval.
//!
//! When the TrustEngine or PolicyEngine flags an action as RequireApproval,
//! the heartbeat loop creates a Proposal. Cloud persists proposals and surfaces
//! them in the UI for workspace owners to approve/reject.

use serde::{Deserialize, Serialize};

/// An action proposed by the AI that requires human approval.
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
