use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::{
    domain::device::repository::{
        DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder,
    },
    dto::entity::device::{
        CreateDeviceRequest, Device, DeviceStats, DeviceStatusUpdate, UpdateDeviceRequest,
    },
    infrastructure::persistence::Database,
    shared::error::{Error, Result},
};

/// SQLite implementation of DeviceRepository
pub struct SqliteDeviceRepository {
    database: Database,
}

impl SqliteDeviceRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    const SELECT_COLUMNS: &str = r#"
        id, name, display_name, device_type, address, description, position,
        driver_name, device_model, protocol_type, factory_name, linked_data,
        driver_options, state, parent_id, product_id, tenant_id, workspace_id, created_at, updated_at
    "#;

    fn row_to_device(&self, row: sqlx::sqlite::SqliteRow) -> Result<Device> {
        use sqlx::Row;
        Ok(Device {
            id: row.get("id"),
            name: row.get("name"),
            display_name: row.get("display_name"),
            device_type: row.get("device_type"),
            address: row.get("address"),
            description: row.get("description"),
            position: row.get("position"),
            driver_name: row.get("driver_name"),
            device_model: row.get("device_model"),
            protocol_type: row.get("protocol_type"),
            factory_name: row.get("factory_name"),
            linked_data: row.get("linked_data"),
            driver_options: row.get("driver_options"),
            state: row.get("state"),
            parent_id: row.get("parent_id"),
            product_id: row.get("product_id"),
            tenant_id: row.get("tenant_id"),
            workspace_id: row.get("workspace_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            tags: None,
            properties: None,
            commands: None,
            is_online: false,
            last_heartbeat: None,
        })
    }
}

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>> {
        let sql = format!("SELECT {} FROM devices WHERE id = ?", Self::SELECT_COLUMNS);
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(self.database.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(self.row_to_device(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Device>> {
        let sql = format!("SELECT {} FROM devices WHERE name = ?", Self::SELECT_COLUMNS);
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .fetch_optional(self.database.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(self.row_to_device(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>> {
        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(Self::SELECT_COLUMNS);
        builder.push(" FROM devices WHERE 1=1");

        if let Some(tenant_id) = &criteria.tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant_id);
        }
        if let Some(workspace_id) = &criteria.workspace_id {
            builder.push(" AND workspace_id = ").push_bind(workspace_id);
        }
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
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn count(&self, criteria: &DeviceCriteria) -> Result<i64> {
        let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM devices WHERE 1=1");

        if let Some(tenant_id) = &criteria.tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant_id);
        }
        if let Some(workspace_id) = &criteria.workspace_id {
            builder.push(" AND workspace_id = ").push_bind(workspace_id);
        }
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

        let mut tx = self.database.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO devices (
                id, name, display_name, device_type, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, product_id, tenant_id, workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(&request.product_id)
        .bind(&request.tenant_id)
        .bind(&request.workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.find_by_id(&id)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device> {
        let mut builder = QueryBuilder::new("UPDATE devices SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

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
        if let Some(product_id) = &request.product_id {
            if has_updates {
                builder.push(", ");
            }
            builder.push("product_id = ").push_bind(product_id);
            has_updates = true;
        }
        if let Some(tenant_id) = &request.tenant_id {
            if has_updates {
                builder.push(", ");
            }
            builder.push("tenant_id = ").push_bind(tenant_id);
            has_updates = true;
        }
        if let Some(workspace_id) = &request.workspace_id {
            if has_updates {
                builder.push(", ");
            }
            builder.push("workspace_id = ").push_bind(workspace_id);
            has_updates = true;
        }

        if !has_updates {
            return self.find_by_id(id).await?.ok_or(Error::NotFound);
        }

        builder.push(", updated_at = ").push_bind(&now);
        builder.push(" WHERE id = ").push_bind(id);

        let result = builder.build().execute(self.database.pool()).await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
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

    async fn create_batch(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut created_devices = Vec::new();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for request in requests {
            let id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO devices (
                    id, name, display_name, device_type, address, description, position,
                    driver_name, device_model, protocol_type, factory_name, linked_data,
                    driver_options, state, parent_id, product_id, tenant_id, workspace_id, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(&request.product_id)
            .bind(&request.tenant_id)
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
                state: Some(0),
                parent_id: request.parent_id.clone(),
                product_id: request.product_id.clone(),
                tenant_id: request.tenant_id.clone(),
                workspace_id: request.workspace_id.clone(),
                created_at: Some(now.clone()),
                updated_at: Some(now.clone()),
                tags: None,
                properties: None,
                commands: None,
                is_online: false,
                last_heartbeat: None,
            };

            created_devices.push(device);
        }

        tx.commit().await?;
        Ok(created_devices)
    }

    async fn update_state(&self, id: &str, state: i32) -> Result<()> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
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

    async fn update_states_batch(&self, updates: &[(String, i32)]) -> Result<u64> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut total_affected = 0u64;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

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

    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        let state = if enabled { 1 } else { 0 };
        match self.update_state(id, state).await {
            Ok(()) => Ok(true),
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn find_children(&self, parent_id: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE parent_id = ? ORDER BY name",
            Self::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(parent_id)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_product_id(&self, product_id: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE product_id = ? ORDER BY name",
            Self::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(product_id)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_by_driver_name(&self, driver_name: &str) -> Result<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE driver_name = ? ORDER BY name",
            Self::SELECT_COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(driver_name)
            .fetch_all(self.database.pool())
            .await?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM devices WHERE name = ?")
            .bind(name)
            .fetch_one(self.database.pool())
            .await?;
        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Device>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(Self::SELECT_COLUMNS);
        builder.push(" FROM devices WHERE id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>> {
        let search_pattern = format!("%{}%", keyword);
        let exact_pattern = format!("{}%", keyword);

        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(Self::SELECT_COLUMNS);
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

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn get_stats(&self) -> Result<DeviceStats> {
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
        .fetch_one(self.database.pool())
        .await?;

        Ok(DeviceStats {
            total_devices: row.get("total_devices"),
            online_devices: row.get("online_devices"),
            offline_devices: row.get("offline_devices"),
            alarm_devices: row.get("alarm_devices"),
        })
    }

    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
            FROM devices
            GROUP BY device_type
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.database.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let device_type: String = row.get("device_type");
            let count: i64 = row.get("count");
            stats.push((device_type, count));
        }
        Ok(stats)
    }

    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(driver_name, 'Unknown') as driver_name, COUNT(*) as count
            FROM devices
            GROUP BY driver_name
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.database.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let driver_name: String = row.get("driver_name");
            let count: i64 = row.get("count");
            stats.push((driver_name, count));
        }
        Ok(stats)
    }

    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>> {
        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(Self::SELECT_COLUMNS);
        builder.push(" FROM devices WHERE ");

        if let Some(root_id) = root_id {
            builder.push("parent_id = ").push_bind(root_id);
        } else {
            builder.push("parent_id IS NULL");
        }

        builder.push(" ORDER BY name");

        let rows = builder.build().fetch_all(self.database.pool()).await?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(self.row_to_device(row)?);
        }
        Ok(devices)
    }

    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>> {
        let mut criteria = DeviceCriteria::default();
        criteria.limit = Some(page_size);
        criteria.offset = Some((page.saturating_sub(1)) * page_size);

        if let Some(enabled) = enabled {
            criteria.state = Some(if enabled { 1 } else { 0 });
        }

        if let Some(search) = search {
            criteria.name = Some(search.to_string());
        }

        self.find_all(&criteria).await
    }

    async fn update_status_batch(&self, updates: &[DeviceStatusUpdate]) -> Result<u64> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut total_affected = 0u64;

        for update in updates {
            let result =
                sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
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
}
