//! Regression test: a fresh database must end up with the seeded per-device
//! properties/actions after `run_migrations`.
//!
//! Bug history (2026-08-18 investigation): 20260723000001 drops + recreates
//! `devices`; under sqlx's default `PRAGMA foreign_keys=ON` the drop performs
//! an implicit DELETE FROM devices, which ON DELETE CASCADE wiped
//! device_properties/device_commands. The 20260727000001 cleanup then deleted
//! the synthetic seed rows by design, leaving thing_properties/thing_actions
//! empty — the thing profile API returned no properties/actions.
//! Fix: FK OFF during the migration run (migrations.rs) + repair migration
//! 20260818000001 re-seeding the January rows after the cleanup.

#[tokio::test]
async fn fresh_db_has_seed_properties_and_actions_after_migrations() {
    let dir = std::env::temp_dir().join("tih-mig-regression");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("fresh-{}-{}.db", std::process::id(), "a"));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect");

    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run_migrations");

    let props: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties")
        .fetch_one(&pool)
        .await
        .unwrap();
    let actions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let env01_props: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties WHERE device_id = 'device-env-01'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // 温度属性是一月真实种子（与合成种子同 tuple 曾被误删），必须存在。
    let env01_temp: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM thing_properties
                       WHERE device_id = 'device-env-01' AND name = 'temperature'
                         AND unit = '°C' AND min_value = -20 AND max_value = 60)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(props >= 35, "expected restored seed properties, got {props}");
    assert!(actions >= 15, "expected restored seed actions, got {actions}");
    assert_eq!(env01_props, 5, "device-env-01 should have 5 seed properties");
    assert!(env01_temp, "device-env-01 temperature property must be restored");

    // Idempotency: running the repair migration's inserts again must not
    // duplicate rows (UNIQUE(device_id, name) + INSERT OR IGNORE).
    let mig = include_str!("../migrations/20260818000001_restore_thing_seed_properties.sql");
    sqlx::raw_sql(mig).execute(&pool).await.unwrap();
    let props_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(props, props_after, "repair migration must be idempotent");
}
