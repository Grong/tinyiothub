//! `hub_ledger` retention (E3 retired): watch/cancel/media telemetry must be
//! time-bounded even though `state_change` rows are no longer written.

use chrono::{Duration, Utc};
use tinyiothub_core::{cron::JobExecutor, models::cron_job::CronJob};
use tinyiothub_runtime::cron_executors::HubLedgerRetentionExecutor;
use tinyiothub_storage::Db;

async fn test_pool() -> sqlx::SqlitePool {
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

fn retention_job(retention_days: i64) -> CronJob {
    CronJob {
        id: "test-hub-ledger-retention".to_string(),
        name: "test".to_string(),
        description: None,
        job_type: "hub_ledger_retention".to_string(),
        cron_expression: "0 47 3 * * *".to_string(),
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

async fn insert_ledger(pool: &sqlx::SqlitePool, kind: &str, age_days: i64) {
    let ts = (Utc::now() - Duration::days(age_days)).to_rfc3339();
    tinyiothub_storage::watch::append_ledger(pool, "ws-1", &ts, kind, Some("t1"), "{}")
        .await
        .expect("append ledger");
}

#[tokio::test]
async fn retention_executor_deletes_only_old_telemetry() {
    let pool = test_pool().await;
    insert_ledger(&pool, "watch_hit", 60).await;
    insert_ledger(&pool, "call_cancelled", 60).await;
    insert_ledger(&pool, "watch_hit", 5).await;
    insert_ledger(&pool, "call_cancelled_unknown", 5).await;

    let executor = HubLedgerRetentionExecutor::new(std::sync::Arc::new(
        crate::shared::runtime_ports::HubLedgerRetentionAdapter(Db::new(pool.clone())),
    ));
    let result = executor.execute(&retention_job(30), "run-1").await.expect("execute");

    assert!(result.output.unwrap().contains("deleted 2 "));
    let remaining: Vec<String> = sqlx::query_scalar("SELECT kind FROM hub_ledger ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("remaining rows");
    assert_eq!(
        remaining,
        vec!["watch_hit".to_string(), "call_cancelled_unknown".to_string()]
    );
}

#[tokio::test]
async fn retention_job_type_is_advertised() {
    let pool = test_pool().await;
    let executor = HubLedgerRetentionExecutor::new(std::sync::Arc::new(
        crate::shared::runtime_ports::HubLedgerRetentionAdapter(Db::new(pool)),
    ));
    assert!(executor.can_handle("hub_ledger_retention"));
    assert!(!executor.can_handle("event_retention"));
}
