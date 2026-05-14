use std::sync::Arc;
use std::future::Future;
use sqlx::Row;
use super::types::*;
use crate::config::EdgeConfig;
use crate::shared::error::EdgeResult;
use tinyiothub_storage::sqlite::Database;

pub struct OfflineBuffer {
    db: Arc<Database>,
    config: EdgeConfig,
}

impl OfflineBuffer {
    pub fn new(db: Arc<Database>, config: EdgeConfig) -> Arc<Self> {
        Arc::new(Self { db, config })
    }

    pub async fn write(&self, msg: BufferMessage) -> EdgeResult<()> {
        let pool = self.db.pool();
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO offline_buffer (msg_type, topic, payload, created_at, priority) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&msg.msg_type)
        .bind(&msg.topic)
        .bind(&msg.payload)
        .bind(now)
        .bind(msg.priority as i32)
        .execute(pool)
        .await?;

        // FIFO eviction for normal-priority messages
        if msg.priority == BufferPriority::Normal {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM offline_buffer WHERE priority = 0"
            ).fetch_one(pool).await?;

            if count as usize > self.config.offline_buffer_max_telemetry {
                let excess = count as usize - self.config.offline_buffer_max_telemetry;
                sqlx::query(
                    "DELETE FROM offline_buffer WHERE id IN (
                        SELECT id FROM offline_buffer WHERE priority = 0 ORDER BY created_at ASC LIMIT ?
                    )"
                ).bind(excess as i64).execute(pool).await?;
            }
        }

        Ok(())
    }

    /// Flush a batch of messages, publishing each via the provided function.
    /// Only deletes rows from the buffer after a confirmed successful publish.
    /// Failed rows have their retry_count incremented.
    /// Returns count of messages successfully flushed.
    pub async fn flush_batch_with<F, Fut>(
        &self,
        batch_size: usize,
        publish: F,
    ) -> EdgeResult<usize>
    where
        F: Fn(String, Vec<u8>) -> Fut,
        Fut: Future<Output = EdgeResult<()>>,
    {
        let pool = self.db.pool();
        let rows = sqlx::query(
            "SELECT id, msg_type, topic, payload FROM offline_buffer ORDER BY created_at ASC LIMIT ?"
        )
        .bind(batch_size as i64)
        .fetch_all(pool)
        .await?;

        let mut sent = 0;
        for row in &rows {
            let id: i64 = row.get("id");
            let topic: String = row.get("topic");
            let payload: Vec<u8> = row.get("payload");

            match publish(topic, payload).await {
                Ok(()) => {
                    // Confirmed — safe to delete
                    sqlx::query("DELETE FROM offline_buffer WHERE id = ?")
                        .bind(id)
                        .execute(pool)
                        .await?;
                    sent += 1;
                }
                Err(e) => {
                    // Failed — keep row, increment retry_count
                    tracing::warn!(id, ?e, "Flush publish failed, keeping in buffer");
                    sqlx::query(
                        "UPDATE offline_buffer SET retry_count = retry_count + 1 WHERE id = ?"
                    )
                    .bind(id)
                    .execute(pool)
                    .await?;
                }
            }
        }

        Ok(sent)
    }

    /// Simple flush without publishing (for backwards compatibility in tests).
    /// Deletes rows immediately — only use when you know MQTT is available.
    pub async fn flush_batch(&self, batch_size: usize) -> EdgeResult<usize> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            "SELECT id, msg_type, topic, payload FROM offline_buffer ORDER BY created_at ASC LIMIT ?"
        )
        .bind(batch_size as i64)
        .fetch_all(pool)
        .await?;

        let mut sent = 0;
        for row in &rows {
            let id: i64 = row.get("id");
            sqlx::query("DELETE FROM offline_buffer WHERE id = ?")
                .bind(id).execute(pool).await?;
            sent += 1;
        }

        Ok(sent)
    }

    pub async fn get_status(&self) -> BufferStatus {
        let pool = self.db.pool();
        let total_telemetry: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM offline_buffer WHERE msg_type = 'telemetry'"
        ).fetch_one(pool).await.unwrap_or(0);

        let total_alarms: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM offline_buffer WHERE msg_type = 'alarm'"
        ).fetch_one(pool).await.unwrap_or(0);

        let oldest: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(created_at) FROM offline_buffer"
        ).fetch_one(pool).await.ok().flatten();

        let newest: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(created_at) FROM offline_buffer"
        ).fetch_one(pool).await.ok().flatten();

        BufferStatus {
            total_telemetry: total_telemetry as u64,
            total_alarms: total_alarms as u64,
            oldest_timestamp: oldest,
            newest_timestamp: newest,
        }
    }
}
