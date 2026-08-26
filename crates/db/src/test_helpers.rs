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
/// only need the schema. Post-baseline incremental migrations are applied
/// too (CEO review T2：直建缺列会让引用新列的领域函数在测试池假败）。
///
/// 与 runner 的漂移控制（data-migration review）：
/// - baseline 版本取自迁移集最小版本（非硬编码常量——未来 re-squash 时
///   自动跟随，不会在旧基线上重放中间迁移）；
/// - 递增迁移在专用连接上 FK OFF 执行（复刻 runner：DROP+重建类迁移在
///   FK ON 下会隐式 DELETE 并级联清空子表——测试池与生产行为必须一致）；
/// - 有意跳过：`_sqlx_migrations` 簿记、checksum 校验、FK 完整性检查
///   （测试池只要 schema，不要 runner 的运维语义）。
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
    let baseline_version = migrator.iter().map(|m| m.version).min().unwrap_or(0);
    let mut conn = pool.acquire().await.expect("acquire migration connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .expect("FK OFF for post-baseline migrations");
    for migration in migrator.iter().filter(|m| m.version > baseline_version) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&mut *conn)
            .await
            .expect("post-baseline migration applies cleanly");
    }
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .expect("restore FK ON");
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
