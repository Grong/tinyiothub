use sqlx::{QueryBuilder, Row};

use crate::database::Database;
use tinyiothub_core::error::{Error, Result};
use tinyiothub_core::models::device::{CreateDeviceRequest, Device, DeviceStatusUpdate, UpdateDeviceRequest};
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

/// SQLite implementation of DeviceRepository
#[derive(Debug, Clone)]
pub struct DeviceRepository {
    database: Database,
    /// Some(workspace_id) 时按租户作用域过滤（E6a 合并原 TenantDeviceRepository 行为）。
    workspace_scope: Option<String>,
}

impl DeviceRepository {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            workspace_scope: None,
        }
    }

    /// 返回带租户作用域的副本：所有查询/写操作限定到该 workspace。
    pub fn for_workspace(self, workspace_id: String) -> Self {
        Self {
            workspace_scope: Some(workspace_id),
            ..self
        }
    }
}

impl DeviceRepository {
    async fn find_by_id_inner(&self, id: &str) -> Result<Option<Device>> {
        let sql = format!("SELECT {} FROM devices WHERE id = ?", device_row_mapper::SELECT_COLUMNS);
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(self.database.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(device_row_mapper::row_to_device(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_name_inner(&self, name: &str) -> Result<Option<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE name = ?",
            device_row_mapper::SELECT_COLUMNS
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .fetch_optional(self.database.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(device_row_mapper::row_to_device(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_all_inner(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
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

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn count_inner(&self, criteria: &DeviceCriteria) -> Result<i64> {
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

        let row = builder.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn create_inner(&self, request: &CreateDeviceRequest) -> Result<Device> {
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
        .execute(self.database.pool())
        .await?;

        self.find_by_id_inner(&id).await?.ok_or(Error::NotFound)
    }

    async fn update_inner(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device> {
        let mut tx = self.database.pool().begin().await?;

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
            return self.find_by_id_inner(id).await?.ok_or(Error::NotFound);
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

    async fn delete_inner(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_ids_inner(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut tx = self.database.pool().begin().await?;
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

    async fn create_batch_inner(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
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

    async fn update_state_inner(&self, id: &str, state: i32) -> Result<()> {
        let now = now_string();
        let result = sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
            .bind(state)
            .bind(now)
            .bind(id)
            .execute(self.database.pool())
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    async fn update_states_batch_inner(&self, updates: &[(String, i32)]) -> Result<u64> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.database.pool().begin().await?;
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

    async fn update_enabled_status_inner(&self, id: &str, enabled: bool) -> Result<bool> {
        let state = if enabled { 1 } else { 0 };
        match self.update_state_inner(id, state).await {
            Ok(()) => Ok(true),
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn find_children_inner(&self, parent_id: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE parent_id = ? ORDER BY name",
            device_row_mapper::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(parent_id)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_template_id_inner(&self, template_id: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE template_id = ? ORDER BY name",
            device_row_mapper::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(template_id)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_driver_name_inner(&self, driver_name: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE driver_name = ? ORDER BY name",
            device_row_mapper::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(driver_name)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_linked_gateway_inner(&self, linked_gateway: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE linked_gateway = ? ORDER BY created_at DESC",
            device_row_mapper::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(linked_gateway)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn exists_by_name_inner(&self, name: &str) -> Result<bool> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM devices WHERE name = ?")
            .bind(name)
            .fetch_one(self.database.pool())
            .await?;
        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    async fn find_by_ids_inner(&self, ids: &[String]) -> Result<Vec<Device>> {
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

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_with_filters_inner(
        &self,
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

        self.find_all_inner(&criteria).await
    }

    async fn update_status_batch_inner(&self, updates: &[DeviceStatusUpdate]) -> Result<u64> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.database.pool().begin().await?;
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

    // ── 租户作用域分发层（E6a 合并自 TenantDeviceRepository）──

    /// Check if a device belongs to this workspace
    async fn device_belongs_to_workspace(&self, ws: &str, device_id: &str) -> Result<bool> {
        let result: Option<(String,)> = sqlx::query_as("SELECT workspace_id FROM devices WHERE id = ?")
            .bind(device_id)
            .fetch_optional(self.database.pool())
            .await?;

        match result {
            Some((workspace_id,)) => Ok(workspace_id == ws),
            None => Ok(false), // Device doesn't exist
        }
    }

    /// Filter device IDs to only those belonging to this workspace
    async fn filter_ids_by_workspace(&self, ws: &str, ids: &[String]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Use QueryBuilder to avoid lifetime issues with dynamic SQL
        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT id FROM devices WHERE workspace_id = ");
        query_builder.push_bind(&ws);
        query_builder.push(" AND id IN (");

        let mut separated = query_builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        query_builder.push(")");

        let query = query_builder.build();
        let rows = query.fetch_all(self.database.pool()).await?;
        Ok(rows.into_iter().map(|row| row.get::<String, _>("id")).collect())
    }

    /// Filter device state updates to only those belonging to this workspace
    async fn filter_state_updates_by_workspace(
        &self,
        ws: &str,
        updates: &[(String, i32)],
    ) -> Result<Vec<(String, i32)>> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();
        let filtered_ids = self.filter_ids_by_workspace(&ws, &ids).await?;

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
    async fn filter_status_updates_by_workspace(
        &self,
        ws: &str,
        updates: &[DeviceStatusUpdate],
    ) -> Result<Vec<DeviceStatusUpdate>> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = updates.iter().map(|update| update.device_id.clone()).collect();
        let filtered_ids = self.filter_ids_by_workspace(&ws, &ids).await?;

        // Create a set for fast lookup
        let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

        let filtered_updates: Vec<DeviceStatusUpdate> = updates
            .iter()
            .filter(|update| filtered_set.contains(&update.device_id))
            .cloned()
            .collect();

        Ok(filtered_updates)
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_id_inner(id).await;
        };
        // Verify device belongs to this workspace
        if !self.device_belongs_to_workspace(&ws, id).await? {
            return Ok(None);
        }

        self.find_by_id_inner(id).await
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_name_inner(name).await;
        };
        let criteria = DeviceCriteria::default()
            .with_name(name.to_string())
            .with_workspace_id(ws.clone());
        let devices = self.find_all_inner(&criteria).await?;
        Ok(devices.into_iter().next())
    }

    pub async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_all_inner(criteria).await;
        };
        let mut criteria = criteria.clone();
        criteria.workspace_id = Some(ws.clone());
        self.find_all_inner(&criteria).await
    }

    pub async fn count(&self, criteria: &DeviceCriteria) -> Result<i64> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.count_inner(criteria).await;
        };
        let mut criteria = criteria.clone();
        criteria.workspace_id = Some(ws.clone());
        self.count_inner(&criteria).await
    }

    pub async fn create(&self, request: &CreateDeviceRequest) -> Result<Device> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.create_inner(request).await;
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
        .bind(&ws)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        // Fetch the created device
        self.find_by_id(&id).await?.ok_or_else(|| {
            tinyiothub_core::error::Error::InvalidArgument(format!("Failed to find created device with id {}", id))
        })
    }

    pub async fn update(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device> {
        let Some(_ws) = self.workspace_scope.clone() else {
            return self.update_inner(id, request).await;
        };
        // Verify device belongs to this workspace before updating
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        self.update_inner(id, request).await
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        let Some(_ws) = self.workspace_scope.clone() else {
            return self.delete_inner(id).await;
        };
        // Verify device belongs to this workspace before deleting
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Ok(0); // Already doesn't exist in this workspace
        }

        self.delete_inner(id).await
    }

    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.delete_by_ids_inner(ids).await;
        };
        // Filter IDs to only those belonging to this workspace
        let filtered_ids = self.filter_ids_by_workspace(&ws, ids).await?;
        if filtered_ids.is_empty() {
            return Ok(0);
        }
        self.delete_by_ids_inner(&filtered_ids).await
    }

    pub async fn create_batch(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.create_batch_inner(requests).await;
        };
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
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
            .bind(&ws)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Fetch created devices
        self.find_by_ids(&device_ids).await
    }

    pub async fn update_state(&self, id: &str, state: i32) -> Result<()> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.update_state_inner(id, state).await;
        };
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
                "Device with id {} not found in workspace {}",
                id, ws
            )));
        }

        self.update_state_inner(id, state).await
    }

    pub async fn update_states_batch(&self, updates: &[(String, i32)]) -> Result<u64> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.update_states_batch_inner(updates).await;
        };
        // Filter updates to only devices in this workspace
        let filtered_updates = self.filter_state_updates_by_workspace(&ws, updates).await?;
        if filtered_updates.is_empty() {
            return Ok(0);
        }
        self.update_states_batch_inner(&filtered_updates).await
    }

    pub async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.update_enabled_status_inner(id, enabled).await;
        };
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
                "Device with id {} not found in workspace {}",
                id, ws
            )));
        }

        self.update_enabled_status_inner(id, enabled).await
    }

    pub async fn find_children(&self, parent_id: &str) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_children_inner(parent_id).await;
        };
        // Verify parent belongs to this workspace
        if !self.device_belongs_to_workspace(&ws, parent_id).await? {
            return Ok(vec![]);
        }

        let criteria = DeviceCriteria::default()
            .with_parent_id(parent_id.to_string())
            .with_workspace_id(ws.clone());
        self.find_all_inner(&criteria).await
    }

    pub async fn find_by_template_id(&self, template_id: &str) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_template_id_inner(template_id).await;
        };
        let criteria = DeviceCriteria::default()
            .with_template_id(template_id.to_string())
            .with_workspace_id(ws.clone());
        self.find_all_inner(&criteria).await
    }

    pub async fn find_by_driver_name(&self, driver_name: &str) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_driver_name_inner(driver_name).await;
        };
        let criteria = DeviceCriteria::default()
            .with_driver_name(driver_name.to_string())
            .with_workspace_id(ws.clone());
        self.find_all_inner(&criteria).await
    }

    pub async fn find_by_linked_gateway(&self, linked_gateway: &str) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_linked_gateway_inner(linked_gateway).await;
        };
        let criteria = DeviceCriteria::default().with_workspace_id(ws.clone());
        let all = self.find_all_inner(&criteria).await?;
        Ok(all
            .into_iter()
            .filter(|d| d.linked_gateway.as_deref() == Some(linked_gateway))
            .collect())
    }

    pub async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let Some(_ws) = self.workspace_scope.clone() else {
            return self.exists_by_name_inner(name).await;
        };
        // Check within this workspace
        let criteria = DeviceCriteria::builder().name(name.to_string()).build();

        let count = self.count(&criteria).await?;
        Ok(count > 0)
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Device>> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.find_by_ids_inner(ids).await;
        };
        // Filter IDs to only those belonging to this workspace
        let filtered_ids = self.filter_ids_by_workspace(&ws, ids).await?;
        if filtered_ids.is_empty() {
            return Ok(vec![]);
        }
        self.find_by_ids_inner(&filtered_ids).await
    }

    pub async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>> {
        let Some(_ws) = self.workspace_scope.clone() else {
            return self.find_with_filters_inner(enabled, search, page, page_size).await;
        };
        use crate::device::DeviceCriteria;

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

        self.find_all(&criteria).await
    }

    pub async fn update_status_batch(&self, updates: &[DeviceStatusUpdate]) -> Result<u64> {
        let Some(ws) = self.workspace_scope.clone() else {
            return self.update_status_batch_inner(updates).await;
        };
        // Filter updates to only devices in this workspace
        let filtered_updates = self.filter_status_updates_by_workspace(&ws, updates).await?;
        if filtered_updates.is_empty() {
            return Ok(0);
        }
        self.update_status_batch_inner(&filtered_updates).await
    }
}
