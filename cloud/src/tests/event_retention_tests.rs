//! Events retention tests (CEO expansion X1 / OV-1).
//!
//! The events table mixes immutable occurrence rows (is_status=0 — log
//! history, safe to time-purge) with mutable status rows (is_status=1 — the
//! LIVE current-state of devices, never time-purged). Every test here exists
//! to prove the purge paths respect that split.

use chrono::{Duration, Utc};
use tinyiothub_core::{cron::JobExecutor, models::cron_job::CronJob};
use tinyiothub_runtime::cron_executors::EventRetentionExecutor;
use tinyiothub_storage::sqlite::Database;

use crate::{
    modules::event::{
        repo::RealTimeEventRepository, sqlite_real_time_event::SqliteRealTimeEventRepository,
    },
    test_utils::seed_test_workspace,
};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool).await.expect("migrations");
    pool
}

/// Insert an event row. `is_status` distinguishes occurrence (0) from status (1).
async fn insert_event(
    pool: &sqlx::SqlitePool,
    id: &str,
    age_days: i64,
    is_status: i64,
    acknowledged: i64,
) {
    // event_subtype varies by id so status rows don't collide on the
    // (correct) status-dedup unique index
    let ts = (Utc::now() - Duration::days(age_days)).to_rfc3339();
    sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, \
         source_type, source_id, device_id, title, content, created_at, workspace_id, \
         occurrence_count, acknowledged, is_status) \
         VALUES (?, 'device', ?, 3, ?, 'thing', 'thing/dev-1', 'dev-1', 't', '{}', ?, 'ws-1', 1, ?, ?)",
    )
    .bind(id)
    .bind(format!("evt_{id}"))
    .bind(&ts)
    .bind(&ts)
    .bind(acknowledged)
    .bind(is_status)
    .execute(pool)
    .await
    .expect("insert event");
}

async fn event_exists(pool: &sqlx::SqlitePool, id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
        > 0
}

fn retention_job(retention_days: i64) -> CronJob {
    CronJob {
        id: "test-retention".to_string(),
        name: "test".to_string(),
        description: None,
        job_type: "event_retention".to_string(),
        cron_expression: "0 17 3 * * *".to_string(),
        config: format!("{{\"retention_days\": {}}}", retention_days),
        timeout_seconds: 300,
        max_retries: 3,
        is_enabled: true,
        is_running: false,
        last_run_at: None,
        last_run_status: None,
        last_run_error: None,
        next_run_at: None,
        run_count: 0,
        success_count: 0,
        fail_count: 0,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
        created_by: None,
        workspace_id: Some("system".to_string()),
    }
}

#[tokio::test]
async fn test_retention_executor_deletes_only_old_occurrence_rows() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES ('dev-1', 'D1', 'ws-1')")
        .execute(&pool)
        .await
        .unwrap();

    insert_event(&pool, "old-occ", 100, 0, 0).await; // old occurrence → DELETE
    insert_event(&pool, "old-status", 100, 1, 0).await; // old status → KEEP (live state)
    insert_event(&pool, "new-occ", 10, 0, 0).await; // recent occurrence → KEEP
    insert_event(&pool, "new-status", 10, 1, 0).await; // recent status → KEEP

    let executor = EventRetentionExecutor::new(Database::new(pool.clone()));
    let result = executor.execute(&retention_job(90), "run-1").await.expect("execute");

    assert!(result.output.unwrap().contains("deleted 1 "));
    assert!(!event_exists(&pool, "old-occ").await, "old occurrence purged");
    assert!(event_exists(&pool, "old-status").await, "old status EXEMPT — it is live state");
    assert!(event_exists(&pool, "new-occ").await);
    assert!(event_exists(&pool, "new-status").await);
}

#[tokio::test]
async fn test_cleanup_old_events_exempts_status_rows() {
    let pool = test_pool().await;
    insert_event(&pool, "old-occ", 100, 0, 0).await;
    insert_event(&pool, "old-status", 100, 1, 0).await;

    let repo = SqliteRealTimeEventRepository::new(Database::new(pool.clone()));
    let deleted = repo.cleanup_old_events(Utc::now() - Duration::days(90)).await.unwrap();

    assert_eq!(deleted, 1);
    assert!(!event_exists(&pool, "old-occ").await);
    assert!(event_exists(&pool, "old-status").await, "status rows exempt from time purge");
}

#[tokio::test]
async fn test_clear_acknowledged_only_removes_occurrence_rows() {
    let pool = test_pool().await;
    insert_event(&pool, "ack-occ", 1, 0, 1).await; // acked occurrence → DELETE
    insert_event(&pool, "ack-status", 1, 1, 1).await; // acked STATUS → KEEP (live state)
    insert_event(&pool, "unack-occ", 1, 0, 0).await; // unacked → KEEP

    let repo = SqliteRealTimeEventRepository::new(Database::new(pool.clone()));
    let deleted = repo.clear_acknowledged_events().await.unwrap();

    assert_eq!(deleted, 1);
    assert!(!event_exists(&pool, "ack-occ").await);
    assert!(event_exists(&pool, "ack-status").await, "acknowledged status row is still live state");
    assert!(event_exists(&pool, "unack-occ").await);
}

#[tokio::test]
async fn test_retention_job_seeded_by_migration() {
    let pool = test_pool().await;
    let (job_type, enabled): (String, i64) = sqlx::query_as(
        "SELECT job_type, is_enabled FROM cron_jobs WHERE id = 'sys-event-retention'",
    )
    .fetch_one(&pool)
    .await
    .expect("retention job seeded");
    assert_eq!(job_type, "event_retention");
    assert_eq!(enabled, 1);
}
