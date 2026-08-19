//! Test helpers for crates that need a schema-complete in-memory pool.

/// Run all migrations in chronological order.
///
/// Delegates to the centralized migration runner (backup before pending
/// migrations, FK OFF during the run, FK integrity check after).
pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    crate::migrations::run_migrations(pool).await
}

/// In-memory pool with the baseline schema applied directly (no migration
/// runner, no `_sqlx_migrations` bookkeeping) — the fast path for tests that
/// only need the schema.
pub async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool");
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .expect("baseline schema applies cleanly");
    pool
}

/// Baseline-built pool paired with its `Db` facade (testing feature).
#[cfg(feature = "testing")]
pub async fn fixture_pool_with_db() -> (sqlx::SqlitePool, crate::Db) {
    let pool = test_pool().await;
    let db = crate::Db::new(pool.clone());
    (pool, db)
}

/// Baseline-built pool with both seed tiers applied (testing feature).
#[cfg(feature = "testing")]
pub async fn fixture_pool_seeded() -> (sqlx::SqlitePool, crate::Db) {
    let (pool, db) = fixture_pool_with_db().await;
    crate::seed::seed_system(&db).await.expect("seed_system");
    crate::seed::seed_demo(&db).await.expect("seed_demo");
    (pool, db)
}
