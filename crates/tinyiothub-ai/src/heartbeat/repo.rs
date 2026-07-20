//! Heartbeat repository traits — storage-agnostic persistence interfaces.

use async_trait::async_trait;

use super::types::{HeartbeatResult, HeartbeatTask, NewHeartbeatTask};
use crate::tool::trust::TrustConfig;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found")]
    NotFound,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Persists heartbeat tasks and results.
#[async_trait]
pub trait HeartbeatTaskRepository: Send + Sync {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError>;

    async fn upsert(&self, workspace_id: &str, task: &HeartbeatTask, expected_version: i64) -> Result<bool, RepoError>;

    async fn insert(&self, workspace_id: &str, priority: &str, text: &str) -> Result<HeartbeatTask, RepoError>;

    async fn set_paused(&self, workspace_id: &str, task_id: i64, paused: bool) -> Result<(), RepoError>;

    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), RepoError>;

    /// Atomically replace a workspace's whole task set.
    ///
    /// Default impl is delete-then-insert (non-atomic); storage backends
    /// should override with a transaction.
    async fn replace_all(&self, workspace_id: &str, tasks: &[NewHeartbeatTask]) -> Result<(), RepoError> {
        for existing in self.list_by_workspace(workspace_id).await? {
            self.delete(workspace_id, existing.id).await?;
        }
        for task in tasks {
            let inserted = self.insert(workspace_id, &task.priority, &task.text).await?;
            if task.paused {
                self.set_paused(workspace_id, inserted.id, true).await?;
            }
        }
        Ok(())
    }

    /// Persist heartbeat execution results (replaces old ActionRepository).
    async fn insert_result(&self, workspace_id: &str, result: &HeartbeatResult) -> Result<(), RepoError>;

    /// Load the workspace's persisted TrustConfig, if any. Default: none —
    /// callers fall back to `TrustConfig::default()`.
    async fn load_trust_config(&self, _workspace_id: &str) -> Result<Option<TrustConfig>, RepoError> {
        Ok(None)
    }
}
