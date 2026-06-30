//! SQLite implementation of DeadLetterQueue trait.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_ai::event::dlq::{DeadLetterEntry, DeadLetterQueue};
use uuid::Uuid;

pub struct SqliteDeadLetterQueue {
    pool: SqlitePool,
}

impl SqliteDeadLetterQueue {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeadLetterQueue for SqliteDeadLetterQueue {
    async fn enqueue(
        &self,
        workspace_id: &str,
        event_type: &str,
        payload_json: &str,
        failure_reason: &str,
    ) -> Result<(), String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_dead_letters (id, workspace_id, event_type, payload_json, failure_reason, enqueued_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(event_type)
        .bind(payload_json)
        .bind(failure_reason)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        tracing::info!(%id, workspace_id, event_type, "Dead-letter entry enqueued");
        Ok(())
    }

    async fn list(&self, workspace_id: &str) -> Result<Vec<DeadLetterEntry>, String> {
        #[derive(Debug, sqlx::FromRow)]
        struct DlqRow {
            id: String,
            workspace_id: String,
            event_type: String,
            payload_json: String,
            failure_reason: String,
            enqueued_at: String,
        }
        let rows = sqlx::query_as::<_, DlqRow>(
            "SELECT id, workspace_id, event_type, payload_json, failure_reason, enqueued_at
             FROM agent_dead_letters WHERE workspace_id = ? ORDER BY enqueued_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows
            .into_iter()
            .map(|r| DeadLetterEntry {
                id: r.id,
                workspace_id: r.workspace_id,
                event_type: r.event_type,
                payload_json: r.payload_json,
                failure_reason: r.failure_reason,
                enqueued_at: r.enqueued_at,
            })
            .collect())
    }

    async fn discard(&self, entry_id: &str) -> Result<(), String> {
        let result = sqlx::query("DELETE FROM agent_dead_letters WHERE id = ?")
            .bind(entry_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        if result.rows_affected() == 0 {
            return Err(format!("Dead-letter entry not found: {}", entry_id));
        }
        Ok(())
    }
}
