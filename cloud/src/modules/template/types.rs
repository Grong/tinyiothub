use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::shared::persistence::Database;

/// 设备模板实体 - 使用 snake_case 数据库字段
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceTemplate {
    pub id: String,
    pub name: String,
    pub display_name: String,        // JSON格式的多语言显示名称
    pub description: Option<String>, // JSON格式的多语言描述
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: String,        // JSON数组格式
    pub device_info: String, // JSON格式的DeviceInfo
    pub properties: String,  // JSON数组格式的PropertyTemplate
    pub commands: String,    // JSON数组格式的CommandTemplate
    pub is_builtin: i32,     // 是否为内置模板
    pub is_active: i32,      // 是否激活
    pub created_at: String,
    pub updated_at: String,
}

/// 设备信息模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceInfo {
    pub default_name_pattern: String, // 例如: "{manufacturer}_{device_type}_{index}"
    pub default_display_name_pattern: Option<String>,
    pub default_description: Option<HashMap<String, String>>,
    pub default_position: Option<String>,
    pub default_driver_options: Option<String>,
    pub required_fields: Vec<String>, // 用户必须填写的字段
}

/// 属性模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyTemplate {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub data_type: String,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: bool,
    pub is_required: bool,
    pub validation_rules: Option<String>, // JSON格式的验证规则
}

/// 命令模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandTemplate {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub parameters: Option<String>,       // JSON格式的参数定义
    pub parameter_schema: Option<String>, // JSON Schema格式的参数验证
    pub is_required: bool,
}

/// 设备模板查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TemplateQueryParams {
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub keyword: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 模板分类
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateCategory {
    pub name: String,
    pub display_name: String,        // JSON格式的多语言显示名称
    pub description: Option<String>, // JSON格式的多语言描述
    pub sort_order: i32,
    pub is_active: i32,
    pub created_at: String,
    /// 模板数量 (不存储在数据库中，通过关联查询获取)
    #[sqlx(skip)]
    pub template_count: i64,
}

/// 创建设备模板请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceTemplateRequest {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Vec<String>,
    pub device_info: DeviceInfo,
    pub properties: Vec<PropertyTemplate>,
    pub commands: Vec<CommandTemplate>,
}

/// 更新设备模板请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceTemplateRequest {
    pub name: Option<String>,
    pub display_name: Option<HashMap<String, String>>,
    pub description: Option<HashMap<String, String>>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub device_info: Option<DeviceInfo>,
    pub properties: Option<Vec<PropertyTemplate>>,
    pub commands: Option<Vec<CommandTemplate>>,
}

/// 设备创建输入
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCreationInput {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub property_values: HashMap<String, String>, // 属性默认值覆盖
    pub enabled_commands: Vec<String>,            // 用户选择启用的命令
    pub tenant_id: Option<String>,               // Will be set from claims, not from request
    pub workspace_id: Option<String>,            // Will be set from X-Workspace-Id header
}

/// 设备预览
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevicePreview {
    pub device_info: tinyiothub_core::models::device::CreateDeviceRequest,
    pub properties: Vec<tinyiothub_core::models::device_property::CreateDevicePropertyRequest>,
    pub commands: Vec<tinyiothub_core::models::device_command::CreateDeviceCommandRequest>,
    pub warnings: Vec<String>,
}

/// 基于模板创建设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceFromTemplateRequest {
    pub template_id: String,
    pub device_input: DeviceCreationInput,
}

/// 模板需求信息 (用于设备创建向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateRequirements {
    pub template_id: String,
    pub template_name: String,
    pub display_name: String,
    pub required_fields: Vec<String>,
    pub available_properties: Vec<PropertyInfo>,
    pub available_commands: Vec<CommandInfo>,
}

/// 属性信息 (用于向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyInfo {
    pub name: String,
    pub display_name: String,
    pub data_type: String,
    pub is_required: bool,
    pub default_value: Option<String>,
    pub validation_rules: Option<String>,
}

/// 命令信息 (用于向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandInfo {
    pub name: String,
    pub display_name: String,
    pub is_required: bool,
    pub parameters: Option<String>,
}

impl DeviceTemplate {
    /// 根据 ID 查找设备模板
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<DeviceTemplate>, sqlx::Error> {
        let template = sqlx::query_as::<_, DeviceTemplate>(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE id = ? AND is_active = 1
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(template)
    }

    /// 根据名称查找设备模板
    pub async fn find_by_name(
        db: &Database,
        name: &str,
    ) -> Result<Option<DeviceTemplate>, sqlx::Error> {
        let template = sqlx::query_as::<_, DeviceTemplate>(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE name = ? AND is_active = 1
            "#,
        )
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(template)
    }

    /// 创建新设备模板
    pub async fn create(
        db: &Database,
        request: &CreateDeviceTemplateRequest,
    ) -> Result<DeviceTemplate, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 序列化复杂字段为JSON
        let display_name_json = serde_json::to_string(&request.display_name).map_err(|e| {
            sqlx::Error::Protocol(format!("Failed to serialize display_name: {}", e))
        })?;
        let description_json =
            request.description.as_ref().map(serde_json::to_string).transpose().map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize description: {}", e))
            })?;
        let tags_json = serde_json::to_string(&request.tags)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize tags: {}", e)))?;
        let device_info_json = serde_json::to_string(&request.device_info).map_err(|e| {
            sqlx::Error::Protocol(format!("Failed to serialize device_info: {}", e))
        })?;
        let properties_json = serde_json::to_string(&request.properties)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize properties: {}", e)))?;
        let commands_json = serde_json::to_string(&request.commands)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize commands: {}", e)))?;

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO device_templates (
                id, name, display_name, description, version, author, category,
                manufacturer, device_type, protocol_type, driver_name, tags,
                device_info, properties, commands, is_builtin, is_active,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&display_name_json)
        .bind(&description_json)
        .bind(&request.version)
        .bind(&request.author)
        .bind(&request.category)
        .bind(&request.manufacturer)
        .bind(&request.device_type)
        .bind(&request.protocol_type)
        .bind(&request.driver_name)
        .bind(&tags_json)
        .bind(&device_info_json)
        .bind(&properties_json)
        .bind(&commands_json)
        .bind(0) // 默认非内置模板
        .bind(1) // 默认激活
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // 返回创建的模板
        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新设备模板
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateDeviceTemplateRequest,
    ) -> Result<DeviceTemplate, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE device_templates SET ");
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
            let display_name_json = serde_json::to_string(display_name).map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize display_name: {}", e))
            })?;
            query.push("display_name = ").push_bind(display_name_json);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query.push(", ");
            }
            let description_json = serde_json::to_string(description).map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize description: {}", e))
            })?;
            query.push("description = ").push_bind(description_json);
            has_updates = true;
        }

        if let Some(version) = &request.version {
            if has_updates {
                query.push(", ");
            }
            query.push("version = ").push_bind(version);
            has_updates = true;
        }

        if let Some(author) = &request.author {
            if has_updates {
                query.push(", ");
            }
            query.push("author = ").push_bind(author);
            has_updates = true;
        }

        if let Some(category) = &request.category {
            if has_updates {
                query.push(", ");
            }
            query.push("category = ").push_bind(category);
            has_updates = true;
        }

        if let Some(manufacturer) = &request.manufacturer {
            if has_updates {
                query.push(", ");
            }
            query.push("manufacturer = ").push_bind(manufacturer);
            has_updates = true;
        }

        if let Some(device_type) = &request.device_type {
            if has_updates {
                query.push(", ");
            }
            query.push("device_type = ").push_bind(device_type);
            has_updates = true;
        }

        if let Some(protocol_type) = &request.protocol_type {
            if has_updates {
                query.push(", ");
            }
            query.push("protocol_type = ").push_bind(protocol_type);
            has_updates = true;
        }

        if let Some(driver_name) = &request.driver_name {
            if has_updates {
                query.push(", ");
            }
            query.push("driver_name = ").push_bind(driver_name);
            has_updates = true;
        }

        if let Some(tags) = &request.tags {
            if has_updates {
                query.push(", ");
            }
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize tags: {}", e)))?;
            query.push("tags = ").push_bind(tags_json);
            has_updates = true;
        }

        if let Some(device_info) = &request.device_info {
            if has_updates {
                query.push(", ");
            }
            let device_info_json = serde_json::to_string(device_info).map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize device_info: {}", e))
            })?;
            query.push("device_info = ").push_bind(device_info_json);
            has_updates = true;
        }

        if let Some(properties) = &request.properties {
            if has_updates {
                query.push(", ");
            }
            let properties_json = serde_json::to_string(properties).map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize properties: {}", e))
            })?;
            query.push("properties = ").push_bind(properties_json);
            has_updates = true;
        }

        if let Some(commands) = &request.commands {
            if has_updates {
                query.push(", ");
            }
            let commands_json = serde_json::to_string(commands).map_err(|e| {
                sqlx::Error::Protocol(format!("Failed to serialize commands: {}", e))
            })?;
            query.push("commands = ").push_bind(commands_json);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        // 总是更新 updated_at
        query.push(", updated_at = ").push_bind(now);
        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除设备模板（软删除）
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result =
            sqlx::query("UPDATE device_templates SET is_active = 0, updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(id)
                .execute(db.pool())
                .await?;

        Ok(result.rows_affected())
    }

    /// 标记模板为内置模板
    pub async fn set_builtin(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE device_templates SET is_builtin = 1 WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 查询设备模板列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &TemplateQueryParams,
    ) -> Result<Vec<DeviceTemplate>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE is_active = 1
            "#,
        );

        // 动态添加查询条件
        if let Some(category) = &params.category {
            query.push(" AND category = ").push_bind(category);
        }

        if let Some(manufacturer) = &params.manufacturer {
            query.push(" AND manufacturer = ").push_bind(manufacturer);
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        if let Some(keyword) = &params.keyword {
            query
                .push(" AND (name LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(" OR display_name LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(" OR tags LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(")");
        }

        // 添加排序
        query.push(" ORDER BY is_builtin DESC, category, name");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let templates = query.build_query_as::<DeviceTemplate>().fetch_all(db.pool()).await?;

        Ok(templates)
    }

    /// 统计设备模板数量
    pub async fn count(db: &Database, params: &TemplateQueryParams) -> Result<i64, sqlx::Error> {
        let mut query =
            QueryBuilder::new("SELECT COUNT(*) as count FROM device_templates WHERE is_active = 1");

        if let Some(category) = &params.category {
            query.push(" AND category = ").push_bind(category);
        }

        if let Some(manufacturer) = &params.manufacturer {
            query.push(" AND manufacturer = ").push_bind(manufacturer);
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        if let Some(keyword) = &params.keyword {
            query
                .push(" AND (name LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(" OR display_name LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(" OR tags LIKE ")
                .push_bind(format!("%{}%", keyword))
                .push(")");
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 根据分类查询设备模板
    pub async fn find_by_category(
        db: &Database,
        category: &str,
    ) -> Result<Vec<DeviceTemplate>, sqlx::Error> {
        let templates = sqlx::query_as::<_, DeviceTemplate>(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE category = ? AND is_active = 1
            ORDER BY is_builtin DESC, name
            "#,
        )
        .bind(category)
        .fetch_all(db.pool())
        .await?;

        Ok(templates)
    }

    /// 搜索设备模板
    pub async fn search(
        db: &Database,
        keyword: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, sqlx::Error> {
        let search_pattern = format!("%{}%", keyword);

        let mut query_str = String::from(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE is_active = 1 AND (
                name LIKE ? OR
                display_name LIKE ? OR
                tags LIKE ?
            )
            ORDER BY is_builtin DESC, name
            "#,
        );

        if let Some(limit) = limit {
            query_str.push_str(&format!(" LIMIT {}", limit));
        }

        let templates = sqlx::query_as::<_, DeviceTemplate>(sqlx::AssertSqlSafe(query_str.clone()))
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(db.pool())
            .await?;

        Ok(templates)
    }

    /// 加载内置模板
    pub async fn load_builtin_templates(db: &Database) -> Result<Vec<DeviceTemplate>, sqlx::Error> {
        let templates = sqlx::query_as::<_, DeviceTemplate>(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE is_builtin = 1 AND is_active = 1
            ORDER BY category, name
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(templates)
    }

    /// 检查模板名称是否存在
    pub async fn exists_by_name(db: &Database, name: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM device_templates WHERE name = ? AND is_active = 1",
        )
        .bind(name)
        .fetch_one(db.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    /// 解析显示名称（多语言支持）
    pub fn get_display_name(&self, language: &str) -> String {
        if let Ok(display_names) =
            serde_json::from_str::<HashMap<String, String>>(&self.display_name)
        {
            display_names
                .get(language)
                .or_else(|| display_names.get("zh")) // 回退到中文
                .or_else(|| display_names.values().next()) // 回退到任意语言
                .cloned()
                .unwrap_or_else(|| self.name.clone())
        } else {
            self.name.clone()
        }
    }

    /// 解析描述（多语言支持）
    pub fn get_description(&self, language: &str) -> Option<String> {
        self.description.as_ref().and_then(|desc_json| {
            serde_json::from_str::<HashMap<String, String>>(desc_json).ok().and_then(
                |descriptions| {
                    descriptions
                        .get(language)
                        .or_else(|| descriptions.get("zh")) // 回退到中文
                        .or_else(|| descriptions.values().next()) // 回退到任意语言
                        .cloned()
                },
            )
        })
    }

    /// 解析标签
    pub fn get_tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// 解析设备信息
    pub fn get_device_info(&self) -> Result<DeviceInfo, serde_json::Error> {
        serde_json::from_str(&self.device_info)
    }

    /// 解析属性模板
    pub fn get_properties(&self) -> Result<Vec<PropertyTemplate>, serde_json::Error> {
        serde_json::from_str(&self.properties)
    }

    /// 解析命令模板
    pub fn get_commands(&self) -> Result<Vec<CommandTemplate>, serde_json::Error> {
        serde_json::from_str(&self.commands)
    }

    /// 检查是否为内置模板
    pub fn is_builtin(&self) -> bool {
        self.is_builtin == 1
    }

    /// 检查是否激活
    pub fn is_active(&self) -> bool {
        self.is_active == 1
    }
}

impl TemplateCategory {
    /// 获取所有模板分类
    pub async fn get_categories(db: &Database) -> Result<Vec<TemplateCategory>, sqlx::Error> {
        let mut categories = sqlx::query_as::<_, TemplateCategory>(
            r#"
            SELECT name, display_name, description, sort_order, is_active, created_at
            FROM template_categories WHERE is_active = 1
            ORDER BY sort_order, name
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        // 为每个分类计算模板数量
        for category in &mut categories {
            let count_row = sqlx::query(
                "SELECT COUNT(*) as count FROM device_templates WHERE category = ? AND is_active = 1"
            )
            .bind(&category.name)
            .fetch_one(db.pool())
            .await?;

            category.template_count = count_row.get("count");
        }

        Ok(categories)
    }

    /// 解析显示名称（多语言支持）
    pub fn get_display_name(&self, language: &str) -> String {
        if let Ok(display_names) =
            serde_json::from_str::<HashMap<String, String>>(&self.display_name)
        {
            display_names
                .get(language)
                .or_else(|| display_names.get("zh")) // 回退到中文
                .or_else(|| display_names.values().next()) // 回退到任意语言
                .cloned()
                .unwrap_or_else(|| self.name.clone())
        } else {
            self.name.clone()
        }
    }

    /// 解析描述（多语言支持）
    pub fn get_description(&self, language: &str) -> Option<String> {
        self.description.as_ref().and_then(|desc_json| {
            serde_json::from_str::<HashMap<String, String>>(desc_json).ok().and_then(
                |descriptions| {
                    descriptions
                        .get(language)
                        .or_else(|| descriptions.get("zh")) // 回退到中文
                        .or_else(|| descriptions.values().next()) // 回退到任意语言
                        .cloned()
                },
            )
        })
    }
}

impl Default for DeviceTemplate {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            display_name: "{}".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            category: String::new(),
            manufacturer: None,
            device_type: String::new(),
            protocol_type: None,
            driver_name: None,
            tags: "[]".to_string(),
            device_info: "{}".to_string(),
            properties: "[]".to_string(),
            commands: "[]".to_string(),
            is_builtin: 0,
            is_active: 1,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
