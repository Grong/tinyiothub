//! Test helpers for crates that need a schema-complete in-memory pool.

/// Run all migrations in chronological order.
///
/// Delegates to the centralized migration runner (backup before pending
/// migrations, FK OFF during the run, FK integrity check after).
pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    crate::migrations::run_migrations(pool).await
}

/// baseline 的版本号——test_pool 直建后跳过它、只应用更新的递增迁移。
const BASELINE_VERSION: i64 = 20260819000001;

/// In-memory pool with the baseline schema applied directly (no migration
/// runner, no `_sqlx_migrations` bookkeeping) — the fast path for tests that
/// only need the schema. Post-baseline incremental migrations are applied
/// too (CEO review T2：直建缺列会让引用新列的领域函数在测试池假败）。
pub async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool");
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .expect("baseline schema applies cleanly");
    // 借 Migrator 的编译期内嵌枚举递增迁移——migrations/ 里的新文件自动覆盖。
    let migrator = sqlx::migrate!("./migrations");
    for migration in migrator.iter().filter(|m| m.version > BASELINE_VERSION) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .expect("post-baseline migration applies cleanly");
    }
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
