//! SQLite implementations of AI crate heartbeat repository traits.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_ai::heartbeat::{
    repo::{HeartbeatTaskRepository, RepoError},
    types::{HeartbeatResult, HeartbeatTask},
};

/// DB row struct with sqlx::FromRow — maps to domain HeartbeatTask.
#[derive(Debug, Clone, sqlx::FromRow)]
struct HeartbeatTaskRow {
    pub id: i64,
    pub workspace_id: String,
    pub priority: String,
    pub text: String,
    pub paused: bool,
    pub version: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<HeartbeatTaskRow> for HeartbeatTask {
    fn from(r: HeartbeatTaskRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            priority: r.priority,
            text: r.text,
            paused: r.paused,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

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
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
        let rows = sqlx::query_as::<_, HeartbeatTaskRow>(
            "SELECT id, workspace_id, priority, text, paused, version,
                    created_at, updated_at
             FROM heartbeat_tasks WHERE workspace_id = ? ORDER BY priority DESC, id ASC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(HeartbeatTask::from).collect())
    }

    async fn upsert(
        &self,
        workspace_id: &str,
        task: &HeartbeatTask,
        expected_version: i64,
    ) -> Result<bool, RepoError> {
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
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn insert(
        &self,
        workspace_id: &str,
        priority: &str,
        text: &str,
    ) -> Result<HeartbeatTask, RepoError> {
        let row = sqlx::query_as::<_, HeartbeatTaskRow>(
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
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(HeartbeatTask::from(row))
    }

    async fn set_paused(
        &self,
        workspace_id: &str,
        task_id: i64,
        paused: bool,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE heartbeat_tasks SET paused = ?, updated_at = CURRENT_TIMESTAMP
             WHERE workspace_id = ? AND id = ?",
        )
        .bind(paused)
        .bind(workspace_id)
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, workspace_id: &str, task_id: i64) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM heartbeat_tasks WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::Database(e.to_string()))?;
        Ok(())
    }

    async fn insert_result(
        &self,
        workspace_id: &str,
        result: &HeartbeatResult,
    ) -> Result<(), RepoError> {
        let actions_json = serde_json::to_string(&result.executed_actions)
            .map_err(|e| RepoError::Serialization(e.to_string()))?;
        let proposals_json = serde_json::to_string(&result.proposals)
            .map_err(|e| RepoError::Serialization(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agent_actions (workspace_id, status, summary, actions_json, proposals_json, error)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(workspace_id)
        .bind(format!("{:?}", result.status))
        .bind(&result.summary)
        .bind(&actions_json)
        .bind(&proposals_json)
        .bind(&result.error)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::Database(e.to_string()))?;

        Ok(())
    }
}
