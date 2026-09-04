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
            created_at: row.created_at,
            updated_at: row.updated_at,
            current_value: None,
            alarm_status: None,
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
               min_value, max_value, default_value, is_read_only, created_at, updated_at
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
               min_value, max_value, default_value, is_read_only, created_at, updated_at
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

        sqlx::query(
            r#"
            INSERT INTO thing_properties (
                id, thing_id, name, display_name, description, data_type, unit,
                min_value, max_value, default_value, is_read_only, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

impl Db {
    /// 按 ID 查物属性（清除运行时字段）。
    pub async fn find_thing_property_by_id(&self, id: &str) -> Result<Option<ThingProperty>, sqlx::Error> {
        find_thing_property_by_id(self.pool(), id).await
    }

    /// 按物 ID 列出属性（按名称排序，清除运行时字段）。
    pub async fn find_thing_properties_by_thing_id(&self, thing_id: &str) -> Result<Vec<ThingProperty>, sqlx::Error> {
        find_thing_properties_by_thing_id(self.pool(), thing_id).await
    }

    /// 批量创建物属性（内部事务，逐条回读）。
    pub async fn create_thing_properties_batch(
        &self,
        requests: &[CreateThingPropertyRequest],
    ) -> Result<Vec<ThingProperty>, sqlx::Error> {
        create_thing_properties_batch(self.pool(), requests).await
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
