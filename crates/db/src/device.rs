use sqlx::{QueryBuilder, Row, SqlitePool};

use crate::database::Db;
use tinyiothub_core::error::{Error, Result};
use tinyiothub_core::models::device::{
    CreateDeviceRequest, Device, DeviceStats, DeviceStatusUpdate, UpdateDeviceRequest,
};
use tinyiothub_core::{generate_id, now_string};

use crate::device_row_mapper;
use serde::{Deserialize, Serialize};

// ── 查询契约类型（自 core::repository::device 迁入，E6a）──

/// Criteria for querying devices
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceCriteria {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub workspace_id: Option<String>,
    pub search_text: Option<String>,
    pub tag_name: Option<String>,
    pub sort_by: DeviceSortBy,
    pub sort_order: DeviceSortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum DeviceSortBy {
    Name,
    #[default]
    CreatedAt,
    UpdatedAt,
    DeviceType,
    DriverName,
    State,
}

/// Sort order for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum DeviceSortOrder {
    Ascending,
    #[default]
    Descending,
}

impl DeviceCriteria {
    pub fn builder() -> DeviceCriteriaBuilder {
        DeviceCriteriaBuilder::new()
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    pub fn with_device_type(mut self, device_type: String) -> Self {
        self.device_type = Some(device_type);
        self
    }

    pub fn with_address(mut self, address: String) -> Self {
        self.address = Some(address);
        self
    }

    pub fn with_driver_name(mut self, driver_name: String) -> Self {
        self.driver_name = Some(driver_name);
        self
    }

    pub fn with_state(mut self, state: i32) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn with_template_id(mut self, template_id: String) -> Self {
        self.template_id = Some(template_id);
        self
    }

    pub fn with_workspace_id(mut self, workspace_id: String) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    pub fn with_search_text(mut self, text: String) -> Self {
        self.search_text = Some(text);
        self
    }

    pub fn with_tag_name(mut self, tag_name: String) -> Self {
        self.tag_name = Some(tag_name);
        self
    }

    pub fn with_sort(mut self, sort_by: DeviceSortBy, sort_order: DeviceSortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for DeviceCriteria
pub struct DeviceCriteriaBuilder {
    criteria: DeviceCriteria,
}

impl DeviceCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: DeviceCriteria::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.criteria.name = Some(name);
        self
    }

    pub fn display_name(mut self, display_name: String) -> Self {
        self.criteria.display_name = Some(display_name);
        self
    }

    pub fn device_type(mut self, device_type: String) -> Self {
        self.criteria.device_type = Some(device_type);
        self
    }

    pub fn address(mut self, address: String) -> Self {
        self.criteria.address = Some(address);
        self
    }

    pub fn driver_name(mut self, driver_name: String) -> Self {
        self.criteria.driver_name = Some(driver_name);
        self
    }

    pub fn state(mut self, state: i32) -> Self {
        self.criteria.state = Some(state);
        self
    }

    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.criteria.parent_id = Some(parent_id);
        self
    }

    pub fn template_id(mut self, template_id: String) -> Self {
        self.criteria.template_id = Some(template_id);
        self
    }

    pub fn workspace_id(mut self, workspace_id: String) -> Self {
        self.criteria.workspace_id = Some(workspace_id);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn tag_name(mut self, tag_name: String) -> Self {
        self.criteria.tag_name = Some(tag_name);
        self
    }

    pub fn sort_by(mut self, sort_by: DeviceSortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: DeviceSortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> DeviceCriteria {
        self.criteria
    }
}

impl Default for DeviceCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criteria_builder() {
        let criteria = DeviceCriteria::builder()
            .name("sensor-01".to_string())
            .device_type("temperature".to_string())
            .driver_name("modbus".to_string())
            .state(1)
            .sort_by(DeviceSortBy::Name)
            .sort_order(DeviceSortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.name, Some("sensor-01".to_string()));
        assert_eq!(criteria.device_type, Some("temperature".to_string()));
        assert_eq!(criteria.driver_name, Some("modbus".to_string()));
        assert_eq!(criteria.state, Some(1));
        assert!(matches!(criteria.sort_by, DeviceSortBy::Name));
        assert!(matches!(criteria.sort_order, DeviceSortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let criteria = DeviceCriteria::default()
            .with_name("sensor-02".to_string())
            .with_state(0)
            .with_sort(DeviceSortBy::State, DeviceSortOrder::Descending)
            .with_pagination(50, 10);

        assert_eq!(criteria.name, Some("sensor-02".to_string()));
        assert_eq!(criteria.state, Some(0));
        assert!(matches!(criteria.sort_by, DeviceSortBy::State));
        assert!(matches!(criteria.sort_order, DeviceSortOrder::Descending));
        assert_eq!(criteria.limit, Some(50));
        assert_eq!(criteria.offset, Some(10));
    }
}

// ──────────────────────────────────────────────
// 设备持久化自由函数（pub(crate)，pool 首参）
// workspace_scope: Some(ws) 时按租户作用域过滤（E6a 合并原
// TenantDeviceRepository 行为）；三处内部事务（update/delete_by_ids/
// create_batch/update_states_batch/update_status_batch/scoped create_batch）
// 保持函数内自包含。SQL 与原仓储实现逐字一致。
// ──────────────────────────────────────────────

async fn find_device_by_id_inner(pool: &SqlitePool, id: &str) -> Result<Option<Device>> {
    let sql = format!("SELECT {} FROM devices WHERE id = ?", device_row_mapper::SELECT_COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        Ok(Some(device_row_mapper::row_to_device(row)?))
    } else {
        Ok(None)
    }
}

async fn find_device_by_name_inner(pool: &SqlitePool, name: &str) -> Result<Option<Device>> {
    let sql = format!(
        "SELECT {} FROM devices WHERE name = ?",
        device_row_mapper::SELECT_COLUMNS
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        Ok(Some(device_row_mapper::row_to_device(row)?))
    } else {
        Ok(None)
    }
}

async fn find_devices_inner(pool: &SqlitePool, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(device_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM devices WHERE 1=1");
    if let Some(workspace_id) = &criteria.workspace_id {
        builder.push(" AND workspace_id = ").push_bind(workspace_id);
    }
    if let Some(name) = &criteria.name {
        builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
    }
    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }
    if let Some(device_type) = &criteria.device_type {
        builder.push(" AND device_type = ").push_bind(device_type);
    }
    if let Some(address) = &criteria.address {
        builder.push(" AND address LIKE ").push_bind(format!("%{}%", address));
    }
    if let Some(driver_name) = &criteria.driver_name {
        builder.push(" AND driver_name = ").push_bind(driver_name);
    }
    if let Some(state) = &criteria.state {
        builder.push(" AND state = ").push_bind(state);
    }
    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }
    if let Some(template_id) = &criteria.template_id {
        builder.push(" AND template_id = ").push_bind(template_id);
    }
    if let Some(search_text) = &criteria.search_text {
        let keywords: Vec<&str> = search_text.split_whitespace().collect();
        if !keywords.is_empty() {
            builder.push(" AND (");
            for (i, kw) in keywords.iter().enumerate() {
                let pattern = format!("%{}%", kw);
                if i > 0 {
                    builder.push(" OR ");
                }
                builder.push("(name LIKE ").push_bind(&pattern);
                builder.push(" OR display_name LIKE ").push_bind(&pattern);
                builder.push(" OR address LIKE ").push_bind(&pattern);
                builder.push(" OR description LIKE ").push_bind(&pattern);
                builder.push(" OR EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = devices.id AND tb.target_type = 'device' AND t.name LIKE ");
                builder.push_bind(&pattern);
                builder.push("))");
            }
            builder.push(")");
        }
    }
    if let Some(tag_name) = &criteria.tag_name {
        let pattern = format!("%{}%", tag_name);
        builder.push(" AND EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = devices.id AND tb.target_type = 'device' AND t.name LIKE ");
        builder.push_bind(&pattern);
        builder.push(")");
    }

    match criteria.sort_by {
        DeviceSortBy::Name => builder.push(" ORDER BY name"),
        DeviceSortBy::CreatedAt => builder.push(" ORDER BY created_at"),
        DeviceSortBy::UpdatedAt => builder.push(" ORDER BY updated_at"),
        DeviceSortBy::DeviceType => builder.push(" ORDER BY device_type"),
        DeviceSortBy::DriverName => builder.push(" ORDER BY driver_name"),
        DeviceSortBy::State => builder.push(" ORDER BY state"),
    };

    match criteria.sort_order {
        DeviceSortOrder::Ascending => builder.push(" ASC"),
        DeviceSortOrder::Descending => builder.push(" DESC"),
    };

    if let Some(limit) = criteria.limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    }
    if let Some(offset) = criteria.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn count_devices_inner(pool: &SqlitePool, criteria: &DeviceCriteria) -> Result<i64> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM devices WHERE 1=1");
    if let Some(workspace_id) = &criteria.workspace_id {
        builder.push(" AND workspace_id = ").push_bind(workspace_id);
    }
    if let Some(name) = &criteria.name {
        builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
    }
    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }
    if let Some(device_type) = &criteria.device_type {
        builder.push(" AND device_type = ").push_bind(device_type);
    }
    if let Some(address) = &criteria.address {
        builder.push(" AND address LIKE ").push_bind(format!("%{}%", address));
    }
    if let Some(driver_name) = &criteria.driver_name {
        builder.push(" AND driver_name = ").push_bind(driver_name);
    }
    if let Some(state) = &criteria.state {
        builder.push(" AND state = ").push_bind(state);
    }
    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }
    if let Some(template_id) = &criteria.template_id {
        builder.push(" AND template_id = ").push_bind(template_id);
    }
    if let Some(search_text) = &criteria.search_text {
        let keywords: Vec<&str> = search_text.split_whitespace().collect();
        if !keywords.is_empty() {
            builder.push(" AND (");
            for (i, kw) in keywords.iter().enumerate() {
                let pattern = format!("%{}%", kw);
                if i > 0 {
                    builder.push(" OR ");
                }
                builder.push("(name LIKE ").push_bind(&pattern);
                builder.push(" OR display_name LIKE ").push_bind(&pattern);
                builder.push(" OR address LIKE ").push_bind(&pattern);
                builder.push(" OR description LIKE ").push_bind(&pattern);
                builder.push(" OR EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = devices.id AND tb.target_type = 'device' AND t.name LIKE ");
                builder.push_bind(&pattern);
                builder.push("))");
            }
            builder.push(")");
        }
    }
    if let Some(tag_name) = &criteria.tag_name {
        let pattern = format!("%{}%", tag_name);
        builder.push(" AND EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = devices.id AND tb.target_type = 'device' AND t.name LIKE ");
        builder.push_bind(&pattern);
        builder.push(")");
    }

    let row = builder.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");
    Ok(count)
}

async fn create_device_inner(pool: &SqlitePool, request: &CreateDeviceRequest) -> Result<Device> {
    let id = generate_id();
    let now = now_string();

    sqlx::query(
        r#"
        INSERT INTO devices (
            id, name, display_name, device_type, address, description, position,
            driver_name, device_model, protocol_type, factory_name, linked_data,
            driver_options, state, parent_id, template_id,
            linked_gateway, fingerprint, workspace_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.device_type)
    .bind(&request.address)
    .bind(&request.description)
    .bind(&request.position)
    .bind(&request.driver_name)
    .bind(&request.device_model)
    .bind(&request.protocol_type)
    .bind(&request.factory_name)
    .bind(&request.linked_data)
    .bind(&request.driver_options)
    .bind(0i32)
    .bind(&request.parent_id)
    .bind(&request.template_id)
    .bind(&request.linked_gateway)
    .bind(&request.fingerprint)
    .bind(&request.workspace_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_device_by_id_inner(pool, &id).await?.ok_or(Error::NotFound)
}

async fn update_device_inner(pool: &SqlitePool, id: &str, request: &UpdateDeviceRequest) -> Result<Device> {
    let mut tx = pool.begin().await?;

    let mut builder = QueryBuilder::new("UPDATE devices SET ");
    let mut has_updates = false;
    let now = now_string();

    if let Some(name) = &request.name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("name = ").push_bind(name);
        has_updates = true;
    }
    if let Some(display_name) = &request.display_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("display_name = ").push_bind(display_name);
        has_updates = true;
    }
    if let Some(device_type) = &request.device_type {
        if has_updates {
            builder.push(", ");
        }
        builder.push("device_type = ").push_bind(device_type);
        has_updates = true;
    }
    if let Some(address) = &request.address {
        if has_updates {
            builder.push(", ");
        }
        builder.push("address = ").push_bind(address);
        has_updates = true;
    }
    if let Some(description) = &request.description {
        if has_updates {
            builder.push(", ");
        }
        builder.push("description = ").push_bind(description);
        has_updates = true;
    }
    if let Some(position) = &request.position {
        if has_updates {
            builder.push(", ");
        }
        builder.push("position = ").push_bind(position);
        has_updates = true;
    }
    if let Some(driver_name) = &request.driver_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("driver_name = ").push_bind(driver_name);
        has_updates = true;
    }
    if let Some(device_model) = &request.device_model {
        if has_updates {
            builder.push(", ");
        }
        builder.push("device_model = ").push_bind(device_model);
        has_updates = true;
    }
    if let Some(protocol_type) = &request.protocol_type {
        if has_updates {
            builder.push(", ");
        }
        builder.push("protocol_type = ").push_bind(protocol_type);
        has_updates = true;
    }
    if let Some(factory_name) = &request.factory_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("factory_name = ").push_bind(factory_name);
        has_updates = true;
    }
    if let Some(linked_data) = &request.linked_data {
        if has_updates {
            builder.push(", ");
        }
        builder.push("linked_data = ").push_bind(linked_data);
        has_updates = true;
    }
    if let Some(linked_gateway) = &request.linked_gateway {
        if has_updates {
            builder.push(", ");
        }
        builder.push("linked_gateway = ").push_bind(linked_gateway);
        has_updates = true;
    }
    if let Some(fingerprint) = &request.fingerprint {
        if has_updates {
            builder.push(", ");
        }
        builder.push("fingerprint = ").push_bind(fingerprint);
        has_updates = true;
    }
    if let Some(driver_options) = &request.driver_options {
        if has_updates {
            builder.push(", ");
        }
        builder.push("driver_options = ").push_bind(driver_options);
        has_updates = true;
    }
    if let Some(state) = &request.state {
        if has_updates {
            builder.push(", ");
        }
        builder.push("state = ").push_bind(state);
        has_updates = true;
    }
    if let Some(parent_id) = &request.parent_id {
        if has_updates {
            builder.push(", ");
        }
        builder.push("parent_id = ").push_bind(parent_id);
        has_updates = true;
    }
    if let Some(template_id) = &request.template_id {
        if has_updates {
            builder.push(", ");
        }
        builder.push("template_id = ").push_bind(template_id);
        has_updates = true;
    }

    if !has_updates {
        return find_device_by_id_inner(pool, id).await?.ok_or(Error::NotFound);
    }

    builder.push(", updated_at = ").push_bind(&now);
    builder.push(" WHERE id = ").push_bind(id);

    let result = builder.build().execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    let sql = format!("SELECT {} FROM devices WHERE id = ?", device_row_mapper::SELECT_COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_one(&mut *tx)
        .await;

    tx.commit().await?;

    match row {
        Ok(row) => device_row_mapper::row_to_device(row),
        Err(_) => Err(Error::NotFound),
    }
}

async fn delete_device_inner(pool: &SqlitePool, id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

async fn delete_devices_by_ids_inner(pool: &SqlitePool, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut builder = QueryBuilder::new("DELETE FROM devices WHERE id IN (");
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let result = builder.build().execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

async fn create_devices_batch_inner(pool: &SqlitePool, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>> {
    if requests.is_empty() {
        return Ok(vec![]);
    }

    let mut tx = pool.begin().await?;
    let mut created_devices = Vec::new();
    let now = now_string();

    for request in requests {
        let id = generate_id();

        sqlx::query(
            r#"
            INSERT INTO devices (
                id, name, display_name, device_type, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, template_id,
                linked_gateway, fingerprint, workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.device_type)
        .bind(&request.address)
        .bind(&request.description)
        .bind(&request.position)
        .bind(&request.driver_name)
        .bind(&request.device_model)
        .bind(&request.protocol_type)
        .bind(&request.factory_name)
        .bind(&request.linked_data)
        .bind(&request.driver_options)
        .bind(0i32)
        .bind(&request.parent_id)
        .bind(&request.template_id)
        .bind(&request.linked_gateway)
        .bind(&request.fingerprint)
        .bind(&request.workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        let device = Device {
            id: id.clone(),
            name: request.name.clone(),
            display_name: request.display_name.clone(),
            device_type: request.device_type.clone(),
            address: request.address.clone(),
            description: request.description.clone(),
            position: request.position.clone(),
            driver_name: request.driver_name.clone(),
            device_model: request.device_model.clone(),
            protocol_type: request.protocol_type.clone(),
            factory_name: request.factory_name.clone(),
            linked_data: request.linked_data.clone(),
            driver_options: request.driver_options.clone(),
            status: tinyiothub_core::models::device::DeviceStatus::Offline,
            parent_id: request.parent_id.clone(),
            template_id: request.template_id.clone(),
            linked_gateway: request.linked_gateway.clone(),
            fingerprint: request.fingerprint.clone(),
            workspace_id: None,
            created_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            tags: None,
            properties: None,
            commands: None,
            last_heartbeat: None,
        };

        created_devices.push(device);
    }

    tx.commit().await?;
    Ok(created_devices)
}

async fn update_device_state_inner(pool: &SqlitePool, id: &str, state: i32) -> Result<()> {
    let now = now_string();
    let result = sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
        .bind(state)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

async fn update_device_states_batch_inner(pool: &SqlitePool, updates: &[(String, i32)]) -> Result<u64> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut total_affected = 0u64;
    let now = now_string();

    for (id, state) in updates {
        let result = sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
            .bind(state)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        total_affected += result.rows_affected();
    }

    tx.commit().await?;
    Ok(total_affected)
}

async fn update_device_enabled_status_inner(pool: &SqlitePool, id: &str, enabled: bool) -> Result<bool> {
    let state = if enabled { 1 } else { 0 };
    match update_device_state_inner(pool, id, state).await {
        Ok(()) => Ok(true),
        Err(Error::NotFound) => Ok(false),
        Err(e) => Err(e),
    }
}

async fn find_device_children_inner(pool: &SqlitePool, parent_id: &str) -> Result<Vec<Device>> {
    let sql = format!(
        "SELECT {} FROM devices WHERE parent_id = ? ORDER BY name",
        device_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(parent_id)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn find_devices_by_template_id_inner(pool: &SqlitePool, template_id: &str) -> Result<Vec<Device>> {
    let sql = format!(
        "SELECT {} FROM devices WHERE template_id = ? ORDER BY name",
        device_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(template_id)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn find_devices_by_driver_name_inner(pool: &SqlitePool, driver_name: &str) -> Result<Vec<Device>> {
    let sql = format!(
        "SELECT {} FROM devices WHERE driver_name = ? ORDER BY name",
        device_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(driver_name)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn find_devices_by_linked_gateway_inner(pool: &SqlitePool, linked_gateway: &str) -> Result<Vec<Device>> {
    let sql = format!(
        "SELECT {} FROM devices WHERE linked_gateway = ? ORDER BY created_at DESC",
        device_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(linked_gateway)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn device_exists_by_name_inner(pool: &SqlitePool, name: &str) -> Result<bool> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM devices WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get("count");
    Ok(count > 0)
}

async fn find_devices_by_ids_inner(pool: &SqlitePool, ids: &[String]) -> Result<Vec<Device>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(device_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM devices WHERE id IN (");
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

async fn find_devices_with_filters_inner(
    pool: &SqlitePool,
    enabled: Option<bool>,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<Vec<Device>> {
    let mut criteria = DeviceCriteria {
        limit: Some(page_size),
        offset: Some((page.saturating_sub(1)) * page_size),
        ..Default::default()
    };

    if let Some(enabled) = enabled {
        criteria.state = Some(if enabled { 1 } else { 0 });
    }

    if let Some(search) = search {
        criteria.search_text = Some(search.to_string());
    }

    find_devices_inner(pool, &criteria).await
}

async fn update_device_status_batch_inner(pool: &SqlitePool, updates: &[DeviceStatusUpdate]) -> Result<u64> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut total_affected = 0u64;

    for update in updates {
        let result = sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
            .bind(update.state)
            .bind(&update.updated_at)
            .bind(&update.device_id)
            .execute(&mut *tx)
            .await?;
        total_affected += result.rows_affected();
    }

    tx.commit().await?;
    Ok(total_affected)
}

// ── 租户作用域辅助（E6a 合并自 TenantDeviceRepository）──

/// Check if a device belongs to this workspace
async fn device_belongs_to_workspace(pool: &SqlitePool, ws: &str, device_id: &str) -> Result<bool> {
    let result: Option<(String,)> = sqlx::query_as("SELECT workspace_id FROM devices WHERE id = ?")
        .bind(device_id)
        .fetch_optional(pool)
        .await?;

    match result {
        Some((workspace_id,)) => Ok(workspace_id == ws),
        None => Ok(false), // Device doesn't exist
    }
}

/// Filter device IDs to only those belonging to this workspace
async fn filter_device_ids_by_workspace(pool: &SqlitePool, ws: &str, ids: &[String]) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Use QueryBuilder to avoid lifetime issues with dynamic SQL
    let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
        sqlx::QueryBuilder::new("SELECT id FROM devices WHERE workspace_id = ");
    query_builder.push_bind(ws);
    query_builder.push(" AND id IN (");

    let mut separated = query_builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    query_builder.push(")");

    let query = query_builder.build();
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>("id")).collect())
}

/// Filter device state updates to only those belonging to this workspace
async fn filter_device_state_updates_by_workspace(
    pool: &SqlitePool,
    ws: &str,
    updates: &[(String, i32)],
) -> Result<Vec<(String, i32)>> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();
    let filtered_ids = filter_device_ids_by_workspace(pool, ws, &ids).await?;

    // Create a set for fast lookup
    let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

    let filtered_updates: Vec<(String, i32)> = updates
        .iter()
        .filter(|(id, _)| filtered_set.contains(id))
        .cloned()
        .collect();

    Ok(filtered_updates)
}

/// Filter device status updates to only those belonging to this workspace
async fn filter_device_status_updates_by_workspace(
    pool: &SqlitePool,
    ws: &str,
    updates: &[DeviceStatusUpdate],
) -> Result<Vec<DeviceStatusUpdate>> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = updates.iter().map(|update| update.device_id.clone()).collect();
    let filtered_ids = filter_device_ids_by_workspace(pool, ws, &ids).await?;

    // Create a set for fast lookup
    let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

    let filtered_updates: Vec<DeviceStatusUpdate> = updates
        .iter()
        .filter(|update| filtered_set.contains(&update.device_id))
        .cloned()
        .collect();

    Ok(filtered_updates)
}

// ── 作用域分发层（workspace_scope = Some 时的行为与原
// for_workspace 作用域仓储的公开方法逐字一致）──

pub(crate) async fn find_device_by_id(pool: &SqlitePool, ws: Option<&str>, id: &str) -> Result<Option<Device>> {
    let Some(ws) = ws else {
        return find_device_by_id_inner(pool, id).await;
    };
    // Verify device belongs to this workspace
    if !device_belongs_to_workspace(pool, ws, id).await? {
        return Ok(None);
    }

    find_device_by_id_inner(pool, id).await
}

pub(crate) async fn find_device_by_name(pool: &SqlitePool, ws: Option<&str>, name: &str) -> Result<Option<Device>> {
    let Some(ws) = ws else {
        return find_device_by_name_inner(pool, name).await;
    };
    let criteria = DeviceCriteria::default()
        .with_name(name.to_string())
        .with_workspace_id(ws.to_string());
    let devices = find_devices_inner(pool, &criteria).await?;
    Ok(devices.into_iter().next())
}

pub(crate) async fn find_devices(pool: &SqlitePool, ws: Option<&str>, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_devices_inner(pool, criteria).await;
    };
    let mut criteria = criteria.clone();
    criteria.workspace_id = Some(ws.to_string());
    find_devices_inner(pool, &criteria).await
}

pub(crate) async fn count_devices(pool: &SqlitePool, ws: Option<&str>, criteria: &DeviceCriteria) -> Result<i64> {
    let Some(ws) = ws else {
        return count_devices_inner(pool, criteria).await;
    };
    let mut criteria = criteria.clone();
    criteria.workspace_id = Some(ws.to_string());
    count_devices_inner(pool, &criteria).await
}

pub(crate) async fn create_device(pool: &SqlitePool, ws: Option<&str>, request: &CreateDeviceRequest) -> Result<Device> {
    let Some(ws) = ws else {
        return create_device_inner(pool, request).await;
    };
    let id = generate_id();
    let now = now_string();

    // Insert device with workspace_id
    sqlx::query(
        r#"
        INSERT INTO devices (
            id, name, display_name, device_type, address, description, position,
            driver_name, device_model, protocol_type, factory_name, linked_data,
            driver_options, state, parent_id, template_id, linked_gateway, fingerprint,
            workspace_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.device_type)
    .bind(&request.address)
    .bind(&request.description)
    .bind(&request.position)
    .bind(&request.driver_name)
    .bind(&request.device_model)
    .bind(&request.protocol_type)
    .bind(&request.factory_name)
    .bind(&request.linked_data)
    .bind(&request.driver_options)
    .bind(0i32) // default state
    .bind(&request.parent_id)
    .bind(&request.template_id)
    .bind(&request.linked_gateway)
    .bind(&request.fingerprint)
    .bind(ws)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Fetch the created device
    find_device_by_id(pool, Some(ws), &id).await?.ok_or_else(|| {
        tinyiothub_core::error::Error::InvalidArgument(format!("Failed to find created device with id {}", id))
    })
}

pub(crate) async fn update_device(
    pool: &SqlitePool,
    ws: Option<&str>,
    id: &str,
    request: &UpdateDeviceRequest,
) -> Result<Device> {
    let Some(_ws) = ws else {
        return update_device_inner(pool, id, request).await;
    };
    // Verify device belongs to this workspace before updating
    let device = find_device_by_id(pool, ws, id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    update_device_inner(pool, id, request).await
}

pub(crate) async fn delete_device(pool: &SqlitePool, ws: Option<&str>, id: &str) -> Result<u64> {
    let Some(_ws) = ws else {
        return delete_device_inner(pool, id).await;
    };
    // Verify device belongs to this workspace before deleting
    let device = find_device_by_id(pool, ws, id).await?;
    if device.is_none() {
        return Ok(0); // Already doesn't exist in this workspace
    }

    delete_device_inner(pool, id).await
}

pub(crate) async fn delete_devices_by_ids(pool: &SqlitePool, ws: Option<&str>, ids: &[String]) -> Result<u64> {
    let Some(ws) = ws else {
        return delete_devices_by_ids_inner(pool, ids).await;
    };
    // Filter IDs to only those belonging to this workspace
    let filtered_ids = filter_device_ids_by_workspace(pool, ws, ids).await?;
    if filtered_ids.is_empty() {
        return Ok(0);
    }
    delete_devices_by_ids_inner(pool, &filtered_ids).await
}

pub(crate) async fn create_devices_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    requests: &[CreateDeviceRequest],
) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return create_devices_batch_inner(pool, requests).await;
    };
    if requests.is_empty() {
        return Ok(vec![]);
    }

    let mut tx = pool.begin().await?;
    let mut device_ids = Vec::new();
    let now = now_string();

    for request in requests {
        let id = generate_id();
        device_ids.push(id.clone());

        sqlx::query(
            r#"
            INSERT INTO devices (
                id, name, display_name, device_type, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, template_id, linked_gateway, fingerprint,
                workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.device_type)
        .bind(&request.address)
        .bind(&request.description)
        .bind(&request.position)
        .bind(&request.driver_name)
        .bind(&request.device_model)
        .bind(&request.protocol_type)
        .bind(&request.factory_name)
        .bind(&request.linked_data)
        .bind(&request.driver_options)
        .bind(0i32) // default state
        .bind(&request.parent_id)
        .bind(&request.template_id)
        .bind(&request.linked_gateway)
        .bind(&request.fingerprint)
        .bind(ws)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Fetch created devices
    find_devices_by_ids(pool, Some(ws), &device_ids).await
}

pub(crate) async fn update_device_state(pool: &SqlitePool, ws: Option<&str>, id: &str, state: i32) -> Result<()> {
    let Some(ws) = ws else {
        return update_device_state_inner(pool, id, state).await;
    };
    let device = find_device_by_id(pool, Some(ws), id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
            "Device with id {} not found in workspace {}",
            id, ws
        )));
    }

    update_device_state_inner(pool, id, state).await
}

pub(crate) async fn update_device_states_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    updates: &[(String, i32)],
) -> Result<u64> {
    let Some(ws) = ws else {
        return update_device_states_batch_inner(pool, updates).await;
    };
    // Filter updates to only devices in this workspace
    let filtered_updates = filter_device_state_updates_by_workspace(pool, ws, updates).await?;
    if filtered_updates.is_empty() {
        return Ok(0);
    }
    update_device_states_batch_inner(pool, &filtered_updates).await
}

pub(crate) async fn update_device_enabled_status(
    pool: &SqlitePool,
    ws: Option<&str>,
    id: &str,
    enabled: bool,
) -> Result<bool> {
    let Some(ws) = ws else {
        return update_device_enabled_status_inner(pool, id, enabled).await;
    };
    let device = find_device_by_id(pool, Some(ws), id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
            "Device with id {} not found in workspace {}",
            id, ws
        )));
    }

    update_device_enabled_status_inner(pool, id, enabled).await
}

pub(crate) async fn find_device_children(pool: &SqlitePool, ws: Option<&str>, parent_id: &str) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_device_children_inner(pool, parent_id).await;
    };
    // Verify parent belongs to this workspace
    if !device_belongs_to_workspace(pool, ws, parent_id).await? {
        return Ok(vec![]);
    }

    let criteria = DeviceCriteria::default()
        .with_parent_id(parent_id.to_string())
        .with_workspace_id(ws.to_string());
    find_devices_inner(pool, &criteria).await
}

pub(crate) async fn find_devices_by_template_id(
    pool: &SqlitePool,
    ws: Option<&str>,
    template_id: &str,
) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_devices_by_template_id_inner(pool, template_id).await;
    };
    let criteria = DeviceCriteria::default()
        .with_template_id(template_id.to_string())
        .with_workspace_id(ws.to_string());
    find_devices_inner(pool, &criteria).await
}

pub(crate) async fn find_devices_by_driver_name(
    pool: &SqlitePool,
    ws: Option<&str>,
    driver_name: &str,
) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_devices_by_driver_name_inner(pool, driver_name).await;
    };
    let criteria = DeviceCriteria::default()
        .with_driver_name(driver_name.to_string())
        .with_workspace_id(ws.to_string());
    find_devices_inner(pool, &criteria).await
}

pub(crate) async fn find_devices_by_linked_gateway(
    pool: &SqlitePool,
    ws: Option<&str>,
    linked_gateway: &str,
) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_devices_by_linked_gateway_inner(pool, linked_gateway).await;
    };
    let criteria = DeviceCriteria::default().with_workspace_id(ws.to_string());
    let all = find_devices_inner(pool, &criteria).await?;
    Ok(all
        .into_iter()
        .filter(|d| d.linked_gateway.as_deref() == Some(linked_gateway))
        .collect())
}

/// 按 id 检查设备是否存在（device_traces 等领域的外键前置检查；自 cloud trace_repository 迁入）。
pub(crate) async fn device_exists_by_id(pool: &SqlitePool, id: &str) -> Result<bool> {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(count)) => Ok(count > 0),
        Ok(None) => Ok(false),
        Err(e) => {
            tracing::debug!("Failed to check device existence for {}: {}", id, e);
            Err(Error::IOError(format!("DB error: {}", e)))
        }
    }
}

pub(crate) async fn device_exists_by_name(pool: &SqlitePool, ws: Option<&str>, name: &str) -> Result<bool> {
    let Some(_ws) = ws else {
        return device_exists_by_name_inner(pool, name).await;
    };
    // Check within this workspace
    let criteria = DeviceCriteria::builder().name(name.to_string()).build();

    let count = count_devices(pool, ws, &criteria).await?;
    Ok(count > 0)
}

pub(crate) async fn find_devices_by_ids(pool: &SqlitePool, ws: Option<&str>, ids: &[String]) -> Result<Vec<Device>> {
    let Some(ws) = ws else {
        return find_devices_by_ids_inner(pool, ids).await;
    };
    // Filter IDs to only those belonging to this workspace
    let filtered_ids = filter_device_ids_by_workspace(pool, ws, ids).await?;
    if filtered_ids.is_empty() {
        return Ok(vec![]);
    }
    find_devices_by_ids_inner(pool, &filtered_ids).await
}

pub(crate) async fn find_devices_with_filters(
    pool: &SqlitePool,
    ws: Option<&str>,
    enabled: Option<bool>,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<Vec<Device>> {
    let Some(_ws) = ws else {
        return find_devices_with_filters_inner(pool, enabled, search, page, page_size).await;
    };

    let mut criteria = DeviceCriteria::builder()
        .limit(page_size)
        .offset((page.saturating_sub(1)) * page_size)
        .build();

    if let Some(enabled) = enabled {
        // Map enabled boolean to state (1 for enabled, 0 for disabled)
        criteria.state = Some(if enabled { 1 } else { 0 });
    }

    if let Some(search) = search {
        criteria.search_text = Some(search.to_string());
    }

    find_devices(pool, ws, &criteria).await
}

pub(crate) async fn update_device_status_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    updates: &[DeviceStatusUpdate],
) -> Result<u64> {
    let Some(ws) = ws else {
        return update_device_status_batch_inner(pool, updates).await;
    };
    // Filter updates to only devices in this workspace
    let filtered_updates = filter_device_status_updates_by_workspace(pool, ws, updates).await?;
    if filtered_updates.is_empty() {
        return Ok(0);
    }
    update_device_status_batch_inner(pool, &filtered_updates).await
}

// ── Task 7 收编：cloud workspace 删除守卫的设备计数（devices 表归本领域）──

pub(crate) async fn count_devices_by_workspace(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

// ── Task 8 收编：cloud driver/legacy query_service_impl 的设备统计查询
// （devices 表归本领域；SQL 逐字迁移）──

pub(crate) async fn device_stats_overview(pool: &SqlitePool) -> Result<DeviceStats> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total_devices,
            COUNT(CASE WHEN state = 1 THEN 1 END) as online_devices,
            COUNT(CASE WHEN state = 0 OR state = 3 THEN 1 END) as offline_devices,
            COUNT(CASE WHEN state = 2 THEN 1 END) as alarm_devices
        FROM devices
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(DeviceStats {
        total_devices: row.get("total_devices"),
        online_devices: row.get("online_devices"),
        offline_devices: row.get("offline_devices"),
        alarm_devices: row.get("alarm_devices"),
    })
}

pub(crate) async fn count_devices_by_type(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
        FROM devices
        GROUP BY device_type
        ORDER BY count DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Vec::new();
    for row in rows {
        let device_type: String = row.get("device_type");
        let count: i64 = row.get("count");
        stats.push((device_type, count));
    }
    Ok(stats)
}

pub(crate) async fn count_devices_by_driver(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(driver_name, 'Unknown') as driver_name, COUNT(*) as count
        FROM devices
        GROUP BY driver_name
        ORDER BY count DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Vec::new();
    for row in rows {
        let driver_name: String = row.get("driver_name");
        let count: i64 = row.get("count");
        stats.push((driver_name, count));
    }
    Ok(stats)
}

// ── 终审修复（F1）：cloud driver/legacy query_service_impl 的 4 处
// QueryBuilder 动态 SQL 迁入本领域（SQL 逐字迁移；返回 DTO 随迁，
// serde 属性保持不变）──

/// 设备状态分布（cloud device dashboard 用，自 cloud driver/legacy/types 迁入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusDistribution {
    /// 在线设备数
    pub online: i64,
    /// 离线设备数
    pub offline: i64,
    /// 故障设备数
    pub error: i64,
    /// 维护中设备数
    pub maintenance: i64,
}

/// 关键设备信息（cloud device dashboard 用，自 cloud driver/legacy/types 迁入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickDevice {
    /// 设备ID
    pub id: String,
    /// 设备名称
    pub name: String,
    /// 设备状态
    pub status: String,
    /// 最后在线时间
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// 设备类型
    pub device_type: String,
}

pub(crate) async fn search_devices(
    pool: &SqlitePool,
    keyword: &str,
    limit: Option<u32>,
) -> Result<Vec<Device>> {
    let search_pattern = format!("%{}%", keyword);
    let exact_pattern = format!("{}%", keyword);

    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(device_row_mapper::SELECT_COLUMNS);
    builder.push(
        " FROM devices WHERE name LIKE ? OR display_name LIKE ? OR address LIKE ? OR description LIKE ?
             ORDER BY CASE
                WHEN name LIKE ? THEN 1
                WHEN display_name LIKE ? THEN 2
                WHEN address LIKE ? THEN 3
                ELSE 4
             END, name",
    );

    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&exact_pattern);
    builder.push_bind(&exact_pattern);
    builder.push_bind(&exact_pattern);

    if let Some(limit) = limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    }

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

pub(crate) async fn device_tree(pool: &SqlitePool, root_id: Option<&str>) -> Result<Vec<Device>> {
    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(device_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM devices WHERE ");

    if let Some(root_id) = root_id {
        builder.push("parent_id = ").push_bind(root_id);
    } else {
        builder.push("parent_id IS NULL");
    }

    builder.push(" ORDER BY name");

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(device_row_mapper::row_to_device(row)?);
    }
    Ok(devices)
}

pub(crate) async fn device_status_distribution(
    pool: &SqlitePool,
    workspace_id: Option<&str>,
) -> Result<DeviceStatusDistribution> {
    let mut builder = QueryBuilder::new(
        "SELECT
                SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END) as online,
                SUM(CASE WHEN state = 0 THEN 1 ELSE 0 END) as offline,
                SUM(CASE WHEN state < 0 THEN 1 ELSE 0 END) as error_count,
                SUM(CASE WHEN state = 2 THEN 1 ELSE 0 END) as maintenance
            FROM devices",
    );

    if let Some(wid) = workspace_id {
        builder.push(" WHERE workspace_id = ").push_bind(wid);
    }

    let row = builder.build().fetch_one(pool).await?;

    Ok(DeviceStatusDistribution {
        online: row.get("online"),
        offline: row.get("offline"),
        error: row.get("error_count"),
        maintenance: row.get("maintenance"),
    })
}

pub(crate) async fn quick_devices(
    pool: &SqlitePool,
    limit: i32,
    workspace_id: Option<&str>,
) -> Result<Vec<QuickDevice>> {
    let mut builder = QueryBuilder::new("SELECT id, name, device_type, state, updated_at FROM devices");

    if let Some(wid) = workspace_id {
        builder.push(" WHERE workspace_id = ").push_bind(wid);
    }

    builder.push(
        " ORDER BY
                CASE
                    WHEN state = 1 THEN 0
                    WHEN state = 0 THEN 1
                    WHEN state < 0 THEN 2
                    ELSE 3
                END,
                updated_at DESC
            LIMIT ",
    );
    builder.push_bind(limit);

    let devices: Vec<(String, String, Option<String>, i32, chrono::NaiveDateTime)> =
        builder.build_query_as().fetch_all(pool).await?;

    let quick_devices = devices
        .into_iter()
        .map(|(id, name, device_type, state, updated_at)| {
            let status = match state {
                1 => "online",
                0 => "offline",
                2 => "maintenance",
                _ => "error",
            };

            QuickDevice {
                id,
                name,
                status: status.to_string(),
                last_seen: updated_at.and_utc(),
                device_type: device_type.unwrap_or_else(|| "unknown".to_string()),
            }
        })
        .collect();

    Ok(quick_devices)
}

impl Db {
    /// 按 ID 查设备；`workspace_scope` 为 Some 时先校验归属（不属于返回 None）。
    pub async fn find_device_by_id(&self, workspace_scope: Option<&str>, id: &str) -> Result<Option<Device>> {
        find_device_by_id(self.pool(), workspace_scope, id).await
    }

    /// 按名称查设备；Some(scope) 时限定该 workspace。
    pub async fn find_device_by_name(&self, workspace_scope: Option<&str>, name: &str) -> Result<Option<Device>> {
        find_device_by_name(self.pool(), workspace_scope, name).await
    }

    /// 按条件列出设备；Some(scope) 时强制 workspace_id = scope。
    pub async fn find_devices(&self, workspace_scope: Option<&str>, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
        find_devices(self.pool(), workspace_scope, criteria).await
    }

    /// 按条件统计设备数；Some(scope) 时强制 workspace_id = scope。
    pub async fn count_devices(&self, workspace_scope: Option<&str>, criteria: &DeviceCriteria) -> Result<i64> {
        count_devices(self.pool(), workspace_scope, criteria).await
    }

    /// 创建设备并回读；Some(scope) 时 workspace_id 取 scope（忽略请求值）。
    pub async fn create_device(&self, workspace_scope: Option<&str>, request: &CreateDeviceRequest) -> Result<Device> {
        create_device(self.pool(), workspace_scope, request).await
    }

    /// 更新设备并回读（内部事务）；Some(scope) 时先校验归属，不存在返回 NotFound。
    pub async fn update_device(
        &self,
        workspace_scope: Option<&str>,
        id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device> {
        update_device(self.pool(), workspace_scope, id, request).await
    }

    /// 删除设备，返回影响行数；Some(scope) 时不属于该 workspace 返回 0。
    pub async fn delete_device(&self, workspace_scope: Option<&str>, id: &str) -> Result<u64> {
        delete_device(self.pool(), workspace_scope, id).await
    }

    /// 批量删除（内部事务）；Some(scope) 时先按 workspace 过滤 ID。
    pub async fn delete_devices_by_ids(&self, workspace_scope: Option<&str>, ids: &[String]) -> Result<u64> {
        delete_devices_by_ids(self.pool(), workspace_scope, ids).await
    }

    /// 批量创建（内部事务）；Some(scope) 时 workspace_id 取 scope 并回读。
    pub async fn create_devices_batch(
        &self,
        workspace_scope: Option<&str>,
        requests: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>> {
        create_devices_batch(self.pool(), workspace_scope, requests).await
    }

    /// 更新单设备状态；Some(scope) 时设备不存在返回 InvalidArgument。
    pub async fn update_device_state(&self, workspace_scope: Option<&str>, id: &str, state: i32) -> Result<()> {
        update_device_state(self.pool(), workspace_scope, id, state).await
    }

    /// 批量更新状态（内部事务）；Some(scope) 时先按 workspace 过滤。
    pub async fn update_device_states_batch(
        &self,
        workspace_scope: Option<&str>,
        updates: &[(String, i32)],
    ) -> Result<u64> {
        update_device_states_batch(self.pool(), workspace_scope, updates).await
    }

    /// 启用/禁用设备（state 1/0）；Some(scope) 时设备不存在返回 InvalidArgument。
    pub async fn update_device_enabled_status(
        &self,
        workspace_scope: Option<&str>,
        id: &str,
        enabled: bool,
    ) -> Result<bool> {
        update_device_enabled_status(self.pool(), workspace_scope, id, enabled).await
    }

    /// 列出子设备；Some(scope) 时父设备不属于该 workspace 返回空。
    pub async fn find_device_children(&self, workspace_scope: Option<&str>, parent_id: &str) -> Result<Vec<Device>> {
        find_device_children(self.pool(), workspace_scope, parent_id).await
    }

    /// 按模板 ID 列出设备；Some(scope) 时限定 workspace。
    pub async fn find_devices_by_template_id(
        &self,
        workspace_scope: Option<&str>,
        template_id: &str,
    ) -> Result<Vec<Device>> {
        find_devices_by_template_id(self.pool(), workspace_scope, template_id).await
    }

    /// 按驱动名列出设备；Some(scope) 时限定 workspace。
    pub async fn find_devices_by_driver_name(
        &self,
        workspace_scope: Option<&str>,
        driver_name: &str,
    ) -> Result<Vec<Device>> {
        find_devices_by_driver_name(self.pool(), workspace_scope, driver_name).await
    }

    /// 按关联网关列出设备；Some(scope) 时限定 workspace。
    pub async fn find_devices_by_linked_gateway(
        &self,
        workspace_scope: Option<&str>,
        linked_gateway: &str,
    ) -> Result<Vec<Device>> {
        find_devices_by_linked_gateway(self.pool(), workspace_scope, linked_gateway).await
    }

    /// 按 id 检查设备是否存在。
    pub async fn device_exists_by_id(&self, id: &str) -> Result<bool> {
        device_exists_by_id(self.pool(), id).await
    }

    /// 设备名是否已存在；Some(scope) 时限定 workspace 内判断。
    pub async fn device_exists_by_name(&self, workspace_scope: Option<&str>, name: &str) -> Result<bool> {
        device_exists_by_name(self.pool(), workspace_scope, name).await
    }

    /// 按 ID 列表查设备；Some(scope) 时先按 workspace 过滤 ID。
    pub async fn find_devices_by_ids(&self, workspace_scope: Option<&str>, ids: &[String]) -> Result<Vec<Device>> {
        find_devices_by_ids(self.pool(), workspace_scope, ids).await
    }

    /// 启用状态 + 搜索文本的分页查询；Some(scope) 时限定 workspace。
    pub async fn find_devices_with_filters(
        &self,
        workspace_scope: Option<&str>,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>> {
        find_devices_with_filters(self.pool(), workspace_scope, enabled, search, page, page_size).await
    }

    /// 批量更新设备状态（含 updated_at，内部事务）；Some(scope) 时先按 workspace 过滤。
    pub async fn update_device_status_batch(
        &self,
        workspace_scope: Option<&str>,
        updates: &[DeviceStatusUpdate],
    ) -> Result<u64> {
        update_device_status_batch(self.pool(), workspace_scope, updates).await
    }

    /// 工作空间下的设备数（workspace 删除守卫用）。
    pub async fn count_devices_by_workspace(&self, workspace_id: &str) -> Result<i64> {
        count_devices_by_workspace(self.pool(), workspace_id).await
    }

    /// 设备总数/在线/离线/告警统计（cloud dashboard 用）。
    pub async fn device_stats_overview(&self) -> Result<DeviceStats> {
        device_stats_overview(self.pool()).await
    }

    /// 按设备类型分组计数（cloud dashboard 用）。
    pub async fn count_devices_by_type(&self) -> Result<Vec<(String, i64)>> {
        count_devices_by_type(self.pool()).await
    }

    /// 按驱动名分组计数（cloud dashboard 用）。
    pub async fn count_devices_by_driver(&self) -> Result<Vec<(String, i64)>> {
        count_devices_by_driver(self.pool()).await
    }

    /// 关键字搜索设备（cloud device dashboard 用）。
    pub async fn search_devices(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>> {
        search_devices(self.pool(), keyword, limit).await
    }

    /// 设备树（按 parent_id 取一层；cloud device dashboard 用）。
    pub async fn device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>> {
        device_tree(self.pool(), root_id).await
    }

    /// 设备状态分布（cloud device dashboard 用）。
    pub async fn device_status_distribution(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<DeviceStatusDistribution> {
        device_status_distribution(self.pool(), workspace_id).await
    }

    /// 关键设备列表（cloud device dashboard 用）。
    pub async fn quick_devices(&self, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickDevice>> {
        quick_devices(self.pool(), limit, workspace_id).await
    }
}

// ──────────────────────────────────────────────
// Open API 投影查询（自 cloud admin/open 迁入，Task 12）
// ──────────────────────────────────────────────

/// Open API thing 列表行。
#[derive(Debug)]
pub struct OpenThingRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub state: i32,
    pub created_at: String,
}

/// Open API thing 详情行。
#[derive(Debug)]
pub struct OpenThingDetailRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub state: i32,
    pub protocol_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Open API：列出 workspace 内 things（最新 100 条）。
pub(crate) async fn list_open_things(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<OpenThingRow>> {
    let rows = sqlx::query(
        "SELECT id, name, display_name, device_type, state, created_at FROM devices WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenThingRow {
            id: row.try_get::<String, _>("id").unwrap_or_default(),
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
            device_type: row.try_get::<Option<String>, _>("device_type").unwrap_or_default(),
            state: row.try_get::<i32, _>("state").unwrap_or_default(),
            created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect())
}

/// Open API：按 id + workspace 查 thing 详情。
pub(crate) async fn find_open_thing(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<OpenThingDetailRow>> {
    let row = sqlx::query(
        "SELECT id, name, display_name, device_type, address, state, protocol_type, created_at, updated_at FROM devices WHERE id = ? AND workspace_id = ? LIMIT 1"
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| OpenThingDetailRow {
        id: row.try_get::<String, _>("id").unwrap_or_default(),
        name: row.try_get::<String, _>("name").unwrap_or_default(),
        display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
        device_type: row.try_get::<Option<String>, _>("device_type").unwrap_or_default(),
        address: row.try_get::<Option<String>, _>("address").unwrap_or_default(),
        state: row.try_get::<i32, _>("state").unwrap_or_default(),
        protocol_type: row.try_get::<Option<String>, _>("protocol_type").unwrap_or_default(),
        created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
    }))
}

/// Open API：查 (id, thing_type)（workspace 作用域）。
pub(crate) async fn find_open_thing_type(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, thing_type FROM devices WHERE id = ? AND workspace_id = ?")
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

impl Db {
    /// Open API：列出 workspace 内 things（最新 100 条）。
    pub async fn list_open_things(&self, workspace_id: &str) -> Result<Vec<OpenThingRow>> {
        list_open_things(self.pool(), workspace_id).await
    }

    /// Open API：按 id + workspace 查 thing 详情。
    pub async fn find_open_thing(&self, id: &str, workspace_id: &str) -> Result<Option<OpenThingDetailRow>> {
        find_open_thing(self.pool(), id, workspace_id).await
    }

    /// Open API：查 (id, thing_type)（workspace 作用域）。
    pub async fn find_open_thing_type(&self, id: &str, workspace_id: &str) -> Result<Option<(String, String)>> {
        find_open_thing_type(self.pool(), id, workspace_id).await
    }
}

// ──────────────────────────────────────────────
// Dashboard 统计（自 cloud admin/monitoring 迁入，Task 12）
// ──────────────────────────────────────────────

/// Dashboard：设备总数（可选 workspace 过滤）。
pub(crate) async fn count_devices_total(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<i64> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => ("SELECT COUNT(*) FROM devices WHERE workspace_id = ?", Some(wid)),
        None => ("SELECT COUNT(*) FROM devices", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid {
        q = q.bind(w);
    }
    let count: i64 = q.fetch_one(pool).await?;
    Ok(count)
}

/// Dashboard：在线设备数（可选 workspace 过滤）。
pub(crate) async fn count_online_devices(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<i64> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => (
            "SELECT COUNT(*) FROM devices WHERE state = 1 AND workspace_id = ?",
            Some(wid),
        ),
        None => ("SELECT COUNT(*) FROM devices WHERE state = 1", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid {
        q = q.bind(w);
    }
    let count: i64 = q.fetch_one(pool).await?;
    Ok(count)
}

impl Db {
    /// Dashboard：设备总数（可选 workspace 过滤）。
    pub async fn count_devices_total(&self, workspace_id: Option<&str>) -> Result<i64> {
        count_devices_total(self.pool(), workspace_id).await
    }

    /// Dashboard：在线设备数（可选 workspace 过滤）。
    pub async fn count_online_devices(&self, workspace_id: Option<&str>) -> Result<i64> {
        count_online_devices(self.pool(), workspace_id).await
    }
}

// ──────────────────────────────────────────────
// 初始化引导：孤儿设备归属（自 cloud shared/initialization.rs 迁入，Task 12；
// 错误保持 sqlx::Error 以沿用调用方既有错误文案）
// ──────────────────────────────────────────────

/// 将未分配设备归属到默认租户。
pub(crate) async fn assign_orphan_devices_to_default_tenant(
    pool: &SqlitePool,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE devices SET tenant_id = 'tenant-default-001' WHERE tenant_id IS NULL")
        .execute(pool)
        .await?;
    Ok(())
}

/// 将默认租户下未分配设备归属到默认工作空间。
pub(crate) async fn assign_orphan_devices_to_default_workspace(
    pool: &SqlitePool,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE devices SET workspace_id = 'ws-default-001' WHERE workspace_id IS NULL AND tenant_id = 'tenant-default-001'"
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 将未分配设备归属到默认租户。
    pub async fn assign_orphan_devices_to_default_tenant(&self) -> std::result::Result<(), sqlx::Error> {
        assign_orphan_devices_to_default_tenant(self.pool()).await
    }

    /// 将默认租户下未分配设备归属到默认工作空间。
    pub async fn assign_orphan_devices_to_default_workspace(&self) -> std::result::Result<(), sqlx::Error> {
        assign_orphan_devices_to_default_workspace(self.pool()).await
    }
}

/// 确保 devices 表存在（edge 网关本地库不走 migrations，Task 13 自 edge storage 收编）。
pub(crate) async fn ensure_devices_table(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            display_name TEXT,
            device_type TEXT,
            address TEXT,
            description TEXT,
            position TEXT,
            driver_name TEXT,
            device_model TEXT,
            protocol_type TEXT,
            factory_name TEXT,
            linked_data TEXT,
            driver_options TEXT,
            state INTEGER NOT NULL DEFAULT 0,
            parent_id TEXT,
            template_id TEXT,
            workspace_id TEXT,
            linked_gateway TEXT,
            fingerprint TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 确保 devices 表存在（edge 本地库 bootstrap 用）。
    pub async fn ensure_devices_table(&self) -> std::result::Result<(), sqlx::Error> {
        ensure_devices_table(self.pool()).await
    }
}
