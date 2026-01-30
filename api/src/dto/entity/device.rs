use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

/// 设备实体 - 使用 snake_case 数据库字段
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub organization_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// 关联的标签列表 (不存储在数据库中，通过关联查询获取)
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<crate::dto::entity::tag::Tag>>,
    /// 设备实时属性数据 (不存储在数据库中，由DataServer更新)
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<crate::dto::entity::device_property::DeviceProperty>>,
    /// 设备指令列表 (不存储在数据库中，由DataServer加载)
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<crate::dto::entity::device_command::DeviceCommand>>,
    /// 设备在线状态 (不存储在数据库中，由DataServer更新)
    #[sqlx(skip)]
    pub is_online: bool,
    /// 最后心跳时间 (不存储在数据库中，由DataServer更新)
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<String>,
}

/// 设备查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceQueryParams {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub organization_id: Option<String>,
}

/// 更新设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub organization_id: Option<String>,
}

/// 设备统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStats {
    pub total_devices: i64,
    pub online_devices: i64,
    pub offline_devices: i64,
    pub alarm_devices: i64,
}

/// 设备状态更新记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStatusUpdate {
    pub device_id: String,
    pub state: i32,
    pub is_online: bool,
    pub last_heartbeat: Option<String>,
    pub updated_at: String,
}

impl Device {
    /// 根据 ID 查找设备
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Device>, sqlx::Error> {
        let device = sqlx::query_as::<_, Device>(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(device)
    }

    /// 根据名称查找设备
    pub async fn find_by_name(db: &Database, name: &str) -> Result<Option<Device>, sqlx::Error> {
        let device = sqlx::query_as::<_, Device>(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(device)
    }

    /// 创建新设备
    pub async fn create(
        db: &Database,
        request: &CreateDeviceRequest,
    ) -> Result<Device, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO devices (
                id, name, display_name, device_type, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
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
        .bind(0) // 默认状态为离线
        .bind(&request.parent_id)
        .bind(&request.product_id)
        .bind(&request.organization_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // 返回创建的设备
        Self::find_by_id(db, &id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新设备信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE devices SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 动态构建更新字段
        if let Some(name) = &request.name {
            if has_updates {
                query.push(", ");
            }
            query.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(display_name) = &request.display_name {
            if has_updates {
                query.push(", ");
            }
            query.push("display_name = ").push_bind(display_name);
            has_updates = true;
        }

        if let Some(device_type) = &request.device_type {
            if has_updates {
                query.push(", ");
            }
            query.push("device_type = ").push_bind(device_type);
            has_updates = true;
        }

        if let Some(address) = &request.address {
            if has_updates {
                query.push(", ");
            }
            query.push("address = ").push_bind(address);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query.push(", ");
            }
            query.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(position) = &request.position {
            if has_updates {
                query.push(", ");
            }
            query.push("position = ").push_bind(position);
            has_updates = true;
        }

        if let Some(driver_name) = &request.driver_name {
            if has_updates {
                query.push(", ");
            }
            query.push("driver_name = ").push_bind(driver_name);
            has_updates = true;
        }

        if let Some(device_model) = &request.device_model {
            if has_updates {
                query.push(", ");
            }
            query.push("device_model = ").push_bind(device_model);
            has_updates = true;
        }

        if let Some(protocol_type) = &request.protocol_type {
            if has_updates {
                query.push(", ");
            }
            query.push("protocol_type = ").push_bind(protocol_type);
            has_updates = true;
        }

        if let Some(factory_name) = &request.factory_name {
            if has_updates {
                query.push(", ");
            }
            query.push("factory_name = ").push_bind(factory_name);
            has_updates = true;
        }

        if let Some(linked_data) = &request.linked_data {
            if has_updates {
                query.push(", ");
            }
            query.push("linked_data = ").push_bind(linked_data);
            has_updates = true;
        }

        if let Some(driver_options) = &request.driver_options {
            if has_updates {
                query.push(", ");
            }
            query.push("driver_options = ").push_bind(driver_options);
            has_updates = true;
        }

        if let Some(state) = &request.state {
            if has_updates {
                query.push(", ");
            }
            query.push("state = ").push_bind(state);
            has_updates = true;
        }

        if let Some(parent_id) = &request.parent_id {
            if has_updates {
                query.push(", ");
            }
            query.push("parent_id = ").push_bind(parent_id);
            has_updates = true;
        }

        if let Some(product_id) = &request.product_id {
            if has_updates {
                query.push(", ");
            }
            query.push("product_id = ").push_bind(product_id);
            has_updates = true;
        }

        if let Some(organization_id) = &request.organization_id {
            if has_updates {
                query.push(", ");
            }
            query.push("organization_id = ").push_bind(organization_id);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id)
                .await?
                .ok_or(sqlx::Error::RowNotFound);
        }

        // 总是更新 updated_at
        query.push(", updated_at = ").push_bind(now);
        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除设备
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 批量删除设备
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut tx = db.pool().begin().await?;
        let mut total_affected = 0u64;

        let mut query = QueryBuilder::new("DELETE FROM devices WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(&mut *tx).await?;
        total_affected += result.rows_affected();

        tx.commit().await?;
        Ok(total_affected)
    }

    /// 查询设备列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &DeviceQueryParams,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE 1=1
            "#,
        );

        // 动态添加查询条件
        if let Some(name) = &params.name {
            query
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(display_name) = &params.display_name {
            query
                .push(" AND display_name LIKE ")
                .push_bind(format!("%{}%", display_name));
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(address) = &params.address {
            query
                .push(" AND address LIKE ")
                .push_bind(format!("%{}%", address));
        }

        if let Some(driver_name) = &params.driver_name {
            query.push(" AND driver_name = ").push_bind(driver_name);
        }

        if let Some(state) = &params.state {
            query.push(" AND state = ").push_bind(state);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        if let Some(product_id) = &params.product_id {
            query.push(" AND product_id = ").push_bind(product_id);
        }

        // 添加排序
        query.push(" ORDER BY created_at DESC");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let devices = query
            .build_query_as::<Device>()
            .fetch_all(db.pool())
            .await?;

        Ok(devices)
    }

    /// 统计设备数量
    pub async fn count(db: &Database, params: &DeviceQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM devices WHERE 1=1");

        if let Some(name) = &params.name {
            query
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(display_name) = &params.display_name {
            query
                .push(" AND display_name LIKE ")
                .push_bind(format!("%{}%", display_name));
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(address) = &params.address {
            query
                .push(" AND address LIKE ")
                .push_bind(format!("%{}%", address));
        }

        if let Some(driver_name) = &params.driver_name {
            query.push(" AND driver_name = ").push_bind(driver_name);
        }

        if let Some(state) = &params.state {
            query.push(" AND state = ").push_bind(state);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        if let Some(product_id) = &params.product_id {
            query.push(" AND product_id = ").push_bind(product_id);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 获取设备统计信息
    pub async fn get_stats(db: &Database) -> Result<DeviceStats, sqlx::Error> {
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
        .fetch_one(db.pool())
        .await?;

        Ok(DeviceStats {
            total_devices: row.get("total_devices"),
            online_devices: row.get("online_devices"),
            offline_devices: row.get("offline_devices"),
            alarm_devices: row.get("alarm_devices"),
        })
    }

    /// 更新设备状态
    pub async fn update_state(db: &Database, id: &str, state: i32) -> Result<(), sqlx::Error> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result = sqlx::query("UPDATE devices SET state = ?, updated_at = ? WHERE id = ?")
            .bind(state)
            .bind(now)
            .bind(id)
            .execute(db.pool())
            .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    /// 根据父设备 ID 查询子设备
    pub async fn find_children(db: &Database, parent_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        let devices = sqlx::query_as::<_, Device>(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE parent_id = ?
            ORDER BY name
            "#,
        )
        .bind(parent_id)
        .fetch_all(db.pool())
        .await?;

        Ok(devices)
    }

    /// 根据产品 ID 查询设备
    pub async fn find_by_product_id(
        db: &Database,
        product_id: &str,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let devices = sqlx::query_as::<_, Device>(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE product_id = ?
            ORDER BY name
            "#,
        )
        .bind(product_id)
        .fetch_all(db.pool())
        .await?;

        Ok(devices)
    }

    /// 根据驱动名称查询设备
    pub async fn find_by_driver_name(
        db: &Database,
        driver_name: &str,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let devices = sqlx::query_as::<_, Device>(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE driver_name = ?
            ORDER BY name
            "#,
        )
        .bind(driver_name)
        .fetch_all(db.pool())
        .await?;

        Ok(devices)
    }

    /// 检查设备名称是否存在
    pub async fn exists_by_name(db: &Database, name: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM devices WHERE name = ?")
            .bind(name)
            .fetch_one(db.pool())
            .await?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    /// 检查设备是否存在（按名称）
    pub async fn exists(db: &Database, name: &str) -> Result<bool, sqlx::Error> {
        Self::exists_by_name(db, name).await
    }

    /// 根据 ID 列表查询设备
    pub async fn find_by_ids(db: &Database, ids: &[String]) -> Result<Vec<Device>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE id IN (
            "#,
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let devices = query
            .build_query_as::<Device>()
            .fetch_all(db.pool())
            .await?;

        Ok(devices)
    }

    /// 获取设备属性 (向后兼容方法)
    pub async fn get_device_properties(
        &self,
        db: &Database,
    ) -> Result<Vec<crate::dto::entity::device_property::DeviceProperty>, sqlx::Error> {
        use crate::dto::entity::device_property::DeviceProperty;
        DeviceProperty::find_by_device_id(db, &self.id).await
    }

    /// 批量创建设备
    pub async fn create_batch(
        db: &Database,
        requests: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>, sqlx::Error> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = db.pool().begin().await?;
        let mut created_devices = Vec::new();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for request in requests {
            let id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO devices (
                    id, name, display_name, device_type, address, description, position,
                    driver_name, device_model, protocol_type, factory_name, linked_data,
                    driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
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
            .bind(0) // 默认状态为离线
            .bind(&request.parent_id)
            .bind(&request.product_id)
            .bind(&request.organization_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            // 创建设备对象
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
                organization_id: request.organization_id.clone(),
                created_at: Some(now.clone()),
                updated_at: Some(now.clone()),
                tags: None,           // 批量创建时不加载标签，需要单独调用
                properties: None,     // 默认无属性数据
                commands: None,       // 默认无指令数据
                is_online: false,     // 默认离线
                last_heartbeat: None, // 默认无心跳
            };

            created_devices.push(device);
        }

        tx.commit().await?;
        Ok(created_devices)
    }

    /// 批量更新设备状态
    pub async fn update_states_batch(
        db: &Database,
        updates: &[(String, i32)],
    ) -> Result<u64, sqlx::Error> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = db.pool().begin().await?;
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

    /// 批量更新设备状态（带详细信息）
    pub async fn update_status_batch(
        db: &Database,
        updates: &[DeviceStatusUpdate],
    ) -> Result<u64, sqlx::Error> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = db.pool().begin().await?;
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

    /// 获取设备树结构（包含子设备）
    pub async fn get_device_tree(
        db: &Database,
        root_id: Option<&str>,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE 
            "#,
        );

        if let Some(root_id) = root_id {
            query.push("parent_id = ").push_bind(root_id);
        } else {
            query.push("parent_id IS NULL");
        }

        query.push(" ORDER BY name");

        let devices = query
            .build_query_as::<Device>()
            .fetch_all(db.pool())
            .await?;

        Ok(devices)
    }

    /// 获取设备统计信息（按类型分组）
    pub async fn get_stats_by_type(db: &Database) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
            FROM devices 
            GROUP BY device_type 
            ORDER BY count DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let device_type: String = row.get("device_type");
            let count: i64 = row.get("count");
            stats.push((device_type, count));
        }

        Ok(stats)
    }

    /// 获取设备统计信息（按驱动分组）
    pub async fn get_stats_by_driver(db: &Database) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(driver_name, 'Unknown') as driver_name, COUNT(*) as count
            FROM devices 
            GROUP BY driver_name 
            ORDER BY count DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let driver_name: String = row.get("driver_name");
            let count: i64 = row.get("count");
            stats.push((driver_name, count));
        }

        Ok(stats)
    }

    /// 搜索设备（模糊匹配多个字段）
    pub async fn search(
        db: &Database,
        keyword: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let search_pattern = format!("%{}%", keyword);
        let exact_pattern = format!("{}%", keyword);

        let mut query_str = String::from(
            r#"
            SELECT id, name, display_name, device_type, address, description, position,
                   driver_name, device_model, protocol_type, factory_name, linked_data,
                   driver_options, state, parent_id, product_id, organization_id, created_at, updated_at
            FROM devices WHERE 
                name LIKE ? OR 
                display_name LIKE ? OR 
                address LIKE ? OR 
                description LIKE ?
            ORDER BY 
                CASE 
                    WHEN name LIKE ? THEN 1
                    WHEN display_name LIKE ? THEN 2
                    WHEN address LIKE ? THEN 3
                    ELSE 4
                END, name
            "#,
        );

        if let Some(limit) = limit {
            query_str.push_str(&format!(" LIMIT {}", limit));
        }

        let devices = sqlx::query_as::<_, Device>(&query_str)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&exact_pattern)
            .bind(&exact_pattern)
            .bind(&exact_pattern)
            .fetch_all(db.pool())
            .await?;

        Ok(devices)
    }

    /// 检查设备是否在线
    pub fn is_online(&self) -> bool {
        self.state.is_some_and(|s| s == 1)
    }

    /// 检查设备是否离线
    pub fn is_offline(&self) -> bool {
        self.state.is_none_or(|s| s == 0 || s == 3)
    }

    /// 检查设备是否有告警
    pub fn has_alarm(&self) -> bool {
        self.state.is_some_and(|s| s == 2)
    }

    /// 获取设备状态描述
    pub fn get_state_description(&self) -> &'static str {
        match self.state {
            Some(0) => "离线",
            Some(1) => "在线",
            Some(2) => "告警",
            Some(3) => "故障",
            _ => "未知",
        }
    }

    /// 获取设备显示名称（优先使用 DisplayName，否则使用 Name）
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.name)
    }

    /// 检查设备是否有父设备
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// 检查设备是否关联了产品
    pub fn has_product(&self) -> bool {
        self.product_id.is_some()
    }

    /// 验证设备配置
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("设备名称不能为空".to_string());
        }

        if self.name.len() > 100 {
            return Err("设备名称长度不能超过100个字符".to_string());
        }

        if let Some(display_name) = &self.display_name {
            if display_name.len() > 200 {
                return Err("显示名称长度不能超过200个字符".to_string());
            }
        }

        if let Some(address) = &self.address {
            if address.len() > 500 {
                return Err("地址长度不能超过500个字符".to_string());
            }
        }

        Ok(())
    }

    /// 获取所有设备 (向后兼容方法)
    pub async fn get_all(db: &Database) -> Result<Vec<Device>, sqlx::Error> {
        let params = DeviceQueryParams::default();
        Device::find_all(db, &params).await
    }

    /// 根据 ID 获取设备 (向后兼容方法)
    pub async fn get_device_by_id(db: &Database, id: &str) -> Result<Option<Device>, sqlx::Error> {
        Self::find_by_id(db, id).await
    }
}
impl Default for Device {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            display_name: None,
            device_type: None,
            address: None,
            description: None,
            position: None,
            driver_name: None,
            device_model: None,
            protocol_type: None,
            factory_name: None,
            linked_data: None,
            driver_options: None,
            state: Some(0), // 默认离线状态
            parent_id: None,
            product_id: None,
            organization_id: None,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            tags: None,           // 默认无标签
            properties: None,     // 默认无属性数据
            commands: None,       // 默认无指令数据
            is_online: false,     // 默认离线
            last_heartbeat: None, // 默认无心跳
        }
    }
}

impl Device {
    /// 根据过滤条件查找设备 - 新API兼容方法
    pub async fn find_with_filters(
        db: &Database,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let mut params = DeviceQueryParams::default();
        params.page = Some(page);
        params.page_size = Some(page_size);

        // 根据enabled参数设置状态过滤
        if let Some(enabled) = enabled {
            params.state = Some(if enabled { 1 } else { 0 });
        }

        // 根据search参数设置名称过滤
        if let Some(search) = search {
            params.name = Some(search.to_string());
        }

        Device::find_all(db, &params).await
    }

    /// 更新设备启用状态 - 新API兼容方法
    pub async fn update_enabled_status(
        db: &Database,
        id: &str,
        enabled: bool,
    ) -> Result<bool, sqlx::Error> {
        let state = if enabled { 1 } else { 0 };
        match Device::update_state(db, id, state).await {
            Ok(()) => Ok(true),
            Err(sqlx::Error::RowNotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// 获取设备创建时间 - 新API兼容方法
    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
            .as_ref()
            .and_then(|s| {
                chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .unwrap_or_else(chrono::Utc::now)
    }

    /// 获取设备更新时间 - 新API兼容方法
    pub fn updated_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.updated_at
            .as_ref()
            .and_then(|s| {
                chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .unwrap_or_else(|| self.created_at())
    }

    /// 检查设备是否启用 - 新API兼容方法
    pub fn enabled(&self) -> bool {
        self.is_online()
    }

    /// 获取设备连接配置 - 新API兼容方法
    pub fn connection_config(&self) -> Option<String> {
        self.driver_options.clone()
    }

    /// 加载设备的标签
    pub async fn load_tags(&mut self, db: &Database) -> Result<(), sqlx::Error> {
        use crate::dto::entity::tag::Tag;
        let tags = Tag::find_by_target_id(db, &self.id).await?;
        self.tags = Some(tags);
        Ok(())
    }

    /// 为设备列表批量加载标签
    pub async fn load_tags_for_devices(
        db: &Database,
        devices: &mut [Device],
    ) -> Result<(), sqlx::Error> {
        use crate::dto::entity::tag::Tag;

        for device in devices {
            let tags = Tag::find_by_target_id(db, &device.id).await?;
            device.tags = Some(tags);
        }

        Ok(())
    }

    /// 创建设备并返回包含标签的完整信息
    pub async fn create_with_tags(
        db: &Database,
        request: &CreateDeviceRequest,
    ) -> Result<Device, sqlx::Error> {
        let mut device = Self::create(db, request).await?;
        device.load_tags(db).await?;
        Ok(device)
    }

    /// 更新设备并返回包含标签的完整信息
    pub async fn update_with_tags(
        db: &Database,
        id: &str,
        request: &UpdateDeviceRequest,
    ) -> Result<Device, sqlx::Error> {
        let mut device = Self::update(db, id, request).await?;
        device.load_tags(db).await?;
        Ok(device)
    }

    /// 根据ID查找设备并包含标签信息
    pub async fn find_by_id_with_tags(
        db: &Database,
        id: &str,
    ) -> Result<Option<Device>, sqlx::Error> {
        if let Some(mut device) = Self::find_by_id(db, id).await? {
            device.load_tags(db).await?;
            Ok(Some(device))
        } else {
            Ok(None)
        }
    }

    /// 查询设备列表并包含标签信息
    pub async fn find_all_with_tags(
        db: &Database,
        params: &DeviceQueryParams,
    ) -> Result<Vec<Device>, sqlx::Error> {
        let mut devices = Self::find_all(db, params).await?;
        Self::load_tags_for_devices(db, &mut devices).await?;
        Ok(devices)
    }
}
