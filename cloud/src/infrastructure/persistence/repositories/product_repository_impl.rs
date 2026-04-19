use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder, Row};

use crate::domain::product::repository::ProductRepository;
use crate::dto::entity::product::{CreateProductRequest, Product, ProductQueryParams, UpdateProductRequest};
use crate::infrastructure::persistence::database::Database;
use crate::shared::error::Result;

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct ProductRow {
    id: String,
    name: String,
    description: Option<String>,
    version: Option<String>,
    manufacturer: Option<String>,
    device_type: Option<String>,
    protocol_type: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<ProductRow> for Product {
    fn from(row: ProductRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            version: row.version,
            manufacturer: row.manufacturer,
            device_type: row.device_type,
            protocol_type: row.protocol_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub struct SqliteProductRepository {
    database: Database,
}

impl SqliteProductRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl ProductRepository for SqliteProductRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Product>> {
        let row = sqlx::query_as::<_, ProductRow>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Product>> {
        let row = sqlx::query_as::<_, ProductRow>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn create(&self, request: &CreateProductRequest) -> Result<Product> {
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
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdateProductRequest) -> Result<Product> {
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
            return self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound);
        }

        query.push(", updated_at = ").push_bind(now);
        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(crate::shared::error::Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM products WHERE id = ?").bind(id).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn find_all(&self, params: &ProductQueryParams) -> Result<Vec<Product>> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE 1=1
            "#,
        );

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &params.manufacturer {
            query.push(" AND manufacturer LIKE ").push_bind(format!("%{}%", manufacturer));
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

        let rows = query.build_query_as::<ProductRow>().fetch_all(self.database.pool()).await?;
        let products: Vec<Product> = rows.into_iter().map(Into::into).collect();

        Ok(products)
    }

    async fn count(&self, params: &ProductQueryParams) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM products WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &params.manufacturer {
            query.push(" AND manufacturer LIKE ").push_bind(format!("%{}%", manufacturer));
        }

        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE name = ?")
            .bind(name)
            .fetch_one(self.database.pool())
            .await?;

        Ok(count > 0)
    }

    async fn find_by_device_type(&self, device_type: &str) -> Result<Vec<Product>> {
        let rows = sqlx::query_as::<_, ProductRow>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE device_type = ?
            ORDER BY name
            "#,
        )
        .bind(device_type)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_manufacturer(&self, manufacturer: &str) -> Result<Vec<Product>> {
        let rows = sqlx::query_as::<_, ProductRow>(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE manufacturer = ?
            ORDER BY name
            "#,
        )
        .bind(manufacturer)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Product>> {
        let search_pattern = format!("%{}%", keyword);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE
                name LIKE
            "#,
        );

        query.push_bind(&search_pattern);
        query.push(" OR description LIKE ");
        query.push_bind(&search_pattern);
        query.push(" OR manufacturer LIKE ");
        query.push_bind(&search_pattern);
        query.push(" ORDER BY name");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = query.build_query_as::<ProductRow>().fetch_all(self.database.pool()).await?;
        let products: Vec<Product> = rows.into_iter().map(Into::into).collect();

        Ok(products)
    }

    async fn get_stats_by_device_type(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(device_type, 'Unknown') as device_type, COUNT(*) as count
            FROM products
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

    async fn get_stats_by_manufacturer(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(manufacturer, 'Unknown') as manufacturer, COUNT(*) as count
            FROM products
            GROUP BY manufacturer
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.database.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let manufacturer: String = row.get("manufacturer");
            let count: i64 = row.get("count");
            stats.push((manufacturer, count));
        }

        Ok(stats)
    }

    async fn find_with_filters(
        &self,
        name: Option<String>,
        manufacturer: Option<String>,
        device_type: Option<String>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<Product>> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, description, version, manufacturer, device_type,
                   protocol_type, created_at, updated_at
            FROM products WHERE 1=1
            "#,
        );

        if let Some(name) = &name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(manufacturer) = &manufacturer {
            query.push(" AND manufacturer LIKE ").push_bind(format!("%{}%", manufacturer));
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

        let rows = query.build_query_as::<ProductRow>().fetch_all(self.database.pool()).await?;
        let products: Vec<Product> = rows.into_iter().map(Into::into).collect();

        Ok(products)
    }

    async fn exists_by_name_excluding_id(&self, name: &str, exclude_id: &str) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE name = ? AND id != ?")
                .bind(name)
                .bind(exclude_id)
                .fetch_one(self.database.pool())
                .await?;

        Ok(count > 0)
    }
}
