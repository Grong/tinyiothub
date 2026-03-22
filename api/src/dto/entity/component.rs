use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// Component entity - 组件实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Component {
    pub id: String,
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub device_num: u32,
    pub description: Option<String>,
    pub options_descriptors: String, // JSON string
    pub location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Component option entity - 组件选项实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComponentOption {
    pub label: String,
    pub name: String,
    pub default_value: String,
    pub option_type: String, // "string", "number", "boolean", "select"
    pub required: bool,
    pub description: Option<String>,
}

/// Query parameters for component search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ComponentQuery {
    pub name: Option<String>,
    pub version: Option<String>,
    pub class_name: Option<String>,
    pub location: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new component
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateComponentRequest {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub device_num: Option<u32>,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
    pub location: Option<String>,
}

/// Request for updating a component
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateComponentRequest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub class_name: Option<String>,
    pub device_num: Option<u32>,
    pub description: Option<String>,
    pub options_descriptors: Option<Vec<ComponentOption>>,
    pub location: Option<String>,
}

impl Component {
    /// 根据 ID 查找组件
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Component>, sqlx::Error> {
        let component = sqlx::query_as::<_, Component>(
            "SELECT id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at FROM components WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(component)
    }

    /// 根据名称和版本查找组件
    pub async fn find_by_name_and_version(
        db: &Database,
        name: &str,
        version: &str,
    ) -> Result<Option<Component>, sqlx::Error> {
        let component = sqlx::query_as::<_, Component>(
            "SELECT id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at FROM components WHERE name = ? AND version = ?"
        )
        .bind(name)
        .bind(version)
        .fetch_optional(db.pool())
        .await?;

        Ok(component)
    }

    /// 创建新组件
    pub async fn create(
        db: &Database,
        request: &CreateComponentRequest,
    ) -> Result<Component, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let options_json = serde_json::to_string(&request.options_descriptors)
            .unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO components (id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.version)
        .bind(&request.class_name)
        .bind(request.device_num.unwrap_or(0))
        .bind(&request.description)
        .bind(&options_json)
        .bind(&request.location)
        .bind(&now)
        .bind(&now)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新组件信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateComponentRequest,
    ) -> Result<Component, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE components SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if let Some(name) = &request.name {
            if has_updates {
                query.push(", ");
            }
            query.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(version) = &request.version {
            if has_updates {
                query.push(", ");
            }
            query.push("version = ").push_bind(version);
            has_updates = true;
        }

        if let Some(class_name) = &request.class_name {
            if has_updates {
                query.push(", ");
            }
            query.push("class_name = ").push_bind(class_name);
            has_updates = true;
        }

        if let Some(device_num) = request.device_num {
            if has_updates {
                query.push(", ");
            }
            query.push("device_num = ").push_bind(device_num);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query.push(", ");
            }
            query.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(options_descriptors) = &request.options_descriptors {
            if has_updates {
                query.push(", ");
            }
            let options_json =
                serde_json::to_string(options_descriptors).unwrap_or_else(|_| "[]".to_string());
            query.push("options_descriptors = ").push_bind(options_json);
            has_updates = true;
        }

        if let Some(location) = &request.location {
            if has_updates {
                query.push(", ");
            }
            query.push("location = ").push_bind(location);
            has_updates = true;
        }

        if has_updates {
            query.push(", updated_at = ").push_bind(&now);
        } else {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除组件
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM components WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// 批量删除组件
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM components WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(db.pool()).await?;
        Ok(result.rows_affected())
    }

    /// 查询组件列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &ComponentQuery,
    ) -> Result<Vec<Component>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at FROM components WHERE 1=1"
        );

        // 动态添加查询条件
        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(version) = &params.version {
            query.push(" AND version = ").push_bind(version);
        }

        if let Some(class_name) = &params.class_name {
            query.push(" AND class_name = ").push_bind(class_name);
        }

        if let Some(location) = &params.location {
            query.push(" AND location LIKE ").push_bind(format!("%{}%", location));
        }

        // 添加排序
        query.push(" ORDER BY name, version");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let components = query.build_query_as::<Component>().fetch_all(db.pool()).await?;

        Ok(components)
    }

    /// 统计组件数量
    pub async fn count(db: &Database, params: &ComponentQuery) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM components WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(version) = &params.version {
            query.push(" AND version = ").push_bind(version);
        }

        if let Some(class_name) = &params.class_name {
            query.push(" AND class_name = ").push_bind(class_name);
        }

        if let Some(location) = &params.location {
            query.push(" AND location LIKE ").push_bind(format!("%{}%", location));
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 根据类名查询组件
    pub async fn find_by_class_name(
        db: &Database,
        class_name: &str,
    ) -> Result<Vec<Component>, sqlx::Error> {
        let components = sqlx::query_as::<_, Component>(
            "SELECT id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at FROM components WHERE class_name = ? ORDER BY name, version"
        )
        .bind(class_name)
        .fetch_all(db.pool())
        .await?;

        Ok(components)
    }

    /// 检查组件名称和版本是否存在
    pub async fn exists_by_name_and_version(
        db: &Database,
        name: &str,
        version: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM components WHERE name = ? AND version = ?")
                .bind(name)
                .bind(version)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }

    /// 检查组件名称和版本是否存在（排除指定 ID）
    pub async fn exists_by_name_and_version_exclude_id(
        db: &Database,
        name: &str,
        version: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM components WHERE name = ? AND version = ? AND id != ?",
        )
        .bind(name)
        .bind(version)
        .bind(exclude_id)
        .fetch_one(db.pool())
        .await?;

        Ok(count > 0)
    }

    /// 根据 ID 列表查询组件
    pub async fn find_by_ids(db: &Database, ids: &[String]) -> Result<Vec<Component>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            "SELECT id, name, version, class_name, device_num, description, options_descriptors, location, created_at, updated_at FROM components WHERE id IN ("
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let components = query.build_query_as::<Component>().fetch_all(db.pool()).await?;

        Ok(components)
    }

    /// 获取所有唯一的类名
    pub async fn get_unique_class_names(db: &Database) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query("SELECT DISTINCT class_name FROM components ORDER BY class_name")
            .fetch_all(db.pool())
            .await?;

        let class_names: Vec<String> = rows.iter().map(|row| row.get("class_name")).collect();

        Ok(class_names)
    }

    /// Create a new component
    pub fn new(request: CreateComponentRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let options_json = serde_json::to_string(&request.options_descriptors)
            .unwrap_or_else(|_| "[]".to_string());

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            version: request.version,
            class_name: request.class_name,
            device_num: request.device_num.unwrap_or(0),
            description: request.description,
            options_descriptors: options_json,
            location: request.location,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Get component options as structured data
    pub fn get_options(&self) -> Vec<ComponentOption> {
        serde_json::from_str(&self.options_descriptors).unwrap_or_else(|_| Vec::new())
    }

    /// Set component options from structured data
    pub fn set_options(&mut self, options: Vec<ComponentOption>) {
        self.options_descriptors =
            serde_json::to_string(&options).unwrap_or_else(|_| "[]".to_string());
        self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    }

    /// Check if component has any options
    pub fn has_options(&self) -> bool {
        !self.get_options().is_empty()
    }

    /// Get component full identifier
    pub fn get_full_identifier(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

impl ComponentOption {
    /// Create a new component option
    pub fn new(
        label: String,
        name: String,
        default_value: String,
        option_type: String,
        required: bool,
    ) -> Self {
        Self { label, name, default_value, option_type, required, description: None }
    }

    /// Validate option value based on type
    pub fn validate_value(&self, value: &str) -> Result<(), String> {
        match self.option_type.as_str() {
            "number" => {
                value.parse::<f64>().map_err(|_| format!("Invalid number: {}", value))?;
            }
            "boolean" => {
                if !["true", "false", "1", "0"].contains(&value.to_lowercase().as_str()) {
                    return Err(format!("Invalid boolean: {}", value));
                }
            }
            _ => {} // String and select types are always valid
        }
        Ok(())
    }
}

// Backward compatibility
pub type ComponentInfo = Component;
pub type ComponentOptions = ComponentOption;
pub type ComponentQueryParams = ComponentQuery;
