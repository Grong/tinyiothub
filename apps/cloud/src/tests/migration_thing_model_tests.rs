//! Thing model migration integration tests (design doc section 八, suite 2)
//!
//! Runs the full migration chain against a fresh in-memory DB and asserts on
//! the resulting schema: thing_properties full shape, no synthetic seed rows,
//! deprecated tables dropped, resources/events/workspaces column changes, and
//! alarm FK repointing to thing_properties.

use sqlx::Row;

use crate::test_utils::seed_test_workspace;

/// Fresh in-memory DB with the full migration chain applied.
async fn migrated_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("migrations");
    pool
}

async fn column_names(pool: &sqlx::SqlitePool, table: &str) -> Vec<String> {
    // Table names are hardcoded constants at the call sites.
    sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})")))
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get::<String, _>("name"))
        .collect()
}

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap();
    count > 0
}

async fn insert_device(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query("INSERT INTO devices (id, name) VALUES (?, ?)")
        .bind(id)
        .bind(format!("Device {id}"))
        .execute(pool)
        .await
        .expect("insert device");
}

// ──────────────────────────────────────────────
// thing_properties schema
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_thing_properties_has_full_columns() {
    let pool = migrated_pool().await;
    let cols = column_names(&pool, "thing_properties").await;
    for col in ["description", "min_value", "max_value", "default_value", "updated_at"] {
        assert!(cols.contains(&col.to_string()), "thing_properties missing column {col}");
    }
}

#[tokio::test]
async fn test_thing_properties_unique_device_name() {
    let pool = migrated_pool().await;
    insert_device(&pool, "dev-u").await;

    sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('prop-1', 'dev-u', 'temperature')")
        .execute(&pool)
        .await
        .unwrap();

    let dup =
        sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('prop-2', 'dev-u', 'temperature')")
            .execute(&pool)
            .await;
    assert!(
        dup.is_err(),
        "UNIQUE(device_id, name) must reject duplicate property names"
    );

    // Same name on a DIFFERENT device is allowed.
    insert_device(&pool, "dev-v").await;
    sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('prop-3', 'dev-v', 'temperature')")
        .execute(&pool)
        .await
        .expect("same property name on another device must be allowed");
}

// ──────────────────────────────────────────────
// Synthetic seed rows removed
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_no_synthetic_seed_rows() {
    let pool = migrated_pool().await;

    let prop_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties WHERE name = 'status' AND display_name = '在线状态'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(prop_count, 0, "synthetic 'status' property seed must be deleted");

    let action_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thing_actions WHERE name = 'reboot' AND display_name = '重启设备'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(action_count, 0, "synthetic 'reboot' action seed must be deleted");
}

// ──────────────────────────────────────────────
// Deprecated tables dropped
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_deprecated_tables_dropped() {
    let pool = migrated_pool().await;
    for table in ["device_event_triggers", "device_properties", "device_commands"] {
        assert!(
            !table_exists(&pool, table).await,
            "{table} must not exist after migrations"
        );
    }
}

// ──────────────────────────────────────────────
// resources schema
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_resources_schema() {
    let pool = migrated_pool().await;
    let cols = column_names(&pool, "resources").await;
    assert!(
        cols.contains(&"resource_type".to_string()),
        "resources must have resource_type"
    );
    assert!(
        !cols.contains(&"parse_status".to_string()),
        "resources.parse_status must be dropped"
    );
}

// ──────────────────────────────────────────────
// events is_status + partial dedup index
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_events_is_status_default_and_dedup_index() {
    let pool = migrated_pool().await;

    let rows = sqlx::query("PRAGMA table_info(events)").fetch_all(&pool).await.unwrap();
    let is_status_row = rows
        .iter()
        .find(|r| r.get::<String, _>("name") == "is_status")
        .expect("events must have is_status column");
    let default: Option<String> = is_status_row.get("dflt_value");
    assert_eq!(default.as_deref(), Some("0"), "is_status must default to 0");

    let index_sql: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_events_status_dedup'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    let sql = index_sql.expect("idx_events_status_dedup must exist");
    assert!(
        sql.contains("is_status = 1"),
        "dedup index must be partial on is_status = 1, got: {sql}"
    );
}

// ──────────────────────────────────────────────
// workspaces.require_action_confirm default
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_workspaces_require_action_confirm_defaults_to_1() {
    let pool = migrated_pool().await;
    // Seed WITHOUT specifying require_action_confirm.
    seed_test_workspace(&pool, "tenant-mig", "ws-mig").await;

    let value: i64 = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = 'ws-mig'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(value, 1, "require_action_confirm must default to 1 (fail closed)");
}

// ──────────────────────────────────────────────
// Alarm FKs reference thing_properties
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_alarm_fks_reference_thing_properties() {
    let pool = migrated_pool().await;
    // Single-connection pool: PRAGMA applies to the only connection.
    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();

    insert_device(&pool, "dev-fk").await;
    sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('prop-fk', 'dev-fk', 'temperature')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level)
         VALUES ('rule-fk', 'dev-fk', 'prop-fk', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_alarms (id, device_id, property_id, rule_id, alarm_level, alarm_message, alarm_time)
         VALUES ('alarm-fk', 'dev-fk', 'prop-fk', 'rule-fk', 'warning', 'too hot', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Deleting the property: rule CASCADEs away, alarm's property_id SET NULL.
    sqlx::query("DELETE FROM thing_properties WHERE id = 'prop-fk'")
        .execute(&pool)
        .await
        .expect("property delete must succeed (FK references thing_properties)");

    let rule_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_alarm_rules WHERE id = 'rule-fk'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rule_count, 0, "rule must CASCADE when its property is deleted");

    let alarm_prop: Option<String> = sqlx::query_scalar("SELECT property_id FROM device_alarms WHERE id = 'alarm-fk'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(alarm_prop, None, "alarm property_id must be SET NULL");

    // Regression (gateway rollback bug): DELETE FROM devices must work with
    // alarm/alarm-rule children present — no dangling device_properties parent.
    insert_device(&pool, "dev-gw").await;
    sqlx::query(
        "INSERT INTO device_alarm_rules (id, device_id, rule_name, rule_type, condition_config, alarm_level)
         VALUES ('rule-gw', 'dev-gw', 'GW Rule', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":1.0}', 'info')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_alarms (id, device_id, rule_id, alarm_level, alarm_message, alarm_time)
         VALUES ('alarm-gw', 'dev-gw', 'rule-gw', 'info', 'msg', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM devices WHERE id = 'dev-gw'")
        .execute(&pool)
        .await
        .expect("DELETE FROM devices with alarm children must succeed");

    let orphan_rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_alarm_rules WHERE device_id = 'dev-gw'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orphan_rules, 0, "device delete must cascade its alarm rules");
}

/// DM-1 regression: upgrade a deployment that ran PR #80 with REAL pre-branch
/// data (alarm rule + alarm referencing a device_properties row, plus a
/// synthetic seed in thing_properties). Before the UNION-copy fix, the FK
/// repoint committed before any real rows existed in thing_properties →
/// deferred FK violation at COMMIT → startup abort on every retry (boot loop).
#[tokio::test]
async fn test_upgrade_path_with_alarm_rules_no_boot_loop() {
    // 1. Apply migrations up to (excluding) the cleanup migration
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    let all = tinyiothub_storage::migrations::load_migrations().expect("load");
    let pre: Vec<_> = all.into_iter().filter(|m| m.version < 20260727000001).collect();
    sqlx::migrate::Migrator::with_migrations(pre)
        .run(&pool)
        .await
        .expect("pre-chain");

    // 2. Seed the #80-lineage state
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES ('dev-1', 'Sensor', 'ws-1')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, default_value, is_read_only, created_at, updated_at)
         VALUES ('prop-real', 'dev-1', 'soil_moisture', '土壤湿度', 'real prop', 'number', '%', 0, 100, NULL, 1, '2026-01-01', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at, updated_at)
         VALUES ('cmd-real', 'dev-1', 'calibrate', '校准', 'real cmd', '[]', '2026-01-01', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // synthetic seed row in the stripped thing_properties (00003 shape)
    sqlx::query(
        "INSERT INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only, created_at, updated_at)
         VALUES ('seed-1', 'dev-1', 'temperature', '温度', 'number', '°C', 1, '2026-07-25', '2026-07-25')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_commands (id, device_id, name, display_name, created_at)
         VALUES ('seed-2', 'dev-1', 'reboot', '重启设备', '2026-07-25')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
         VALUES ('rule-1', 'dev-1', 'prop-real', 'High Temp', 'threshold', '{}', 'warning', 1, datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_alarms (id, device_id, property_id, rule_id, alarm_level, alarm_message, alarm_time, created_at)
         VALUES ('alarm-1', 'dev-1', 'prop-real', 'rule-1', 'warning', 'temp high', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    // 3. Run the full chain — must NOT abort on FK violations
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("upgrade must not boot-loop");

    // 4. Real rows preserved with metadata and IDs; seeds gone; refs resolve
    let (desc, min_v): (Option<String>, Option<f64>) =
        sqlx::query_as("SELECT description, min_value FROM thing_properties WHERE id = 'prop-real'")
            .fetch_one(&pool)
            .await
            .expect("real property preserved with metadata");
    assert_eq!(desc.as_deref(), Some("real prop"));
    assert_eq!(min_v, Some(0.0));

    let seed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties WHERE (name, display_name) IN (VALUES ('temperature','温度'))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(seed_count, 0, "synthetic seed deleted");

    let cmd_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM thing_actions WHERE id = 'cmd-real')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(cmd_exists, "real command preserved");

    let resolved: Option<String> = sqlx::query_scalar(
        "SELECT p.name FROM device_alarm_rules r JOIN thing_properties p ON p.id = r.property_id WHERE r.id = 'rule-1'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(resolved.as_deref(), Some("soil_moisture"), "alarm rule still resolves");

    let alarm_resolved: Option<String> = sqlx::query_scalar(
        "SELECT p.name FROM device_alarms a JOIN thing_properties p ON p.id = a.property_id WHERE a.id = 'alarm-1'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(alarm_resolved.as_deref(), Some("soil_moisture"), "alarm still resolves");

    assert!(!table_exists(&pool, "device_properties").await, "old table dropped");
    assert!(!table_exists(&pool, "device_commands").await, "old table dropped");
}
