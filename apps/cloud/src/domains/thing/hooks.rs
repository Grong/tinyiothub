//! Thing-owned hooks seam (G5a) — the trait through which thing HTTP
//! handlers consume agent-owned capabilities (param validation, the
//! pending-action token store, and the unified policy confirm gate).
//!
//! The thing domain defines the trait; the agent domain implements it
//! (`AgentThingActionHooks`, agent-side host adapter) and the composition
//! layer injects `Arc<dyn ThingActionHooks>` into `AppState`. Dependency
//! direction: agent → thing (never the reverse).

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

/// Agent-owned capabilities consumed by the thing domain's action handlers.
#[async_trait::async_trait]
pub trait ThingActionHooks: Send + Sync {
    /// Validate `params` against the action's JSON schema. `Ok(())` when
    /// valid; `Err(msg)` with a human-readable reason otherwise.
    fn validate_params(&self, schema_json: &str, params: Option<&Value>) -> Result<(), String>;

    /// Store a pending action and return its confirmation token.
    fn store_pending(
        &self,
        thing_id: String,
        action_name: String,
        params: Option<Value>,
        workspace_id: String,
    ) -> String;

    /// Take (remove and return) the pending action for `token`, if any.
    fn take_pending(&self, token: &str) -> Option<PendingThingAction>;

    /// Run the unified policy confirm gate for one invoke.
    async fn decide_confirm(
        &self,
        workspace_id: &str,
        action_name: &str,
        require_confirm: bool,
    ) -> ThingConfirmVerdict;
}
