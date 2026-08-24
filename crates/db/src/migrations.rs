//! Embedded SQLite migrations and the safe migration runner (backup, FK enforcement).
//!
//! History: 68 incremental migrations were squashed into
//! `20260819000001_baseline.sql` (Task 2, verified by
//! `tests/baseline_schema_tests.rs` against the old chain's terminal schema).

use sqlx::{migrate::Migrator, sqlite::SqlitePool};

/// Run migrations with full safety checks.
///
/// 1. Back up the database file to `backups/` (only when migrations are actually pending).
/// 2. Run the embedded migration set on a dedicated connection with FK OFF.
/// 3. Enforce referential integrity — abort startup on FK violations (`PRAGMA foreign_key_check`
///    inside a migration script only returns rows; sqlx discards them, so the check must happen
///    here).
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let migrator = sqlx::migrate!("./migrations");

    if pending_migrations_exist(pool, &migrator).await? {
        backup_before_migrate(pool).await?;
    }

    // CEO review T4：老链库（68 迁移时代的 _sqlx_migrations 行）会在 sqlx 校验期
    // 以 VersionMissing 失败——响亮但文案误导（"恢复备份"会让运维陷入恢复→再崩
    // 死循环）。备份已先行，这里显式检测并给出 Q2 的可行动指引。
    reject_legacy_chain_db(pool, &migrator).await?;

    // FK 加固（2026-08-18 调查）：sqlx 默认 foreign_keys=ON；SQLite 在 FK 开启时
    // DROP TABLE 会先隐式 DELETE FROM 该表，触发子表 ON DELETE CASCADE。
    // 迁移在专用连接上以 FK OFF 运行；pragma 是连接级设置，
    // 归还连接池前恢复 ON，运行期连接的 FK 行为不受影响。
    let mut mig_conn = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&mut *mig_conn).await?;
    let mig_result = migrator.run(&mut *mig_conn).await;
    sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *mig_conn).await?;
    drop(mig_conn);
    mig_result.map_err(|e| {
        sqlx::Error::Configuration(
            format!(
                "Migration failed: {}. Restore the pre-migration backup from the backups/ directory next to the database file.",
                e
            )
            .into(),
        )
    })?;

    enforce_foreign_key_integrity(pool).await?;

    Ok(())
}

/// True when at least one migration in the set has not been applied yet.
async fn pending_migrations_exist(pool: &SqlitePool, migrator: &Migrator) -> Result<bool, sqlx::Error> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = '_sqlx_migrations'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        return Ok(migrator.iter().next().is_some());
    }

    for m in migrator.iter() {
        let applied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = ?)")
            .bind(m.version)
            .fetch_one(pool)
            .await?;
        if !applied {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Abort with an actionable message when the database was created by the legacy
/// 68-migration chain (CEO review T4).
///
/// sqlx's own `VersionMissing` validation error fires later and only says the
/// applied version "is missing in the resolved migrations" — paired with the
/// generic "restore the backup" remediation it sends operators into a
/// restore-and-crash loop. The settled decision (db-overhaul Q2) is that legacy
/// deployments rebuild: no data-migration path exists. The pre-migration backup
/// has already been written by the time this runs.
async fn reject_legacy_chain_db(pool: &SqlitePool, migrator: &Migrator) -> Result<(), sqlx::Error> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = '_sqlx_migrations'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        return Ok(());
    }

    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
        .fetch_all(pool)
        .await?;
    let known: std::collections::HashSet<i64> = migrator.iter().map(|m| m.version).collect();
    let mut legacy: Vec<i64> = applied.into_iter().filter(|v| !known.contains(v)).collect();
    legacy.sort_unstable();

    if legacy.is_empty() {
        return Ok(());
    }

    // data-migration review：未知版本全部比本构建的迁移更新时，这是
    // "降级"（库由更新二进制创建）而非老链——正确动作是升级二进制，
    // 删库重建的指引在此场景是错的。
    let max_known = known.iter().max().copied().unwrap_or(0);
    if legacy.iter().all(|v| *v > max_known) {
        return Err(sqlx::Error::Configuration(
            format!(
                "This database was migrated by a NEWER version of this application ({} applied version(s) newer than any migration this build knows, newest: {}). \
                 Do NOT delete the database — run the newer binary instead.",
                legacy.len(),
                legacy[legacy.len() - 1]
            )
            .into(),
        ));
    }

    Err(sqlx::Error::Configuration(
        format!(
            "This database was created by the legacy migration chain ({} applied version(s) unknown to this build, oldest: {}). \
             There is deliberately no data-migration path (db-overhaul decision Q2). \
             A pre-migration backup has been written to the backups/ directory next to the database file. \
             To run this version: stop the app, move the database file away (keep it as your archive), and start again — \
             a fresh baseline database with system seed data is built automatically. \
             如需保留旧数据：先用旧版本二进制导出，再启动新版本。",
            legacy.len(),
            legacy[0]
        )
        .into(),
    ))
}

/// Snapshot the database file to `<db-dir>/backups/<name>-<utc-ts>.db`.
///
/// Uses `VACUUM INTO`, which produces a consistent copy while the pool holds
/// the database open. Skips in-memory databases (tests). A backup failure
/// aborts startup — the baseline migration must never run without a restore point.
async fn backup_before_migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let db_file: Option<String> = sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
        .fetch_optional(pool)
        .await?;

    let Some(db_file) = db_file else { return Ok(()) };
    if db_file.is_empty() {
        // In-memory database (":memory:") — nothing to back up.
        return Ok(());
    }

    let db_path = std::path::Path::new(&db_file);
    let backup_dir = db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("backups");
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| sqlx::Error::Configuration(format!("Failed to create backup directory: {}", e).into()))?;

    let stem = db_path.file_stem().and_then(|s| s.to_str()).unwrap_or("database");
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = backup_dir.join(format!("{}-{}.db", stem, ts));
    let dest_str = dest.to_string_lossy().replace('\'', "''");

    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{}'", dest_str)))
        .execute(pool)
        .await
        .map_err(|e| {
            sqlx::Error::Configuration(
                format!(
                    "Pre-migration backup to '{}' failed: {}. Aborting before any migration runs.",
                    dest_str, e
                )
                .into(),
            )
        })?;

    tracing::info!(dest = %dest.display(), "database backed up before migrations");
    Ok(())
}

/// Abort startup when referential integrity is broken after migrations.
///
/// `PRAGMA foreign_key_check` as a migration-script statement only RETURNS
/// rows — sqlx discards them, so a rebuild that orphans FK rows would commit
/// silently.
async fn enforce_foreign_key_integrity(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let violations: Vec<(String, i64, String, i64)> =
        sqlx::query_as("SELECT \"table\", rowid, \"parent\", fkid FROM pragma_foreign_key_check")
            .fetch_all(pool)
            .await?;

    if violations.is_empty() {
        return Ok(());
    }

    for (table, rowid, parent, _fkid) in violations.iter().take(10) {
        tracing::error!(table, rowid, parent, "foreign key violation after migration");
    }
    Err(sqlx::Error::Configuration(
        format!(
            "{} foreign key violations detected after migration. Startup aborted; restore the pre-migration backup from the backups/ directory.",
            violations.len()
        )
        .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CEO review T4: a database created by the legacy 68-migration chain must
    /// abort with an actionable message (rebuild per Q2), not sqlx's generic
    /// VersionMissing + "restore the backup" boot-loop text.
    #[tokio::test]
    async fn legacy_chain_db_aborts_with_rebuild_guidance() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        // 老链版本号（不在当前迁移集内）。
        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (20260106000002, 'legacy', 1, X'00', 1)")
            .execute(&pool)
            .await
            .unwrap();

        let err = run_migrations(&pool).await.expect_err("legacy chain DB must abort");
        let msg = err.to_string();
        assert!(
            msg.contains("legacy migration chain"),
            "message should name the cause: {msg}"
        );
        assert!(msg.contains("no data-migration path"), "message should state Q2: {msg}");
        assert!(
            msg.contains("move the database file away"),
            "message should be actionable: {msg}"
        );
    }

    /// data-migration review：库由更新二进制创建（降级场景）时，报错应
    /// 指引升级二进制，而非删库重建。
    #[tokio::test]
    async fn newer_binary_db_aborts_with_upgrade_guidance() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (99999999999999, 'future', 1, X'00', 1)")
            .execute(&pool)
            .await
            .unwrap();

        let err = run_migrations(&pool).await.expect_err("newer-binary DB must abort");
        let msg = err.to_string();
        assert!(
            msg.contains("NEWER version"),
            "should identify downgrade scenario: {msg}"
        );
        assert!(
            msg.contains("run the newer binary"),
            "should guide upgrade, not rebuild: {msg}"
        );
        assert!(
            !msg.contains("no data-migration path"),
            "must not give rebuild guidance: {msg}"
        );
    }

    /// Regression: after run_migrations, a plain DELETE FROM devices must
    /// still work — the gateway pairing rollback depends on it (FK cascade
    /// wiped child rows in the historical chain; FK OFF during migration is
    /// the fix).
    #[tokio::test]
    async fn delete_from_devices_works_after_migrations() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();

        // seed_system 提供 subscription_plans → 默认租户/工作区链（Task 3）。
        crate::seed::seed_system(&crate::Db::new(pool.clone())).await.unwrap();
        sqlx::query("INSERT INTO devices (id, name, workspace_id, created_at, updated_at) VALUES ('d1','gw','ws-default-001','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();

        let result = sqlx::query("DELETE FROM devices WHERE id = 'd1'").execute(&pool).await;
        assert!(result.is_ok(), "DELETE FROM devices failed: {:?}", result.err());
    }
}
