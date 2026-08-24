//! device→thing 重命名迁移的 schema 断言（Task 1）。
//! 全量迁移跑完后：新名存在、旧名消失、tags CHECK 不含 'device'。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

async fn fresh_migrated_pool() -> (SqlitePool, std::path::PathBuf) {
    // 测试在同一进程内并行：pid 相同，需原子序号保证每个测试独立 DB 文件，
    // 否则 VACUUM INTO 备份与建库互相 "database is locked"。
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("thing-rename-schema-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    tinyiothub_storage::migrations::run_migrations(&pool).await.unwrap();
    (pool, path)
}

async fn table_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table'
           AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%' ORDER BY name",
    )
    .fetch_all(pool).await.unwrap()
}

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    // table 名均为上方硬编码常量，AssertSqlSafe 仅满足 sqlx 0.9 的 SqlSafeStr bound。
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT name FROM pragma_table_info('{table}')")))
        .fetch_all(pool).await.unwrap()
}

#[tokio::test]
async fn device_tables_renamed_to_thing() {
    let (pool, path) = fresh_migrated_pool().await;
    let tables = table_names(&pool).await;
    for new in ["things", "thing_traces", "thing_memory", "thing_alarm_rules", "thing_alarms"] {
        assert!(tables.contains(&new.to_string()), "missing table {new}");
    }
    for old in ["devices", "device_traces", "device_memory", "device_alarm_rules", "device_alarms"] {
        assert!(!tables.contains(&old.to_string()), "stale table {old}");
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn device_id_columns_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    // baseline 的 knowledge_relations 无 device_id 列（仅有 workspace/entity 外键），
    // 不在重命名清单内。
    for t in [
        "messages", "thing_traces", "events", "thing_memory", "batch_command_items",
        "resources", "thing_properties", "thing_actions", "thing_alarm_rules",
        "thing_alarms", "knowledge_entities", "agent_memories", "agent_actions",
    ] {
        let cols = columns(&pool, t).await;
        assert!(cols.contains(&"thing_id".to_string()), "{t} missing thing_id");
        assert!(!cols.contains(&"device_id".to_string()), "{t} still has device_id");
    }
    let jobs = columns(&pool, "jobs").await;
    assert!(jobs.contains(&"target_thing_id".to_string()));
    assert!(!jobs.contains(&"target_device_id".to_string()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn device_type_and_limit_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    for t in ["things", "thing_templates"] {
        let cols = columns(&pool, t).await;
        assert!(cols.contains(&"category".to_string()), "{t} missing category");
        assert!(!cols.contains(&"device_type".to_string()), "{t} still has device_type");
        assert!(cols.contains(&"thing_type".to_string()) || t == "thing_templates",
                "things.thing_type must survive");
    }
    let plans = columns(&pool, "subscription_plans").await;
    assert!(plans.contains(&"thing_limit".to_string()));
    assert!(!plans.contains(&"device_limit".to_string()));
    // messages.device_type 与 device_id 配对(FK→devices),随同一映射改名。
    let msgs = columns(&pool, "messages").await;
    assert!(msgs.contains(&"category".to_string()), "messages missing category");
    assert!(!msgs.contains(&"device_type".to_string()), "messages still has device_type");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn tags_check_and_trigger_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    let tags_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='tags'",
    )
    .fetch_one(&pool).await.unwrap();
    assert!(!tags_sql.contains("'device'"), "tags CHECK still allows 'device': {tags_sql}");
    let trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='keep_thing_memory_limit'",
    )
    .fetch_optional(&pool).await.unwrap();
    assert!(trigger.is_some(), "keep_thing_memory_limit missing");
    let old_trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='keep_device_memory_limit'",
    )
    .fetch_optional(&pool).await.unwrap();
    assert!(old_trigger.is_none());
    let _ = std::fs::remove_file(path);
}
