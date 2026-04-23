use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder, Row};

use crate::domain::role::repository::RoleRepository;
use crate::dto::entity::role::{CreateRoleRequest, Role, RoleQueryParams, RoleStats, UpdateRoleRequest};
use tinyiothub_storage::sqlite::Database;
use tinyiothub_core::error::Result;

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct RoleRow {
    id: String,
    name: String,
    description: Option<String>,
    is_administrator: i32,
}

impl From<RoleRow> for Role {
    fn from(row: RoleRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            is_administrator: row.is_administrator,
        }
    }
}

pub struct SqliteRoleRepository {
    database: Database,
}

impl SqliteRoleRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl RoleRepository for SqliteRoleRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Role>> {
        let row = sqlx::query_as::<_, RoleRow>(
            "SELECT id, name, description, is_administrator FROM roles WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>> {
        let row = sqlx::query_as::<_, RoleRow>(
            "SELECT id, name, description, is_administrator FROM roles WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn create(&self, request: &CreateRoleRequest) -> Result<Role> {
        let id = uuid::Uuid::new_v4().to_string();
        let is_admin = request.is_administrator.unwrap_or(0);

        sqlx::query(
            r#"
            INSERT INTO roles (id, name, description, IsAdministrator)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(is_admin)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(tinyiothub_core::error::Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
        let mut query = QueryBuilder::new("UPDATE roles SET ");
        let mut has_updates = false;

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

        if let Some(is_administrator) = request.is_administrator {
            if has_updates {
                query.push(", ");
            }
            query.push("is_administrator = ").push_bind(is_administrator);
            has_updates = true;
        }

        if !has_updates {
            return self.find_by_id(id).await?.ok_or(tinyiothub_core::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(tinyiothub_core::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM roles WHERE id = ?").bind(id).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM roles WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(self.database.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn find_all(&self, params: &RoleQueryParams) -> Result<Vec<Role>> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator FROM roles WHERE 1=1",
        );

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query.push(" AND description LIKE ").push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        query.push(" ORDER BY name");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let rows = query.build_query_as::<RoleRow>().fetch_all(self.database.pool()).await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count(&self, params: &RoleQueryParams) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM roles WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query.push(" AND description LIKE ").push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    async fn get_stats(&self) -> Result<RoleStats> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_roles,
                COUNT(CASE WHEN is_administrator = 1 THEN 1 END) as admin_roles,
                COUNT(CASE WHEN is_administrator = 0 THEN 1 END) as user_roles
            FROM roles
            "#,
        )
        .fetch_one(self.database.pool())
        .await?;

        let stats = RoleStats {
            total_roles: row.get("total_roles"),
            admin_roles: row.get("admin_roles"),
            user_roles: row.get("user_roles"),
        };

        Ok(stats)
    }

    async fn find_admin_roles(&self) -> Result<Vec<Role>> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT id, name, description, is_administrator FROM roles WHERE is_administrator = 1 ORDER BY name"
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_user_roles(&self) -> Result<Vec<Role>> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT id, name, description, is_administrator FROM roles WHERE is_administrator = 0 ORDER BY name"
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(self.database.pool())
            .await?;

        Ok(count > 0)
    }

    async fn exists_by_name_exclude_id(&self, name: &str, exclude_id: &str) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE name = ? AND id != ?")
                .bind(name)
                .bind(exclude_id)
                .fetch_one(self.database.pool())
                .await?;

        Ok(count > 0)
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Role>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator FROM roles WHERE id IN (",
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let rows = query.build_query_as::<RoleRow>().fetch_all(self.database.pool()).await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn is_administrator_role(&self, id: &str) -> Result<bool> {
        let role: Option<i32> =
            sqlx::query_scalar("SELECT is_administrator FROM roles WHERE id = ?")
                .bind(id)
                .fetch_optional(self.database.pool())
                .await?;

        Ok(role.unwrap_or(0) == 1)
    }

    async fn find_with_filters(
        &self,
        _enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>> {
        let mut params = RoleQueryParams::default();
        params.page = Some(page);
        params.page_size = Some(page_size);

        if let Some(search) = search {
            params.name = Some(search.to_string());
        }

        self.find_all(&params).await
    }

    async fn update_enabled_status(&self, id: &str, _enabled: bool) -> Result<bool> {
        match self.find_by_id(id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}
