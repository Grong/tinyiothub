//! 派生属性三列迁移与查询（真实 DB，项目铁律）。
//! 实测 schema：devices 已于 20260825000001 改名 things（索引同步为
//! idx_things_parent_id）、thing_properties.device_id 改名 thing_id。

use tinyiothub_core::models::thing_property::CreateThingPropertyRequest;
use tinyiothub_storage::Db;

#[sqlx::test]
async fn derived_columns_default_and_roundtrip(pool: sqlx::SqlitePool) {
    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .unwrap();

    // 0) PRAGMA 自证：三列已就位
    let cols: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('thing_properties')")
        .fetch_all(&pool)
        .await
        .unwrap();
    let cols: Vec<String> = cols.into_iter().map(|(c,)| c).collect();
    for col in ["source", "expression", "refs"] {
        assert!(cols.contains(&col.to_string()), "缺列 {col}: {cols:?}");
    }

    // 前置：一个 thing（FK）
    sqlx::query("INSERT INTO things (id, name) VALUES ('t1', 'demo')")
        .execute(&pool)
        .await
        .unwrap();

    // 1) 插入普通属性（不带 source）→ 读回 source='template'（迁移默认值）
    sqlx::query("INSERT INTO thing_properties (id, thing_id, name) VALUES ('p1', 't1', 'temp')")
        .execute(&pool)
        .await
        .unwrap();
    let (source,): (String,) = sqlx::query_as("SELECT source FROM thing_properties WHERE id = 'p1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(source, "template");

    // 2) 经请求路径插入派生属性（expression + refs）
    //    → find_derived_properties 恰好返回它
    let db = Db::new(pool.clone());
    let created = db
        .create_thing_properties_batch(&[CreateThingPropertyRequest {
            thing_id: "t1".to_string(),
            name: "total_power".to_string(),
            display_name: None,
            description: None,
            data_type: Some("float".to_string()),
            unit: None,
            min_value: None,
            max_value: None,
            default_value: None,
            is_read_only: None,
            expression: Some("{电表1.用电数}".to_string()),
            source: Some("derived".to_string()),
            refs: Some(r#"[{"thing_id":"t1","property_name":"用电数"}]"#.to_string()),
        }])
        .await
        .unwrap();
    assert_eq!(created.len(), 1);
    let refs_json = r#"[{"thing_id":"m1","property_name":"用电数"}]"#;
    sqlx::query("UPDATE thing_properties SET refs = ? WHERE id = ?")
        .bind(refs_json)
        .bind(&created[0].id)
        .execute(&pool)
        .await
        .unwrap();

    let derived = db.find_derived_properties().await.unwrap();
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0].name, "total_power");
    assert_eq!(derived[0].expression.as_deref(), Some("{电表1.用电数}"));
    assert_eq!(derived[0].refs.as_deref(), Some(refs_json));
    // 普通属性（expression 为 NULL）不混入
    assert!(derived.iter().all(|p| p.id != "p1"));

    // 3) PRAGMA index_list(things) 含 idx_things_parent_id
    //    （devices 已改名 things；索引由 20260825000001 创建）
    let idx: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_index_list('things')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        idx.iter().any(|(n,)| n == "idx_things_parent_id"),
        "缺 idx_things_parent_id: {idx:?}"
    );
}
