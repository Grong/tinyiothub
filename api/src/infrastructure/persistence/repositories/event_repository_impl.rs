use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{
    domain::event::{
        entities::Event,
        repositories::{
            EventCriteria, EventRepository, EventStatistics, ExportFormat, SortBy, SortOrder,
            StatisticsParams,
        },
        value_objects::{EventId, EventLevel, EventSource, EventType, RichContent},
        Result,
    },
    infrastructure::persistence::Database,
};

/// SQLite implementation of EventRepository
pub struct SqliteEventRepository {
    database: Database,
}

impl SqliteEventRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl EventRepository for SqliteEventRepository {
    async fn save(&self, event: &Event) -> Result<()> {
        let sql = r#"
            INSERT INTO events (
                id, event_type, event_subtype, event_level, timestamp, source_type, source_id, 
                device_id, user_id, title, content, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let event_type_str = match event.event_type() {
            EventType::System(_) => "system",
            EventType::Device(_) => "device",
        };
        let event_subtype_str = serde_json::to_string(event.event_type())?;
        let event_level = event.level().to_numeric();
        let content_str = serde_json::to_string(event.content())?;
        let timestamp_str = event.timestamp().to_rfc3339();
        let created_at_str = Utc::now().to_rfc3339();

        sqlx::query(sql)
            .bind(event.id().to_string())
            .bind(event_type_str)
            .bind(event_subtype_str)
            .bind(event_level)
            .bind(timestamp_str)
            .bind(event.source().source_type())
            .bind(event.source().source_id())
            .bind(event.source().device_id())
            .bind(event.source().user_id())
            .bind(event.content().title())
            .bind(content_str)
            .bind(created_at_str)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &EventId) -> Result<Option<Event>> {
        let sql = r#"
            SELECT id, event_type, event_subtype, event_level, timestamp, source_type, source_id, 
                   device_id, user_id, title, content
            FROM events 
            WHERE id = ?
        "#;

        let row =
            sqlx::query(sql).bind(id.to_string()).fetch_optional(self.database.pool()).await?;

        if let Some(row) = row {
            let event = self.row_to_event(row)?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    async fn find_by_criteria(&self, criteria: &EventCriteria) -> Result<Vec<Event>> {
        // Build base SQL
        let mut sql = String::from(
            "SELECT id, event_type, event_subtype, event_level, timestamp, source_type, source_id, device_id, user_id, title, content FROM events WHERE 1=1"
        );

        // Add time range filters
        if criteria.start_time.is_some() {
            sql.push_str(" AND timestamp >= ?");
        }

        if criteria.end_time.is_some() {
            sql.push_str(" AND timestamp <= ?");
        }

        // Add level filters
        if let Some(levels) = &criteria.levels {
            if !levels.is_empty() {
                let placeholders = vec!["?"; levels.len()].join(",");
                sql.push_str(&format!(" AND event_level IN ({})", placeholders));
            }
        }

        // Add device ID filters
        if let Some(device_ids) = &criteria.device_ids {
            if !device_ids.is_empty() {
                let placeholders = vec!["?"; device_ids.len()].join(",");
                sql.push_str(&format!(" AND device_id IN ({})", placeholders));
            }
        }

        // Add search text filter
        if criteria.search_text.is_some() {
            sql.push_str(" AND (title LIKE ? OR content LIKE ?)");
        }

        // Add sorting
        match criteria.sort_by {
            SortBy::Timestamp => sql.push_str(" ORDER BY timestamp"),
            SortBy::Level => sql.push_str(" ORDER BY event_level"),
            SortBy::EventType => sql.push_str(" ORDER BY event_type"),
            SortBy::Source => sql.push_str(" ORDER BY source_type"),
        }

        match criteria.sort_order {
            SortOrder::Ascending => sql.push_str(" ASC"),
            SortOrder::Descending => sql.push_str(" DESC"),
        }

        // Add pagination
        if let Some(limit) = criteria.limit {
            sql.push_str(" LIMIT ?");

            if let Some(offset) = criteria.offset {
                sql.push_str(" OFFSET ?");
            }
        }

        // Build query with parameters
        let mut query = sqlx::query(&sql);

        // Bind time range parameters
        if let Some(start_time) = criteria.start_time {
            let start_str = start_time.to_rfc3339();
            query = query.bind(start_str);
        }

        if let Some(end_time) = criteria.end_time {
            let end_str = end_time.to_rfc3339();
            query = query.bind(end_str);
        }

        // Bind level filters
        if let Some(levels) = &criteria.levels {
            for level in levels {
                query = query.bind(level.to_numeric());
            }
        }

        // Bind device ID filters
        if let Some(device_ids) = &criteria.device_ids {
            for device_id in device_ids {
                query = query.bind(device_id.clone());
            }
        }

        // Bind search text filter
        if let Some(search_text) = &criteria.search_text {
            let search_pattern = format!("%{}%", search_text);
            query = query.bind(search_pattern.clone());
            query = query.bind(search_pattern);
        }

        // Bind pagination parameters
        if let Some(limit) = criteria.limit {
            query = query.bind(limit as i64);

            if let Some(offset) = criteria.offset {
                query = query.bind(offset as i64);
            }
        }

        // Execute query
        let rows = query.fetch_all(self.database.pool()).await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_event(row)?);
        }

        Ok(events)
    }

    async fn count_by_level(&self, level: EventLevel) -> Result<u64> {
        let sql = "SELECT COUNT(*) as count FROM events WHERE event_level = ?";
        let level_num = level.to_numeric();

        let row = sqlx::query(sql).bind(level_num).fetch_one(self.database.pool()).await?;

        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    async fn count_by_type(&self, event_type: &EventType) -> Result<u64> {
        let sql = "SELECT COUNT(*) as count FROM events WHERE event_type = ?";
        let type_str = serde_json::to_string(event_type)?;

        let row = sqlx::query(sql).bind(type_str).fetch_one(self.database.pool()).await?;

        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    async fn save_batch(&self, events: &[Event]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        // 使用事务批量插入
        let mut tx = self.database.pool().begin().await?;

        let sql = r#"
            INSERT INTO events (
                id, event_type, event_subtype, event_level, timestamp, source_type, source_id, 
                device_id, user_id, title, content, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        for event in events {
            let event_type_str = match event.event_type() {
                EventType::System(_) => "system",
                EventType::Device(_) => "device",
            };
            let event_subtype_str = serde_json::to_string(event.event_type())?;
            let event_level = event.level().to_numeric();
            let content_str = serde_json::to_string(event.content())?;
            let timestamp_str = event.timestamp().to_rfc3339();
            let created_at_str = Utc::now().to_rfc3339();

            if let Err(e) = sqlx::query(sql)
                .bind(event.id().to_string())
                .bind(event_type_str)
                .bind(event_subtype_str)
                .bind(event_level)
                .bind(timestamp_str)
                .bind(event.source().source_type())
                .bind(event.source().source_id())
                .bind(event.source().device_id())
                .bind(event.source().user_id())
                .bind(event.content().title())
                .bind(content_str)
                .bind(created_at_str)
                .execute(&mut *tx)
                .await
            {
                // 回滚事务并返回错误
                let _ = tx.rollback().await;
                return Err(e.into());
            }
        }

        // 提交事务
        tx.commit().await?;

        Ok(())
    }

    async fn get_statistics(&self, _params: &StatisticsParams) -> Result<EventStatistics> {
        // Simplified implementation - in real version would implement full statistics
        let total_count = self.get_total_count().await?;

        Ok(EventStatistics { total_count, groups: vec![] })
    }

    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64> {
        let sql = "DELETE FROM events WHERE timestamp < ?";
        let before_str = before.to_rfc3339();

        let result = sqlx::query(sql).bind(before_str).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn export_events(
        &self,
        criteria: &EventCriteria,
        format: ExportFormat,
    ) -> Result<Vec<u8>> {
        let events = self.find_by_criteria(criteria).await?;

        match format {
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&events)?;
                Ok(json.into_bytes())
            }
            ExportFormat::Csv => {
                // Simplified CSV export
                let mut csv = String::from("id,event_type,level,timestamp,source,title\n");
                for event in events {
                    csv.push_str(&format!(
                        "{},{:?},{:?},{},{},{}\n",
                        event.id(),
                        event.event_type(),
                        event.level(),
                        event.timestamp(),
                        event.source().source_type(),
                        event.content().title()
                    ));
                }
                Ok(csv.into_bytes())
            }
            ExportFormat::Excel => {
                // For now, return CSV format - in real implementation would generate Excel
                self.export_events(criteria, ExportFormat::Csv).await
            }
        }
    }
}

impl SqliteEventRepository {
    fn row_to_event(&self, row: sqlx::sqlite::SqliteRow) -> Result<Event> {
        let id_str: String = row.get("id");
        let event_subtype_str: String = row.get("event_subtype");
        let event_level_num: i32 = row.get("event_level");
        let timestamp_str: String = row.get("timestamp");
        let content_str: String = row.get("content");

        let id = EventId::from_string(id_str.clone());

        // 解析事件类型（JSON 格式）
        let event_type: EventType = serde_json::from_str(&event_subtype_str).map_err(|e| {
            tracing::error!(
                "Failed to deserialize event_type for event {}: {} - content: {}",
                id_str,
                e,
                event_subtype_str
            );
            e
        })?;

        let level = EventLevel::from_numeric(event_level_num).unwrap_or(EventLevel::Info);
        let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let source_type: String = row.get("source_type");
        let source_id: String = row.get("source_id");
        let device_id: Option<String> = row.get("device_id");
        let user_id: Option<String> = row.get("user_id");

        let source = EventSource::new(source_type, source_id, device_id, user_id);

        // 解析内容
        let content: RichContent = if content_str.trim().is_empty() {
            tracing::warn!("Empty content for event {}, using default", id_str);
            RichContent::new("Empty Event".to_string(), vec![])
        } else {
            serde_json::from_str(&content_str).map_err(|e| {
                tracing::error!(
                    "Failed to deserialize content for event {}: {} - content: {}",
                    id_str,
                    e,
                    &content_str[..content_str.len().min(200)]
                );
                e
            })?
        };

        Ok(Event::reconstruct(id, event_type, level, timestamp, source, content, None))
    }

    async fn get_total_count(&self) -> Result<u64> {
        let sql = "SELECT COUNT(*) as count FROM events";
        let row = sqlx::query(sql).fetch_one(self.database.pool()).await?;

        let count: i64 = row.get("count");
        Ok(count as u64)
    }
}
