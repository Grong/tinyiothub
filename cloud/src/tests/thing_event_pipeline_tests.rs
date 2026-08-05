//! Thing event pipeline integration tests (design doc section 八, suite 1)
//!
//! Covers `route_thing_event` end-to-end against a real migrated SQLite DB:
//! persistence shape, unknown-event degradation, event alarm rule firing
//! (R2 acceptance path), throttling, append dedup semantics, and the
//! real-time status upsert path.

use std::sync::Arc;

use sqlx::Row;
use tinyiothub_core::models::event::{
    ContentElement, DeviceEventType, Event, EventLevel, EventSource, EventType, RichContent,
    TextFormat,
};
use tinyiothub_storage::Database;

use crate::{
    modules::{
        alarm::{
            AlarmRepository, AlarmRuleRepository, AlarmService, SqliteAlarmRepository,
            SqliteAlarmRuleRepository,
        },
    },
    test_utils::seed_test_workspace,
};
use tinyiothub_event::{
    bus::ThingEventBus,
    repositories::RealTimeEventRepository,
    router::{ThingEventInput, ThrottleState, route_thing_event},
    sqlite_real_time_event::SqliteRealTimeEventRepository,
};

/// Migrated single-connection in-memory pool (single connection so PRAGMAs
/// and in-memory state are consistent).
async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool).await.expect("migrations");
    pool
}

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, workspace_id: &str) {
    // devices.workspace_id has a FK to workspaces — seed it (idempotent).
    seed_test_workspace(pool, "tenant-1", workspace_id).await;
    sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES (?, ?, ?)")
        .bind(id)
        .bind(format!("Device {id}"))
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("insert device");
}

/// Insert a template whose `events` JSON defines the given event names, and a
/// device created from it.
async fn insert_device_with_template(
    pool: &sqlx::SqlitePool,
    device_id: &str,
    workspace_id: &str,
    template_id: &str,
    events_json: &str,
) {
    seed_test_workspace(pool, "tenant-1", workspace_id).await;
    sqlx::query(
        "INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES ('test-cat', '{}', datetime('now'))",
    )
    .execute(pool)
    .await
    .expect("insert template category");

    sqlx::query(
        "INSERT INTO thing_templates (id, name, display_name, version, category, device_type, events, created_at, updated_at)
         VALUES (?, ?, ?, '1.0', 'test-cat', 'sensor', ?, datetime('now'), datetime('now'))",
    )
    .bind(template_id)
    .bind(format!("tpl-{template_id}"))
    .bind(format!("Template {template_id}"))
    .bind(events_json)
    .execute(pool)
    .await
    .expect("insert template");

    sqlx::query("INSERT INTO devices (id, name, workspace_id, template_id) VALUES (?, ?, ?, ?)")
        .bind(device_id)
        .bind(format!("Device {device_id}"))
        .bind(workspace_id)
        .bind(template_id)
        .execute(pool)
        .await
        .expect("insert device with template");
}

fn input(
    thing_id: &str,
    workspace_id: &str,
    event_name: &str,
    level: EventLevel,
) -> ThingEventInput {
    ThingEventInput {
        thing_id: thing_id.to_string(),
        workspace_id: workspace_id.to_string(),
        event_name: event_name.to_string(),
        level,
        data: serde_json::json!({"value": 42}),
        ts: None,
        template_events: None,
    }
}

// ──────────────────────────────────────────────
// Persistence shape
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_route_event_persists_row_with_expected_shape() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-1", "ws-1").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-1", "ws-1", "temp_high", EventLevel::Warning),
    )
    .await;

    assert!(!result.throttled && !result.unknown_event && !result.malformed);
    assert!(!result.event_id.is_empty(), "persisted event must return its id");

    let row = sqlx::query(
        "SELECT event_subtype, event_level, workspace_id, content, metadata, is_status
         FROM events WHERE id = ?",
    )
    .bind(&result.event_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let subtype: String = row.get("event_subtype");
    assert_eq!(subtype, "temp_high", "event_subtype must be the raw event name, not enum JSON");
    assert_eq!(row.get::<i32, _>("event_level"), EventLevel::Warning.to_numeric());
    assert_eq!(row.get::<String, _>("workspace_id"), "ws-1");

    let content: String = row.get("content");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&content).unwrap(),
        serde_json::json!({"value": 42}),
        "content must be the raw data JSON"
    );

    let metadata: String = row.get("metadata");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata).unwrap()["unknown_event"],
        false
    );
    assert_eq!(row.get::<i32, _>("is_status"), 0, "thing events are append-type rows");
}

// ──────────────────────────────────────────────
// Unknown event name handling
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_unknown_event_name_degrades_to_info_flagged() {
    let pool = test_pool().await;
    insert_device_with_template(&pool, "dev-t", "ws-1", "tpl-1", r#"[{"name":"known_event"}]"#)
        .await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-t", "ws-1", "mystery_event", EventLevel::Error),
    )
    .await;

    assert!(result.unknown_event, "event not in the template's events list must be flagged");
    assert!(!result.malformed, "unknown event is degraded, never an error to the device");

    let row = sqlx::query("SELECT event_level, metadata FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        row.get::<i32, _>("event_level"),
        EventLevel::Info.to_numeric(),
        "unknown events degrade to info level"
    );
    let metadata: String = row.get("metadata");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata).unwrap()["unknown_event"],
        true
    );
}

#[tokio::test]
async fn test_known_template_event_not_flagged() {
    let pool = test_pool().await;
    insert_device_with_template(&pool, "dev-k", "ws-1", "tpl-k", r#"[{"name":"known_event"}]"#)
        .await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-k", "ws-1", "known_event", EventLevel::Error),
    )
    .await;

    assert!(!result.unknown_event);
    let level: i32 = sqlx::query_scalar("SELECT event_level FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(level, EventLevel::Error.to_numeric(), "known events keep their level");
}

#[tokio::test]
async fn test_device_without_template_not_flagged() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-nt", "ws-1").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-nt", "ws-1", "any_name_at_all", EventLevel::Error),
    )
    .await;

    assert!(!result.unknown_event, "device without a template accepts all names unflagged");
    let level: i32 = sqlx::query_scalar("SELECT event_level FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(level, EventLevel::Error.to_numeric());
}

// ──────────────────────────────────────────────
// Alarm firing (R2 acceptance path — real DB, real AlarmService)
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_event_alarm_rule_fires_device_alarm() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-al", "ws-al").await;

    sqlx::query(
        "INSERT INTO device_alarm_rules (id, device_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, workspace_id)
         VALUES ('rule-ev', 'dev-al', 'Temp High', 'event', '{\"eventName\":\"temp_high\",\"minLevel\":\"warning\"}', 'warning', 1, 'ws-al')",
    )
    .execute(&pool)
    .await
    .expect("insert event rule");

    let db = Arc::new(Database::new(pool.clone()));
    let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
    let rule_repo: Arc<dyn AlarmRuleRepository> =
        Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
    let alarm_service = Arc::new(AlarmService::new(alarm_repo, rule_repo));

    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();
    let result = route_thing_event(
        &pool,
        &throttle,
        Some(alarm_service.clone()),
        &bus,
        "device",
        input("dev-al", "ws-al", "temp_high", EventLevel::Warning),
    )
    .await;
    assert!(!result.malformed);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-al' AND rule_id = 'rule-ev'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "matching event rule must create a device_alarms row");

    // A different event name must NOT fire the rule.
    let throttle2 = ThrottleState::new(60);
    route_thing_event(
        &pool,
        &throttle2,
        Some(alarm_service),
        &bus,
        "device",
        input("dev-al", "ws-al", "other_event", EventLevel::Critical),
    )
    .await;
    let count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-al' AND rule_id = 'rule-ev'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count_after, 1, "non-matching event name must not fire the rule");
}

#[tokio::test]
async fn test_event_alarm_rule_respects_min_level() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-ml", "ws-ml").await;

    sqlx::query(
        "INSERT INTO device_alarm_rules (id, device_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, workspace_id)
         VALUES ('rule-ml', 'dev-ml', 'Temp High', 'event', '{\"eventName\":\"temp_high\",\"minLevel\":\"error\"}', 'error', 1, 'ws-ml')",
    )
    .execute(&pool)
    .await
    .expect("insert event rule");

    let db = Arc::new(Database::new(pool.clone()));
    let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
    let rule_repo: Arc<dyn AlarmRuleRepository> =
        Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
    let alarm_service = Arc::new(AlarmService::new(alarm_repo, rule_repo));

    // Warning < min_level error → no alarm.
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();
    route_thing_event(
        &pool,
        &throttle,
        Some(alarm_service),
        &bus,
        "device",
        input("dev-ml", "ws-ml", "temp_high", EventLevel::Warning),
    )
    .await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-ml'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "event below min_level must not fire the rule");
}

// ──────────────────────────────────────────────
// Throttle integration
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_throttle_admits_60_rejects_61st_but_spares_critical() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-th", "ws-1").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    for i in 0..60 {
        let r = route_thing_event(
            &pool,
            &throttle,
            None,
            &bus,
            "device",
            input("dev-th", "ws-1", "ping", EventLevel::Info),
        )
        .await;
        assert!(!r.throttled, "event {} must be admitted", i + 1);
        assert!(!r.malformed, "event {} must persist", i + 1);
    }

    let r61 = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-th", "ws-1", "ping", EventLevel::Info),
    )
    .await;
    assert!(r61.throttled, "61st info event within the window must be throttled");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE device_id = 'dev-th'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 60, "throttled event must not be persisted");

    let rc = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-th", "ws-1", "meltdown", EventLevel::Critical),
    )
    .await;
    assert!(!rc.throttled, "critical events are exempt from throttling");
    assert!(!rc.malformed);

    let count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE device_id = 'dev-th'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_after, 61, "critical event during the storm must be admitted");
}

// ──────────────────────────────────────────────
// Append dedup semantics (occurrence events are outside the dedup index)
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_append_events_same_subtype_both_insert() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-dd", "ws-1").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    for _ in 0..2 {
        let r = route_thing_event(
            &pool,
            &throttle,
            None,
            &bus,
            "device",
            input("dev-dd", "ws-1", "door_open", EventLevel::Info),
        )
        .await;
        assert!(!r.malformed, "append event must not hit a UNIQUE violation");
        assert!(!r.event_id.is_empty());
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE device_id = 'dev-dd' AND event_subtype = 'door_open'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2, "dedup index only covers is_status=1 rows; appends are allowed");
}

// ──────────────────────────────────────────────
// Real-time status upsert
// ──────────────────────────────────────────────

/// Build a status-type event (Device + Warning/Error/Critical satisfies
/// `should_update_real_time_status()`).
fn status_event(device_id: &str, level: EventLevel) -> Event {
    let content = RichContent::new(
        format!("status from {device_id}"),
        vec![ContentElement::Text { content: "status".to_string(), format: TextFormat::Plain }],
    );
    Event::new(
        EventType::Device(DeviceEventType::PropertyChange),
        level,
        EventSource::device_property(
            device_id.to_string(),
            "temperature".to_string(),
            "test".to_string(),
        ),
        content,
    )
    .expect("valid status event")
}

/// Upsert via the repository (predicate fixed to match idx_events_status_dedup):
/// two status upserts for the same key merge into one row with
/// occurrence_count=2, acknowledgment reset, level refreshed.
#[tokio::test]
async fn test_status_upsert_via_repo_merges_repeat_occurrences() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-st", "ws-st").await;
    let repo = SqliteRealTimeEventRepository::new(Database::new(pool.clone()));

    repo.upsert_status(&status_event("dev-st", EventLevel::Warning)).await.unwrap();
    // acknowledge the row, then a second (escalated) occurrence arrives
    sqlx::query("UPDATE events SET acknowledged = 1, acknowledged_by = 'u1', acknowledged_at = '2026-01-01' WHERE device_id = 'dev-st'")
        .execute(&pool)
        .await
        .unwrap();
    repo.upsert_status(&status_event("dev-st", EventLevel::Critical)).await.unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE device_id = 'dev-st'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "repeat status occurrence must merge into one row");

    let (occ, level, ack): (i64, i64, i64) = sqlx::query_as(
        "SELECT occurrence_count, event_level, acknowledged FROM events WHERE device_id = 'dev-st'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(occ, 2, "occurrence_count accumulates");
    assert_eq!(level, EventLevel::Critical.to_numeric() as i64, "level refreshed to latest");
    assert_eq!(ack, 0, "acknowledgment resets on new occurrence");
}

/// The mandated upsert semantics (design 八·1): one row per
/// (event_type, event_subtype, device_id) status key, occurrence_count
/// accumulates, acknowledgment resets, level refreshes. Verified against the
/// migrated schema using the conflict target the index actually supports
/// (`is_status = 1 AND device_id IS NOT NULL`) — proving the bug is confined
/// to the repository's ON CONFLICT predicate, not the schema.
#[tokio::test]
async fn test_status_upsert_merges_repeat_occurrences() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-st", "ws-st").await;

    // Same upsert shape as SqliteRealTimeEventRepository::upsert_status, with
    // the conflict-target predicate matching idx_events_status_dedup.
    let upsert = r#"
        INSERT INTO events (
            id, event_type, event_subtype, event_level, timestamp,
            source_type, source_id, device_id, title, content,
            occurrence_count, acknowledged, workspace_id, is_status
        ) VALUES (?, 'device', ?, ?, ?, 'device_property', ?, 'dev-st', 't', '{}', 1, 0, 'ws-st', 1)
        ON CONFLICT(event_type, event_subtype, device_id) WHERE is_status = 1 AND device_id IS NOT NULL
        DO UPDATE SET
            occurrence_count = occurrence_count + 1,
            event_level = excluded.event_level,
            timestamp = excluded.timestamp,
            acknowledged = 0,
            acknowledged_by = NULL,
            acknowledged_at = NULL
    "#;
    let subtype =
        serde_json::to_string(&EventType::Device(DeviceEventType::PropertyChange)).unwrap();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(upsert)
        .bind("evt-1")
        .bind(&subtype)
        .bind(EventLevel::Warning.to_numeric())
        .bind(&now)
        .bind("dev-st:temperature")
        .execute(&pool)
        .await
        .unwrap();

    // Simulate a human acknowledging the status row, then a repeat occurrence
    // at a higher level arrives.
    sqlx::query(
        "UPDATE events SET acknowledged = 1, acknowledged_by = 'user-1' WHERE device_id = 'dev-st'",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(upsert)
        .bind("evt-2")
        .bind(&subtype)
        .bind(EventLevel::Error.to_numeric())
        .bind(&now)
        .bind("dev-st:temperature")
        .execute(&pool)
        .await
        .unwrap();

    let rows = sqlx::query(
        "SELECT occurrence_count, acknowledged, acknowledged_by, event_level, is_status, workspace_id
         FROM events WHERE device_id = 'dev-st'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "repeat status event must upsert, not append");
    let row = &rows[0];
    assert_eq!(row.get::<i64, _>("occurrence_count"), 2);
    assert!(!row.get::<bool, _>("acknowledged"), "new occurrence resets acknowledgment");
    assert!(row.get::<Option<String>, _>("acknowledged_by").is_none());
    assert_eq!(
        row.get::<i32, _>("event_level"),
        EventLevel::Error.to_numeric(),
        "level is refreshed to the latest occurrence"
    );
    assert_eq!(row.get::<i32, _>("is_status"), 1);
    assert_eq!(row.get::<String, _>("workspace_id"), "ws-st");
}

#[tokio::test]
async fn test_status_upsert_ignores_info_level_events() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-si", "ws-si").await;
    let repo = SqliteRealTimeEventRepository::new(Database::new(pool.clone()));

    // Info-level device events do not satisfy should_update_real_time_status().
    repo.upsert_status(&status_event("dev-si", EventLevel::Info)).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE device_id = 'dev-si'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "info-level events must not create status rows");
}
