use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

/// 产品实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Product {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 产品查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ProductQueryParams {
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建产品请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateProductRequest {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
}

/// 更新产品请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
}

impl Product {
    /// 根据 ID 查找产品
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Product>, sqlx::Error> {
        let product = sqlx::query_as::<_, Product>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(product)
    }

    /// 根据名称查找产品
    pub async fn find_by_name(db: &Database, name: &str) -> Result<Option<Product>, sqlx::Error> {
        let product = sqlx::query_as::<_, Product>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(product)
    }

    /// 创建新产品
    pub async fn create(
        db: &Database,
        request: &CreateProductRequest,
    ) -> Result<Product, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO products (
                id, name, description, version, manufacturer, device_type,
                protocol_type, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.version)
        .bind(&request.manufacturer)
        .bind(&request.device_type)
        .bind(&request.protocol_type)
        .bind(&now)
        .bind(&now)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新产品信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateProductRequest,
    ) -> Result<Product, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE products SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if let Some(name) = &request.name {
            if has_updates {
                query.push(", ");
            }
            query.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query.push(", ");
            }
            query.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(version) = &request.version {
            if has_updates {
                query.push(", ");
            }
            query.push("version = ").push_bind(version);
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

        if !has_updates {
            return Self::find_by_id(db, id)
                .await?
                .ok_or(sqlx::Error::RowNotFound);
        }

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

    /// 删除产品
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM products WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 查询产品列表
    pub async fn find_all(
        db: &Database,
        params: &ProductQueryParams,
    ) -> Result<Vec<Product>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE 1=1
            "#,
        );

        if let Some(name) = &params.name {
            query
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &params.manufacturer {
            query
                .push(" AND manufacturer LIKE ")
                .push_bind(format!("%{}%", manufacturer));
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let products = query
            .build_query_as::<Product>()
            .fetch_all(db.pool())
            .await?;

        Ok(products)
    }

    /// 统计产品数量
    pub async fn count(db: &Database, params: &ProductQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM products WHERE 1=1");

        if let Some(name) = &params.name {
            query
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &params.manufacturer {
            query
                .push(" AND manufacturer LIKE ")
                .push_bind(format!("%{}%", manufacturer));
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 检查产品名称是否存在
    pub async fn exists_by_name(db: &Database, name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE name = ?")
            .bind(name)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 根据设备类型查询产品
    pub async fn find_by_device_type(
        db: &Database,
        device_type: &str,
    ) -> Result<Vec<Product>, sqlx::Error> {
        let products = sqlx::query_as::<_, Product>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE device_type = ?
            ORDER BY name
            "#,
        )
        .bind(device_type)
        .fetch_all(db.pool())
        .await?;

        Ok(products)
    }

    /// 根据制造商查询产品
    pub async fn find_by_manufacturer(
        db: &Database,
        manufacturer: &str,
    ) -> Result<Vec<Product>, sqlx::Error> {
        let products = sqlx::query_as::<_, Product>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE manufacturer = ?
            ORDER BY name
            "#,
        )
        .bind(manufacturer)
        .fetch_all(db.pool())
        .await?;

        Ok(products)
    }

    /// 搜索产品
    pub async fn search(
        db: &Database,
        keyword: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Product>, sqlx::Error> {
        let search_pattern = format!("%{}%", keyword);

        let mut query_str = String::from(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE 
                name LIKE ? OR 
                description LIKE ? OR 
                manufacturer LIKE ?
            ORDER BY name
            "#,
        );

        if let Some(limit) = limit {
            query_str.push_str(&format!(" LIMIT {}", limit));
        }

        let products = sqlx::query_as::<_, Product>(&query_str)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(db.pool())
            .await?;

        Ok(products)
    }

    /// 获取产品统计信息（按设备类型分组）
    pub async fn get_stats_by_device_type(
        db: &Database,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
            FROM products 
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

    /// 获取产品统计信息（按制造商分组）
    pub async fn get_stats_by_manufacturer(
        db: &Database,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(manufacturer, 'Unknown') as manufacturer, COUNT(*) as count
            FROM products 
            GROUP BY manufacturer 
            ORDER BY count DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let manufacturer: String = row.get("manufacturer");
            let count: i64 = row.get("count");
            stats.push((manufacturer, count));
        }

        Ok(stats)
    }

    /// 带过滤条件查找产品列表
    pub async fn find_with_filters(
        db: &Database,
        name: Option<String>,
        manufacturer: Option<String>,
        device_type: Option<String>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<Product>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, description, version, manufacturer, device_type, 
                   protocol_type, created_at, updated_at
            FROM products WHERE 1=1
            "#,
        );

        if let Some(name) = &name {
            query
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &manufacturer {
            query
                .push(" AND manufacturer LIKE ")
                .push_bind(format!("%{}%", manufacturer));
        }

        if let Some(device_type) = &device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(page_size) = page_size {
            let offset = page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let products = query
            .build_query_as::<Product>()
            .fetch_all(db.pool())
            .await?;

        Ok(products)
    }

    /// 检查产品名称是否存在（排除指定ID）
    pub async fn exists_by_name_excluding_id(
        db: &Database,
        name: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE name = ? AND id != ?")
                .bind(name)
                .bind(exclude_id)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }
}
