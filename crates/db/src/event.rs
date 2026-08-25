//! Event 持久化：领域事件与实时状态（P-集中化 E3，自 event crate 迁入）。
//!
//! 类型随 repo 住 db（方案 B）；Event 实体本身在 core::models::event（全域共享），
//! 本文件只持有查询/统计/实时状态的仓储侧类型。event crate 经 re-export 兼容。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tinyiothub_core::models::event::{Event, EventId, EventLevel, EventSource, EventType, RichContent};

use crate::database::Db;
use crate::error::Result;

// ──────────────────────────────────────────────
// 仓储侧类型（查询条件/统计/实时状态）— 自 event/types.rs 迁入
// ──────────────────────────────────────────────

pub struct EventCriteria {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub event_types: Option<Vec<EventType>>,
    pub levels: Option<Vec<EventLevel>>,
    pub source_types: Option<Vec<String>>,
    pub device_ids: Option<Vec<String>>,
    pub user_ids: Option<Vec<String>>,
    pub search_text: Option<String>,
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortBy {
    Timestamp,
    Level,
    EventType,
    Source,
}

/// Sort order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Parameters for event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsParams {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub group_by: GroupBy,
    pub device_ids: Option<Vec<String>>,
}

/// Grouping options for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupBy {
    Level,
    EventType,
    Source,
    Hour,
    Day,
    Week,
    Month,
}

/// Event statistics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStatistics {
    pub total_count: u64,
    pub groups: Vec<StatisticsGroup>,
}

/// Statistics group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsGroup {
    pub key: String,
    pub count: u64,
    pub percentage: f64,
}

/// Export format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Excel,
}

impl Default for EventCriteria {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            event_types: None,
            levels: None,
            source_types: None,
            device_ids: None,
            user_ids: None,
            search_text: None,
            sort_by: SortBy::Timestamp,
            sort_order: SortOrder::Descending,
            limit: None,
            offset: None,
        }
    }
}

impl EventCriteria {
    pub fn builder() -> EventCriteriaBuilder {
        EventCriteriaBuilder::new()
    }

    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    pub fn with_event_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    pub fn with_levels(mut self, levels: Vec<EventLevel>) -> Self {
        self.levels = Some(levels);
        self
    }

    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    pub fn with_sort(mut self, sort_by: SortBy, sort_order: SortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for EventCriteria
pub struct EventCriteriaBuilder {
    criteria: EventCriteria,
}

impl EventCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: EventCriteria::default(),
        }
    }

    pub fn start_time(mut self, start: DateTime<Utc>) -> Self {
        self.criteria.start_time = Some(start);
        self
    }

    pub fn end_time(mut self, end: DateTime<Utc>) -> Self {
        self.criteria.end_time = Some(end);
        self
    }

    pub fn event_types(mut self, types: Vec<EventType>) -> Self {
        self.criteria.event_types = Some(types);
        self
    }

    pub fn levels(mut self, levels: Vec<EventLevel>) -> Self {
        self.criteria.levels = Some(levels);
        self
    }

    pub fn device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.criteria.device_ids = Some(device_ids);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn sort_by(mut self, sort_by: SortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: SortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> EventCriteria {
        self.criteria
    }
}

impl Default for EventCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────
// Real-Time Event DTOs (from real_time_event_repository.rs)
// ──────────────────────────────────────────────

/// Filter for real-time events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealTimeFilter {
    pub device_ids: Option<Vec<String>>,
    pub event_types: Option<Vec<EventType>>,
    pub source_types: Option<Vec<String>>,
    pub acknowledged: Option<bool>,
    pub min_level: Option<EventLevel>,
    /// Tenant isolation (eng-review T1): restrict to one workspace.
    pub workspace_id: Option<String>,
}

impl RealTimeFilter {
    pub fn builder() -> RealTimeFilterBuilder {
        RealTimeFilterBuilder::new()
    }

    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    pub fn with_event_types(mut self, event_types: Vec<EventType>) -> Self {
        self.event_types = Some(event_types);
        self
    }

    pub fn with_acknowledged(mut self, acknowledged: bool) -> Self {
        self.acknowledged = Some(acknowledged);
        self
    }

    pub fn with_min_level(mut self, level: EventLevel) -> Self {
        self.min_level = Some(level);
        self
    }

    pub fn unacknowledged() -> Self {
        Self::default().with_acknowledged(false)
    }

    pub fn critical_and_errors() -> Self {
        Self::default().with_min_level(EventLevel::Error)
    }
}

/// Builder for RealTimeFilter
pub struct RealTimeFilterBuilder {
    filter: RealTimeFilter,
}

impl RealTimeFilterBuilder {
    pub fn new() -> Self {
        Self {
            filter: RealTimeFilter::default(),
        }
    }

    pub fn device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.filter.device_ids = Some(device_ids);
        self
    }

    pub fn event_types(mut self, event_types: Vec<EventType>) -> Self {
        self.filter.event_types = Some(event_types);
        self
    }

    pub fn source_types(mut self, source_types: Vec<String>) -> Self {
        self.filter.source_types = Some(source_types);
        self
    }

    pub fn acknowledged(mut self, acknowledged: bool) -> Self {
        self.filter.acknowledged = Some(acknowledged);
        self
    }

    pub fn min_level(mut self, level: EventLevel) -> Self {
        self.filter.min_level = Some(level);
        self
    }

    pub fn build(self) -> RealTimeFilter {
        self.filter
    }
}

impl Default for RealTimeFilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time event representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeEvent {
    pub id: EventId,
    pub event_type: EventType,
    pub level: EventLevel,
    pub source: EventSource,
    pub title: String,
    pub content_preview: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

impl RealTimeEvent {
    pub fn is_critical(&self) -> bool {
        matches!(self.level, EventLevel::Critical)
    }

    pub fn needs_attention(&self) -> bool {
        matches!(self.level, EventLevel::Critical | EventLevel::Error)
    }

    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.timestamp
    }

    pub fn is_stale(&self, threshold: chrono::Duration) -> bool {
        self.age() > threshold
    }
}

/// Status summary for real-time events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub total_active: u64,
    pub critical_count: u64,
    pub error_count: u64,
    pub warning_count: u64,
    pub unacknowledged_count: u64,
    pub by_device: Vec<DeviceStatusSummary>,
    pub by_type: Vec<TypeStatusSummary>,
}

impl StatusSummary {
    pub fn has_critical_issues(&self) -> bool {
        self.critical_count > 0
    }

    pub fn has_unacknowledged(&self) -> bool {
        self.unacknowledged_count > 0
    }

    pub fn health_status(&self) -> HealthStatus {
        if self.critical_count > 0 {
            HealthStatus::Critical
        } else if self.error_count > 0 {
            HealthStatus::Error
        } else if self.warning_count > 0 {
            HealthStatus::Warning
        } else {
            HealthStatus::Healthy
        }
    }
}

/// Overall system health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Error,
    Critical,
}

/// Device-specific status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusSummary {
    pub thing_id: String,
    pub active_count: u64,
    pub highest_level: EventLevel,
    pub latest_timestamp: DateTime<Utc>,
}

/// Type-specific status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeStatusSummary {
    pub event_type: EventType,
    pub active_count: u64,
    pub highest_level: EventLevel,
}

pub(crate) async fn insert_event(pool: &SqlitePool, event: &Event) -> Result<()> {
    let sql = r#"
        INSERT INTO events (
            id, event_type, event_subtype, event_level, timestamp, source_type, source_id,
            thing_id, user_id, title, content, created_at, workspace_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;

    let event_type_str = match event.event_type() {
        EventType::System(_) => "system",
        EventType::Device(_) => "device",
        EventType::Ai(_) => "ai",
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
        .bind("")
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn find_event_by_id(pool: &SqlitePool, id: &EventId) -> Result<Option<Event>> {
    let sql = r#"
        SELECT id, event_type, event_subtype, event_level, timestamp, source_type, source_id, 
               thing_id, user_id, title, content
        FROM events 
        WHERE id = ?
    "#;

    let row = sqlx::query(sql).bind(id.to_string()).fetch_optional(pool).await?;

    if let Some(row) = row {
        let event = row_to_event(row)?;
        Ok(Some(event))
    } else {
        Ok(None)
    }
}

pub(crate) async fn query_events(pool: &SqlitePool, criteria: &EventCriteria) -> Result<Vec<Event>> {
    // Build base SQL
    let mut sql = String::from(
        "SELECT id, event_type, event_subtype, event_level, timestamp, source_type, source_id, thing_id, user_id, title, content FROM events WHERE 1=1",
    );

    // Add time range filters
    if criteria.start_time.is_some() {
        sql.push_str(" AND timestamp >= ?");
    }

    if criteria.end_time.is_some() {
        sql.push_str(" AND timestamp <= ?");
    }

    // Add level filters
    if let Some(levels) = &criteria.levels
        && !levels.is_empty()
    {
        let placeholders = vec!["?"; levels.len()].join(",");
        sql.push_str(&format!(" AND event_level IN ({})", placeholders));
    }

    // Add device ID filters
    if let Some(device_ids) = &criteria.device_ids
        && !device_ids.is_empty()
    {
        let placeholders = vec!["?"; device_ids.len()].join(",");
        sql.push_str(&format!(" AND thing_id IN ({})", placeholders));
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
    if let Some(_limit) = criteria.limit {
        sql.push_str(" LIMIT ?");

        if let Some(_offset) = criteria.offset {
            sql.push_str(" OFFSET ?");
        }
    }

    // Build query with parameters
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

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
        for thing_id in device_ids {
            query = query.bind(thing_id.clone());
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
    let rows = query.fetch_all(pool).await?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row_to_event(row)?);
    }

    Ok(events)
}

pub(crate) async fn count_events_by_level(pool: &SqlitePool, level: EventLevel) -> Result<u64> {
    let sql = "SELECT COUNT(*) as count FROM events WHERE event_level = ?";
    let level_num = level.to_numeric();

    let row = sqlx::query(sql).bind(level_num).fetch_one(pool).await?;

    let count: i64 = row.get("count");
    Ok(count as u64)
}

pub(crate) async fn count_events_by_type(pool: &SqlitePool, event_type: &EventType) -> Result<u64> {
    let sql = "SELECT COUNT(*) as count FROM events WHERE event_type = ?";
    let type_str = serde_json::to_string(event_type)?;

    let row = sqlx::query(sql).bind(type_str).fetch_one(pool).await?;

    let count: i64 = row.get("count");
    Ok(count as u64)
}

pub(crate) async fn insert_events_batch(pool: &SqlitePool, events: &[Event]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }

    // 使用事务批量插入
    let mut tx = pool.begin().await?;

    let sql = r#"
        INSERT INTO events (
            id, event_type, event_subtype, event_level, timestamp, source_type, source_id,
            thing_id, user_id, title, content, created_at, workspace_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;

    for event in events {
        let event_type_str = match event.event_type() {
            EventType::System(_) => "system",
            EventType::Device(_) => "device",
            EventType::Ai(_) => "ai",
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
            .bind("")
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

pub(crate) async fn get_event_statistics(pool: &SqlitePool, _params: &StatisticsParams) -> Result<EventStatistics> {
    // Simplified implementation - in real version would implement full statistics
    let total_count = count_events(pool).await?;

    Ok(EventStatistics {
        total_count,
        groups: vec![],
    })
}

pub(crate) async fn cleanup_old_events(pool: &SqlitePool, before: DateTime<Utc>) -> Result<u64> {
    let sql = "DELETE FROM events WHERE timestamp < ?";
    let before_str = before.to_rfc3339();

    let result = sqlx::query(sql).bind(before_str).execute(pool).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn export_events(
    pool: &SqlitePool,
    criteria: &EventCriteria,
    format: ExportFormat,
) -> Result<Vec<u8>> {
    let events = query_events(pool, criteria).await?;

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
            Box::pin(export_events(pool, criteria, ExportFormat::Csv)).await
        }
    }
}

fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<Event> {
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
    let thing_id: Option<String> = row.get("thing_id");
    let user_id: Option<String> = row.get("user_id");

    let source = EventSource::new(source_type, source_id, thing_id, user_id);

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

    Ok(Event::reconstruct(id, event_type, level, timestamp, source, content))
}

pub(crate) async fn count_events(pool: &SqlitePool) -> Result<u64> {
    let sql = "SELECT COUNT(*) as count FROM events";
    let row = sqlx::query(sql).fetch_one(pool).await?;

    let count: i64 = row.get("count");
    Ok(count as u64)
}

pub(crate) async fn upsert_event_status(pool: &SqlitePool, event: &Event) -> Result<()> {
    // Only store events that should update real-time status
    if !event.should_update_real_time_status() {
        return Ok(());
    }

    // eng-review T2/OV-2: status rows carry is_status=1 and are the ONLY
    // rows covered by the dedup index. Repeat occurrences refresh level,
    // accumulate occurrence_count, and reset acknowledgment (a NEW
    // occurrence of a previously-acked event is actionable again).
    let sql = r#"
        INSERT INTO events (
            id, event_type, event_subtype, event_level, timestamp,
            source_type, source_id, thing_id, user_id,
            title, content, occurrence_count, acknowledged,
            acknowledged_by, acknowledged_at, workspace_id, is_status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, NULL, NULL, ?, 1)
        ON CONFLICT(event_type, event_subtype, thing_id) WHERE is_status = 1 AND thing_id IS NOT NULL
        DO UPDATE SET
            occurrence_count = occurrence_count + 1,
            event_level = excluded.event_level,
            timestamp = excluded.timestamp,
            source_id = excluded.source_id,
            title = excluded.title,
            content = excluded.content,
            acknowledged = 0,
            acknowledged_by = NULL,
            acknowledged_at = NULL
    "#;

    let content_json = serde_json::to_string(event.content())?;
    let event_subtype_json = serde_json::to_string(event.event_type())?;
    let device_id_bind: Option<String> = event.source().device_id().map(|s| s.to_string());
    let user_id_bind: Option<String> = event.source().user_id().map(|s| s.to_string());

    // Resolve tenant scope from the owning device (was hardcoded '')
    let workspace_id: String = match &device_id_bind {
        Some(did) => sqlx::query_scalar("SELECT COALESCE(workspace_id, '') FROM things WHERE id = ?")
            .bind(did)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default(),
        None => String::new(),
    };

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
        .bind(&workspace_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn remove_event_status(pool: &SqlitePool, source: &EventSource, event_type: &EventType) -> Result<()> {
    let event_subtype_json = serde_json::to_string(event_type)?;

    // Align with dedup index columns: (event_type, event_subtype, thing_id)
    let sql = r#"
        DELETE FROM events
        WHERE event_type = ? AND event_subtype = ?
          AND thing_id = ?
    "#;

    sqlx::query(sql)
        .bind(event_type.type_string())
        .bind(&event_subtype_json)
        .bind(source.device_id().map(|s| s.to_string()))
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn find_realtime_events(pool: &SqlitePool, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>> {
    let rows = query_realtime_event_rows(pool, filter).await?;
    let mut events = Vec::new();
    for row in &rows {
        events.push(row_to_real_time_event(row)?);
    }
    Ok(events)
}

pub(crate) async fn get_realtime_status_summary(pool: &SqlitePool, _filter: &RealTimeFilter) -> Result<StatusSummary> {
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

    let rows = sqlx::query(sql).fetch_all(pool).await?;

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
            thing_id,
            COUNT(*) as active_count,
            MAX(event_level) as highest_level,
            MAX(timestamp) as latest_timestamp
        FROM events
        WHERE thing_id IS NOT NULL AND occurrence_count >= 1 AND event_level >= 3
        GROUP BY thing_id
    "#;

    let device_rows = sqlx::query(device_sql).fetch_all(pool).await?;

    let mut by_device = Vec::new();
    for row in device_rows {
        let thing_id: String = row.get("thing_id");
        let active_count: i64 = row.get("active_count");
        let highest_level_int: i32 = row.get("highest_level");
        let latest_timestamp_str: String = row.get("latest_timestamp");

        let highest_level = EventLevel::from_numeric(highest_level_int).unwrap_or(EventLevel::Info);
        let latest_timestamp = DateTime::parse_from_rfc3339(&latest_timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        by_device.push(DeviceStatusSummary {
            thing_id,
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

pub(crate) async fn acknowledge_event(
    pool: &SqlitePool,
    id: &EventId,
    user_id: &str,
    workspace_id: &str,
) -> Result<()> {
    // Tenant isolation (eng-review T1): only ack events in the caller's workspace
    let sql = r#"
        UPDATE events
        SET acknowledged = 1, acknowledged_by = ?, acknowledged_at = ?
        WHERE id = ? AND workspace_id = ?
    "#;

    let result = sqlx::query(sql)
        .bind(user_id)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(workspace_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        // Event not found in this workspace (missing or cross-tenant) —
        // surface it instead of silently pretending the ack worked
        return Err(crate::DbError::NotFound {
            id: format!("{} (in workspace)", id),
        });
    }
    Ok(())
}

pub(crate) async fn clear_acknowledged_events(pool: &SqlitePool) -> Result<u64> {
    // Occurrence rows only — an acknowledged STATUS row is the live
    // current-state of a device and must never be bulk-deleted (X1/OV-1)
    let sql = "DELETE FROM events WHERE acknowledged = 1 AND is_status = 0";

    let result = sqlx::query(sql).execute(pool).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn cleanup_old_realtime_events(pool: &SqlitePool, before: DateTime<Utc>) -> Result<u64> {
    // Occurrence rows only — status rows (is_status=1) are live state,
    // not log history, and are exempt from time-based purge (X1/OV-1)
    let sql = "DELETE FROM events WHERE timestamp < ? AND is_status = 0";

    let result = sqlx::query(sql).bind(before.to_rfc3339()).execute(pool).await?;

    Ok(result.rows_affected())
}

/// Build and execute the active events query with dynamic filters.
/// Uses string interpolation with SQL-escaped values
/// (safe because all values come from internal domain logic, not raw user
/// input).
async fn query_realtime_event_rows(pool: &SqlitePool, filter: &RealTimeFilter) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
    let mut conditions: Vec<String> = Vec::new();

    // -- device_ids filter --
    if let Some(ref device_ids) = filter.device_ids
        && !device_ids.is_empty()
    {
        let quoted: Vec<String> = device_ids
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect();
        conditions.push(format!("thing_id IN ({})", quoted.join(",")));
    }

    // -- workspace filter (eng-review T1: tenant isolation) --
    if let Some(ref workspace_id) = filter.workspace_id {
        conditions.push(format!("workspace_id = '{}'", workspace_id.replace('\'', "''")));
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
        && !event_types.is_empty()
    {
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
        && !source_types.is_empty()
    {
        let quoted: Vec<String> = source_types
            .iter()
            .map(|st| format!("'{}'", st.replace('\'', "''")))
            .collect();
        conditions.push(format!("source_type IN ({})", quoted.join(",")));
    }

    let mut sql = String::from(
        r#"SELECT id, event_type, event_subtype, event_level, timestamp,
                  source_type, source_id, thing_id, user_id,
                  title, content, occurrence_count,
                  acknowledged, acknowledged_by, acknowledged_at
           FROM events"#,
    );

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY timestamp DESC");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
    Ok(rows)
}

fn row_to_real_time_event(row: &sqlx::sqlite::SqliteRow) -> Result<RealTimeEvent> {
    let id_str: String = row.get("id");
    let _event_type_str: String = row.get("event_type");
    let event_subtype_str: String = row.get("event_subtype");
    let event_level_int: i32 = row.get("event_level");
    let timestamp_str: String = row.get("timestamp");
    let title: String = row.get("title");
    let source_type: String = row.get("source_type");
    let source_id: String = row.get("source_id");
    let thing_id: Option<String> = row.get("thing_id");
    let user_id: Option<String> = row.get("user_id");
    let acknowledged: bool = row.get("acknowledged");
    let acknowledged_by: Option<String> = row.get("acknowledged_by");
    let acknowledged_at_str: Option<String> = row.get("acknowledged_at");

    let id = EventId::from_string(id_str);
    let event_type: EventType =
        serde_json::from_str(&event_subtype_str).map_err(|e| crate::DbError::Validation { message: e.to_string() })?;
    let level = EventLevel::from_numeric(event_level_int).unwrap_or(EventLevel::Info);
    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let source = EventSource::new(source_type, source_id, thing_id, user_id);

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

// ──────────────────────────────────────────────
// 收编的外部裸 SQL（runtime_ports EventRetentionAdapter / thing_agent_host）
// ──────────────────────────────────────────────

/// thing 事件游标回放的原始行（cloud thing_agent_host 映射为 ThingEventSignal）。
#[derive(Debug, Clone)]
pub struct ThingEventReplayRow {
    pub rid: i64,
    pub workspace_id: Option<String>,
    pub thing_id: Option<String>,
    pub event_subtype: String,
    pub event_level: i32,
    pub content: String,
    pub metadata: Option<String>,
    pub actor: String,
}

/// 事件保留：删除 cutoff 之前的 occurrence 行（status 行豁免，X1/OV-1）。
/// SQL 与原 runtime_ports EventRetentionAdapter 内联语句逐字一致。
pub(crate) async fn delete_occurrence_events_before(pool: &SqlitePool, cutoff_rfc3339: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM events WHERE is_status = 0 AND timestamp < ?")
        .bind(cutoff_rfc3339)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// thing 来源事件的游标回放（rowid 单调递增，O27）。SQL 与原
/// thing_agent_host::replay_events_since 内联语句逐字一致。
pub(crate) async fn fetch_thing_events_since(
    pool: &SqlitePool,
    cursor: i64,
    min_level: i32,
) -> Result<Vec<ThingEventReplayRow>> {
    let rows = sqlx::query(
        "SELECT rowid AS rid, workspace_id, thing_id, event_subtype, event_level, content, metadata, actor \
         FROM events \
         WHERE rowid > ? AND event_level >= ? AND source_type = 'thing' \
         ORDER BY rowid ASC",
    )
    .bind(cursor)
    .bind(min_level)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| ThingEventReplayRow {
            rid: row.get("rid"),
            workspace_id: row.get("workspace_id"),
            thing_id: row.get("thing_id"),
            event_subtype: row.get("event_subtype"),
            event_level: row.get("event_level"),
            content: row.get("content"),
            metadata: row.get("metadata"),
            actor: row.get("actor"),
        })
        .collect())
}

/// agent 来源告警行（event_subtype = 'thing_agent_alert'，Error 级）。SQL 与原
/// thing_agent_host::notify_alert 内联语句逐字一致。
pub(crate) async fn insert_agent_alert_event(
    pool: &SqlitePool,
    workspace_id: &str,
    title: &str,
    content: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, source_id, thing_id, user_id, title, content, metadata, created_at, workspace_id, actor) \
         VALUES (?, 'agent', 'thing_agent_alert', 4, ?, 'agent', 'thing-agent', NULL, NULL, ?, ?, '{}', ?, ?, 'agent')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&now)
    .bind(title)
    .bind(content)
    .bind(&now)
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// thing 事件查询行（agent query_events 工具用，自 cloud agent tools/thing 迁入）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ThingEventQueryRow {
    pub id: String,
    pub event_type: String,
    pub event_subtype: Option<String>,
    pub event_level: i32,
    pub timestamp: Option<String>,
    pub source_type: String,
    pub source_id: String,
    pub title: Option<String>,
    pub content: String,
    pub created_at: String,
}

/// thing 事件过滤查询（device + workspace 作用域，可选 event_type/level/since）。
/// SQL 与原 query_events 工具内联 QueryBuilder 逐字一致。
pub(crate) async fn search_thing_events(
    pool: &SqlitePool,
    thing_id: &str,
    workspace_id: &str,
    event_name: Option<&str>,
    level: Option<i32>,
    since: Option<&str>,
    limit: i64,
) -> Result<Vec<ThingEventQueryRow>> {
    // Build dynamic query with QueryBuilder
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, event_type, event_subtype, event_level, timestamp, \
         source_type, source_id, title, content, created_at \
         FROM events WHERE thing_id = ",
    );
    builder.push_bind(thing_id);
    builder.push(" AND workspace_id = ");
    builder.push_bind(workspace_id);

    if let Some(event_name) = event_name {
        builder.push(" AND event_type = ");
        builder.push_bind(event_name);
    }
    if let Some(level) = level {
        builder.push(" AND event_level = ");
        builder.push_bind(level);
    }
    if let Some(since) = since {
        builder.push(" AND created_at >= ");
        builder.push_bind(since);
    }

    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(limit);

    let rows = builder.build_query_as::<ThingEventQueryRow>().fetch_all(pool).await?;
    Ok(rows)
}

// ──────────────────────────────────────────────
// Db 委托（event 前缀 vs realtime 前缀，对应两个原 repo）
// ──────────────────────────────────────────────

impl Db {
    /// 插入事件。
    pub async fn insert_event(&self, event: &Event) -> Result<()> {
        insert_event(self.pool(), event).await
    }

    /// 按 ID 查询事件。
    pub async fn find_event_by_id(&self, id: &EventId) -> Result<Option<Event>> {
        find_event_by_id(self.pool(), id).await
    }

    /// 按条件查询事件。
    pub async fn query_events(&self, criteria: &EventCriteria) -> Result<Vec<Event>> {
        query_events(self.pool(), criteria).await
    }

    /// 按级别计数。
    pub async fn count_events_by_level(&self, level: EventLevel) -> Result<u64> {
        count_events_by_level(self.pool(), level).await
    }

    /// 按类型计数。
    pub async fn count_events_by_type(&self, event_type: &EventType) -> Result<u64> {
        count_events_by_type(self.pool(), event_type).await
    }

    /// 事务批量插入。
    pub async fn insert_events_batch(&self, events: &[Event]) -> Result<()> {
        insert_events_batch(self.pool(), events).await
    }

    /// 事件统计。
    pub async fn get_event_statistics(&self, params: &StatisticsParams) -> Result<EventStatistics> {
        get_event_statistics(self.pool(), params).await
    }

    /// 删除指定时间之前的事件。
    pub async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64> {
        cleanup_old_events(self.pool(), before).await
    }

    /// 导出事件。
    pub async fn export_events(&self, criteria: &EventCriteria, format: ExportFormat) -> Result<Vec<u8>> {
        export_events(self.pool(), criteria, format).await
    }

    /// 事件总数。
    pub async fn count_events(&self) -> Result<u64> {
        count_events(self.pool()).await
    }

    /// upsert 实时状态行。
    pub async fn upsert_event_status(&self, event: &Event) -> Result<()> {
        upsert_event_status(self.pool(), event).await
    }

    /// 删除实时状态行。
    pub async fn remove_event_status(&self, source: &EventSource, event_type: &EventType) -> Result<()> {
        remove_event_status(self.pool(), source, event_type).await
    }

    /// 查询实时事件。
    pub async fn find_realtime_events(&self, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>> {
        find_realtime_events(self.pool(), filter).await
    }

    /// 实时状态汇总。
    pub async fn get_realtime_status_summary(&self, filter: &RealTimeFilter) -> Result<StatusSummary> {
        get_realtime_status_summary(self.pool(), filter).await
    }

    /// 确认事件。
    pub async fn acknowledge_event(&self, id: &EventId, user_id: &str, workspace_id: &str) -> Result<()> {
        acknowledge_event(self.pool(), id, user_id, workspace_id).await
    }

    /// 清除已确认 occurrence 行。
    pub async fn clear_acknowledged_events(&self) -> Result<u64> {
        clear_acknowledged_events(self.pool()).await
    }

    /// 按时间清除 occurrence 行。
    pub async fn cleanup_old_realtime_events(&self, before: DateTime<Utc>) -> Result<u64> {
        cleanup_old_realtime_events(self.pool(), before).await
    }

    /// 事件保留：删除 cutoff 之前的 occurrence 行（收编自 runtime_ports 适配器）。
    pub async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64> {
        delete_occurrence_events_before(self.pool(), cutoff_rfc3339).await
    }

    /// thing 事件游标回放（收编自 thing_agent_host）。
    pub async fn replay_thing_events_since(&self, cursor: i64, min_level: i32) -> Result<Vec<ThingEventReplayRow>> {
        fetch_thing_events_since(self.pool(), cursor, min_level).await
    }

    /// 写入 agent 告警事件行（收编自 thing_agent_host）。
    pub async fn insert_agent_alert_event(&self, workspace_id: &str, title: &str, content: &str) -> Result<()> {
        insert_agent_alert_event(self.pool(), workspace_id, title, content).await
    }

    /// thing 事件过滤查询（query_events 工具用，收编自 cloud agent tools/thing）。
    pub async fn search_thing_events(
        &self,
        thing_id: &str,
        workspace_id: &str,
        event_name: Option<&str>,
        level: Option<i32>,
        since: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ThingEventQueryRow>> {
        search_thing_events(self.pool(), thing_id, workspace_id, event_name, level, since, limit).await
    }
}

// ──────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::event::SystemEventType;

    #[test]
    fn test_criteria_builder() {
        let now = Utc::now();
        let criteria = EventCriteria::builder()
            .start_time(now)
            .event_types(vec![EventType::System(SystemEventType::UserAuth)])
            .levels(vec![EventLevel::Error, EventLevel::Critical])
            .device_ids(vec!["device1".to_string(), "device2".to_string()])
            .sort_by(SortBy::Level)
            .sort_order(SortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.start_time, Some(now));
        assert_eq!(criteria.event_types.as_ref().unwrap().len(), 1);
        assert_eq!(criteria.levels.as_ref().unwrap().len(), 2);
        assert_eq!(criteria.device_ids.as_ref().unwrap().len(), 2);
        assert!(matches!(criteria.sort_by, SortBy::Level));
        assert!(matches!(criteria.sort_order, SortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let now = Utc::now();
        let later = now + chrono::Duration::hours(1);

        let criteria = EventCriteria::default()
            .with_time_range(now, later)
            .with_levels(vec![EventLevel::Critical])
            .with_sort(SortBy::Timestamp, SortOrder::Descending)
            .with_pagination(50, 0);

        assert_eq!(criteria.start_time, Some(now));
        assert_eq!(criteria.end_time, Some(later));
        assert_eq!(criteria.levels.as_ref().unwrap().len(), 1);
        assert_eq!(criteria.limit, Some(50));
    }

    #[test]
    fn test_real_time_filter_builder() {
        let filter = RealTimeFilter::builder()
            .device_ids(vec!["device1".to_string(), "device2".to_string()])
            .event_types(vec![EventType::System(SystemEventType::UserAuth)])
            .acknowledged(false)
            .min_level(EventLevel::Error)
            .build();

        assert_eq!(filter.device_ids.as_ref().unwrap().len(), 2);
        assert_eq!(filter.event_types.as_ref().unwrap().len(), 1);
        assert_eq!(filter.acknowledged, Some(false));
        assert_eq!(filter.min_level, Some(EventLevel::Error));
    }

    #[test]
    fn test_real_time_filter_convenience_methods() {
        let unack_filter = RealTimeFilter::unacknowledged();
        assert_eq!(unack_filter.acknowledged, Some(false));

        let critical_filter = RealTimeFilter::critical_and_errors();
        assert_eq!(critical_filter.min_level, Some(EventLevel::Error));
    }

    #[test]
    fn test_health_status() {
        let mut summary = StatusSummary {
            total_active: 10,
            critical_count: 0,
            error_count: 0,
            warning_count: 5,
            unacknowledged_count: 3,
            by_device: vec![],
            by_type: vec![],
        };

        assert_eq!(summary.health_status(), HealthStatus::Warning);
        assert!(!summary.has_critical_issues());
        assert!(summary.has_unacknowledged());

        summary.critical_count = 1;
        assert_eq!(summary.health_status(), HealthStatus::Critical);
        assert!(summary.has_critical_issues());
    }
}

// ──────────────────────────────────────────────
// Open API 投影查询（自 cloud admin/open 迁入，Task 12）
// ──────────────────────────────────────────────

/// Open API thing 事件行。
#[derive(Debug)]
pub struct OpenEventRow {
    pub id: String,
    pub event_type: String,
    pub event_level: i64,
    pub title: Option<String>,
    pub created_at: String,
}

/// Open API 全量事件行（含 thing_id）。
#[derive(Debug)]
pub struct OpenEventWithDeviceRow {
    pub id: String,
    pub event_type: String,
    pub event_level: i64,
    pub title: Option<String>,
    pub thing_id: Option<String>,
    pub created_at: String,
}

/// Open API：列出 thing 的事件（最新 100 条，workspace 作用域）。
pub(crate) async fn list_open_thing_events(
    pool: &SqlitePool,
    thing_id: &str,
    workspace_id: &str,
) -> Result<Vec<OpenEventRow>> {
    let rows = sqlx::query(
        "SELECT id, event_type, event_level, title, created_at FROM events WHERE thing_id = ? AND workspace_id = ? ORDER BY created_at DESC LIMIT 100"
    )
    .bind(thing_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenEventRow {
            id: row.try_get::<String, _>("id").unwrap_or_default(),
            event_type: row.try_get::<String, _>("event_type").unwrap_or_default(),
            event_level: row.try_get::<i64, _>("event_level").unwrap_or(0),
            title: row.try_get::<Option<String>, _>("title").unwrap_or_default(),
            created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect())
}

/// Open API：列出 workspace 全部事件（最新 100 条）。
pub(crate) async fn list_open_events(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<OpenEventWithDeviceRow>> {
    let rows = sqlx::query(
        "SELECT id, event_type, event_level, title, thing_id, created_at FROM events WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenEventWithDeviceRow {
            id: row.try_get::<String, _>("id").unwrap_or_default(),
            event_type: row.try_get::<String, _>("event_type").unwrap_or_default(),
            event_level: row.try_get::<i64, _>("event_level").unwrap_or(0),
            title: row.try_get::<Option<String>, _>("title").unwrap_or_default(),
            thing_id: row.try_get::<Option<String>, _>("thing_id").unwrap_or_default(),
            created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect())
}

impl Db {
    /// Open API：列出 thing 的事件（最新 100 条，workspace 作用域）。
    pub async fn list_open_thing_events(&self, thing_id: &str, workspace_id: &str) -> Result<Vec<OpenEventRow>> {
        list_open_thing_events(self.pool(), thing_id, workspace_id).await
    }

    /// Open API：列出 workspace 全部事件（最新 100 条）。
    pub async fn list_open_events(&self, workspace_id: &str) -> Result<Vec<OpenEventWithDeviceRow>> {
        list_open_events(self.pool(), workspace_id).await
    }
}

// ──────────────────────────────────────────────
// Thing 事件入库（自 cloud event/router.rs 迁入，Task 12）
// ──────────────────────────────────────────────

/// Thing 事件入库参数。
pub struct ThingEventInsert<'a> {
    pub event_id: &'a str,
    pub event_subtype: &'a str,
    pub level_num: i32,
    pub timestamp: &'a str,
    pub source_type: &'a str,
    pub source_id: &'a str,
    pub thing_id: Option<&'a str>,
    pub user_id: Option<&'a str>,
    pub title: &'a str,
    pub content: &'a str,
    pub metadata: &'a str,
    pub created_at: &'a str,
    pub workspace_id: &'a str,
    pub actor: &'a str,
}

/// 持久化 thing 事件，返回 last_insert_rowid。
pub(crate) async fn insert_thing_event(pool: &SqlitePool, input: &ThingEventInsert<'_>) -> Result<i64> {
    let res = sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, source_id, thing_id, user_id, title, content, metadata, created_at, workspace_id, actor) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(input.event_id)
    .bind("device")
    .bind(input.event_subtype)
    .bind(input.level_num)
    .bind(input.timestamp)
    .bind(input.source_type)
    .bind(input.source_id)
    .bind(input.thing_id)
    .bind(input.user_id)
    .bind(input.title)
    .bind(input.content)
    .bind(input.metadata)
    .bind(input.created_at)
    .bind(input.workspace_id)
    .bind(input.actor)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

impl Db {
    /// 持久化 thing 事件，返回 last_insert_rowid。
    pub async fn insert_thing_event(&self, input: &ThingEventInsert<'_>) -> Result<i64> {
        insert_thing_event(self.pool(), input).await
    }
}
