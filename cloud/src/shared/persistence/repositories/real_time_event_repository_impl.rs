use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{
    modules::event::{
        Result,
        entities::Event,
        repositories::{
            DeviceStatusSummary, RealTimeEvent, RealTimeEventRepository, RealTimeFilter,
            StatusSummary,
        },
        value_objects::{EventId, EventLevel, EventSource, EventType},
    },
    shared::persistence::Database,
};

/// SQLite implementation of RealTimeEventRepository.
///
/// After the Thing Ontology migration, the `real_time_events` table is gone.
/// This implementation now writes to the `events` table, which has absorbed the
/// real-time status columns (occurrence_count, acknowledged, acknowledged_by,
/// acknowledged_at, workspace_id) and an upsert dedup index on
/// (event_type, event_subtype, device_id) WHERE device_id IS NOT NULL.
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
            INSERT INTO events (
                id, event_type, event_subtype, event_level, timestamp,
                source_type, source_id, device_id, user_id,
                title, content, occurrence_count, acknowledged,
                acknowledged_by, acknowledged_at, workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, NULL, NULL, '')
            ON CONFLICT(event_type, event_subtype, device_id) WHERE device_id IS NOT NULL
            DO UPDATE SET
                occurrence_count = occurrence_count + 1,
                timestamp = excluded.timestamp,
                source_id = excluded.source_id,
                title = excluded.title,
                content = excluded.content
        "#;

        let content_json = serde_json::to_string(event.content())?;
        let event_subtype_json = serde_json::to_string(event.event_type())?;
        let device_id_bind: Option<String> = event.source().device_id().map(|s| s.to_string());
        let user_id_bind: Option<String> = event.source().user_id().map(|s| s.to_string());

        sqlx::query(sql)
            .bind(event.id().to_string())
            .bind(event.event_type().type_string())
            .bind(&event_subtype_json)
            .bind(event.level().to_numeric())
            .bind(event.timestamp().to_rfc3339())
            .bind(event.source().source_type())
            .bind(event.source().source_id())
            .bind(&device_id_bind)
            .bind(&user_id_bind)
            .bind(event.content().title())
            .bind(content_json)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn remove_status(&self, source: &EventSource, event_type: &EventType) -> Result<()> {
        let event_subtype_json = serde_json::to_string(event_type)?;

        // Align with dedup index columns: (event_type, event_subtype, device_id)
        let sql = r#"
            DELETE FROM events
            WHERE event_type = ? AND event_subtype = ?
              AND device_id = ?
        "#;

        sqlx::query(sql)
            .bind(event_type.type_string())
            .bind(&event_subtype_json)
            .bind(source.device_id().map(|s| s.to_string()))
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn find_active_events(&self, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>> {
        let rows = self.execute_active_events_query(filter).await?;
        let mut events = Vec::new();
        for row in &rows {
            events.push(self.row_to_real_time_event(row)?);
        }
        Ok(events)
    }

    async fn get_status_summary(&self, _filter: &RealTimeFilter) -> Result<StatusSummary> {
        // Get total counts by event_level (INTEGER)
        let sql = r#"
            SELECT
                event_level,
                COUNT(*) as count,
                SUM(CASE WHEN acknowledged = 0 THEN 1 ELSE 0 END) as unacknowledged_count
            FROM events
            WHERE occurrence_count >= 1 AND event_level >= 3
            GROUP BY event_level
        "#;

        let rows = sqlx::query(sql).fetch_all(self.database.pool()).await?;

        let mut total_active = 0u64;
        let mut critical_count = 0u64;
        let mut error_count = 0u64;
        let mut warning_count = 0u64;
        let mut unacknowledged_count = 0u64;

        for row in rows {
            let level: i32 = row.get("event_level");
            let count: i64 = row.get("count");
            let unack_count: i64 = row.get("unacknowledged_count");

            total_active += count as u64;
            unacknowledged_count += unack_count as u64;

            match level {
                5 => critical_count = count as u64,
                4 => error_count = count as u64,
                3 => warning_count = count as u64,
                _ => {}
            }
        }

        // Get device summaries
        let device_sql = r#"
            SELECT
                device_id,
                COUNT(*) as active_count,
                MAX(event_level) as highest_level,
                MAX(timestamp) as latest_timestamp
            FROM events
            WHERE device_id IS NOT NULL AND occurrence_count >= 1 AND event_level >= 3
            GROUP BY device_id
        "#;

        let device_rows = sqlx::query(device_sql).fetch_all(self.database.pool()).await?;

        let mut by_device = Vec::new();
        for row in device_rows {
            let device_id: String = row.get("device_id");
            let active_count: i64 = row.get("active_count");
            let highest_level_int: i32 = row.get("highest_level");
            let latest_timestamp_str: String = row.get("latest_timestamp");

            let highest_level =
                EventLevel::from_numeric(highest_level_int).unwrap_or(EventLevel::Info);
            let latest_timestamp = DateTime::parse_from_rfc3339(&latest_timestamp_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            by_device.push(DeviceStatusSummary {
                device_id,
                active_count: active_count as u64,
                highest_level,
                latest_timestamp,
            });
        }

        let by_type = Vec::new();

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
            UPDATE events
            SET acknowledged = 1, acknowledged_by = ?, acknowledged_at = ?
            WHERE id = ?
        "#;

        sqlx::query(sql)
            .bind(user_id)
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn clear_acknowledged_events(&self) -> Result<u64> {
        let sql = "DELETE FROM events WHERE acknowledged = 1";

        let result = sqlx::query(sql).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64> {
        let sql = "DELETE FROM events WHERE timestamp < ?";

        let result =
            sqlx::query(sql).bind(before.to_rfc3339()).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }
}

impl SqliteRealTimeEventRepository {
    /// Build and execute the active events query with dynamic filters.
    /// Uses string interpolation with SQL-escaped values
    /// (safe because all values come from internal domain logic, not raw user
    /// input).
    async fn execute_active_events_query(
        &self,
        filter: &RealTimeFilter,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
        let mut conditions: Vec<String> = Vec::new();

        // -- device_ids filter --
        if let Some(ref device_ids) = filter.device_ids
            && !device_ids.is_empty() {
                let quoted: Vec<String> =
                    device_ids.iter().map(|id| format!("'{}'", id.replace('\'', "''"))).collect();
                conditions.push(format!("device_id IN ({})", quoted.join(",")));
            }

        // -- acknowledged filter --
        if let Some(acknowledged) = filter.acknowledged {
            conditions.push(format!("acknowledged = {}", if acknowledged { 1 } else { 0 }));
        }

        // -- min_level filter (event_level is now INTEGER) --
        if let Some(ref min_level) = filter.min_level {
            conditions.push(format!("event_level >= {}", min_level.to_numeric()));
        }

        // -- event_types filter --
        if let Some(ref event_types) = filter.event_types
            && !event_types.is_empty() {
                let type_conds: Vec<String> = event_types
                    .iter()
                    .map(|et| {
                        let subtype_json = serde_json::to_string(et).unwrap_or_default();
                        format!(
                            "(event_type = '{}' AND event_subtype = '{}')",
                            et.type_string().replace('\'', "''"),
                            subtype_json.replace('\'', "''")
                        )
                    })
                    .collect();
                conditions.push(format!("({})", type_conds.join(" OR ")));
            }

        // -- source_types filter --
        if let Some(ref source_types) = filter.source_types
            && !source_types.is_empty() {
                let quoted: Vec<String> =
                    source_types.iter().map(|st| format!("'{}'", st.replace('\'', "''"))).collect();
                conditions.push(format!("source_type IN ({})", quoted.join(",")));
            }

        let mut sql = String::from(
            r#"SELECT id, event_type, event_subtype, event_level, timestamp,
                      source_type, source_id, device_id, user_id,
                      title, content, occurrence_count,
                      acknowledged, acknowledged_by, acknowledged_at
               FROM events"#,
        );

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(self.database.pool()).await?;
        Ok(rows)
    }

    fn row_to_real_time_event(&self, row: &sqlx::sqlite::SqliteRow) -> Result<RealTimeEvent> {
        let id_str: String = row.get("id");
        let _event_type_str: String = row.get("event_type");
        let event_subtype_str: String = row.get("event_subtype");
        let event_level_int: i32 = row.get("event_level");
        let timestamp_str: String = row.get("timestamp");
        let title: String = row.get("title");
        let source_type: String = row.get("source_type");
        let source_id: String = row.get("source_id");
        let device_id: Option<String> = row.get("device_id");
        let user_id: Option<String> = row.get("user_id");
        let acknowledged: bool = row.get("acknowledged");
        let acknowledged_by: Option<String> = row.get("acknowledged_by");
        let acknowledged_at_str: Option<String> = row.get("acknowledged_at");

        let id = EventId::from_string(id_str);
        let event_type: EventType = serde_json::from_str(&event_subtype_str).map_err(|e| {
            crate::modules::event::EventError::Validation { message: e.to_string() }
        })?;
        let level = EventLevel::from_numeric(event_level_int).unwrap_or(EventLevel::Info);
        let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let source = EventSource::new(source_type, source_id, device_id, user_id);

        // Use the content field as a preview — truncate to 100 chars
        let content_raw: Option<String> = row.get("content");
        let content_preview = content_raw.unwrap_or_default().chars().take(100).collect::<String>();

        let acknowledged_at = acknowledged_at_str
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

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
