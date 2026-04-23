use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};
use tinyiothub_core::error::Result;
use tinyiothub_core::models::device::{CreateDeviceRequest, Device, DeviceStatusUpdate, UpdateDeviceRequest};
use tinyiothub_storage::traits::device::{DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder};
use tinyiothub_storage::sqlite::device_row_mapper;
use crate::infrastructure::persistence::database::Database;

/// Tenant-aware device repository adapter
///
/// Wraps a DeviceRepository implementation and automatically adds
/// workspace filtering to enforce tenant isolation.
#[derive(Debug, Clone)]
pub struct TenantDeviceRepository<R: DeviceRepository> {
    inner: R,
    workspace_id: String,
    database: Database,
}

impl<R: DeviceRepository> TenantDeviceRepository<R> {
    /// Create a new tenant-aware device repository adapter
    pub fn new(inner: R, workspace_id: String, database: Database) -> Self {
        Self { inner, workspace_id, database }
    }

    /// Get the workspace ID this adapter is filtering for
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Check if a device belongs to this workspace
    async fn device_belongs_to_workspace(&self, device_id: &str) -> Result<bool> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT workspace_id FROM devices WHERE id = ?"
        )
            .bind(device_id)
            .fetch_optional(self.database.pool())
            .await?;

        match result {
            Some((workspace_id,)) => Ok(workspace_id == self.workspace_id),
            None => Ok(false), // Device doesn't exist
        }
    }

    /// Filter device IDs to only those belonging to this workspace
    async fn filter_ids_by_workspace(&self, ids: &[String]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Use QueryBuilder to avoid lifetime issues with dynamic SQL
        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT id FROM devices WHERE workspace_id = ");
        query_builder.push_bind(&self.workspace_id);
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
    async fn filter_state_updates_by_workspace(&self, updates: &[(String, i32)]) -> Result<Vec<(String, i32)>> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();
        let filtered_ids = self.filter_ids_by_workspace(&ids).await?;

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
    async fn filter_status_updates_by_workspace(&self, updates: &[DeviceStatusUpdate]) -> Result<Vec<DeviceStatusUpdate>> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = updates.iter().map(|update| update.device_id.clone()).collect();
        let filtered_ids = self.filter_ids_by_workspace(&ids).await?;

        // Create a set for fast lookup
        let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

        let filtered_updates: Vec<DeviceStatusUpdate> = updates
            .iter()
            .filter(|update| filtered_set.contains(&update.device_id))
            .cloned()
            .collect();

        Ok(filtered_updates)
    }
}

#[async_trait]
impl<R: DeviceRepository + Send + Sync> DeviceRepository for TenantDeviceRepository<R> {
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>> {
        // Verify device belongs to this workspace
        if !self.device_belongs_to_workspace(id).await? {
            return Ok(None);
        }

        self.inner.find_by_id(id).await
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Device>> {
        // Find by name within this workspace using QueryBuilder
        use tinyiothub_storage::sqlite::device_row_mapper;

        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT ");
        query_builder.push(device_row_mapper::SELECT_COLUMNS);
        query_builder.push(" FROM devices WHERE name = ");
        query_builder.push_bind(name);
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(&self.workspace_id);

        let query = query_builder.build();
        let row = query.fetch_optional(self.database.pool()).await?;

        if let Some(row) = row {
            Ok(Some(device_row_mapper::row_to_device(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(device_row_mapper::SELECT_COLUMNS);
        builder.push(" FROM devices WHERE workspace_id = ");
        builder.push_bind(&self.workspace_id);

        // Add criteria filters
        if let Some(name) = &criteria.name {
            builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }
        if let Some(display_name) = &criteria.display_name {
            builder.push(" AND display_name LIKE ")
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
        if let Some(product_id) = &criteria.product_id {
            builder.push(" AND product_id = ").push_bind(product_id);
        }
        if let Some(search_text) = &criteria.search_text {
            let pattern = format!("%{}%", search_text);
            builder.push(" AND (name LIKE ").push_bind(&pattern);
            builder.push(" OR display_name LIKE ").push_bind(&pattern);
            builder.push(" OR address LIKE ").push_bind(&pattern);
            builder.push(" OR description LIKE ").push_bind(pattern);
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

    async fn count(&self, criteria: &DeviceCriteria) -> Result<i64> {
        let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM devices WHERE workspace_id = ");
        builder.push_bind(&self.workspace_id);

        if let Some(name) = &criteria.name {
            builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }
        if let Some(display_name) = &criteria.display_name {
            builder.push(" AND display_name LIKE ")
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
        if let Some(product_id) = &criteria.product_id {
            builder.push(" AND product_id = ").push_bind(product_id);
        }
        if let Some(search_text) = &criteria.search_text {
            let pattern = format!("%{}%", search_text);
            builder.push(" AND (name LIKE ").push_bind(&pattern);
            builder.push(" OR display_name LIKE ").push_bind(&pattern);
            builder.push(" OR address LIKE ").push_bind(&pattern);
            builder.push(" OR description LIKE ").push_bind(pattern);
            builder.push(")");
        }

        let row = builder.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn create(&self, request: &CreateDeviceRequest) -> Result<Device> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Insert device with workspace_id
        sqlx::query(
            r#"
            INSERT INTO devices (
                id, name, display_name, device_type, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, product_id, workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(&request.product_id)
        .bind(&self.workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        // Fetch the created device
        self.find_by_id(&id).await?
            .ok_or_else(|| tinyiothub_core::error::Error::InvalidArgument(
                format!("Failed to find created device with id {}", id)
            ))
    }

    async fn update(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device> {
        // Verify device belongs to this workspace before updating
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        self.inner.update(id, request).await
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        // Verify device belongs to this workspace before deleting
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Ok(0); // Already doesn't exist in this workspace
        }

        self.inner.delete(id).await
    }

    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        // Filter IDs to only those belonging to this workspace
        let filtered_ids = self.filter_ids_by_workspace(ids).await?;
        if filtered_ids.is_empty() {
            return Ok(0);
        }
        self.inner.delete_by_ids(&filtered_ids).await
    }

    async fn create_batch(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut device_ids = Vec::new();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for request in requests {
            let id = uuid::Uuid::new_v4().to_string();
            device_ids.push(id.clone());

            sqlx::query(
                r#"
                INSERT INTO devices (
                    id, name, display_name, device_type, address, description, position,
                    driver_name, device_model, protocol_type, factory_name, linked_data,
                    driver_options, state, parent_id, product_id, workspace_id, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(&request.product_id)
            .bind(&self.workspace_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Fetch created devices
        self.find_by_ids(&device_ids).await
    }

    async fn update_state(&self, id: &str, state: i32) -> Result<()> {
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::InvalidArgument(
                format!("Device with id {} not found in workspace {}", id, self.workspace_id)
            ));
        }

        self.inner.update_state(id, state).await
    }

    async fn update_states_batch(&self, updates: &[(String, i32)]) -> Result<u64> {
        // Filter updates to only devices in this workspace
        let filtered_updates = self.filter_state_updates_by_workspace(updates).await?;
        if filtered_updates.is_empty() {
            return Ok(0);
        }
        self.inner.update_states_batch(&filtered_updates).await
    }

    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        let device = self.find_by_id(id).await?;
        if device.is_none() {
            return Err(tinyiothub_core::error::Error::InvalidArgument(
                format!("Device with id {} not found in workspace {}", id, self.workspace_id)
            ));
        }

        self.inner.update_enabled_status(id, enabled).await
    }

    async fn find_children(&self, parent_id: &str) -> Result<Vec<Device>> {
        // First verify parent belongs to this workspace
        if !self.device_belongs_to_workspace(parent_id).await? {
            return Ok(vec![]);
        }

        // Find children within this workspace using QueryBuilder
        use tinyiothub_storage::sqlite::device_row_mapper;

        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT ");
        query_builder.push(device_row_mapper::SELECT_COLUMNS);
        query_builder.push(" FROM devices WHERE parent_id = ");
        query_builder.push_bind(parent_id);
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(&self.workspace_id);

        let query = query_builder.build();
        let rows = query.fetch_all(self.database.pool()).await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_product_id(&self, product_id: &str) -> Result<Vec<Device>> {
        // Find devices by product_id within this workspace using QueryBuilder
        use tinyiothub_storage::sqlite::device_row_mapper;

        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT ");
        query_builder.push(device_row_mapper::SELECT_COLUMNS);
        query_builder.push(" FROM devices WHERE product_id = ");
        query_builder.push_bind(product_id);
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(&self.workspace_id);

        let query = query_builder.build();
        let rows = query.fetch_all(self.database.pool()).await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_driver_name(&self, driver_name: &str) -> Result<Vec<Device>> {
        // Find devices by driver_name within this workspace using QueryBuilder
        use tinyiothub_storage::sqlite::device_row_mapper;

        let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
            sqlx::QueryBuilder::new("SELECT ");
        query_builder.push(device_row_mapper::SELECT_COLUMNS);
        query_builder.push(" FROM devices WHERE driver_name = ");
        query_builder.push_bind(driver_name);
        query_builder.push(" AND workspace_id = ");
        query_builder.push_bind(&self.workspace_id);

        let query = query_builder.build();
        let rows = query.fetch_all(self.database.pool()).await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(device_row_mapper::row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        // Check within this workspace
        let criteria = DeviceCriteria::builder()
            .name(name.to_string())
            .build();

        let count = self.count(&criteria).await?;
        Ok(count > 0)
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Device>> {
        // Filter IDs to only those belonging to this workspace
        let filtered_ids = self.filter_ids_by_workspace(ids).await?;
        if filtered_ids.is_empty() {
            return Ok(vec![]);
        }
        self.inner.find_by_ids(&filtered_ids).await
    }

    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>> {
        use tinyiothub_storage::traits::device::DeviceCriteria;

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

    async fn update_status_batch(&self, updates: &[DeviceStatusUpdate]) -> Result<u64> {
        // Filter updates to only devices in this workspace
        let filtered_updates = self.filter_status_updates_by_workspace(updates).await?;
        if filtered_updates.is_empty() {
            return Ok(0);
        }
        self.inner.update_status_batch(&filtered_updates).await
    }
}