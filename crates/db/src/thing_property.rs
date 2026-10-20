use sqlx::{FromRow, SqlitePool};

use crate::database::Db;
use tinyiothub_core::models::thing_property::*;
use tinyiothub_core::{generate_id, now_string};

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct ThingPropertyRow {
    id: String,
    thing_id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    data_type: Option<String>,
    unit: Option<String>,
    min_value: Option<f64>,
    max_value: Option<f64>,
    default_value: Option<String>,
    is_read_only: i32,
    source: String,
    expression: Option<String>,
    refs: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

impl From<ThingPropertyRow> for ThingProperty {
    fn from(row: ThingPropertyRow) -> Self {
        Self {
            id: row.id,
            thing_id: row.thing_id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            data_type: row.data_type,
            unit: row.unit,
            min_value: row.min_value,
            max_value: row.max_value,
            default_value: row.default_value,
            is_read_only: row.is_read_only,
            source: Some(row.source),
            expression: row.expression,
            refs: row.refs,
            created_at: row.created_at,
            updated_at: row.updated_at,
            current_value: None,
            alarm_status: None,
            derived_status: None,
        }
    }
}

/// Find a thing property by ID
pub(crate) async fn find_thing_property_by_id(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ThingProperty>, sqlx::Error> {
    let row = sqlx::query_as::<_, ThingPropertyRow>(
        r#"
        SELECT id, thing_id, name, display_name, description, data_type, unit,
               min_value, max_value, default_value, is_read_only, source, expression, refs,
               created_at, updated_at
        FROM thing_properties WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let mut property: Option<ThingProperty> = row.map(Into::into);
    if let Some(ref mut prop) = property {
        prop.clear_runtime_data();
    }

    Ok(property)
}

/// Find properties by thing ID
pub(crate) async fn find_thing_properties_by_thing_id(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Vec<ThingProperty>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ThingPropertyRow>(
        r#"
        SELECT id, thing_id, name, display_name, description, data_type, unit,
               min_value, max_value, default_value, is_read_only, source, expression, refs,
               created_at, updated_at
        FROM thing_properties WHERE thing_id = ?
        ORDER BY name
        "#,
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await?;

    let mut properties: Vec<ThingProperty> = rows.into_iter().map(Into::into).collect();
    for prop in &mut properties {
        prop.clear_runtime_data();
    }

    Ok(properties)
}

/// 全量派生属性（启动重建内存反向索引用，spec §二）。
pub(crate) async fn find_derived_properties(pool: &SqlitePool) -> Result<Vec<ThingProperty>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ThingPropertyRow>(
        r#"SELECT id, thing_id, name, display_name, description, data_type, unit,
                  min_value, max_value, default_value, is_read_only, source, expression, refs,
                  created_at, updated_at
           FROM thing_properties WHERE expression IS NOT NULL"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// 在调用方事务内批量插入物属性（场景实例化器专用），返回新建 id 列表。
/// 不自行 commit/rollback；公开入口 create_thing_properties_batch 是薄包装。
pub(crate) async fn create_thing_properties_batch_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    requests: &[CreateThingPropertyRequest],
) -> Result<Vec<String>, sqlx::Error> {
    let mut created_ids = Vec::new();

    for request in requests {
        let id = generate_id();
        let now = now_string();
        let is_read_only = request.is_read_only.unwrap_or(0);
        // source 显式写穿（Task 4 评审：派生创建必须落 'derived'，模板/手动路径
        // 不传则保持默认 'template'）；refs 不传保持 NULL（与模板拷贝存量一致），
        // 派生保存路径由调用方恒写非空 JSON。
        let source = request.source.as_deref().unwrap_or("template");

        sqlx::query(
            r#"
            INSERT INTO thing_properties (
                id, thing_id, name, display_name, description, data_type, unit,
                min_value, max_value, default_value, is_read_only, source, expression, refs,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.thing_id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&request.data_type)
        .bind(&request.unit)
        .bind(request.min_value)
        .bind(request.max_value)
        .bind(&request.default_value)
        .bind(is_read_only)
        .bind(source)
        .bind(&request.expression)
        .bind(&request.refs)
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await?;

        created_ids.push(id);
    }

    Ok(created_ids)
}

/// Batch create thing properties
pub(crate) async fn create_thing_properties_batch(
    pool: &SqlitePool,
    requests: &[CreateThingPropertyRequest],
) -> Result<Vec<ThingProperty>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let created_ids = create_thing_properties_batch_tx(&mut tx, requests).await?;
    tx.commit().await?;

    let mut results = Vec::new();
    for id in created_ids {
        if let Some(property) = find_thing_property_by_id(pool, &id).await? {
            results.push(property);
        }
    }

    Ok(results)
}

/// 更新属性定义（Task 8 管理面 PUT）：标量字段 None 保持不变（COALESCE）。
/// `derived` 三态（PUT expression 字段语义，X2 后端半）：
/// - `None` 缺省，不动 source/expression/refs/is_read_only。
/// - `Some(None)` 即 expression=null 解绑：清 expression/refs，source 回
///   'template'（模板拷贝/手动无法区分，统一 'template'），is_read_only 归 0
///   回普通可写属性；属性行与运行时当前值保留（只动定义列）。
/// - `Some(Some((expression, refs_json)))` 写穿派生三件套：source='derived'
///   且 is_read_only=1（派生校验已通过，refs 恒非空 JSON）。
///
/// 单事务两条 UPDATE；UNIQUE(thing_id, name) 冲突原样抛给调用方映 409。
pub(crate) async fn update_thing_property_definition(
    pool: &SqlitePool,
    id: &str,
    thing_id: &str,
    name: Option<&str>,
    display_name: Option<&str>,
    unit: Option<&str>,
    data_type: Option<&str>,
    derived: Option<Option<(&str, &str)>>,
) -> Result<u64, sqlx::Error> {
    let now = now_string();
    let mut tx = pool.begin().await?;
    let r = sqlx::query(
        r#"
        UPDATE thing_properties SET
            name = COALESCE(?, name),
            display_name = COALESCE(?, display_name),
            unit = COALESCE(?, unit),
            data_type = COALESCE(?, data_type),
            updated_at = ?
        WHERE id = ? AND thing_id = ?
        "#,
    )
    .bind(name)
    .bind(display_name)
    .bind(unit)
    .bind(data_type)
    .bind(&now)
    .bind(id)
    .bind(thing_id)
    .execute(&mut *tx)
    .await?;
    match derived {
        Some(Some((expression, refs_json))) => {
            sqlx::query(
                r#"
                UPDATE thing_properties SET
                    source = 'derived', expression = ?, refs = ?, is_read_only = 1, updated_at = ?
                WHERE id = ? AND thing_id = ?
                "#,
            )
            .bind(expression)
            .bind(refs_json)
            .bind(&now)
            .bind(id)
            .bind(thing_id)
            .execute(&mut *tx)
            .await?;
        }
        Some(None) => {
            sqlx::query(
                r#"
                UPDATE thing_properties SET
                    source = 'template', expression = NULL, refs = NULL, is_read_only = 0, updated_at = ?
                WHERE id = ? AND thing_id = ?
                "#,
            )
            .bind(&now)
            .bind(id)
            .bind(thing_id)
            .execute(&mut *tx)
            .await?;
        }
        None => {}
    }
    tx.commit().await?;
    Ok(r.rows_affected())
}

/// 删除属性定义（Task 8 管理面 DELETE），返回受影响行数（0 = 不存在）。
pub(crate) async fn delete_thing_property(pool: &SqlitePool, id: &str, thing_id: &str) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM thing_properties WHERE id = ? AND thing_id = ?")
        .bind(id)
        .bind(thing_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

impl Db {
    /// 按 ID 查物属性（清除运行时字段）。
    pub async fn find_thing_property_by_id(&self, id: &str) -> Result<Option<ThingProperty>, sqlx::Error> {
        find_thing_property_by_id(self.pool(), id).await
    }

    /// 按物 ID 列出属性（按名称排序，清除运行时字段）。
    pub async fn find_thing_properties_by_thing_id(&self, thing_id: &str) -> Result<Vec<ThingProperty>, sqlx::Error> {
        find_thing_properties_by_thing_id(self.pool(), thing_id).await
    }

    /// 全量派生属性（expression IS NOT NULL；启动重建内存反向索引用）。
    pub async fn find_derived_properties(&self) -> Result<Vec<ThingProperty>, sqlx::Error> {
        find_derived_properties(self.pool()).await
    }

    /// 批量创建物属性（内部事务，逐条回读）。
    pub async fn create_thing_properties_batch(
        &self,
        requests: &[CreateThingPropertyRequest],
    ) -> Result<Vec<ThingProperty>, sqlx::Error> {
        create_thing_properties_batch(self.pool(), requests).await
    }

    /// 更新属性定义（标量 COALESCE 保旧；derived 三态：None 不动 /
    /// Some(None) 解绑回退普通属性 / Some(Some((expression, refs_json))) 写穿派生三件套）。
    pub async fn update_thing_property_definition(
        &self,
        id: &str,
        thing_id: &str,
        name: Option<&str>,
        display_name: Option<&str>,
        unit: Option<&str>,
        data_type: Option<&str>,
        derived: Option<Option<(&str, &str)>>,
    ) -> Result<u64, sqlx::Error> {
        update_thing_property_definition(self.pool(), id, thing_id, name, display_name, unit, data_type, derived).await
    }

    /// 删除属性定义，返回受影响行数（0 = 不存在）。
    pub async fn delete_thing_property(&self, id: &str, thing_id: &str) -> Result<u64, sqlx::Error> {
        delete_thing_property(self.pool(), id, thing_id).await
    }

    /// 场景实例化器：在调用方事务内批量插入物属性，返回新建 id 列表。
    pub async fn create_thing_properties_batch_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        requests: &[CreateThingPropertyRequest],
    ) -> Result<Vec<String>, sqlx::Error> {
        create_thing_properties_batch_tx(tx, requests).await
    }

    /// 场景实例化器：事务内按 (thing_id, name) 查属性 id（告警规则 property_ref 解析用）。
    pub async fn find_thing_property_id_by_name_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        thing_id: &str,
        name: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT id FROM thing_properties WHERE thing_id = ? AND name = ?")
            .bind(thing_id)
            .bind(name)
            .fetch_optional(&mut **tx)
            .await
    }
}
