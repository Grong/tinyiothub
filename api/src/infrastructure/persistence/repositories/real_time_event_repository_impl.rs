use crate::domain::event::{
    entities::Event,
    repositories::{
        DeviceStatusSummary, RealTimeEvent, RealTimeEventRepository, RealTimeFilter, StatusSummary,
    },
    value_objects::{EventId, EventLevel, EventSource, EventType},
    Result,
};
use crate::infrastructure::persistence::Database;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

/// SQLite implementation of RealTimeEventRepository
pub struct SqliteRealTimeEventRepository {
    database: Database,
}

impl SqliteRealTimeEventRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl RealTimeEventRepository for SqliteRealTimeEventRepository {
    async fn upsert_status(&self, event: &Event) -> Result<()> {
        // Only store events that should update real-time status
        if !event.should_update_real_time_status() {
            return Ok(());
        }

        let sql = r#"
            INSERT OR REPLACE INTO real_time_events (
                id, event_type, level, source_type, source_id, device_id, user_id,
                title, content_preview, timestamp, acknowledged, acknowledged_by, acknowledged_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let event_type_str = serde_json::to_string(event.event_type())?;
        let level_str = format!("{:?}", event.level());
        let _source_str = serde_json::to_string(event.source())?;
        let content_preview = event.content().get_preview(100); // First 100 chars

        sqlx::query(sql)
            .bind(event.id().to_string())
            .bind(event_type_str)
            .bind(level_str)
            .bind(event.source().source_type())
            .bind(event.source().source_id())
            .bind(event.source().device_id())
            .bind(event.source().user_id())
            .bind(event.content().title())
            .bind(content_preview)
            .bind(event.timestamp())
            .bind(false) // acknowledged
            .bind(None::<String>) // acknowledged_by
            .bind(None::<DateTime<Utc>>) // acknowledged_at
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn remove_status(&self, source: &EventSource, event_type: &EventType) -> Result<()> {
        let sql = r#"
            DELETE FROM real_time_events 
            WHERE source_type = ? AND source_id = ? AND event_type = ?
        "#;

        let event_type_str = serde_json::to_string(event_type)?;

        sqlx::query(sql)
            .bind(source.source_type())
            .bind(source.source_id())
            .bind(event_type_str)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn find_active_events(&self, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>> {
        let mut base_sql = String::from(
            r#"SELECT id, event_type, level, source_type, source_id, device_id, user_id,
                   title, content_preview, timestamp, acknowledged, acknowledged_by, acknowledged_at
            FROM real_time_events WHERE 1=1"#
        );

        // Build query dynamically based on filter
        let query = match (&filter.device_ids, filter.acknowledged) {
            // Both device_ids and acknowledged are set
            (Some(device_ids), Some(acknowledged)) if !device_ids.is_empty() => {
                let placeholders = device_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                base_sql.push_str(&format!(" AND device_id IN ({})", placeholders));
                base_sql.push_str(" AND acknowledged = ?");
                base_sql.push_str(" ORDER BY timestamp DESC");

                let mut q = sqlx::query(&base_sql);
                for device_id in device_ids {
                    q = q.bind(device_id);
                }
                q.bind(acknowledged)
            }
            // Only device_ids is set and non-empty
            (Some(device_ids), None) if !device_ids.is_empty() => {
                let placeholders = device_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                base_sql.push_str(&format!(" AND device_id IN ({})", placeholders));
                base_sql.push_str(" ORDER BY timestamp DESC");

                let mut q = sqlx::query(&base_sql);
                for device_id in device_ids {
                    q = q.bind(device_id);
                }
                q
            }
            // Only acknowledged is set
            (_, Some(acknowledged)) if filter.device_ids.as_ref().map_or(true, |ids| ids.is_empty()) => {
                base_sql.push_str(" AND acknowledged = ?");
                base_sql.push_str(" ORDER BY timestamp DESC");
                sqlx::query(&base_sql).bind(acknowledged)
            }
            // Neither is set
            _ => {
                base_sql.push_str(" ORDER BY timestamp DESC");
                sqlx::query(&base_sql)
            }
        };

        // Execute query with proper parameter binding
        let rows = query.fetch_all(self.database.pool()).await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_real_time_event(row)?);
        }

        Ok(events)
    }

    async fn get_status_summary(&self, _filter: &RealTimeFilter) -> Result<StatusSummary> {
        // Get total counts by level
        let sql = r#"
            SELECT 
                level,
                COUNT(*) as count,
                SUM(CASE WHEN acknowledged = 0 THEN 1 ELSE 0 END) as unacknowledged_count
            FROM real_time_events 
            GROUP BY level
        "#;

        let rows = sqlx::query(sql).fetch_all(self.database.pool()).await?;

        let mut total_active = 0u64;
        let mut critical_count = 0u64;
        let mut error_count = 0u64;
        let mut warning_count = 0u64;
        let mut unacknowledged_count = 0u64;

        for row in rows {
            let level: String = row.get("level");
            let count: i64 = row.get("count");
            let unack_count: i64 = row.get("unacknowledged_count");

            total_active += count as u64;
            unacknowledged_count += unack_count as u64;

            match level.as_str() {
                "Critical" => critical_count = count as u64,
                "Error" => error_count = count as u64,
                "Warning" => warning_count = count as u64,
                _ => {}
            }
        }

        // Get device summaries
        let device_sql = r#"
            SELECT 
                device_id,
                COUNT(*) as active_count,
                MAX(level) as highest_level,
                MAX(timestamp) as latest_timestamp
            FROM real_time_events 
            WHERE device_id IS NOT NULL
            GROUP BY device_id
        "#;

        let device_rows = sqlx::query(device_sql)
            .fetch_all(self.database.pool())
            .await?;

        let mut by_device = Vec::new();
        for row in device_rows {
            let device_id: String = row.get("device_id");
            let active_count: i64 = row.get("active_count");
            let highest_level_str: String = row.get("highest_level");
            let latest_timestamp: DateTime<Utc> = row.get("latest_timestamp");

            let highest_level = match highest_level_str.as_str() {
                "Critical" => EventLevel::Critical,
                "Error" => EventLevel::Error,
                "Warning" => EventLevel::Warning,
                _ => EventLevel::Info,
            };

            by_device.push(DeviceStatusSummary {
                device_id,
                active_count: active_count as u64,
                highest_level,
                latest_timestamp,
            });
        }

        // Get type summaries (simplified)
        let by_type = Vec::new(); // Would implement full type summary in real version

        Ok(StatusSummary {
            total_active,
            critical_count,
            error_count,
            warning_count,
            unacknowledged_count,
            by_device,
            by_type,
        })
    }

    async fn acknowledge_event(&self, id: &EventId, user_id: &str) -> Result<()> {
        let sql = r#"
            UPDATE real_time_events 
            SET acknowledged = 1, acknowledged_by = ?, acknowledged_at = ?
            WHERE id = ?
        "#;

        sqlx::query(sql)
            .bind(user_id)
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn clear_acknowledged_events(&self) -> Result<u64> {
        let sql = "DELETE FROM real_time_events WHERE acknowledged = 1";

        let result = sqlx::query(sql).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64> {
        let sql = "DELETE FROM real_time_events WHERE timestamp < ?";

        let result = sqlx::query(sql)
            .bind(before)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }
}

impl SqliteRealTimeEventRepository {
    fn row_to_real_time_event(&self, row: sqlx::sqlite::SqliteRow) -> Result<RealTimeEvent> {
        let id_str: String = row.get("id");
        let event_type_str: String = row.get("event_type");
        let level_str: String = row.get("level");
        let timestamp: DateTime<Utc> = row.get("timestamp");
        let title: String = row.get("title");
        let content_preview: String = row.get("content_preview");
        let acknowledged: bool = row.get("acknowledged");
        let acknowledged_by: Option<String> = row.get("acknowledged_by");
        let acknowledged_at: Option<DateTime<Utc>> = row.get("acknowledged_at");

        let id = EventId::from_string(id_str);
        let event_type: EventType = serde_json::from_str(&event_type_str)?;
        let level = match level_str.as_str() {
            "Info" => EventLevel::Info,
            "Warning" => EventLevel::Warning,
            "Error" => EventLevel::Error,
            "Critical" => EventLevel::Critical,
            "Debug" => EventLevel::Debug,
            _ => EventLevel::Info,
        };

        let source_type: String = row.get("source_type");
        let source_id: String = row.get("source_id");
        let device_id: Option<String> = row.get("device_id");
        let user_id: Option<String> = row.get("user_id");

        let source = EventSource::new(source_type, source_id, device_id, user_id);

        Ok(RealTimeEvent {
            id,
            event_type,
            level,
            source,
            title,
            content_preview,
            timestamp,
            acknowledged,
            acknowledged_by,
            acknowledged_at,
        })
    }
}
