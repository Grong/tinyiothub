//! Seed module tests: both tiers are idempotent and produce the expected
//! system/demo rows on a baseline-built pool.

use tinyiothub_storage::Db;
use tinyiothub_storage::seed;
use tinyiothub_storage::test_helpers;

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

    let templates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_templates WHERE is_builtin = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(templates, 16);

    let admin: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = 'admin')")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(admin);
}

/// seed SQL 内嵌拷贝校验（C1 回归）：内置场景包的 device_info 必须可解析，
/// 其所有 alarm_rules 的 condition 必须可反序列化为 AlarmCondition（实例化器
/// 落库时做同样解析），rule_type 必须在展开器允许集合内。
#[tokio::test]
async fn seed_system_builtin_scene_templates_have_valid_alarm_rules() {
    use tinyiothub_storage::alarm_rule::AlarmCondition;
    use tinyiothub_storage::scene_template::{ALLOWED_RULE_TYPES, SceneAlarmRule, SceneTemplateFile, ThingNodeDef};

    fn collect_alarm_rules(t: &SceneTemplateFile) -> Vec<&SceneAlarmRule> {
        fn walk<'a>(nodes: &'a [ThingNodeDef], out: &mut Vec<&'a SceneAlarmRule>) {
            for n in nodes {
                out.extend(n.alarm_rules.iter());
                walk(&n.children, out);
            }
        }
        let mut out: Vec<&SceneAlarmRule> = t.alarm_rules.iter().collect();
        walk(&t.children, &mut out);
        out
    }

    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();

    for id in ["builtin_smart_campus", "builtin_smart_building", "builtin_smart_floor"] {
        let device_info: String = sqlx::query_scalar("SELECT device_info FROM thing_templates WHERE id = ?")
            .bind(id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        let t = SceneTemplateFile::from_json(&device_info).unwrap_or_else(|e| panic!("{id} device_info 解析失败: {e}"));
        assert!(!t.children.is_empty(), "{id} 必须有 children");
        // spec §4：seed 行 default_knowledge 列与根节点 knowledge 一致（非 NULL）
        let knowledge_col: Option<String> =
            sqlx::query_scalar("SELECT default_knowledge FROM thing_templates WHERE id = ?")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            knowledge_col.as_deref(),
            t.default_knowledge.as_deref(),
            "{id} default_knowledge 列须与根节点 knowledge 一致"
        );
        for rule in collect_alarm_rules(&t) {
            serde_json::from_value::<AlarmCondition>(rule.condition.clone())
                .unwrap_or_else(|e| panic!("{id} 规则「{}」condition 非法: {e}（{:?}）", rule.name, rule.condition));
            assert!(
                ALLOWED_RULE_TYPES.contains(&rule.rule_type.as_str()),
                "{id} 规则「{}」rule_type 非法: {}",
                rule.name,
                rule.rule_type
            );
        }
    }
}

#[tokio::test]
async fn seed_demo_creates_env01_with_properties() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties WHERE thing_id = 'device-env-01'")
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

    let things: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(things, 8);

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
