//! Agent-side implementation of `tinyiothub_core::thing_hooks::ThingActionHooks`.
//!
//! This is the adapter that lets the thing domain's HTTP handlers consume
//! agent-owned capabilities (param validation, the pending-action token
//! store, and the unified policy confirm gate) through the core trait,
//! so `modules::thing` carries no `modules::agent` dependency edge.
//!
//! The adapter owns construction of the [`SqlitePolicyEngine`] — callers
//! only hand it a connection pool.

use sqlx::SqlitePool;
use crate::loop_::types::{ChatConfirmAdapter, ChatConfirmVerdict};
use tinyiothub_core::thing_hooks::{PendingThingAction, ThingActionHooks, ThingConfirmVerdict};

use crate::host::{
    policy_engine::SqlitePolicyEngine,
    tools::thing::{store_pending_action, take_pending_action, validate_action_params},
};

/// [`ThingActionHooks`] backed by the agent domain's real implementations.
pub struct AgentThingActionHooks {
    pool: SqlitePool,
}

impl AgentThingActionHooks {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ThingActionHooks for AgentThingActionHooks {
    fn validate_params(
        &self,
        schema_json: &str,
        params: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        validate_action_params(schema_json, params)
    }

    fn store_pending(
        &self,
        thing_id: String,
        action_name: String,
        params: Option<serde_json::Value>,
        workspace_id: String,
    ) -> String {
        store_pending_action(thing_id, action_name, params, workspace_id)
    }

    fn take_pending(&self, token: &str) -> Option<PendingThingAction> {
        take_pending_action(token).map(|p| PendingThingAction {
            thing_id: p.thing_id,
            action_name: p.action_name,
            params: p.params,
            workspace_id: p.workspace_id,
        })
    }

    async fn decide_confirm(
        &self,
        workspace_id: &str,
        action_name: &str,
        require_confirm: bool,
    ) -> ThingConfirmVerdict {
        let adapter = ChatConfirmAdapter::new(std::sync::Arc::new(SqlitePolicyEngine::new(
            self.pool.clone(),
        )));
        match adapter.decide(workspace_id, action_name, require_confirm).await {
            ChatConfirmVerdict::Execute => ThingConfirmVerdict::Execute,
            ChatConfirmVerdict::RequireToken => ThingConfirmVerdict::RequireToken,
            ChatConfirmVerdict::Deny { reason } => ThingConfirmVerdict::Deny { reason },
        }
    }
}
