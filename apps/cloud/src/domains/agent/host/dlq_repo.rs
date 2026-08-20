// 数据实现，留 cloud（D2）
//! SQLite implementation of DeadLetterQueue trait.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tinyiothub_agent::runtime::event::dlq::{DeadLetterEntry, DeadLetterQueue};

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
        let id = tinyiothub_storage::Db::new(self.pool.clone())
            .enqueue_agent_dead_letter(workspace_id, event_type, payload_json, failure_reason)
            .await
            .map_err(|e| e.to_string())?;
        tracing::info!(%id, workspace_id, event_type, "Dead-letter entry enqueued");
        Ok(())
    }

    async fn list(&self, workspace_id: &str) -> Result<Vec<DeadLetterEntry>, String> {
        let rows = tinyiothub_storage::Db::new(self.pool.clone())
            .list_agent_dead_letters(workspace_id)
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
        let affected = tinyiothub_storage::Db::new(self.pool.clone())
            .delete_agent_dead_letter(entry_id)
            .await
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err(format!("Dead-letter entry not found: {}", entry_id));
        }
        Ok(())
    }
}
