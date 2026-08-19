//! Seed module tests: both tiers are idempotent and produce the expected
//! system/demo rows on a baseline-built pool.

use tinyiothub_storage::seed;
use tinyiothub_storage::test_helpers;
use tinyiothub_storage::Db;

#[tokio::test]
async fn seed_system_is_idempotent_and_creates_default_workspace() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_system(&db).await.unwrap(); // 二次调用零变化
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn seed_system_creates_subscription_plans_and_builtin_templates() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();

    // tenants.plan_id FK target (Task 2 parked finding).
    let plans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscription_plans")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(plans, 4);

    let templates: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thing_templates WHERE is_builtin = 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(templates, 13);

    let admin: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = 'admin')")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(admin);
}

#[tokio::test]
async fn seed_demo_creates_env01_with_properties() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties WHERE device_id = 'device-env-01'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(n, 5);
}

#[tokio::test]
async fn seed_demo_is_idempotent() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();

    let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(devices, 8);

    let props: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(props, 35);

    let actions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_actions")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(actions, 15);
}
