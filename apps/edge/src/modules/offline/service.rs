use super::types::*;
use crate::config::EdgeConfig;
use crate::shared::error::EdgeResult;
use std::future::Future;
use std::sync::Arc;
use tinyiothub_storage::Db;

pub struct OfflineBuffer {
    db: Arc<Db>,
    config: EdgeConfig,
}

impl OfflineBuffer {
    pub fn new(db: Arc<Db>, config: EdgeConfig) -> Arc<Self> {
        Arc::new(Self { db, config })
    }

    pub async fn write(&self, msg: BufferMessage) -> EdgeResult<()> {
        let now = chrono::Utc::now().timestamp_millis();

        self.db
            .insert_offline_message(&msg.msg_type, &msg.topic, &msg.payload, now, msg.priority as i32)
            .await?;

        // FIFO eviction for normal-priority messages
        if msg.priority == BufferPriority::Normal {
            let count = self.db.count_normal_priority_offline().await?;

            if count as usize > self.config.offline_buffer_max_telemetry {
                let excess = count as usize - self.config.offline_buffer_max_telemetry;
                self.db.evict_oldest_normal_offline(excess as i64).await?;
            }
        }

        Ok(())
    }

    /// Flush a batch of messages, publishing each via the provided function.
    /// Only deletes rows from the buffer after a confirmed successful publish.
    /// Failed rows have their retry_count incremented.
    /// Returns count of messages successfully flushed.
    pub async fn flush_batch_with<F, Fut>(&self, batch_size: usize, publish: F) -> EdgeResult<usize>
    where
        F: Fn(String, Vec<u8>) -> Fut,
        Fut: Future<Output = EdgeResult<()>>,
    {
        let rows = self.db.fetch_offline_batch(batch_size as i64).await?;

        let mut sent = 0;
        for row in &rows {
            match publish(row.topic.clone(), row.payload.clone()).await {
                Ok(()) => {
                    // Confirmed — safe to delete
                    self.db.delete_offline_message(row.id).await?;
                    sent += 1;
                }
                Err(e) => {
                    // Failed — keep row, increment retry_count
                    tracing::warn!(id = row.id, ?e, "Flush publish failed, keeping in buffer");
                    self.db.increment_offline_retry(row.id).await?;
                }
            }
        }

        Ok(sent)
    }

    /// Simple flush without publishing (for backwards compatibility in tests).
    /// Deletes rows immediately — only use when you know MQTT is available.
    pub async fn flush_batch(&self, batch_size: usize) -> EdgeResult<usize> {
        let rows = self.db.fetch_offline_batch(batch_size as i64).await?;

        let mut sent = 0;
        for row in &rows {
            self.db.delete_offline_message(row.id).await?;
            sent += 1;
        }

        Ok(sent)
    }

    pub async fn get_status(&self) -> BufferStatus {
        let status = self.db.offline_buffer_status().await.unwrap_or_default();

        BufferStatus {
            total_telemetry: status.total_telemetry as u64,
            total_alarms: status.total_alarms as u64,
            oldest_timestamp: status.oldest,
            newest_timestamp: status.newest,
        }
    }
}
