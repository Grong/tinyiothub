//! Thing-owned hooks seam (G5a) — the trait through which thing HTTP
//! handlers consume agent-owned capabilities (param validation, the
//! pending-action token store, and the unified policy confirm gate).
//!
//! The thing domain defines the trait; the agent domain implements it
//! (`AgentThingActionHooks`, agent-side host adapter) and the composition
//! layer injects `Arc<dyn ThingActionHooks>` into `AppState`. Dependency
//! direction: agent → thing (never the reverse).

use serde_json::Value;

/// A pending thing action awaiting user confirmation (value type flowing
/// agent→thing: stored by the agent-side hooks impl, returned to the thing
/// handlers on confirm). Mirrors the agent-side token-store entry,
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

/// How a thing's capabilities become available at creation time.
///
/// `Declarative` — the installed driver package declares a fixed capability
/// set; template rows must not be copied. `TemplateBootstrap` — a dynamic
/// capability driver starts from template rows and then `describe()` refines
/// them during materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationMode {
    Declarative,
    TemplateBootstrap,
}

/// Driver-access capabilities consumed by thing creation (E4a).
///
/// The thing domain defines the trait; the composition layer injects the
/// access-service-backed implementation, keeping thing → runtime free of a
/// direct dependency on the access service.
#[async_trait::async_trait]
pub trait ThingDeclarationHook: Send + Sync {
    /// Validate the driver's package config against `options` and decide how
    /// template rows should be handled. `Err(msg)` rejects the creation.
    async fn prepare_creation(
        &self,
        workspace_id: &str,
        driver_name: &str,
        options: Option<&str>,
    ) -> Result<CreationMode, String>;

    /// Bind the created thing to its driver: resolve/validate the device
    /// declaration and materialize it into properties/actions.
    async fn materialize(&self, workspace_id: &str, thing_id: &str) -> Result<(), String>;
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
    async fn decide_confirm(&self, workspace_id: &str, action_name: &str, require_confirm: bool)
    -> ThingConfirmVerdict;
}
