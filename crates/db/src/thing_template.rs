//! Thing 模板持久化：thing_templates / template_categories 表
//!（自 cloud domains/thing/template/{types,repo}.rs、marketplace installer、
//! import_export 迁入，Task 12）。
//!
//! 类型随 repo 住 db：ThingTemplate/TemplateCategory 等行类型与请求类型，
//! cloud 侧直接引用本模块路径。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, SqlitePool};
use tinyiothub_core::models::template_error::TemplateError;
use tracing::{debug, info, warn};

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型 — 自 cloud template/types.rs 迁入
// ──────────────────────────────────────────────

/// 设备模板实体 - 使用 snake_case 数据库字段
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ThingTemplate {
    pub id: String,
    pub name: String,
    pub display_name: String,        // JSON格式的多语言显示名称
    pub description: Option<String>, // JSON格式的多语言描述
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: String,        // JSON数组格式
    pub device_info: String, // JSON格式的ThingInfo
    pub properties: String,  // JSON数组格式的PropertyTemplate
    pub actions: String,     // JSON数组格式的CommandTemplate
    pub is_builtin: i32,     // 是否为内置模板
    pub is_active: i32,      // 是否激活
    pub created_at: String,
    pub updated_at: String,
    pub workspace_id: Option<String>,
}

/// 设备信息模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ThingInfo {
    pub default_name_pattern: String, // 例如: "{manufacturer}_{device_type}_{index}"
    pub default_display_name_pattern: Option<HashMap<String, String>>,
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
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
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
pub struct CreateThingTemplateRequest {
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
    pub device_info: ThingInfo,
    pub properties: Vec<PropertyTemplate>,
    pub commands: Vec<CommandTemplate>,
    pub workspace_id: Option<String>,
}

/// 更新设备模板请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateThingTemplateRequest {
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
    pub device_info: Option<ThingInfo>,
    pub properties: Option<Vec<PropertyTemplate>>,
    pub commands: Option<Vec<CommandTemplate>>,
}

/// 模板筛选条件
#[derive(Debug, Clone, Default)]
pub struct TemplateFilters {
    pub categories: Vec<String>,
    pub manufacturers: Vec<String>,
    pub protocol_types: Vec<String>,
    pub device_types: Vec<String>,
    pub tags: Vec<String>,
    pub is_builtin: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// ──────────────────────────────────────────────
// 行类型 — 自 import_export.rs / marketplace installer 迁入
// ──────────────────────────────────────────────

/// thing_templates 子集行（import/export 用）。
#[derive(Debug, sqlx::FromRow)]
pub struct ThingTemplateRow {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: String,
    pub category: String,
    pub thing_type: String,
    pub properties: String, // JSON array
    pub actions: String,    // JSON array
    pub events: String,     // JSON array
    pub default_knowledge: Option<String>,
    pub workspace_id: Option<String>,
}

/// ParsedTemplate (internal representation, import 用)
#[derive(Debug, Clone)]
pub struct ParsedTemplate {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub thing_type: String,
    pub device_type: String,
    pub properties: String, // JSON array
    pub actions: String,    // JSON array
    pub events: String,     // JSON array
}

/// Lightweight thing_template row for marketplace listing.
#[derive(Debug, sqlx::FromRow)]
pub struct ThingTemplateListRow {
    pub id: String,
    pub name: String,
    pub thing_type: String,
    pub description: Option<String>,
    pub properties: String,
    pub actions: String,
    pub events: String,
    pub is_builtin: i32,
    pub category: String,
    pub created_at: String,
}

/// thing_templates 全量行（marketplace install 复制源）。
#[derive(Debug, sqlx::FromRow)]
pub struct ThingTemplateFullRow {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub thing_type: String,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: String,
    pub device_info: String,
    pub properties: String,
    pub actions: String,
    pub events: String,
    pub default_knowledge: Option<String>,
}

// ──────────────────────────────────────────────
// 类型辅助方法（纯函数，自 cloud 迁入）
// ──────────────────────────────────────────────

impl ThingTemplate {
    /// 从文件加载的请求直接转换为 ThingTemplate（不经过数据库）
    pub fn from_request(request: &CreateThingTemplateRequest) -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        ThingTemplate {
            id: format!("builtin_{}", request.name),
            name: request.name.clone(),
            display_name: serde_json::to_string(&request.display_name).unwrap_or_default(),
            description: request
                .description
                .as_ref()
                .map(|d| serde_json::to_string(d).unwrap_or_default()),
            version: request.version.clone(),
            author: request.author.clone(),
            category: request.category.clone(),
            manufacturer: request.manufacturer.clone(),
            protocol_type: request.protocol_type.clone(),
            driver_name: request.driver_name.clone(),
            tags: serde_json::to_string(&request.tags).unwrap_or_default(),
            device_info: serde_json::to_string(&request.device_info).unwrap_or_default(),
            properties: serde_json::to_string(&request.properties).unwrap_or_default(),
            actions: serde_json::to_string(&request.commands).unwrap_or_default(),
            is_builtin: 1,
            is_active: 1,
            created_at: now.clone(),
            updated_at: now,
            workspace_id: None,
        }
    }

    /// 解析显示名称（多语言支持）
    pub fn get_display_name(&self, language: &str) -> String {
        if let Ok(display_names) = serde_json::from_str::<HashMap<String, String>>(&self.display_name) {
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
            serde_json::from_str::<HashMap<String, String>>(desc_json)
                .ok()
                .and_then(|descriptions| {
                    descriptions
                        .get(language)
                        .or_else(|| descriptions.get("zh")) // 回退到中文
                        .or_else(|| descriptions.values().next()) // 回退到任意语言
                        .cloned()
                })
        })
    }

    /// 解析标签
    pub fn get_tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// 解析设备信息
    pub fn get_thing_info(&self) -> Result<ThingInfo, serde_json::Error> {
        serde_json::from_str(&self.device_info)
    }

    /// 解析属性模板
    pub fn get_properties(&self) -> Result<Vec<PropertyTemplate>, serde_json::Error> {
        serde_json::from_str(&self.properties)
    }

    /// 解析命令模板
    pub fn get_commands(&self) -> Result<Vec<CommandTemplate>, serde_json::Error> {
        serde_json::from_str(&self.actions)
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

impl Default for ThingTemplate {
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
            protocol_type: None,
            driver_name: None,
            tags: "[]".to_string(),
            device_info: "{}".to_string(),
            properties: "[]".to_string(),
            actions: "[]".to_string(),
            is_builtin: 0,
            is_active: 1,
            created_at: now.clone(),
            updated_at: now,
            workspace_id: None,
        }
    }
}

impl TemplateCategory {
    /// 解析显示名称（多语言支持）
    pub fn get_display_name(&self, language: &str) -> String {
        if let Ok(display_names) = serde_json::from_str::<HashMap<String, String>>(&self.display_name) {
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
            serde_json::from_str::<HashMap<String, String>>(desc_json)
                .ok()
                .and_then(|descriptions| {
                    descriptions
                        .get(language)
                        .or_else(|| descriptions.get("zh")) // 回退到中文
                        .or_else(|| descriptions.values().next()) // 回退到任意语言
                        .cloned()
                })
        })
    }
}

// ──────────────────────────────────────────────
// 持久化函数（SQLite）
// ──────────────────────────────────────────────

/// 根据 ID 查找设备模板
pub(crate) async fn find_thing_template_by_id(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<ThingTemplate>, sqlx::Error> {
    let template = sqlx::query_as::<_, ThingTemplate>(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE id = ? AND is_active = 1
              AND (workspace_id IS NULL OR workspace_id = ?)
            "#,
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(template)
}

/// 根据名称查找设备模板
pub(crate) async fn find_thing_template_by_name(
    pool: &SqlitePool,
    name: &str,
    workspace_id: &str,
) -> Result<Option<ThingTemplate>, sqlx::Error> {
    let template = sqlx::query_as::<_, ThingTemplate>(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE name = ? AND is_active = 1
              AND (workspace_id IS NULL OR workspace_id = ?)
            "#,
    )
    .bind(name)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(template)
}

/// 插入新设备模板（裸 INSERT，不含校验；内部事务保持原语义）
pub(crate) async fn insert_thing_template(
    pool: &SqlitePool,
    request: &CreateThingTemplateRequest,
) -> Result<ThingTemplate, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 序列化复杂字段为JSON
    let display_name_json = serde_json::to_string(&request.display_name)
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize display_name: {}", e)))?;
    let description_json = request
        .description
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize description: {}", e)))?;
    let tags_json = serde_json::to_string(&request.tags)
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize tags: {}", e)))?;
    let device_info_json = serde_json::to_string(&request.device_info)
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize device_info: {}", e)))?;
    let properties_json = serde_json::to_string(&request.properties)
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize properties: {}", e)))?;
    let commands_json = serde_json::to_string(&request.commands)
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize commands: {}", e)))?;

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
            INSERT INTO thing_templates (
                id, name, display_name, description, version, author, category,
                manufacturer, protocol_type, driver_name, tags,
                device_info, properties, actions, is_builtin, is_active,
                created_at, updated_at, workspace_id
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
    .bind(&request.workspace_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 返回创建的模板
    find_thing_template_by_id(pool, &id, "")
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 更新设备模板（裸 UPDATE，不含校验）
pub(crate) async fn update_thing_template_row(
    pool: &SqlitePool,
    id: &str,
    request: &UpdateThingTemplateRequest,
) -> Result<ThingTemplate, sqlx::Error> {
    let mut query = QueryBuilder::new("UPDATE thing_templates SET ");
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
        let display_name_json = serde_json::to_string(display_name)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize display_name: {}", e)))?;
        query.push("display_name = ").push_bind(display_name_json);
        has_updates = true;
    }

    if let Some(description) = &request.description {
        if has_updates {
            query.push(", ");
        }
        let description_json = serde_json::to_string(description)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize description: {}", e)))?;
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
        let device_info_json = serde_json::to_string(device_info)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize device_info: {}", e)))?;
        query.push("device_info = ").push_bind(device_info_json);
        has_updates = true;
    }

    if let Some(properties) = &request.properties {
        if has_updates {
            query.push(", ");
        }
        let properties_json = serde_json::to_string(properties)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize properties: {}", e)))?;
        query.push("properties = ").push_bind(properties_json);
        has_updates = true;
    }

    if let Some(commands) = &request.commands {
        if has_updates {
            query.push(", ");
        }
        let commands_json = serde_json::to_string(commands)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize commands: {}", e)))?;
        query.push("actions = ").push_bind(commands_json);
        has_updates = true;
    }

    if !has_updates {
        return find_thing_template_by_id(pool, id, "")
            .await?
            .ok_or(sqlx::Error::RowNotFound);
    }

    // 总是更新 updated_at
    query.push(", updated_at = ").push_bind(now);
    query.push(" WHERE id = ").push_bind(id);

    let result = query.build().execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    find_thing_template_by_id(pool, id, "")
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除设备模板（软删除）
pub(crate) async fn soft_delete_thing_template(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query("UPDATE thing_templates SET is_active = 0, updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// 标记模板为内置模板
pub(crate) async fn set_thing_template_builtin(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE thing_templates SET is_builtin = 1 WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 查询设备模板列表（支持分页和筛选）
pub(crate) async fn find_thing_templates(
    pool: &SqlitePool,
    params: &TemplateQueryParams,
    workspace_id: &str,
) -> Result<Vec<ThingTemplate>, sqlx::Error> {
    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE is_active = 1
            "#,
    );
    query.push(" AND (workspace_id IS NULL OR workspace_id = ");
    query.push_bind(workspace_id);
    query.push(")");

    // 动态添加查询条件
    if let Some(category) = &params.category {
        query.push(" AND category = ").push_bind(category);
    }

    if let Some(manufacturer) = &params.manufacturer {
        query.push(" AND manufacturer = ").push_bind(manufacturer);
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

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    Ok(templates)
}

/// 统计设备模板数量
pub(crate) async fn count_thing_templates(
    pool: &SqlitePool,
    params: &TemplateQueryParams,
    workspace_id: &str,
) -> Result<i64, sqlx::Error> {
    let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM thing_templates WHERE is_active = 1");
    query.push(" AND (workspace_id IS NULL OR workspace_id = ");
    query.push_bind(workspace_id);
    query.push(")");

    if let Some(category) = &params.category {
        query.push(" AND category = ").push_bind(category);
    }

    if let Some(manufacturer) = &params.manufacturer {
        query.push(" AND manufacturer = ").push_bind(manufacturer);
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

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");

    Ok(count)
}

/// 根据分类查询设备模板
pub(crate) async fn find_thing_templates_by_category(
    pool: &SqlitePool,
    category: &str,
    workspace_id: &str,
) -> Result<Vec<ThingTemplate>, sqlx::Error> {
    let templates = sqlx::query_as::<_, ThingTemplate>(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE category = ? AND is_active = 1
              AND (workspace_id IS NULL OR workspace_id = ?)
            ORDER BY is_builtin DESC, name
            "#,
    )
    .bind(category)
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    Ok(templates)
}

/// 搜索设备模板
pub(crate) async fn search_thing_templates(
    pool: &SqlitePool,
    keyword: &str,
    workspace_id: &str,
    limit: Option<u32>,
) -> Result<Vec<ThingTemplate>, sqlx::Error> {
    let search_pattern = format!("%{}%", keyword);

    let mut query_str = String::from(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE is_active = 1 AND (
                name LIKE ? OR
                display_name LIKE ? OR
                tags LIKE ?
            )
              AND (workspace_id IS NULL OR workspace_id = ?)
            ORDER BY is_builtin DESC, name
            "#,
    );

    if let Some(limit) = limit {
        query_str.push_str(&format!(" LIMIT {}", limit));
    }

    let templates = sqlx::query_as::<_, ThingTemplate>(sqlx::AssertSqlSafe(query_str.clone()))
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;

    Ok(templates)
}

/// 加载内置模板
pub(crate) async fn load_builtin_thing_templates(pool: &SqlitePool) -> Result<Vec<ThingTemplate>, sqlx::Error> {
    let templates = sqlx::query_as::<_, ThingTemplate>(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates WHERE is_builtin = 1 AND is_active = 1
            ORDER BY category, name
            "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(templates)
}

/// 检查模板名称是否存在
pub(crate) async fn thing_template_exists_by_name(pool: &SqlitePool, name: &str) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM thing_templates WHERE name = ? AND is_active = 1")
        .bind(name)
        .fetch_one(pool)
        .await?;

    let count: i64 = row.get("count");
    Ok(count > 0)
}

/// 获取所有模板分类（含每分类模板数量）
pub(crate) async fn list_thing_template_categories(pool: &SqlitePool) -> Result<Vec<TemplateCategory>, sqlx::Error> {
    let mut categories = sqlx::query_as::<_, TemplateCategory>(
        r#"
            SELECT name, display_name, description, sort_order, is_active, created_at
            FROM template_categories WHERE is_active = 1
            ORDER BY sort_order, name
            "#,
    )
    .fetch_all(pool)
    .await?;

    // 为每个分类计算模板数量
    for category in &mut categories {
        let count_row =
            sqlx::query("SELECT COUNT(*) as count FROM thing_templates WHERE category = ? AND is_active = 1")
                .bind(&category.name)
                .fetch_one(pool)
                .await?;

        category.template_count = count_row.get("count");
    }

    Ok(categories)
}

// ─── 搜索服务（自 TemplateSearchService 迁入）───

/// 构建搜索条件
fn push_template_search_conditions(query: &mut QueryBuilder<sqlx::Sqlite>, params: &TemplateQueryParams) {
    // 分类筛选
    if let Some(category) = &params.category {
        query.push(" AND category = ").push_bind(category);
    }

    // 厂商筛选
    if let Some(manufacturer) = &params.manufacturer {
        query.push(" AND manufacturer = ").push_bind(manufacturer);
    }

    // 协议类型筛选
    if let Some(protocol_type) = &params.protocol_type {
        query.push(" AND protocol_type = ").push_bind(protocol_type);
    }

    // 关键词搜索
    if let Some(keyword) = &params.keyword {
        let search_pattern = format!("%{}%", keyword);
        query
            .push(" AND (name LIKE ")
            .push_bind(search_pattern.clone())
            .push(" OR display_name LIKE ")
            .push_bind(search_pattern.clone())
            .push(" OR tags LIKE ")
            .push_bind(search_pattern.clone())
            .push(" OR description LIKE ")
            .push_bind(search_pattern)
            .push(")");
    }
}

/// 高级搜索模板
pub(crate) async fn advanced_search_thing_templates(
    pool: &SqlitePool,
    params: &TemplateQueryParams,
) -> Result<Vec<ThingTemplate>, TemplateError> {
    info!("执行高级模板搜索，参数: {:?}", params);

    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates WHERE is_active = 1
            "#,
    );

    // 构建搜索条件
    push_template_search_conditions(&mut query, params);

    // 添加排序
    query.push(" ORDER BY ");
    query.push("is_builtin DESC, "); // 内置模板优先
    query.push("category, name");

    // 添加分页
    if let Some(page_size) = params.page_size {
        let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
        query.push(" LIMIT ").push_bind(page_size as i64);
        query.push(" OFFSET ").push_bind(offset as i64);
    }

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    info!("高级搜索找到 {} 个模板", templates.len());
    Ok(templates)
}

/// 按分类搜索模板
pub(crate) async fn search_thing_templates_by_category(
    pool: &SqlitePool,
    category: &str,
    limit: Option<u32>,
) -> Result<Vec<ThingTemplate>, TemplateError> {
    info!("按分类搜索模板: {}", category);

    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates
            WHERE is_active = 1 AND category =
            "#,
    );
    query.push_bind(category);
    query.push(" ORDER BY is_builtin DESC, name");

    if let Some(limit) = limit {
        query.push(" LIMIT ").push_bind(limit as i64);
    }

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    info!("在分类 {} 中找到 {} 个模板", category, templates.len());
    Ok(templates)
}

/// 按厂商搜索模板
pub(crate) async fn search_thing_templates_by_manufacturer(
    pool: &SqlitePool,
    manufacturer: &str,
    limit: Option<u32>,
) -> Result<Vec<ThingTemplate>, TemplateError> {
    info!("按厂商搜索模板: {}", manufacturer);

    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates
            WHERE is_active = 1 AND manufacturer =
            "#,
    );
    query.push_bind(manufacturer);
    query.push(" ORDER BY is_builtin DESC, category, name");

    if let Some(limit) = limit {
        query.push(" LIMIT ").push_bind(limit as i64);
    }

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    info!("厂商 {} 的模板找到 {} 个", manufacturer, templates.len());
    Ok(templates)
}

/// 按协议类型搜索模板
pub(crate) async fn search_thing_templates_by_protocol(
    pool: &SqlitePool,
    protocol_type: &str,
    limit: Option<u32>,
) -> Result<Vec<ThingTemplate>, TemplateError> {
    info!("按协议类型搜索模板: {}", protocol_type);

    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates
            WHERE is_active = 1 AND protocol_type =
            "#,
    );
    query.push_bind(protocol_type);
    query.push(" ORDER BY is_builtin DESC, category, name");

    if let Some(limit) = limit {
        query.push(" LIMIT ").push_bind(limit as i64);
    }

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    info!("协议类型 {} 的模板找到 {} 个", protocol_type, templates.len());
    Ok(templates)
}

/// 多条件组合筛选
pub(crate) async fn filter_thing_templates(
    pool: &SqlitePool,
    filters: &TemplateFilters,
) -> Result<Vec<ThingTemplate>, TemplateError> {
    info!("执行多条件组合筛选: {:?}", filters);

    let mut query = QueryBuilder::new(
        r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at
            FROM thing_templates WHERE is_active = 1
            "#,
    );

    // 分类筛选
    if !filters.categories.is_empty() {
        query.push(" AND category IN (");
        let mut separated = query.separated(", ");
        for category in &filters.categories {
            separated.push_bind(category);
        }
        separated.push_unseparated(")");
    }

    // 厂商筛选
    if !filters.manufacturers.is_empty() {
        query.push(" AND manufacturer IN (");
        let mut separated = query.separated(", ");
        for manufacturer in &filters.manufacturers {
            separated.push_bind(manufacturer);
        }
        separated.push_unseparated(")");
    }

    // 协议类型筛选
    if !filters.protocol_types.is_empty() {
        query.push(" AND protocol_type IN (");
        let mut separated = query.separated(", ");
        for protocol_type in &filters.protocol_types {
            separated.push_bind(protocol_type);
        }
        separated.push_unseparated(")");
    }

    // 标签筛选
    if !filters.tags.is_empty() {
        for tag in &filters.tags {
            query.push(" AND tags LIKE ").push_bind(format!("%{}%", tag));
        }
    }

    // 内置模板筛选
    if let Some(is_builtin) = filters.is_builtin {
        let builtin_value = if is_builtin { 1 } else { 0 };
        query.push(" AND is_builtin = ").push_bind(builtin_value);
    }

    // 添加排序
    query.push(" ORDER BY is_builtin DESC, category, name");

    // 添加分页
    if let Some(limit) = filters.limit {
        query.push(" LIMIT ").push_bind(limit as i64);
        if let Some(offset) = filters.offset {
            query.push(" OFFSET ").push_bind(offset as i64);
        }
    }

    let templates = query.build_query_as::<ThingTemplate>().fetch_all(pool).await?;

    info!("组合筛选找到 {} 个模板", templates.len());
    Ok(templates)
}

/// 获取搜索建议
pub(crate) async fn get_thing_template_search_suggestions(
    pool: &SqlitePool,
    keyword: &str,
    limit: u32,
) -> Result<Vec<String>, TemplateError> {
    info!("获取搜索建议，关键词: {}", keyword);

    let search_pattern = format!("%{}%", keyword);

    let suggestions = sqlx::query(
        r#"
            SELECT DISTINCT name as suggestion FROM thing_templates
            WHERE is_active = 1 AND name LIKE ?
            UNION
            SELECT DISTINCT category as suggestion FROM thing_templates
            WHERE is_active = 1 AND category LIKE ?
            UNION
            SELECT DISTINCT manufacturer as suggestion FROM thing_templates
            WHERE is_active = 1 AND manufacturer IS NOT NULL AND manufacturer LIKE ?
            UNION
            SELECT DISTINCT protocol_type as suggestion FROM thing_templates
            WHERE is_active = 1 AND protocol_type IS NOT NULL AND protocol_type LIKE ?
            ORDER BY suggestion
            LIMIT ?
            "#,
    )
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let suggestions: Vec<String> = suggestions
        .into_iter()
        .map(|row| row.get::<String, _>("suggestion"))
        .collect();

    debug!("找到 {} 个搜索建议", suggestions.len());
    Ok(suggestions)
}

/// 获取热门搜索关键词
pub(crate) async fn get_popular_thing_template_keywords(
    pool: &SqlitePool,
    limit: u32,
) -> Result<Vec<String>, TemplateError> {
    info!("获取热门搜索关键词");

    let popular = sqlx::query(
        r#"
            SELECT category as keyword, COUNT(*) as count FROM thing_templates
            WHERE is_active = 1
            GROUP BY category
            UNION
            SELECT manufacturer as keyword, COUNT(*) as count FROM thing_templates
            WHERE is_active = 1 AND manufacturer IS NOT NULL
            GROUP BY manufacturer
            UNION
            SELECT protocol_type as keyword, COUNT(*) as count FROM thing_templates
            WHERE is_active = 1 AND protocol_type IS NOT NULL
            GROUP BY protocol_type
            ORDER BY count DESC
            LIMIT ?
            "#,
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let keywords: Vec<String> = popular.into_iter().map(|row| row.get::<String, _>("keyword")).collect();

    debug!("找到 {} 个热门关键词", keywords.len());
    Ok(keywords)
}

/// 统计搜索结果数量
pub(crate) async fn count_thing_template_search_results(
    pool: &SqlitePool,
    params: &TemplateQueryParams,
) -> Result<i64, TemplateError> {
    let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM thing_templates WHERE is_active = 1");

    push_template_search_conditions(&mut query, params);

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");

    Ok(count)
}

// ─── 组合操作（自 TemplateRepository 校验逻辑迁入）───

/// 创建新模板（含名称唯一性与分类校验）
pub(crate) async fn create_thing_template(
    pool: &SqlitePool,
    request: &CreateThingTemplateRequest,
) -> Result<ThingTemplate, TemplateError> {
    info!("创建新设备模板: {}", request.name);

    if thing_template_exists_by_name(pool, &request.name).await? {
        return Err(TemplateError::TemplateNameExists {
            name: request.name.clone(),
        });
    }

    let categories = list_thing_template_categories(pool).await?;
    if !categories.iter().any(|c| c.name == request.category) {
        return Err(TemplateError::CategoryNotFound {
            category: request.category.clone(),
        });
    }

    let template = insert_thing_template(pool, request).await?;

    info!("成功创建设备模板: {} (ID: {})", template.name, template.id);
    Ok(template)
}

/// 更新模板（含存在性、名称冲突与分类校验）
pub(crate) async fn update_thing_template(
    pool: &SqlitePool,
    id: &str,
    request: &UpdateThingTemplateRequest,
) -> Result<ThingTemplate, TemplateError> {
    info!("更新设备模板: {}", id);

    if find_thing_template_by_id(pool, id, "").await?.is_none() {
        return Err(TemplateError::TemplateNotFound { id: id.to_string() });
    }

    if let Some(new_name) = &request.name {
        let existing = find_thing_template_by_name(pool, new_name, "").await?;
        if let Some(existing_template) = existing
            && existing_template.id != id
        {
            return Err(TemplateError::TemplateNameExists { name: new_name.clone() });
        }
    }

    if let Some(new_category) = &request.category {
        let categories = list_thing_template_categories(pool).await?;
        if !categories.iter().any(|c| c.name == *new_category) {
            return Err(TemplateError::CategoryNotFound {
                category: new_category.clone(),
            });
        }
    }

    let template = update_thing_template_row(pool, id, request).await?;

    info!("成功更新设备模板: {} (ID: {})", template.name, template.id);
    Ok(template)
}

/// 删除模板（含存在性校验，软删除）
pub(crate) async fn delete_thing_template(pool: &SqlitePool, id: &str) -> Result<bool, TemplateError> {
    info!("删除设备模板: {}", id);

    if find_thing_template_by_id(pool, id, "").await?.is_none() {
        return Err(TemplateError::TemplateNotFound { id: id.to_string() });
    }

    let rows_affected = soft_delete_thing_template(pool, id).await?;
    let success = rows_affected > 0;

    if success {
        info!("成功删除设备模板: {}", id);
    } else {
        warn!("删除设备模板失败，可能已被删除: {}", id);
    }

    Ok(success)
}

// ─── thing service 侧模板 JSON 查询（自 thing/service/mod.rs 迁入）───

/// 模板 properties JSON（用于复制到 thing_properties）。
pub(crate) async fn find_thing_template_properties(pool: &SqlitePool, id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT properties FROM thing_templates WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(json,)| json))
}

/// 模板 actions JSON（用于复制到 thing_actions）。
pub(crate) async fn find_thing_template_actions(pool: &SqlitePool, id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT actions FROM thing_templates WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(json,)| json))
}

// ─── marketplace installer / import_export 查询 ───

/// List thing_templates available as marketplace items.
/// Shows built-in templates (workspace_id IS NULL) first,
/// then workspace-scoped templates.
pub(crate) async fn list_marketplace_thing_templates(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<ThingTemplateListRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingTemplateListRow>(
        "SELECT id, name, thing_type, description, properties, actions, events, \
             is_builtin, category, created_at \
             FROM thing_templates WHERE is_active = 1 \
             AND (workspace_id IS NULL OR workspace_id = ?) \
             ORDER BY is_builtin DESC, name",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

/// 按 ID 查 thing_templates 全量行（install 复制源）。
pub(crate) async fn find_thing_template_full(
    pool: &SqlitePool,
    template_id: &str,
) -> Result<Option<ThingTemplateFullRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingTemplateFullRow>(
        "SELECT name, display_name, description, version, author, category, \
             manufacturer, thing_type, protocol_type, driver_name, \
             tags, device_info, properties, actions, events, default_knowledge \
             FROM thing_templates WHERE id = ? AND is_active = 1",
    )
    .bind(template_id)
    .fetch_optional(pool)
    .await
}

/// 将源模板复制插入目标 workspace（marketplace install）。
pub(crate) async fn insert_thing_template_copy(
    pool: &SqlitePool,
    source: &ThingTemplateFullRow,
    new_id: &str,
    final_name: &str,
    target_workspace_id: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO thing_templates \
             (id, name, display_name, description, version, author, category, \
              manufacturer, thing_type, protocol_type, driver_name, \
              tags, device_info, properties, actions, events, default_knowledge, \
              is_builtin, is_active, workspace_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 1, ?, ?, ?)",
    )
    .bind(new_id)
    .bind(final_name)
    .bind(&source.display_name)
    .bind(&source.description)
    .bind(&source.version)
    .bind(&source.author)
    .bind(&source.category)
    .bind(&source.manufacturer)
    .bind(&source.thing_type)
    .bind(&source.protocol_type)
    .bind(&source.driver_name)
    .bind(&source.tags)
    .bind(&source.device_info)
    .bind(&source.properties)
    .bind(&source.actions)
    .bind(&source.events)
    .bind(&source.default_knowledge)
    .bind(target_workspace_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// 统计 workspace + name 冲突数（marketplace install / import 共用）。
pub(crate) async fn count_thing_template_name_conflicts(
    pool: &SqlitePool,
    workspace_key: &str,
    name: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_templates \
         WHERE COALESCE(workspace_id, '') = ? AND name = ?",
    )
    .bind(workspace_key)
    .bind(name)
    .fetch_one(pool)
    .await
}

/// 插入解析后的模板（import；生成 id 与时间戳）。
pub(crate) async fn insert_parsed_thing_template(
    pool: &SqlitePool,
    template: &ParsedTemplate,
    workspace_id: Option<&str>,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let default_knowledge: Option<String> = None;

    let display_name = template.display_name.clone();
    let description = template.description.clone().unwrap_or_default();
    let version = "1.0.0".to_string();
    let category = "imported".to_string();
    let tags = "[]".to_string();
    let device_info = "{}".to_string();

    sqlx::query(
        "INSERT INTO thing_templates \
         (id, name, display_name, description, version, author, category, \
          manufacturer, thing_type, protocol_type, driver_name, \
          tags, device_info, properties, actions, events, default_knowledge, \
          is_builtin, is_active, workspace_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 1, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&template.name)
    .bind(&display_name)
    .bind(&description)
    .bind(&version)
    .bind::<Option<String>>(None) // author
    .bind(&category)
    .bind::<Option<String>>(None) // manufacturer
    .bind(&template.thing_type)
    .bind::<Option<String>>(None) // protocol_type
    .bind::<Option<String>>(None) // driver_name
    .bind(&tags)
    .bind(&device_info)
    .bind(&template.properties)
    .bind(&template.actions)
    .bind(&template.events)
    .bind(&default_knowledge)
    .bind(workspace_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(id)
}

/// 按 ID 加载模板行（export 用）。
pub(crate) async fn find_thing_template_row(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ThingTemplateRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingTemplateRow>(
        "SELECT id, name, display_name, description, version, category, \
         thing_type, properties, actions, events, \
         default_knowledge, workspace_id \
         FROM thing_templates WHERE id = ? AND is_active = 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// 根据 ID 查找设备模板（workspace 作用域）。
    pub async fn find_thing_template_by_id(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<Option<ThingTemplate>, sqlx::Error> {
        find_thing_template_by_id(self.pool(), id, workspace_id).await
    }

    /// 根据名称查找设备模板（workspace 作用域）。
    pub async fn find_thing_template_by_name(
        &self,
        name: &str,
        workspace_id: &str,
    ) -> Result<Option<ThingTemplate>, sqlx::Error> {
        find_thing_template_by_name(self.pool(), name, workspace_id).await
    }

    /// 查询设备模板列表（分页 + 筛选）。
    pub async fn find_thing_templates(
        &self,
        params: &TemplateQueryParams,
        workspace_id: &str,
    ) -> Result<Vec<ThingTemplate>, sqlx::Error> {
        find_thing_templates(self.pool(), params, workspace_id).await
    }

    /// 统计设备模板数量。
    pub async fn count_thing_templates(
        &self,
        params: &TemplateQueryParams,
        workspace_id: &str,
    ) -> Result<i64, sqlx::Error> {
        count_thing_templates(self.pool(), params, workspace_id).await
    }

    /// 根据分类查询设备模板。
    pub async fn find_thing_templates_by_category(
        &self,
        category: &str,
        workspace_id: &str,
    ) -> Result<Vec<ThingTemplate>, sqlx::Error> {
        find_thing_templates_by_category(self.pool(), category, workspace_id).await
    }

    /// 关键词搜索设备模板。
    pub async fn search_thing_templates(
        &self,
        keyword: &str,
        workspace_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<ThingTemplate>, sqlx::Error> {
        search_thing_templates(self.pool(), keyword, workspace_id, limit).await
    }

    /// 加载内置模板。
    pub async fn load_builtin_thing_templates(&self) -> Result<Vec<ThingTemplate>, sqlx::Error> {
        load_builtin_thing_templates(self.pool()).await
    }

    /// 检查模板名称是否存在。
    pub async fn thing_template_exists_by_name(&self, name: &str) -> Result<bool, sqlx::Error> {
        thing_template_exists_by_name(self.pool(), name).await
    }

    /// 获取所有模板分类（含模板数量）。
    pub async fn list_thing_template_categories(&self) -> Result<Vec<TemplateCategory>, sqlx::Error> {
        list_thing_template_categories(self.pool()).await
    }

    /// 标记模板为内置模板。
    pub async fn set_thing_template_builtin(&self, id: &str) -> Result<(), sqlx::Error> {
        set_thing_template_builtin(self.pool(), id).await
    }

    /// 高级搜索模板。
    pub async fn advanced_search_thing_templates(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<Vec<ThingTemplate>, TemplateError> {
        advanced_search_thing_templates(self.pool(), params).await
    }

    /// 按分类搜索模板。
    pub async fn search_thing_templates_by_category(
        &self,
        category: &str,
        limit: Option<u32>,
    ) -> Result<Vec<ThingTemplate>, TemplateError> {
        search_thing_templates_by_category(self.pool(), category, limit).await
    }

    /// 按厂商搜索模板。
    pub async fn search_thing_templates_by_manufacturer(
        &self,
        manufacturer: &str,
        limit: Option<u32>,
    ) -> Result<Vec<ThingTemplate>, TemplateError> {
        search_thing_templates_by_manufacturer(self.pool(), manufacturer, limit).await
    }

    /// 按协议类型搜索模板。
    pub async fn search_thing_templates_by_protocol(
        &self,
        protocol_type: &str,
        limit: Option<u32>,
    ) -> Result<Vec<ThingTemplate>, TemplateError> {
        search_thing_templates_by_protocol(self.pool(), protocol_type, limit).await
    }

    /// 多条件组合筛选模板。
    pub async fn filter_thing_templates(
        &self,
        filters: &TemplateFilters,
    ) -> Result<Vec<ThingTemplate>, TemplateError> {
        filter_thing_templates(self.pool(), filters).await
    }

    /// 获取模板搜索建议。
    pub async fn get_thing_template_search_suggestions(
        &self,
        keyword: &str,
        limit: u32,
    ) -> Result<Vec<String>, TemplateError> {
        get_thing_template_search_suggestions(self.pool(), keyword, limit).await
    }

    /// 获取热门模板搜索关键词。
    pub async fn get_popular_thing_template_keywords(&self, limit: u32) -> Result<Vec<String>, TemplateError> {
        get_popular_thing_template_keywords(self.pool(), limit).await
    }

    /// 统计模板搜索结果数量。
    pub async fn count_thing_template_search_results(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<i64, TemplateError> {
        count_thing_template_search_results(self.pool(), params).await
    }

    /// 创建新模板（含校验）。
    pub async fn create_thing_template(
        &self,
        request: &CreateThingTemplateRequest,
    ) -> Result<ThingTemplate, TemplateError> {
        create_thing_template(self.pool(), request).await
    }

    /// 更新模板（含校验）。
    pub async fn update_thing_template(
        &self,
        id: &str,
        request: &UpdateThingTemplateRequest,
    ) -> Result<ThingTemplate, TemplateError> {
        update_thing_template(self.pool(), id, request).await
    }

    /// 删除模板（含校验，软删除）。
    pub async fn delete_thing_template(&self, id: &str) -> Result<bool, TemplateError> {
        delete_thing_template(self.pool(), id).await
    }

    /// 模板 properties JSON。
    pub async fn find_thing_template_properties(&self, id: &str) -> Result<Option<String>, sqlx::Error> {
        find_thing_template_properties(self.pool(), id).await
    }

    /// 模板 actions JSON。
    pub async fn find_thing_template_actions(&self, id: &str) -> Result<Option<String>, sqlx::Error> {
        find_thing_template_actions(self.pool(), id).await
    }

    /// Marketplace：列出可安装的 thing_templates。
    pub async fn list_marketplace_thing_templates(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ThingTemplateListRow>, sqlx::Error> {
        list_marketplace_thing_templates(self.pool(), workspace_id).await
    }

    /// Marketplace：按 ID 查全量模板行（install 复制源）。
    pub async fn find_thing_template_full(
        &self,
        template_id: &str,
    ) -> Result<Option<ThingTemplateFullRow>, sqlx::Error> {
        find_thing_template_full(self.pool(), template_id).await
    }

    /// Marketplace：将源模板复制插入目标 workspace。
    pub async fn insert_thing_template_copy(
        &self,
        source: &ThingTemplateFullRow,
        new_id: &str,
        final_name: &str,
        target_workspace_id: &str,
        now: &str,
    ) -> Result<(), sqlx::Error> {
        insert_thing_template_copy(self.pool(), source, new_id, final_name, target_workspace_id, now).await
    }

    /// 统计 workspace + name 冲突数。
    pub async fn count_thing_template_name_conflicts(
        &self,
        workspace_key: &str,
        name: &str,
    ) -> Result<i64, sqlx::Error> {
        count_thing_template_name_conflicts(self.pool(), workspace_key, name).await
    }

    /// 插入解析后的模板（import；返回新 id）。
    pub async fn insert_parsed_thing_template(
        &self,
        template: &ParsedTemplate,
        workspace_id: Option<&str>,
    ) -> Result<String, sqlx::Error> {
        insert_parsed_thing_template(self.pool(), template, workspace_id).await
    }

    /// 按 ID 加载模板行（export 用）。
    pub async fn find_thing_template_row(&self, id: &str) -> Result<Option<ThingTemplateRow>, sqlx::Error> {
        find_thing_template_row(self.pool(), id).await
    }
}

// ──────────────────────────────────────────────
// things × thing_templates JOIN 查询（自 cloud event/router.rs、
// shared/mqtt_client.rs 迁入，Task 12）
// ──────────────────────────────────────────────

/// Thing 创建模板的 events JSON（无模板 → None）。
pub(crate) async fn find_thing_template_events_by_thing(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT t.events FROM things d JOIN thing_templates t ON t.id = d.template_id WHERE d.id = ?")
            .bind(thing_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(ev,)| ev))
}

/// Thing 的 (workspace_id, 模板 events JSON)（单次往返）。
pub(crate) async fn find_thing_workspace_and_template_events(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT d.workspace_id, t.events FROM things d              LEFT JOIN thing_templates t ON t.id = d.template_id WHERE d.id = ?",
    )
    .bind(thing_id)
    .fetch_optional(pool)
    .await
}

impl Db {
    /// Thing 创建模板的 events JSON（无模板 → None）。
    pub async fn find_thing_template_events_by_thing(&self, thing_id: &str) -> Result<Option<String>, sqlx::Error> {
        find_thing_template_events_by_thing(self.pool(), thing_id).await
    }

    /// Thing 的 (workspace_id, 模板 events JSON)（单次往返）。
    pub async fn find_thing_workspace_and_template_events(
        &self,
        thing_id: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
        find_thing_workspace_and_template_events(self.pool(), thing_id).await
    }
}
