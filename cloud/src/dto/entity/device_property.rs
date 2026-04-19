use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};

use crate::infrastructure::persistence::database::Database;

/// 设备属性实体 - 使用现代化 SQLx 实现
///
/// 使用 SQLx 最佳实践：
/// - 使用 snake_case 字段名映射到 snake_case 数据库列
/// - 使用类型安全的查询构建器
/// - 分离持久化字段和运行时字段
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceProperty {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: i32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    // 运行时属性（不存储在数据库中）
    #[sqlx(skip)]
    pub current_value: Option<String>,
    #[sqlx(skip)]
    pub alarm_status: Option<i32>,
}

/// 设备属性查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DevicePropertyQueryParams {
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub data_type: Option<String>,
    pub is_read_only: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建设备属性请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDevicePropertyRequest {
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: Option<i32>,
}

/// 更新设备属性请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDevicePropertyRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: Option<i32>,
}

/// 设备属性值更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyValueRequest {
    pub value: String,
    pub timestamp: Option<String>,
}

/// 设备属性统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevicePropertyStats {
    pub total_properties: i64,
    pub read_only_properties: i64,
    pub writable_properties: i64,
    pub alarm_properties: i64,
}

/// Value label for enumeration properties
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ValueLabel {
    pub value: String,
    pub label: String,
}

impl ValueLabel {
    pub fn new(value: String, label: String) -> Self {
        Self { value, label }
    }
}

impl DeviceProperty {
    /// 根据 ID 查找设备属性
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<DeviceProperty>, sqlx::Error> {
        let mut property = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        // 初始化运行时字段

        if let Some(ref mut prop) = property {
            prop.clear_runtime_data();
        }

        Ok(property)
    }

    /// 根据设备 ID 查找所有属性
    pub async fn find_by_device_id(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut properties = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE device_id = ?
            ORDER BY name
            "#,
        )
        .bind(device_id)
        .fetch_all(db.pool())
        .await?;

        // 初始化运行时字段

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok(properties)
    }

    /// 根据设备 ID 和属性名查找属性
    pub async fn find_by_device_and_name(
        db: &Database,
        device_id: &str,
        name: &str,
    ) -> Result<Option<DeviceProperty>, sqlx::Error> {
        let mut property = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE device_id = ? AND name = ?
            "#,
        )
        .bind(device_id)
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        // 初始化运行时字段

        if let Some(ref mut prop) = property {
            prop.clear_runtime_data();
        }

        Ok(property)
    }

    /// 创建新设备属性
    pub async fn create(
        db: &Database,
        request: &CreateDevicePropertyRequest,
    ) -> Result<DeviceProperty, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let is_read_only = request.is_read_only.unwrap_or(0);

        // 使用事务确保数据一致性
        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO device_properties (
                id, device_id, name, display_name, description, data_type, unit,
                min_value, max_value, default_value, is_read_only, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.device_id)
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
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // 返回创建的属性
        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新设备属性
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateDevicePropertyRequest,
    ) -> Result<DeviceProperty, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new("UPDATE device_properties SET ");
        let mut has_updates = false;

        // 动态构建更新字段
        if let Some(name) = &request.name {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(display_name) = &request.display_name {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("display_name = ").push_bind(display_name);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(data_type) = &request.data_type {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("data_type = ").push_bind(data_type);
            has_updates = true;
        }

        if let Some(unit) = &request.unit {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("unit = ").push_bind(unit);
            has_updates = true;
        }

        if let Some(min_value) = request.min_value {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("min_value = ").push_bind(min_value);
            has_updates = true;
        }

        if let Some(max_value) = request.max_value {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("max_value = ").push_bind(max_value);
            has_updates = true;
        }

        if let Some(default_value) = &request.default_value {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("default_value = ").push_bind(default_value);
            has_updates = true;
        }

        if let Some(is_read_only) = request.is_read_only {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("is_read_only = ").push_bind(is_read_only);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query_builder.push(" WHERE id = ").push_bind(id);

        let mut tx = db.pool().begin().await?;
        let result = query_builder.build().execute(&mut *tx).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除设备属性
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM device_properties WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 批量删除设备属性
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query_builder =
            QueryBuilder::<Sqlite>::new("DELETE FROM device_properties WHERE id IN (");
        let mut separated = query_builder.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query_builder.build().execute(db.pool()).await?;
        Ok(result.rows_affected())
    }

    /// 根据设备 ID 删除所有属性
    pub async fn delete_by_device_id(db: &Database, device_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM device_properties WHERE device_id = ?")
            .bind(device_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 查询设备属性列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &DevicePropertyQueryParams,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE 1=1
            "#,
        );

        // 动态添加查询条件
        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(display_name) = &params.display_name {
            query_builder.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
        }

        if let Some(data_type) = &params.data_type {
            query_builder.push(" AND data_type = ").push_bind(data_type);
        }

        if let Some(is_read_only) = params.is_read_only {
            query_builder.push(" AND is_read_only = ").push_bind(is_read_only);
        }

        // 添加排序
        query_builder.push(" ORDER BY device_id, name");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size as i64);
            query_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let mut properties =
            query_builder.build_query_as::<DeviceProperty>().fetch_all(db.pool()).await?;

        // 初始化运行时字段

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok(properties)
    }

    /// 统计设备属性数量
    pub async fn count(
        db: &Database,
        params: &DevicePropertyQueryParams,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM device_properties WHERE 1=1");

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(display_name) = &params.display_name {
            query_builder.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
        }

        if let Some(data_type) = &params.data_type {
            query_builder.push(" AND data_type = ").push_bind(data_type);
        }

        if let Some(is_read_only) = params.is_read_only {
            query_builder.push(" AND is_read_only = ").push_bind(is_read_only);
        }

        let row = query_builder.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }

    /// 获取设备属性统计信息
    pub async fn get_stats(db: &Database) -> Result<DevicePropertyStats, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_properties,
                COUNT(CASE WHEN is_read_only = 1 THEN 1 END) as read_only_properties,
                COUNT(CASE WHEN is_read_only = 0 THEN 1 END) as writable_properties,
                0 as alarm_properties
            FROM device_properties
            "#,
        )
        .fetch_one(db.pool())
        .await?;

        Ok(DevicePropertyStats {
            total_properties: row.get("total_properties"),
            read_only_properties: row.get("read_only_properties"),
            writable_properties: row.get("writable_properties"),
            alarm_properties: row.get("alarm_properties"),
        })
    }

    /// 根据设备 ID 获取统计信息
    pub async fn get_stats_by_device(
        db: &Database,
        device_id: &str,
    ) -> Result<DevicePropertyStats, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_properties,
                COUNT(CASE WHEN is_read_only = 1 THEN 1 END) as read_only_properties,
                COUNT(CASE WHEN is_read_only = 0 THEN 1 END) as writable_properties,
                0 as alarm_properties
            FROM device_properties WHERE device_id = ?
            "#,
        )
        .bind(device_id)
        .fetch_one(db.pool())
        .await?;

        Ok(DevicePropertyStats {
            total_properties: row.get("total_properties"),
            read_only_properties: row.get("read_only_properties"),
            writable_properties: row.get("writable_properties"),
            alarm_properties: row.get("alarm_properties"),
        })
    }

    /// 检查属性名称在设备中是否存在
    pub async fn exists_in_device(
        db: &Database,
        device_id: &str,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        let row =
            sqlx::query("SELECT COUNT(*) FROM device_properties WHERE device_id = ? AND name = ?")
                .bind(device_id)
                .bind(name)
                .fetch_one(db.pool())
                .await?;

        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    /// 根据数据类型查询属性
    pub async fn find_by_data_type(
        db: &Database,
        data_type: &str,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut properties = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE data_type = ?
            ORDER BY device_id, name
            "#,
        )
        .bind(data_type)
        .fetch_all(db.pool())
        .await?;

        // 初始化运行时字段

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok(properties)
    }

    /// 查询可写属性
    pub async fn find_writable_by_device(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut properties = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE device_id = ? AND is_read_only = 0
            ORDER BY name
            "#,
        )
        .bind(device_id)
        .fetch_all(db.pool())
        .await?;

        // 初始化运行时字段

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok(properties)
    }

    /// 查询只读属性
    pub async fn find_readonly_by_device(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut properties = sqlx::query_as::<_, DeviceProperty>(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE device_id = ? AND is_read_only = 1
            ORDER BY name
            "#,
        )
        .bind(device_id)
        .fetch_all(db.pool())
        .await?;

        // 初始化运行时字段

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok(properties)
    }

    /// 批量创建设备属性
    pub async fn create_batch(
        db: &Database,
        requests: &[CreateDevicePropertyRequest],
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        let mut tx = db.pool().begin().await?;
        let mut created_ids = Vec::new();

        for request in requests {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let is_read_only = request.is_read_only.unwrap_or(0);

            sqlx::query(
                r#"
                INSERT INTO device_properties (
                    id, device_id, name, display_name, description, data_type, unit,
                    min_value, max_value, default_value, is_read_only, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&request.device_id)
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
            .execute(&mut *tx)
            .await?;

            created_ids.push(id);
        }

        tx.commit().await?;

        // 获取所有创建的属性
        let mut results = Vec::new();
        for id in created_ids {
            if let Some(property) = Self::find_by_id(db, &id).await? {
                results.push(property);
            }
        }

        Ok(results)
    }

    /// 设置属性当前值（运行时数据，不持久化）
    pub fn set_current_value(&mut self, value: String) {
        self.current_value = Some(value);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
    }

    /// 设置当前值（可选）并更新时间戳
    pub fn set_current_value_option(&mut self, value: Option<String>) {
        if value.is_some() {
            self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
        self.current_value = value;
    }

    /// 清除运行时数据（当前值、告警状态）
    /// 注意：不清除 updated_at，因为它是数据库字段
    pub fn clear_runtime_data(&mut self) {
        self.current_value = None;
        self.alarm_status = None;
    }

    /// 获取最后更新时间（使用 updated_at）
    pub fn get_last_update_time(&self) -> Option<&String> {
        self.updated_at.as_ref()
    }

    /// 设置告警状态（运行时数据，不持久化）
    pub fn set_alarm_status(&mut self, status: i32) {
        self.alarm_status = Some(status);
    }

    /// 验证属性值是否在范围内
    pub fn validate_value(&self, value: &str) -> Result<(), String> {
        // 根据数据类型验证值
        match self.data_type.as_deref() {
            Some("int") | Some("integer") => {
                let val: i64 = value.parse().map_err(|_| "无效的整数值".to_string())?;

                if let Some(min) = self.min_value {
                    if (val as f64) < min {
                        return Err(format!("值 {} 小于最小值 {}", val, min));
                    }
                }

                if let Some(max) = self.max_value {
                    if (val as f64) > max {
                        return Err(format!("值 {} 大于最大值 {}", val, max));
                    }
                }
            }
            Some("float") | Some("double") | Some("number") => {
                let val: f64 = value.parse().map_err(|_| "无效的数值".to_string())?;

                if let Some(min) = self.min_value {
                    if val < min {
                        return Err(format!("值 {} 小于最小值 {}", val, min));
                    }
                }

                if let Some(max) = self.max_value {
                    if val > max {
                        return Err(format!("值 {} 大于最大值 {}", val, max));
                    }
                }
            }
            Some("bool") | Some("boolean") => {
                if !matches!(value.to_lowercase().as_str(), "true" | "false" | "0" | "1") {
                    return Err("无效的布尔值".to_string());
                }
            }
            _ => {
                // 字符串类型或其他类型，暂不验证
            }
        }

        Ok(())
    }

    /// Find properties with pagination and sorting
    pub async fn find_paginated(
        db: &Database,
        params: &DevicePropertyQueryParams,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
    ) -> Result<(Vec<DeviceProperty>, i64), sqlx::Error> {
        // Get total count first
        let total_count = Self::count(db, params).await?;

        // Build the main query
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, name, display_name, description, data_type, unit,
                   min_value, max_value, default_value, is_read_only, created_at, updated_at
            FROM device_properties WHERE 1=1
            "#,
        );

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(name) = &params.name {
            query_builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(display_name) = &params.display_name {
            query_builder.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
        }

        if let Some(data_type) = &params.data_type {
            query_builder.push(" AND data_type = ").push_bind(data_type);
        }

        if let Some(is_read_only) = params.is_read_only {
            query_builder.push(" AND is_read_only = ").push_bind(is_read_only);
        }

        // Add sorting
        let sort_column = match sort_by {
            Some("name") => "name",
            Some("displayName") => "display_name",
            Some("dataType") => "data_type",
            Some("createdAt") => "created_at",
            _ => "name",
        };

        let sort_direction = match sort_order {
            Some("asc") => "ASC",
            _ => "DESC",
        };

        query_builder.push(format!(" ORDER BY {} {}", sort_column, sort_direction));

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let mut properties =
            query_builder.build_query_as::<DeviceProperty>().fetch_all(db.pool()).await?;

        // Initialize runtime fields

        for prop in &mut properties {
            prop.clear_runtime_data();
        }

        Ok((properties, total_count))
    }

    /// 根据设备 ID 获取属性(向后兼容方法)
    pub async fn get_properties_with_device_id(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceProperty>, sqlx::Error> {
        Self::find_by_device_id(db, device_id).await
    }
}

impl Default for DeviceProperty {
    fn default() -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: String::new(),
            name: String::new(),
            display_name: None,
            description: None,
            data_type: Some("string".to_string()),
            unit: None,
            min_value: None,
            max_value: None,
            default_value: None,
            is_read_only: 0,
            created_at: Some(now.clone()),
            updated_at: Some(now),
            current_value: None,
            alarm_status: None,
        }
    }
}
