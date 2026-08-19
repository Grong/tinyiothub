//! Regression test: a fresh database must migrate cleanly and expose the
//! thing-model tables after `run_migrations`.
//!
//! Bug history (2026-08-18 investigation): the historical chain rebuilt
//! `devices` under FK ON, whose implicit DELETE cascaded into
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

    // thing_properties exists with UNIQUE(device_id, name) — enforced by an
    // explicit unique index in the baseline schema.
    let unique_idx: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_index_list('thing_properties') il
            JOIN pragma_index_info(il.name) ii
            WHERE il.[unique] = 1
            GROUP BY il.name
            HAVING group_concat(ii.name, ',' ORDER BY ii.seqno) = 'device_id,name'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        unique_idx,
        "thing_properties must have UNIQUE(device_id, name)"
    );

    // thing_actions exists.
    let actions_table: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='thing_actions')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(actions_table, "thing_actions table must exist");

    // Fresh-db seed path: env01 carries its 5 properties after both tiers.
    let db = tinyiothub_storage::Db::new(pool.clone());
    tinyiothub_storage::seed::seed_system(&db)
        .await
        .expect("seed_system");
    tinyiothub_storage::seed::seed_demo(&db)
        .await
        .expect("seed_demo");
    let env01_props: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties WHERE device_id = 'device-env-01'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(env01_props, 5, "env01 must have its 5 seed properties");

    // Idempotency: re-running migrations on the migrated DB is a no-op.
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run_migrations (second run must be idempotent)");

    let _ = std::fs::remove_file(&path);
}
