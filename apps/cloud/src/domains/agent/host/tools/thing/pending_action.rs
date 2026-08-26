// Confirmation token store for invoke_action

use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::Value;

/// Pending action awaiting user confirmation.
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub token: String,
    pub thing_id: String,
    pub action_name: String,
    pub params: Option<Value>,
    pub workspace_id: String,
    pub created_at: Instant,
}

/// Pending-action confirmation store type (G3 — injected, no global).
pub type PendingActionStore = DashMap<String, PendingAction>;

const CONFIRMATION_TTL: Duration = Duration::from_secs(30 * 60);

/// Store a pending action and return its confirmation token.
pub fn store_pending_action(
    store: &PendingActionStore,
    thing_id: String,
    action_name: String,
    params: Option<Value>,
    workspace_id: String,
) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    let pending = PendingAction {
        token: token.clone(),
        thing_id,
        action_name,
        params,
        workspace_id,
        created_at: Instant::now(),
    };
    store.insert(token.clone(), pending);
    token
}

/// Retrieve and consume a pending action by token (returns None if expired or not found).
pub fn take_pending_action(store: &PendingActionStore, token: &str) -> Option<PendingAction> {
    cleanup_expired_tokens(store);
    let entry = store.remove(token)?;
    if entry.1.created_at.elapsed() > CONFIRMATION_TTL {
        return None;
    }
    Some(entry.1)
}

/// Cleanup expired tokens (called on every take — keeps the map bounded).
pub fn cleanup_expired_tokens(store: &PendingActionStore) {
    store.retain(|_, v| v.created_at.elapsed() <= CONFIRMATION_TTL);
}
