//! 派生表达式保存时校验：refs 解析/同 workspace 存在性/DAG 环（spec §二）。

use crate::domains::thing::service::derived_validation::validate_derived_expression;
use crate::test_utils::seed_test_workspace;

async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("migrations");
    pool
}

async fn insert_thing(pool: &sqlx::SqlitePool, id: &str, name: &str, workspace_id: &str) {
    sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES (?, ?, ?, '2025-01-01', '2025-01-01')")
        .bind(id)
        .bind(name)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("insert thing");
}

async fn insert_property(
    pool: &sqlx::SqlitePool,
    id: &str,
    thing_id: &str,
    name: &str,
    source: &str,
    expression: Option<&str>,
    refs: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO thing_properties (id, thing_id, name, source, expression, refs, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, '2025-01-01', '2025-01-01')",
    )
    .bind(id)
    .bind(thing_id)
    .bind(name)
    .bind(source)
    .bind(expression)
    .bind(refs)
    .execute(pool)
    .await
    .expect("insert property");
}

#[tokio::test]
async fn validate_ok_resolves_names_to_ids() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    insert_thing(&pool, "t-owner", "总表", "ws1").await;
    insert_thing(&pool, "t-meter1", "电表1", "ws1").await;
    insert_property(&pool, "p1", "t-meter1", "用电数", "template", None, None).await;

    let resolved = validate_derived_expression(&pool, "ws1", "t-owner", "{电表1.用电数} * 2")
        .await
        .expect("合法表达式应通过");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].thing_id, "t-meter1", "物名应解析为真实 id");
    assert_eq!(resolved[0].property_name, "用电数");
}

#[tokio::test]
async fn validate_rejects_unknown_thing_or_property() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    insert_thing(&pool, "t-owner", "总表", "ws1").await;
    insert_thing(&pool, "t-meter1", "电表1", "ws1").await;
    insert_property(&pool, "p1", "t-meter1", "用电数", "template", None, None).await;

    let err = validate_derived_expression(&pool, "ws1", "t-owner", "{电表1.不存在的属性} + {幽灵.x}")
        .await
        .expect_err("未知物/属性应拒绝");
    assert!(err.contains("无效引用"), "错误应含无效引用清单: {err}");
    assert!(err.contains("电表1 无属性 不存在的属性"), "{err}");
    assert!(err.contains("物 幽灵 不存在"), "{err}");
}

#[tokio::test]
async fn validate_rejects_cross_workspace_ref() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    seed_test_workspace(&pool, "tenant1", "ws2").await;
    insert_thing(&pool, "t-owner", "总表", "ws1").await;
    // 同名物只存在于 ws2（D3：跨 workspace 引用必须拒绝）
    insert_thing(&pool, "t-meter2", "电表1", "ws2").await;
    insert_property(&pool, "p2", "t-meter2", "用电数", "template", None, None).await;

    let err = validate_derived_expression(&pool, "ws1", "t-owner", "{电表1.用电数}")
        .await
        .expect_err("跨 workspace 引用应拒绝");
    assert!(err.contains("物 电表1 不存在"), "{err}");
}

#[tokio::test]
async fn validate_rejects_cycle() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    insert_thing(&pool, "t-a", "A", "ws1").await;
    insert_thing(&pool, "t-b", "B", "ws1").await;
    insert_property(&pool, "pa", "t-a", "total", "template", None, None).await;
    // B.x 已是派生：{A.total}（既有边 B → A）
    insert_property(
        &pool,
        "pb",
        "t-b",
        "x",
        "derived",
        Some("{A.total}"),
        Some(r#"[{"thing_id":"t-a","property_name":"total"}]"#),
    )
    .await;

    // 现在要把 A.total 改成派生 {B.x}（新边 A → B）→ A→B→A 成环
    let err = validate_derived_expression(&pool, "ws1", "t-a", "{B.x}")
        .await
        .expect_err("成环应拒绝");
    assert!(err.contains("引用成环"), "错误应含环路径: {err}");
}

#[tokio::test]
async fn validate_self_reference_rejected() {
    let pool = test_pool().await;
    seed_test_workspace(&pool, "tenant1", "ws1").await;
    insert_thing(&pool, "t-a", "A", "ws1").await;
    insert_property(&pool, "pa", "t-a", "total", "template", None, None).await;

    // owner 引用自己的属性 → 环长 1
    let err = validate_derived_expression(&pool, "ws1", "t-a", "{A.total} + 1")
        .await
        .expect_err("自引用应拒绝");
    assert!(err.contains("引用成环"), "{err}");
}
