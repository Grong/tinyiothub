//! Patrol repositories — persistence traits and SQLite implementations.

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::types::{HeartbeatTask, PatrolReport};

/// Persists patrol results (agent_actions table).
#[async_trait]
pub trait ActionRepository: Send + Sync {
    async fn insert_patrol_actions(
        &self,
        workspace_id: &str,
        report: &PatrolReport,
    ) -> Result<(), ActionRepoError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ActionRepoError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Persists heartbeat tasks (heartbeat_tasks table — replaces HEARTBEAT.md).
#[async_trait]
pub trait HeartbeatTaskRepository: Send + Sync {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, sqlx::Error>;
    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, sqlx::Error>;
    async fn insert(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, sqlx::Error>;
    async fn set_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), sqlx::Error>;
    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), sqlx::Error>;
}

/// SQLite implementation of ActionRepository.
pub struct SqliteActionRepository {
    pool: SqlitePool,
}

impl SqliteActionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ActionRepository for SqliteActionRepository {
    async fn insert_patrol_actions(
        &self,
        workspace_id: &str,
        report: &PatrolReport,
    ) -> Result<(), ActionRepoError> {
        let actions_json = serde_json::to_string(&report.executed_actions)
            .map_err(|e| ActionRepoError::Serialization(e.to_string()))?;
        let proposals_json = serde_json::to_string(&report.pending_proposals)
            .map_err(|e| ActionRepoError::Serialization(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agent_actions (workspace_id, status, summary, actions_json, proposals_json, error)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(workspace_id)
        .bind(format!("{:?}", report.status))
        .bind(&report.summary)
        .bind(&actions_json)
        .bind(&proposals_json)
        .bind(&report.error)
        .execute(&self.pool)
        .await
        .map_err(|e| ActionRepoError::Database(e.to_string()))?;

        Ok(())
    }
}

/// SQLite implementation of HeartbeatTaskRepository.
pub struct SqliteHeartbeatTaskRepository {
    pool: SqlitePool,
}

impl SqliteHeartbeatTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HeartbeatTaskRepository for SqliteHeartbeatTaskRepository {
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, sqlx::Error> {
        sqlx::query_as::<_, HeartbeatTask>(
            "SELECT id, workspace_id, priority, text, paused, version,
                    created_at, updated_at
             FROM heartbeat_tasks WHERE workspace_id = ? ORDER BY priority DESC, id ASC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE heartbeat_tasks
             SET priority = ?, text = ?, paused = ?, version = version + 1,
                 updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ? AND version = ?",
        )
        .bind(&task.priority)
        .bind(&task.text)
        .bind(task.paused)
        .bind(workspace_id)
        .bind(task.id)
        .bind(expected_version)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn insert(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, sqlx::Error> {
        sqlx::query_as::<_, HeartbeatTask>(
            "INSERT INTO heartbeat_tasks (workspace_id, priority, text)
             VALUES (?, ?, ?)
             RETURNING id, workspace_id, priority, text, paused, version,
                       created_at, updated_at",
        )
        .bind(workspace_id)
        .bind(priority)
        .bind(text)
        .fetch_one(&self.pool)
        .await
    }

    async fn set_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE heartbeat_tasks SET paused = ?, updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ?",
        )
        .bind(paused)
        .bind(workspace_id)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(task_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
