//! Thing action hooks — the seam that lets the thing domain invoke/confirm
//! device actions without depending on the agent (or mcp) domain.
//!
//! The thing HTTP handlers (`invoke_action` / `confirm_action`) need four
//! agent-owned capabilities: parameter-schema validation, the pending-action
//! confirmation token store, and the unified policy confirm gate. This trait
//! is the contract; the agent domain provides the implementation and the
//! composition layer injects it as `Arc<dyn ThingActionHooks>`.
//!
//! Core guardrail: traits + value types only, no logic here.

use serde_json::Value;

/// A pending thing action awaiting user confirmation (value type crossing
/// the thing→agent boundary). Mirrors the agent-side token-store entry,
/// minus the token itself and the creation timestamp, which the thing
/// handlers never read.
#[derive(Debug, Clone)]
pub struct PendingThingAction {
    pub thing_id: String,
    pub action_name: String,
    pub params: Option<Value>,
    pub workspace_id: String,
}

/// Verdict of the confirm gate for one `invoke_action` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThingConfirmVerdict {
    /// Dispatch immediately.
    Execute,
    /// Mint a confirmation token; the caller must confirm before dispatch.
    RequireToken,
    /// Refuse the action (policy Block rule).
    Deny { reason: String },
}

/// Agent-provided capabilities consumed by the thing action handlers.
#[async_trait::async_trait]
pub trait ThingActionHooks: Send + Sync {
    /// Validate invoke params against the action's parameter schema JSON.
    /// Returns a human-readable error message on mismatch.
    fn validate_params(&self, schema_json: &str, params: Option<&Value>) -> Result<(), String>;

    /// Store a pending action and return its confirmation token.
    fn store_pending(
        &self,
        thing_id: String,
        action_name: String,
        params: Option<Value>,
        workspace_id: String,
    ) -> String;

    /// Retrieve and consume a pending action by token
    /// (`None` if expired or unknown).
    fn take_pending(&self, token: &str) -> Option<PendingThingAction>;

    /// Policy confirm gate: Block → Deny; RequireApproval → RequireToken;
    /// otherwise the workspace `require_action_confirm` toggle decides.
    async fn decide_confirm(
        &self,
        workspace_id: &str,
        action_name: &str,
        require_confirm: bool,
    ) -> ThingConfirmVerdict;
}
