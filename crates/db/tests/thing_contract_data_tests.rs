//! 契约数据迁移测试：tag_bindings.target_type 与 permissions device 系行。
//! 老库（baseline 终态 + 中间三迁移 + 旧契约样本数据）跑契约迁移后：
//! target_type 'device'→'thing'（'app' 行不动）、permissions device 系
//! 更名为 thing 系（id/name/resource_type）、role_permissions 引用同步、
//! user 系权限不动、FK 无违规。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 直建 baseline 终态、回放中间三迁移的 SQL 效果，再插入旧契约样本数据。
/// 与 thing_rename_data_tests 相同的隔离模式：pid + 原子序号。
async fn baseline_pool_with_samples() -> (SqlitePool, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("thing-contract-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    // 1. 直建 baseline 终态（不经 run_migrations）。
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // 2. 中间三迁移从未在此直建库上执行过——按版本序直接回放其 SQL,
    //    否则 things/thing_id 等重命名效果不存在。重命名迁移含
    //    DROP TABLE tags(FK ON 时会隐式 DELETE 并级联),需 FK OFF——
    //    与 run_migrations 行为一致;max_connections=1,pragma 作用于唯一连接。
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20260824000001_workspaces_trust_config_updated_at.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260824000002_agent_actions_tick_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260825000001_rename_device_to_thing.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // 3. 旧契约样本（迁移后 schema、迁移前契约值）:
    //    tag 绑定 'device' + 'app' 各一;permissions device 系 + user 系各一;
    //    role_permissions 引用 device 系权限。
    sqlx::query("INSERT INTO tags (id, type, name) VALUES ('t1', 'thing', 'tag1')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO things (id, name, thing_type) VALUES ('d1', 'demo', 'device')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tag_bindings (id, tag_id, target_id, target_type) VALUES ('b1', 't1', 'd1', 'device')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tag_bindings (id, tag_id, target_id, target_type) VALUES ('b2', 't1', 'd1', 'app')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO permissions (id, name, resource_type, action) VALUES ('perm-device-read', 'device:read', 'device', 'read')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO permissions (id, name, resource_type, action) VALUES ('perm-user-read', 'user:read', 'user', 'read')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO roles (id, name) VALUES ('r1', 'role1')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_permissions (id, role_id, permission_id) VALUES ('rp1', 'r1', 'perm-device-read')")
        .execute(&pool)
        .await
        .unwrap();

    (pool, path)
}

/// 把 baseline + 中间三迁移标记为已应用（checksum 取自嵌入迁移集，
/// 否则 sqlx 校验期 VersionMismatch),再跑 run_migrations——
/// 只有契约数据迁移实际执行。与 thing_rename_data_tests 相同的标记法。
async fn mark_applied_and_migrate(pool: &SqlitePool) {
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
    .execute(pool)
    .await
    .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    for version in [20260819000001i64, 20260824000001, 20260824000002, 20260825000001] {
        let m = migrator
            .iter()
            .find(|m| m.version == version)
            .unwrap_or_else(|| panic!("migration {version} missing from embedded set"));
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(m.version)
        .bind(m.description.as_ref())
        .bind(m.checksum.as_ref())
        .execute(pool)
        .await
        .unwrap();
    }

    tinyiothub_storage::migrations::run_migrations(pool).await.unwrap();
}

#[tokio::test]
async fn contract_data_migrated() {
    let (pool, path) = baseline_pool_with_samples().await;

    mark_applied_and_migrate(&pool).await;

    let tt: String = sqlx::query_scalar("SELECT target_type FROM tag_bindings WHERE id='b1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tt, "thing");
    let app: String = sqlx::query_scalar("SELECT target_type FROM tag_bindings WHERE id='b2'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(app, "app", "非 device 行不得受影响");
    let perm: (String, String, String) =
        sqlx::query_as("SELECT id, name, resource_type FROM permissions WHERE action='read' AND resource_type='thing'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        perm,
        (
            "perm-thing-read".to_string(),
            "thing:read".to_string(),
            "thing".to_string()
        )
    );
    let rp: String = sqlx::query_scalar("SELECT permission_id FROM role_permissions WHERE id='rp1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rp, "perm-thing-read");
    let user_perm: String = sqlx::query_scalar("SELECT name FROM permissions WHERE id='perm-user-read'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(user_perm, "user:read", "user 系权限不得受影响");
    let fk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fk, 0);

    drop(pool);
    let _ = std::fs::remove_file(path);
}
