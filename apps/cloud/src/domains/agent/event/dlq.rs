//! Dead Letter Queue — persistence for events that failed after all retries.
//!
//! Cloud implements this trait (e.g., SQLite or file-based storage)
//! so failed events can be inspected and replayed by operators.

use async_trait::async_trait;

/// A single entry in the dead-letter queue.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeadLetterEntry {
    pub id: String,
    pub workspace_id: String,
    pub event_type: String,
    pub payload_json: String,
    pub failure_reason: String,
    pub enqueued_at: String,
}

/// Persistence for events that exhausted all retries.
/// Cloud implements this; the AI crate only calls `enqueue`.
#[async_trait]
pub trait DeadLetterQueue: Send + Sync {
    /// Store a failed event for later operator inspection/replay.
    async fn enqueue(
        &self,
        workspace_id: &str,
        event_type: &str,
        payload_json: &str,
        failure_reason: &str,
    ) -> Result<(), String>;

    /// List all dead-letter entries for a workspace (admin/debug).
    async fn list(&self, workspace_id: &str) -> Result<Vec<DeadLetterEntry>, String>;

    /// Discard a dead-letter entry (operator acknowledged).
    async fn discard(&self, entry_id: &str) -> Result<(), String>;
}
