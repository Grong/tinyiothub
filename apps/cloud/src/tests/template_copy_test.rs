//! 模板属性/命令拷贝回归（Fix A，2026-10-20）：
//! ①组合模板（场景包，device_info 内嵌场景 JSON，properties 列='[]'）建物时
//!   从 device_info 提取 properties/commands 骨架；
//! ②单体模板 properties 列是 snake_case 键 + i18n display_name 对象时，
//!   字段不退化（data_type/is_read_only/display_name 保留）。
//! 真实 DB（项目铁律：集成测试 sqlx 真实库，禁 mock）。

use sqlx::SqlitePool;

use crate::domains::thing::service::ThingService;
use crate::domains::thing::types::CreateThingRequest;
use crate::test_utils::setup_test_app_with_pool;

/// 场景包模板（组合模板）：properties 独立列为空，骨架在 device_info 场景 JSON 中。
const SCENE_TEMPLATE_JSON: &str = r#"{
  "name": "test_campus",
  "display_name": {"zh": "测试园区", "en": "Test Campus"},
  "version": "1.0.0",
  "category": "scenes",
  "device_info": {"default_name_pattern": "{scene_name}", "required_fields": []},
  "properties": [
    {"name": "total_power", "display_name": {"zh": "总用电", "en": "Total Power"},
     "data_type": "number", "unit": "kWh", "is_read_only": true, "is_required": false},
    {"name": "device_count", "display_name": {"zh": "设备数量", "en": "Device Count"},
     "data_type": "int", "unit": "个", "is_read_only": true, "is_required": false}
  ],
  "commands": [
    {"name": "refresh", "display_name": {"zh": "刷新", "en": "Refresh"}, "is_required": false}
  ],
  "children": [
    {"key": "building", "category": "building",
     "device_info": {"default_name_pattern": "{index}号楼"}}
  ]
}"#;

async fn insert_template(
    pool: &SqlitePool,
    id: &str,
    category: &str,
    device_info: &str,
    properties: &str,
    actions: &str,
) {
    // category 是 FK → template_categories(name)，先播种（export_template_test.rs:63 同款）
    sqlx::query("INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES (?, '{\"zh\":\"分类\"}', '2026-01-01 00:00:00')")
        .bind(category)
        .execute(pool)
        .await
        .expect("seed template category");
    sqlx::query(
        r#"INSERT INTO thing_templates
           (id, name, display_name, description, version, author, category, manufacturer,
            thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
            events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at)
           VALUES (?, ?, '{"zh":"t"}', NULL, '1.0.0', 'test', ?, 'test',
                   'space', NULL, NULL, '[]', ?, ?, ?, '[]', NULL, 1, 1, NULL,
                   datetime('now'), datetime('now'))"#,
    )
    .bind(id)
    .bind(id)
    .bind(category)
    .bind(device_info)
    .bind(properties)
    .bind(actions)
    .execute(pool)
    .await
    .expect("insert template");
}

fn create_req(name: &str, template_id: &str) -> CreateThingRequest {
    CreateThingRequest {
        name: name.to_string(),
        thing_type: Some("space".to_string()),
        category: None,
        description: None,
        parent_id: None,
        template_id: Some(template_id.to_string()),
        protocol_type: None,
        driver_name: None,
        tags: None,
        workspace_id: None,
    }
}

async fn props_of(
    pool: &SqlitePool,
    thing_id: &str,
) -> Vec<(String, Option<String>, Option<String>, i32, Option<String>)> {
    sqlx::query_as(
        "SELECT name, display_name, data_type, is_read_only, unit FROM thing_properties WHERE thing_id = ? ORDER BY name",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
    .expect("query props")
}

#[tokio::test]
async fn create_from_scene_pack_copies_skeleton_properties_and_commands() {
    let (_app, pool) = setup_test_app_with_pool().await;
    insert_template(&pool, "tpl_scene", "scenes", SCENE_TEMPLATE_JSON, "[]", "[]").await;

    let svc = ThingService::new(pool.clone());
    let created = svc
        .create_thing(&create_req("我的园区", "tpl_scene"), None)
        .await
        .expect("create thing from scene pack");

    let props = props_of(&pool, &created.id).await;
    assert_eq!(props.len(), 2, "场景包骨架应拷出 2 个属性，实际: {props:?}");

    let total_power = props.iter().find(|p| p.0 == "total_power").expect("total_power 存在");
    assert_eq!(total_power.1.as_deref(), Some("总用电"), "display_name 取 zh 不退化");
    assert_eq!(total_power.2.as_deref(), Some("number"), "data_type 保留非 'string'");
    assert_eq!(total_power.3, 1, "is_read_only 保留");
    assert_eq!(total_power.4.as_deref(), Some("kWh"), "unit 保留");

    assert!(props.iter().any(|p| p.0 == "device_count"), "device_count 存在");

    let commands: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name, display_name FROM thing_actions WHERE thing_id = ?")
            .bind(&created.id)
            .fetch_all(&pool)
            .await
            .expect("query actions");
    assert_eq!(commands.len(), 1, "场景包命令骨架应拷出");
    assert_eq!(commands[0].1.as_deref(), Some("刷新"));
}

#[tokio::test]
async fn create_from_entity_template_snake_case_fields_not_degraded() {
    let (_app, pool) = setup_test_app_with_pool().await;
    let entity_props = r#"[{"name":"water_usage","display_name":{"zh":"用水数","en":"Water Usage"},
        "data_type":"number","unit":"m³","min_value":0.0,"max_value":99999999.0,
        "default_value":"0.0","is_read_only":true,"is_required":true}]"#;
    let entity_acts = r#"[{"name":"read_all","display_name":{"zh":"读取全部"},"parameters":"{}","is_required":true}]"#;
    insert_template(
        &pool,
        "tpl_entity",
        "sensors",
        r#"{"default_name_pattern":"water_meter_{index}"}"#,
        entity_props,
        entity_acts,
    )
    .await;

    let svc = ThingService::new(pool.clone());
    let created = svc
        .create_thing(&create_req("水表A", "tpl_entity"), None)
        .await
        .expect("create thing from entity template");

    let props = props_of(&pool, &created.id).await;
    assert_eq!(props.len(), 1, "单体模板属性应拷出，实际: {props:?}");
    let (name, display_name, data_type, is_read_only, unit) = &props[0];
    assert_eq!(name, "water_usage");
    assert_eq!(
        display_name.as_deref(),
        Some("用水数"),
        "snake_case i18n 对象 display_name 取 zh"
    );
    assert_eq!(
        data_type.as_deref(),
        Some("number"),
        "snake_case data_type 不退化为 'string'"
    );
    assert_eq!(*is_read_only, 1, "snake_case is_read_only 保留");
    assert_eq!(unit.as_deref(), Some("m³"));

    let commands: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name, display_name FROM thing_actions WHERE thing_id = ?")
            .bind(&created.id)
            .fetch_all(&pool)
            .await
            .expect("query actions");
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].1.as_deref(),
        Some("读取全部"),
        "snake_case 命令 display_name 保留"
    );
}

#[tokio::test]
async fn create_from_entity_template_camel_case_still_works() {
    // 兼容性反向验证：camelCase 键的旧数据路径不回归。
    let (_app, pool) = setup_test_app_with_pool().await;
    let camel_props = r#"[{"name":"temp","displayName":"温度","dataType":"float","unit":"°C",
        "minValue":-40.0,"maxValue":125.0,"defaultValue":"25.0","isReadOnly":true}]"#;
    insert_template(
        &pool,
        "tpl_camel",
        "sensors",
        r#"{"default_name_pattern":"sensor_{index}"}"#,
        camel_props,
        "[]",
    )
    .await;

    let svc = ThingService::new(pool.clone());
    let created = svc
        .create_thing(&create_req("传感器A", "tpl_camel"), None)
        .await
        .expect("create thing from camelCase template");

    let props = props_of(&pool, &created.id).await;
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].1.as_deref(), Some("温度"));
    assert_eq!(props[0].2.as_deref(), Some("float"));
    assert_eq!(props[0].3, 1);
}
