//! Heartbeat repository traits — storage-agnostic persistence interfaces.

use async_trait::async_trait;

use super::types::{HeartbeatResult, HeartbeatTask};

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

    /// Persist heartbeat execution results (replaces old ActionRepository).
    async fn insert_result(&self, workspace_id: &str, result: &HeartbeatResult) -> Result<(), RepoError>;
}
