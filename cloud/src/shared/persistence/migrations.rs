use sqlx::{
    migrate::{Migration, Migrator},
    sqlite::SqlitePool,
};

/// Migration versions whose SQL files have been deleted.
///
/// These are kept here to prevent `VersionMissing` panics from orphaned
/// records in `_sqlx_migrations`.
const SKIP_MIGRATIONS: &[i64] = &[
    20260107000001, // deleted: test data properties/commands for non-existent devices
    20260114000001, // deleted: test data events referencing non-existent devices
    20260418000001, // deleted: storage tenant_id for tags (already in cloud base schema)
    20260608000001, // replaced by 20260608000002 (bug: didn't handle failed-rerun case)
];

/// Load migrations embedded at compile time, filtering out deleted versions.
///
/// Uses `sqlx::migrate!("./migrations")` which embeds all `.sql` files
/// relative to `CARGO_MANIFEST_DIR`. No runtime filesystem access is
/// required, fixing the Docker "migrations not found" error.
pub fn load_migrations() -> Result<Vec<Migration>, sqlx::migrate::MigrateError> {
    let migrator = sqlx::migrate!("./migrations");

    let mut combined: Vec<Migration> = Vec::new();
    for m in migrator.iter().cloned() {
        if !SKIP_MIGRATIONS.contains(&m.version) {
            combined.push(m);
        }
    }

    Ok(combined)
}

/// Run migrations with full safety checks.
///
/// 1. Clean up orphaned records in `_sqlx_migrations` for deleted versions.
/// 2. Back up the database file to `data/backups/` (only when migrations
///    are actually pending).
/// 3. Run the migration set.
/// 4. Copy real property/command data from pre-Thing-Ontology tables
///    (preserving IDs), backfill `events.workspace_id`.
/// 5. Repair schema inconsistencies (add missing columns).
/// 6. Enforce referential integrity — abort startup on FK violations
///    (`PRAGMA foreign_key_check` inside a migration script only returns
///    rows; sqlx discards them, so the check must happen here).
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    cleanup_orphaned_migration_records(pool).await?;

    let migrations = load_migrations().map_err(|e| {
        sqlx::Error::Configuration(format!("Failed to load migrations: {}", e).into())
    })?;

    if pending_migrations_exist(pool, &migrations).await? {
        backup_before_migrate(pool).await?;
    }

    // Ensure the thing-model UNION sources exist (DM-1): lineages where a
    // prior repair already dropped device_properties/device_commands get
    // empty shells so the cleanup migration's UNION SELECTs are always valid.
    prepare_thing_model_copy(pool).await?;

    Migrator::with_migrations(migrations)
        .run(pool)
        .await
        .map_err(|e| {
            sqlx::Error::Configuration(
                format!(
                    "Migration failed: {}. Restore the pre-migration backup from the backups/ directory next to the database file.",
                    e
                )
                .into(),
            )
        })?;

    repair_thing_model_data(pool).await?;
    ensure_schema_consistency(pool).await?;
    enforce_foreign_key_integrity(pool).await?;

    Ok(())
}

/// True when at least one migration in the set has not been applied yet.
async fn pending_migrations_exist(
    pool: &SqlitePool,
    migrations: &[Migration],
) -> Result<bool, sqlx::Error> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = '_sqlx_migrations'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        return Ok(!migrations.is_empty());
    }

    for m in migrations {
        let applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = ?)")
                .bind(m.version)
                .fetch_one(pool)
                .await?;
        if !applied {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Snapshot the database file to `<db-dir>/backups/<name>-<utc-ts>.db`.
///
/// Uses `VACUUM INTO`, which produces a consistent copy while the pool holds
/// the database open. Skips in-memory databases (tests). A backup failure
/// aborts startup — per the Thing Ontology design (section 七 step 0), the
/// mega-migration must never run without a restore point.
async fn backup_before_migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let db_file: Option<String> =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_optional(pool)
            .await?;

    let Some(db_file) = db_file else { return Ok(()) };
    if db_file.is_empty() {
        // In-memory database (":memory:") — nothing to back up.
        return Ok(());
    }

    let db_path = std::path::Path::new(&db_file);
    let backup_dir = db_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| {
        sqlx::Error::Configuration(format!("Failed to create backup directory: {}", e).into())
    })?;

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

/// Pre-migration guard (DM-1): ensure the UNION sources used by
/// 20260727000001 exist in every lineage. A prior repair (or a hand-migrated
/// dev DB) may have dropped device_properties/device_commands already;
/// recreating them as empty shells keeps the cleanup migration's
/// `INSERT ... SELECT ... FROM device_properties` valid everywhere.
async fn prepare_thing_model_copy(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Only create shells when the base schema migration (20260106000002,
    // which CREATEs these tables) has already run — otherwise it would
    // collide with "table already exists" on fresh databases.
    let base_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = 20260106000002)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !base_applied {
        return Ok(());
    }

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS device_properties (
            id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            name TEXT NOT NULL,
            display_name TEXT,
            description TEXT,
            data_type TEXT NOT NULL DEFAULT 'string',
            unit TEXT,
            min_value REAL,
            max_value REAL,
            default_value TEXT,
            is_read_only INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS device_commands (
            id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            name TEXT NOT NULL,
            display_name TEXT,
            description TEXT,
            parameters TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Post-migration data repair for the Thing Ontology rebuild (eng-review T3/T12).
///
/// The real-data copy happens INLINE in 20260727000001 (UNION into the
/// rebuild inserts) — doing it here would run AFTER the FK repoints commit,
/// leaving dangling parents and boot-looping the deploy (DM-1). This step:
/// 1. Drops the old device_properties / device_commands tables (data already
///    merged by the migration; empty shells where nothing ever existed).
/// 2. Backfills events.workspace_id from the owning device (design 七·1 OV6;
///    rows whose device is gone keep '' and are logged).
async fn repair_thing_model_data(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if table_exists(pool, "device_properties").await? {
        sqlx::query("DROP TABLE device_properties").execute(pool).await?;
        tracing::info!("dropped device_properties (data merged into thing_properties by migration)");
    }

    if table_exists(pool, "device_commands").await? {
        sqlx::query("DROP TABLE device_commands").execute(pool).await?;
        tracing::info!("dropped device_commands (data merged into thing_actions by migration)");
    }

    let backfilled = sqlx::query(
        "UPDATE events
         SET workspace_id = (
             SELECT workspace_id FROM devices WHERE devices.id = events.device_id
         )
         WHERE (workspace_id IS NULL OR workspace_id = '')
           AND device_id IS NOT NULL
           AND EXISTS (SELECT 1 FROM devices WHERE devices.id = events.device_id)",
    )
    .execute(pool)
    .await?;
    if backfilled.rows_affected() > 0 {
        tracing::info!(rows = backfilled.rows_affected(), "backfilled events.workspace_id");
    }

    let dangling: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events
         WHERE (workspace_id IS NULL OR workspace_id = '') AND device_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    if dangling > 0 {
        tracing::warn!(rows = dangling, "events with dangling device_id keep empty workspace_id");
    }

    Ok(())
}

async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
    )
    .bind(table)
    .fetch_one(pool)
    .await
}

/// Abort startup when referential integrity is broken after migrations.
///
/// `PRAGMA foreign_key_check` as a migration-script statement only RETURNS
/// rows — sqlx discards them, so a rebuild that orphans FK rows would commit
/// silently. This is the enforcement point the Thing Ontology design
/// (section 七·1) actually requires.
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

/// Delete `_sqlx_migrations` records for versions whose files no longer exist.
///
/// Without this, SQLx's `Migrator` panics with `VersionMissing` when it sees
/// a record for a version not present in the migration set.
async fn cleanup_orphaned_migration_records(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // On fresh databases the `_sqlx_migrations` table does not exist yet.
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

    for version in SKIP_MIGRATIONS {
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
            .bind(version)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Ensure tables have all expected columns by adding missing ones.
///
/// This repairs databases where a migration's `CREATE TABLE IF NOT EXISTS`
/// was a no-op on an existing table, or where `ALTER TABLE ADD COLUMN`
/// migrations need to be idempotent.
///
/// Uses `PRAGMA table_info()` to check, then `ALTER TABLE ADD COLUMN`.
async fn ensure_schema_consistency(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    add_column_if_missing(pool, "notification_channels", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "notification_rules", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "notification_history", "workspace_id", "TEXT").await?;

    add_column_if_missing(pool, "chat_sessions", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "chat_sessions", "metadata", "TEXT NOT NULL DEFAULT '{}'").await?;

    add_column_if_missing(pool, "chat_messages", "tool_call_id", "TEXT").await?;
    add_column_if_missing(pool, "chat_messages", "tool_name", "TEXT").await?;

    add_column_if_missing(pool, "roles", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "users", "phone", "TEXT").await?;
    add_column_if_missing(pool, "device_alarms", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "device_alarm_rules", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "api_keys", "workspace_id", "TEXT").await?;

    Ok(())
}

/// Add a column to a table if it doesn't already exist.
///
/// Safe to call multiple times (idempotent).
async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<(), sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_table_info(?)
            WHERE name = ?
        )",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await?;

    if !exists {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, column_def);
        sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: after run_migrations (which drops device_properties /
    /// device_commands in repair_thing_model_data), a plain DELETE FROM
    /// devices must still work — the gateway pairing rollback depends on it.
    #[tokio::test]
    async fn delete_from_devices_works_after_repair() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();

        sqlx::query("INSERT INTO tenants (id, name, slug, created_at, updated_at) VALUES ('t1','t','t','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws','t1','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO devices (id, name, workspace_id, created_at, updated_at) VALUES ('d1','gw','ws1','2025-01-01','2025-01-01')")
            .execute(&pool).await.unwrap();

        let result = sqlx::query("DELETE FROM devices WHERE id = 'd1'").execute(&pool).await;
        assert!(result.is_ok(), "DELETE FROM devices failed: {:?}", result.err());

        // Old tables must be gone, new ones present
        let old_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='device_properties')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!old_exists, "device_properties should be dropped by repair");
    }
}
