//! device→thing 重命名迁移的数据保留回归测试（Task 2）。
//! 老库（baseline 终态 + 样本数据）跑剩余迁移后：数据保留、列名已改、
//! tags 'device'→'thing'、FK 无违规。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 直建 baseline 终态并把它标记为已应用，使 run_migrations 只跑后续迁移
/// （含重命名迁移）——模拟"老库升级"路径。
async fn baseline_pool_with_seed_data() -> (SqlitePool, std::path::PathBuf) {
    // 与 thing_rename_schema_tests 相同的隔离模式：pid + 原子序号。
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("thing-rename-data-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    // 1. 只建 baseline 终态（不经 run_migrations，直接执行 baseline.sql）。
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool).await.unwrap();

    // 2. 把 baseline 标记为已应用，否则 run_migrations 会重放 baseline
    //    报 "table already exists"。checksum 必须取自嵌入迁移集，
    //    否则 sqlx 校验期 VersionMismatch 失败。
    sqlx::raw_sql(
        "CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(&pool).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");
    let baseline = migrator.iter()
        .find(|m| m.version == 20260819000001)
        .expect("baseline migration missing from embedded set");
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (?, ?, 1, ?, 0)",
    )
    .bind(baseline.version)
    .bind(baseline.description.as_ref())
    .bind(baseline.checksum.as_ref())
    .execute(&pool).await.unwrap();

    // 3. 插入覆盖各改名面的样本数据（baseline 列名）。
    sqlx::query("INSERT INTO devices (id, name, thing_type, device_type) VALUES ('d1', 'demo', 'device', 'sensor')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('p1', 'd1', 'temp')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tags (id, type, name) VALUES ('t1', 'device', 'tag1')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO device_memory (workspace_id, agent_id, device_id, snapshot_data, snapshot_time)
                 VALUES ('w1', 'a1', 'd1', '{}', 1)")
        .execute(&pool).await.unwrap();
    // events.event_level 是 INTEGER NOT NULL。
    sqlx::query("INSERT INTO events (id, event_level, event_type, event_subtype, timestamp, source_type, title, device_id)
                 VALUES ('e1', 2, 't', 'st', '2026-08-25T00:00:00Z', 'device', 'x', 'd1')")
        .execute(&pool).await.unwrap();
    // 最终迁移还改了 messages.device_type→category / device_id→thing_id、
    // agent_memories.device_id、agent_actions.device_id，一并覆盖。
    sqlx::query("INSERT INTO messages (id, level, title, device_type, device_id)
                 VALUES ('m1', 1, 'hello', 'sensor', 'd1')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_memories (id, workspace_id, agent_id, content, device_id)
                 VALUES ('am1', 'w1', 'a1', 'note', 'd1')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, device_id)
                 VALUES ('aa1', 'w1', 'a1', 'alarm', 'restart', 'payload', 'd1')")
        .execute(&pool).await.unwrap();

    (pool, path)
}

#[tokio::test]
async fn baseline_data_survives_rename() {
    let (pool, path) = baseline_pool_with_seed_data().await;

    // 4. 跑剩余迁移（含重命名）。
    tinyiothub_storage::migrations::run_migrations(&pool).await.unwrap();

    // 5. 数据保留 + 列名已改 + tags 值已转换。
    let things_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM things WHERE id = 'd1' AND name = 'demo' AND thing_type = 'device' AND category = 'sensor'",
    )
    .fetch_one(&pool).await.unwrap();
    assert_eq!(things_count, 1);
    let prop: String = sqlx::query_scalar("SELECT thing_id FROM thing_properties WHERE id = 'p1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(prop, "d1");
    let tag_type: String = sqlx::query_scalar("SELECT type FROM tags WHERE id = 't1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(tag_type, "thing");
    let mem: String = sqlx::query_scalar("SELECT thing_id FROM thing_memory WHERE workspace_id = 'w1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(mem, "d1");
    let evt: String = sqlx::query_scalar("SELECT thing_id FROM events WHERE id = 'e1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(evt, "d1");
    let msg: (String, String) = sqlx::query_as("SELECT thing_id, category FROM messages WHERE id = 'm1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(msg, ("d1".to_string(), "sensor".to_string()));
    let amem: String = sqlx::query_scalar("SELECT thing_id FROM agent_memories WHERE id = 'am1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(amem, "d1");
    let aact: String = sqlx::query_scalar("SELECT thing_id FROM agent_actions WHERE id = 'aa1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(aact, "d1");

    // 6. FK 完整性（run_migrations 内部也强制，这里显式断言作为回归文档）。
    let violations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(violations, 0, "foreign_key_check violations after rename");

    drop(pool);
    let _ = std::fs::remove_file(path);
}
