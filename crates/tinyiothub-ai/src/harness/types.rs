//! Harness types — pipeline stages, step definitions, verdicts, and loop report.
//!
//! All types are zeroclaw-free. Cloud crate translates TurnEvent → StreamEvent.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::heartbeat::types::{ExecutedAction, HeartbeatSignal};
use crate::proposal::Proposal;

// ── Stream Event (zeroclaw-free observation) ─────────────────────────

/// AI-crate-native streaming event. Cloud implementations translate TurnEvent into this.
/// This keeps the AI crate decoupled from zeroclaw.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// A text chunk from the LLM (for real-time streaming).
    Chunk { delta: String },
    /// A reasoning/thinking chunk.
    Thinking { delta: String },
    /// LLM decided to call a tool. With AutonomyLevel::Full, the tool
    /// executes immediately; harness observes this for step tracking.
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    /// A tool execution result (includes harness block/propose outcomes).
    ToolResult {
        id: String,
        name: String,
        output: String,
        success: bool,
    },
    /// Final consolidated text after the turn completes.
    Final { text: String },
    /// Error during streaming execution.
    Error { message: String },
}

// ── Signal Types ─────────────────────────────────────────────────────

/// The source that triggered this harness tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalSource {
    Timer,
    Event,
    Chat,
}

/// Payload carried by a harness signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalPayload {
    Timer,
    Alarm(HeartbeatSignal),
    Chat {
        message: String,
        session_key: String,
        user_id: String,
    },
}

/// Unified signal entering the harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessSignal {
    pub workspace_id: String,
    pub source: SignalSource,
    pub payload: SignalPayload,
    pub priority: crate::heartbeat::types::SignalPriority,
}

// ── Plan Types ───────────────────────────────────────────────────────

/// How to handle a step failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureAction {
    Retry { max: u32 },
    SkipAndContinue,
    Escalate { message: String },
}

/// A single step in the execution plan, derived from Skill templates or hardcoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step identifier ("1", "2", "3").
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Whether this step must succeed for the tick to pass.
    pub required: bool,
    /// Maximum retries on tool failure (from PostToolUse verification).
    pub max_retries: u32,
    /// Suggested tool names (hints for the LLM, not a restriction).
    pub tool_hints: Vec<String>,
    /// What happens when the step fails after exhausting retries.
    pub on_failure: FailureAction,
}

// ── Execute Types ────────────────────────────────────────────────────

/// Status of a single step execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed { reason: String },
    Skipped,
    AwaitingApproval { proposal_id: String },
}

/// Record of a single tool call within a step (for cross_check verification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub args: serde_json::Value,
    pub success: bool,
    pub output: String,
    /// Whether this tool call was proposed (not executed).
    pub proposed: bool,
    /// PostToolUse readback verification result, if applicable.
    pub readback_verified: Option<bool>,
    pub readback_detail: Option<String>,
}

/// Result of executing one PlanStep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub status: StepStatus,
    pub output: String,
    pub tool_calls: Vec<ToolCallRecord>,
    pub retries: u32,
    pub duration_ms: u64,
}

/// Outcome of a pre-tool-use trust evaluation in harness execute.rs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreToolDecision {
    /// Tool is allowed — proceed with execution.
    Allow,
    /// Tool is blocked — skip, record the reason.
    Block { reason: String },
    /// Tool requires human approval — create Proposal, skip execution.
    Propose { reason: String },
}

impl From<crate::tool::trust::TrustDecision> for PreToolDecision {
    fn from(d: crate::tool::trust::TrustDecision) -> Self {
        match d {
            crate::tool::trust::TrustDecision::Allow => PreToolDecision::Allow,
            crate::tool::trust::TrustDecision::Block { reason } => PreToolDecision::Block { reason },
            crate::tool::trust::TrustDecision::Propose { reason } => PreToolDecision::Propose { reason },
        }
    }
}

// ── Verify Types ─────────────────────────────────────────────────────

/// Verdict on a single step after cross-checking self-report vs actual tool calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepVerdict {
    /// LLM's self-report matches actual tool execution.
    Consistent,
    /// LLM reported success but tool calls tell a different story.
    Lying { reason: String },
    /// Required step produced no tool calls or incomplete execution.
    Incomplete { reason: String },
}

/// Verdict on the entire tick after quality gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TickVerdict {
    /// All steps passed verification.
    Pass,
    /// Some steps failed but escalated via Proposals.
    Partial { escalated: Vec<String> },
    /// Unrecoverable failure.
    Fail { reason: String },
}

/// Tracks consecutive lie detections across ticks for agent degradation.
#[derive(Debug, Clone, Default)]
pub struct LieCounter {
    /// Number of consecutive ticks where lies were detected.
    pub consecutive_ticks: u32,
    /// Maximum consecutive lies before agent is degraded to read-only.
    pub degrade_threshold: u32,
}

impl LieCounter {
    pub fn new(degrade_threshold: u32) -> Self {
        Self {
            consecutive_ticks: 0,
            degrade_threshold,
        }
    }

    /// Record a tick result. Returns true if the agent should be degraded.
    pub fn record(&mut self, lie_detected: bool) -> bool {
        if lie_detected {
            self.consecutive_ticks += 1;
        } else {
            self.consecutive_ticks = 0;
        }
        self.consecutive_ticks >= self.degrade_threshold
    }

    pub fn is_degraded(&self) -> bool {
        self.consecutive_ticks >= self.degrade_threshold
    }

    pub fn reset(&mut self) {
        self.consecutive_ticks = 0;
    }
}

// ── Loop Context ─────────────────────────────────────────────────────

/// Context loaded at the start of each harness tick.
#[derive(Debug, Clone)]
pub struct LoopContext {
    pub workspace_id: String,
    pub agent_id: String,
    pub trust_config: crate::tool::trust::TrustConfig,
    pub tasks: Vec<crate::heartbeat::types::HeartbeatTask>,
    pub history: std::collections::VecDeque<LoopReport>,
    pub system_prompt: String,
}

/// Maximum number of historical tick reports kept in sliding window.
pub const MAX_HISTORY_TICKS: usize = 20;

// ── Loop Report ──────────────────────────────────────────────────────

/// Unified output of a harness tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopReport {
    pub workspace_id: String,
    pub trigger_source: SignalSource,
    pub verdict: TickVerdict,
    pub steps: Vec<StepResult>,
    pub executed_actions: Vec<ExecutedAction>,
    pub proposals: Vec<Proposal>,
    pub duration_ms: u64,
    pub tool_call_count: u32,
    pub lie_detected: bool,
    /// Per-stage duration in milliseconds.
    pub stage_durations: HashMap<String, u64>,
}

// ── Harness Error ────────────────────────────────────────────────────

/// Unified error type for harness operations.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("Stage '{stage}' failed for workspace '{workspace_id}': {reason}")]
    StageError {
        stage: String,
        workspace_id: String,
        reason: String,
    },

    #[error("LLM call failed: {0}")]
    LlmError(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("Trust blocked: {0}")]
    TrustBlocked(String),

    #[error("Verify failed: {0}")]
    VerifyFailed(String),
}
