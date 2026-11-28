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
    assert_eq!(templates, 17);

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
        // 防漂移（Task 9 fix round 1）：种子副本须含园区指标骨架，
        // 与 templates/builtin/scenes/smart_campus.json 保持一致
        if id == "builtin_smart_campus" {
            let names: Vec<&str> = t.properties.iter().map(|p| p.name.as_str()).collect();
            for expected in ["total_power", "total_water", "device_count"] {
                assert!(
                    names.contains(&expected),
                    "builtin_smart_campus 种子副本缺少园区指标骨架属性 {expected}（现有: {names:?}）"
                );
            }
        }
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
async fn seed_demo_creates_utility_and_safety_scenario_devices() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();

    let devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM things WHERE id IN (
            'device-water-main', 'device-water-zone-a',
            'device-power-lighting', 'device-power-hvac',
            'device-light-workshop', 'device-light-corridor', 'device-light-outdoor',
            'device-fire-smoke-a', 'device-fire-heat-a',
            'device-security-entrance',
            'device-access-entrance', 'device-access-workshop'
        )",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(devices, 12);

    let energy_properties: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties
         WHERE thing_id IN (
            'device-power-lighting', 'device-power-hvac',
            'device-light-workshop', 'device-light-corridor', 'device-light-outdoor'
         ) AND name IN ('active_power', 'power', 'energy_today')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(energy_properties, 10);
}

#[tokio::test]
async fn seed_demo_adds_energy_and_safety_tags() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();

    let tags: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tags WHERE id IN (
            'tag-device-009', 'tag-device-010', 'tag-device-011',
            'tag-device-012', 'tag-device-013', 'tag-device-014',
            'tag-device-015', 'tag-device-016', 'tag-device-017'
        )",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(tags, 9);

    let bindings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tag_bindings WHERE id BETWEEN 'binding-020' AND 'binding-037'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(bindings, 18);

    let lighting: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tag_bindings b JOIN tags t ON t.id = b.tag_id
         WHERE b.target_id IN ('device-light-workshop', 'device-light-corridor', 'device-light-outdoor')
           AND t.name = '照明'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(lighting, 3);

    let hvac: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tag_bindings b JOIN tags t ON t.id = b.tag_id
         WHERE b.target_id = 'device-power-hvac' AND t.name IN ('电力仪表', '暖通')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(hvac, 2);
}

async fn demo_counts(pool: &sqlx::SqlitePool) -> (i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM things), (SELECT COUNT(*) FROM thing_properties), \
                (SELECT COUNT(*) FROM thing_actions), (SELECT COUNT(*) FROM tags), \
                (SELECT COUNT(*) FROM tag_bindings), (SELECT COUNT(*) FROM watch_rules), \
                (SELECT COUNT(*) FROM resources WHERE resource_type = 'document')",
    )
    .fetch_one(pool)
    .await
    .expect("demo counts")
}

#[tokio::test]
async fn seed_demo_is_idempotent() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    let first = demo_counts(db.pool()).await;
    seed::seed_demo(&db).await.unwrap();
    let second = demo_counts(db.pool()).await;
    assert_eq!(first, second, "demo seed must be idempotent");

    // The demo now has a hierarchy (spaces/buildings/line) plus devices.
    assert!(first.0 >= 40, "expected factory hierarchy + devices, got {first:?}");
    assert!(first.3 >= 30, "expected location + functional tags, got {first:?}");
    assert!(first.5 >= 4, "expected demo watch rules, got {first:?}");
    assert!(
        first.6 >= 60,
        "expected generated device/space documents, got {first:?}"
    );
}

#[tokio::test]
async fn seed_demo_builds_hierarchy_and_location_tags() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();

    let root_type: String = sqlx::query_scalar("SELECT thing_type FROM things WHERE id = 'site-demo-factory'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(root_type, "space");

    let building_type: String = sqlx::query_scalar("SELECT thing_type FROM things WHERE id = 'bldg-a'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(building_type, "building");

    let env_parent: Option<String> = sqlx::query_scalar("SELECT parent_id FROM things WHERE id = 'device-env-01'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(env_parent.as_deref(), Some("space-a-workshop"));

    let cold_parent: Option<String> = sqlx::query_scalar("SELECT parent_id FROM things WHERE id = 'device-cold-01'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(cold_parent.as_deref(), Some("space-cold"));

    let shaped: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE parent_id IS NOT NULL")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(shaped, 40, "all non-root demo things must be parented");

    let location_tags: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE id LIKE 'tag-loc-%'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(location_tags, 11);

    let cold_tag: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tag_bindings b JOIN tags t ON t.id = b.tag_id \
         WHERE b.target_id = 'device-cold-01' AND t.name = '冷库'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(cold_tag, 1);

    let new_devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM things WHERE driver_name = 'simulator' AND id IN ( \
            'device-env-03', 'device-env-04', 'device-fire-smoke-b', 'device-fire-heat-b', \
            'device-access-b', 'device-cam-03', 'device-light-warehouse', 'device-light-power', \
            'device-robot-02', 'device-security-warehouse')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(new_devices, 10);
}

#[tokio::test]
async fn seed_demo_generates_markdown_device_and_area_docs() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();

    let cold_doc: String = sqlx::query_scalar("SELECT content FROM resources WHERE id = 'doc-device-cold-01-manual'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(cold_doc.contains("设备档案"));
    assert!(cold_doc.contains("属性与量程"));
    assert!(cold_doc.contains("巡检与故障处置"));

    let area_doc: String = sqlx::query_scalar("SELECT content FROM resources WHERE id = 'doc-space-a-workshop-area'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(area_doc.contains("区域说明与巡检路线"));
    assert!(area_doc.contains("下辖设备"));
    assert!(area_doc.contains("冷链") || area_doc.contains("巡检路线"));
}
