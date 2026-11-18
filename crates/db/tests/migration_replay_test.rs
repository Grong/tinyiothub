//! Regression test: a fresh database must migrate cleanly and expose the
//! thing-model tables after `run_migrations`.
//!
//! Bug history (2026-08-18 investigation): the historical chain rebuilt
//! `things` under FK ON, whose implicit DELETE cascaded into
//! device_properties/device_commands and wiped seed rows. Fix: FK OFF during
//! the migration run (migrations.rs).
//!
//! Task 3 note: seed rows live in `seed::seed_system` / `seed::seed_demo`
//! (out of migration history); the seed-row assertion below verifies the
//! fresh-db path end to end (migrate → seed → env01 has its 5 properties).

#[tokio::test]
async fn fresh_db_migrates_with_thing_model_tables() {
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

    // thing_properties exists with UNIQUE(thing_id, name) — enforced by an
    // explicit unique index in the baseline schema.
    let unique_idx: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_index_list('thing_properties') il
            JOIN pragma_index_info(il.name) ii
            WHERE il.[unique] = 1
            GROUP BY il.name
            HAVING group_concat(ii.name, ',' ORDER BY ii.seqno) = 'thing_id,name'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(unique_idx, "thing_properties must have UNIQUE(thing_id, name)");

    // thing_actions exists.
    let actions_table: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='thing_actions')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(actions_table, "thing_actions table must exist");

    // Fresh-db seed path: env01 carries its 5 properties after both tiers.
    let db = tinyiothub_storage::Db::new(pool.clone());
    tinyiothub_storage::seed::seed_system(&db).await.expect("seed_system");
    tinyiothub_storage::seed::seed_demo(&db).await.expect("seed_demo");
    let env01_props: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties WHERE thing_id = 'device-env-01'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(env01_props, 5, "env01 must have its 5 seed properties");

    // The whole file-backed chain must leave referential integrity intact.
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .expect("foreign_key_check");
    assert!(
        violations.is_empty(),
        "migration chain broke foreign keys: {violations:?}"
    );

    // Idempotency: re-running migrations on the migrated DB is a no-op.
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run_migrations (second run must be idempotent)");
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .expect("foreign_key_check after replay");
    assert!(
        violations.is_empty(),
        "migration replay broke foreign keys: {violations:?}"
    );

    let _ = std::fs::remove_file(&path);
}

/// Task 19C4: the `watch_rules` FK-removal rebuild preserves every row
/// (including `owner_task_id`) and leaves no reference to the retired platform
/// `agent_tasks` table.
#[tokio::test]
async fn watch_rules_rebuild_preserves_rows_and_drops_the_platform_task_fk() {
    let pool = tinyiothub_storage::test_helpers::test_pool().await;

    // Recreate the pre-19C4 shape: `owner_task_id REFERENCES agent_tasks(id)`.
    sqlx::raw_sql(
        "DROP TABLE watch_rules;
         CREATE TABLE watch_rules (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL,
             kind TEXT NOT NULL,
             condition TEXT NOT NULL,
             persistent INTEGER NOT NULL DEFAULT 0,
             timeout_secs INTEGER NOT NULL DEFAULT 600,
             created_at TEXT NOT NULL,
             owner_task_id TEXT REFERENCES agent_tasks(id)
         );
         CREATE INDEX idx_watch_rules_ws ON watch_rules(workspace_id);
         CREATE INDEX idx_watch_rules_owner_task ON watch_rules(owner_task_id);",
    )
    .execute(&pool)
    .await
    .expect("legacy watch_rules shape");

    sqlx::raw_sql(
        "INSERT OR IGNORE INTO subscription_plans (id, name, display_name)
             VALUES ('plan_free','free','Free');
         INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1');
         INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws-1','workspace','t1','now','now');",
    )
    .execute(&pool)
    .await
    .expect("workspace fixture");
    sqlx::query(
        "INSERT INTO agent_tasks
             (id, workspace_id, idempotency_key, objective, created_by, status,
              state_version, created_at, updated_at)
         VALUES ('task-owner', 'ws-1', 'fk-parent', 'inspect', 'user-1', 'active', 0, 'now', 'now')",
    )
    .execute(&pool)
    .await
    .expect("platform task fixture");
    sqlx::query(
        "INSERT INTO watch_rules
             (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
         VALUES ('watch-owned', 'ws-1', 'availability', '{}', 1, 60, 'now', 'task-owner'),
                ('watch-bare', 'ws-1', 'availability', '{}', 0, 60, 'now', NULL)",
    )
    .execute(&pool)
    .await
    .expect("watch rows");

    sqlx::raw_sql(include_str!(
        "../migrations/20261106000002_watch_rules_drop_platform_task_fk.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply 19C4 watch_rules rebuild");

    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, owner_task_id FROM watch_rules ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("rows after rebuild");
    assert_eq!(
        rows,
        vec![
            ("watch-bare".to_string(), None),
            ("watch-owned".to_string(), Some("task-owner".to_string())),
        ],
        "the rebuild must preserve rows and owner associations"
    );
    let fks: Vec<(i64, i64, String, String, String, String, String, String)> =
        sqlx::query_as("PRAGMA foreign_key_list(watch_rules)")
            .fetch_all(&pool)
            .await
            .expect("foreign keys");
    assert!(
        fks.is_empty(),
        "watch_rules must no longer reference the retired platform agent_tasks table: {fks:?}"
    );
    let indexes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'index' AND name IN ('idx_watch_rules_ws', 'idx_watch_rules_owner_task')",
    )
    .fetch_one(&pool)
    .await
    .expect("indexes");
    assert_eq!(indexes, 2, "both indexes must be recreated");
}

/// Task 19C4: the media-evidence rebuild preserves rows and drops the retired
/// platform Task/Job foreign keys.
#[tokio::test]
async fn media_evidence_rebuild_preserves_rows_and_drops_retired_fks() {
    let pool = tinyiothub_storage::test_helpers::test_pool().await;
    sqlx::raw_sql(
        "DROP TABLE media_evidence;
         CREATE TABLE media_evidence (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id),
             task_id TEXT REFERENCES agent_tasks(id),
             run_id TEXT,
             tool_call_id TEXT,
             job_id TEXT REFERENCES device_jobs(id),
             device_id TEXT NOT NULL,
             source_id TEXT NOT NULL,
             source_seq INTEGER NOT NULL,
             media_time TEXT NOT NULL,
             mime TEXT NOT NULL,
             size_bytes INTEGER NOT NULL,
             sha256 TEXT NOT NULL,
             storage_path TEXT NOT NULL,
             dedup_key TEXT NOT NULL UNIQUE,
             created_at TEXT NOT NULL,
             expires_at TEXT NOT NULL
         );
         CREATE INDEX idx_media_evidence_task ON media_evidence(task_id, created_at);
         CREATE INDEX idx_media_evidence_expiry ON media_evidence(expires_at);",
    )
    .execute(&pool)
    .await
    .expect("legacy media_evidence shape");
    sqlx::raw_sql(
        "INSERT OR IGNORE INTO subscription_plans (id, name, display_name)
             VALUES ('plan_free','free','Free');
         INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1');
         INSERT OR IGNORE INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES ('ws-1','workspace','t1','now','now');
         INSERT INTO agent_tasks
             (id, workspace_id, idempotency_key, objective, created_by, status,
              state_version, created_at, updated_at)
             VALUES ('task-owner', 'ws-1', 'evidence-parent', 'inspect', 'user-1', 'active', 0, 'now', 'now');",
    )
    .execute(&pool)
    .await
    .expect("media fixtures");
    sqlx::query(
        "INSERT INTO media_evidence
             (id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id,
              source_id, source_seq, media_time, mime, size_bytes, sha256,
              storage_path, dedup_key, created_at, expires_at)
         VALUES ('evidence-1', 'ws-1', 'task-owner', NULL, NULL, NULL, 'dev-1',
                 'camera', 1, 'now', 'image/png', 4, 'abcd',
                 'private/evidence-1', 'dedup-1', 'now', 'later')",
    )
    .execute(&pool)
    .await
    .expect("media row");

    sqlx::raw_sql(include_str!(
        "../migrations/20261106000003_media_evidence_drop_retired_fks.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply 19C4 media_evidence rebuild");

    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, task_id FROM media_evidence ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("rows after rebuild");
    assert_eq!(
        rows,
        vec![("evidence-1".to_string(), Some("task-owner".to_string()))],
        "the rebuild must preserve media rows"
    );
    let fks: Vec<(i64, i64, String, String, String, String, String, String)> =
        sqlx::query_as("PRAGMA foreign_key_list(media_evidence)")
            .fetch_all(&pool)
            .await
            .expect("foreign keys");
    assert_eq!(fks.len(), 1, "only the workspaces FK may remain: {fks:?}");
    assert_eq!(fks[0].2, "workspaces");
    let indexes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'index' AND name IN ('idx_media_evidence_task', 'idx_media_evidence_expiry')",
    )
    .fetch_one(&pool)
    .await
    .expect("indexes");
    assert_eq!(indexes, 2, "both indexes must be recreated");
}
