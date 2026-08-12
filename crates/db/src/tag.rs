//! Tag 持久化：标签与标签绑定（P-集中化 E5，自 thing crate 迁入）。

use serde::{Deserialize, Serialize};
use tinyiothub_core::models::tag::{CreateTagBindingRequest, CreateTagRequest, UpdateTagRequest};

use tinyiothub_core::error::Result;

use crate::database::Database;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自 thing/tag/types.rs 迁入
// ──────────────────────────────────────────────

/// Tag entity - 标签实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tag {
    pub id: String,
    #[serde(rename = "type")]
    pub tag_type: String, // "device" or "app"
    pub name: String,
    pub tenant_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Tag binding entity - 标签绑定实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TagBinding {
    pub id: String,
    pub tag_id: String,
    pub target_id: String,
    pub tenant_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Query parameters for tag search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TagQuery {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub tag_type: Option<String>,
    pub target_id: Option<String>,
    pub tenant_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

// ──────────────────────────────────────────────
// Repositories
// ──────────────────────────────────────────────

use sqlx::{FromRow, QueryBuilder, Row};

// ── Row types (internal) ────────────────────────────────

#[derive(Debug, Clone, FromRow)]
struct TagRow {
    id: String,
    #[sqlx(rename = "type")]
    tag_type: String,
    name: String,
    tenant_id: Option<String>,
    created_by: Option<String>,
    created_at: String,
}

impl From<TagRow> for Tag {
    fn from(row: TagRow) -> Self {
        Self {
            id: row.id,
            tag_type: row.tag_type,
            name: row.name,
            tenant_id: row.tenant_id,
            created_by: row.created_by,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TagBindingRow {
    id: String,
    tag_id: String,
    target_id: String,
    tenant_id: Option<String>,
    created_by: Option<String>,
    created_at: String,
}

impl From<TagBindingRow> for TagBinding {
    fn from(row: TagBindingRow) -> Self {
        Self {
            id: row.id,
            tag_id: row.tag_id,
            target_id: row.target_id,
            tenant_id: row.tenant_id,
            created_by: row.created_by,
            created_at: row.created_at,
        }
    }
}

// ── SQLite implementations ──────────────────────────────

pub struct TagRepository {
    database: crate::database::Database,
}

impl TagRepository {
    pub fn new(database: crate::database::Database) -> Self {
        Self { database }
    }
}

impl TagRepository {
    pub async fn find_by_id(&self, id: &str) -> Result<Option<Tag>> {
        let row = sqlx::query_as::<_, TagRow>(
            "SELECT id, type, name, tenant_id, created_by, created_at FROM tags WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn find_by_name_and_type(&self, name: &str, tag_type: &str) -> Result<Option<Tag>> {
        let row = sqlx::query_as::<_, TagRow>(
            "SELECT id, type, name, tenant_id, created_by, created_at FROM tags WHERE name = ? AND type = ?",
        )
        .bind(name)
        .bind(tag_type)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn create(&self, request: &CreateTagRequest, created_by: &str, tenant_id: &str) -> Result<Tag> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO tags (id, type, name, tenant_id, created_by, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.tag_type)
        .bind(&request.name)
        .bind(tenant_id)
        .bind(created_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    pub async fn update(&self, id: &str, request: &UpdateTagRequest) -> Result<Tag> {
        let mut query = QueryBuilder::new("UPDATE tags SET ");
        let mut has_updates = false;

        if let Some(name) = &request.name {
            if has_updates {
                query.push(", ");
            }
            query.push("name = ").push_bind(name);
            has_updates = true;
        }

        if !has_updates {
            return self
                .find_by_id(id)
                .await?
                .ok_or(tinyiothub_core::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        self.find_by_id(id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    pub async fn delete(&self, id: &str, tenant_id: &str) -> Result<u64> {
        let mut tx = self.database.pool().begin().await?;

        sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ? AND tenant_id = ?")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM tags WHERE id = ? AND tenant_id = ?")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(result.rows_affected())
    }

    pub async fn find_all(&self, params: &TagQuery) -> Result<Vec<Tag>> {
        let mut query =
            QueryBuilder::new("SELECT id, type, name, tenant_id, created_by, created_at FROM tags WHERE 1=1");

        if let Some(tenant_id) = &params.tenant_id {
            query.push(" AND tenant_id = ").push_bind(tenant_id);
        }

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(tag_type) = &params.tag_type {
            query.push(" AND type = ").push_bind(tag_type);
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let rows = query.build_query_as::<TagRow>().fetch_all(self.database.pool()).await?;
        let tags: Vec<Tag> = rows.into_iter().map(Into::into).collect();

        Ok(tags)
    }

    pub async fn count(&self, params: &TagQuery) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM tags WHERE 1=1");

        if let Some(tenant_id) = &params.tenant_id {
            query.push(" AND tenant_id = ").push_bind(tenant_id);
        }

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(tag_type) = &params.tag_type {
            query.push(" AND type = ").push_bind(tag_type);
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    pub async fn find_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<Tag>> {
        let skip_tenant = tenant_id.is_empty();
        let sql = if skip_tenant {
            "SELECT t.id, t.type, t.name, t.tenant_id, t.created_by, t.created_at \
             FROM tags t \
             INNER JOIN tag_bindings tb ON t.id = tb.tag_id \
             WHERE tb.target_id = ? \
             ORDER BY t.created_at DESC"
        } else {
            "SELECT t.id, t.type, t.name, t.tenant_id, t.created_by, t.created_at \
             FROM tags t \
             INNER JOIN tag_bindings tb ON t.id = tb.tag_id \
             WHERE tb.target_id = ? AND t.tenant_id = ? AND tb.tenant_id = ? \
             ORDER BY t.created_at DESC"
        };

        let mut query = sqlx::query_as::<_, TagRow>(sql).bind(target_id);
        if !skip_tenant {
            query = query.bind(tenant_id).bind(tenant_id);
        }
        let rows = query.fetch_all(self.database.pool()).await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn exists_by_name_and_type(&self, name: &str, tag_type: &str, tenant_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ? AND tenant_id = ?")
            .bind(name)
            .bind(tag_type)
            .bind(tenant_id)
            .fetch_one(self.database.pool())
            .await?;

        Ok(count > 0)
    }

    pub async fn exists_by_name_and_type_exclude_id(
        &self,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
        tenant_id: &str,
    ) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ? AND id != ? AND tenant_id = ?")
                .bind(name)
                .bind(tag_type)
                .bind(exclude_id)
                .bind(tenant_id)
                .fetch_one(self.database.pool())
                .await?;

        Ok(count > 0)
    }
}

pub struct TagBindingRepository {
    database: crate::database::Database,
}

impl TagBindingRepository {
    pub fn new(database: crate::database::Database) -> Self {
        Self { database }
    }
}

impl TagBindingRepository {
    pub async fn find_by_id(&self, id: &str) -> Result<Option<TagBinding>> {
        let row = sqlx::query_as::<_, TagBindingRow>(
            "SELECT id, tag_id, target_id, tenant_id, created_by, created_at FROM tag_bindings WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn create(
        &self,
        request: &CreateTagBindingRequest,
        created_by: &str,
        tenant_id: &str,
    ) -> Result<TagBinding> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO tag_bindings (id, tag_id, target_id, target_type, tenant_id, created_by, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.tag_id)
        .bind(&request.target_id)
        .bind(&request.target_type)
        .bind(tenant_id)
        .bind(created_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    pub async fn delete(&self, id: &str, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE id = ? AND tenant_id = ?")
            .bind(id)
            .bind(tenant_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete_by_tag_and_target(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ? AND target_id = ? AND tenant_id = ?")
            .bind(tag_id)
            .bind(target_id)
            .bind(tenant_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn find_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>> {
        let rows = sqlx::query_as::<_, TagBindingRow>(
            "SELECT id, tag_id, target_id, tenant_id, created_by, created_at FROM tag_bindings WHERE tag_id = ? AND tenant_id = ? ORDER BY created_at DESC"
        )
        .bind(tag_id)
        .bind(tenant_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn find_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>> {
        let rows = sqlx::query_as::<_, TagBindingRow>(
            "SELECT id, tag_id, target_id, tenant_id, created_by, created_at FROM tag_bindings WHERE target_id = ? AND tenant_id = ? ORDER BY created_at DESC"
        )
        .bind(target_id)
        .bind(tenant_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn count_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND tenant_id = ?")
            .bind(tag_id)
            .bind(tenant_id)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count)
    }

    pub async fn count_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_bindings WHERE target_id = ? AND tenant_id = ?")
            .bind(target_id)
            .bind(tenant_id)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count)
    }

    pub async fn exists(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND target_id = ? AND tenant_id = ?",
        )
        .bind(tag_id)
        .bind(target_id)
        .bind(tenant_id)
        .fetch_one(self.database.pool())
        .await?;

        Ok(count > 0)
    }

    pub async fn find_by_tag_and_target(
        &self,
        tag_id: &str,
        target_id: &str,
        tenant_id: &str,
    ) -> Result<Option<TagBinding>> {
        let row = sqlx::query_as::<_, TagBindingRow>(
            "SELECT id, tag_id, target_id, tenant_id, created_by, created_at FROM tag_bindings WHERE tag_id = ? AND target_id = ? AND tenant_id = ? LIMIT 1",
        )
        .bind(tag_id)
        .bind(target_id)
        .bind(tenant_id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn create_batch(
        &self,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
        tenant_id: &str,
    ) -> Result<Vec<TagBinding>> {
        if bindings.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut created_bindings = Vec::new();

        for request in bindings {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND target_id = ? AND tenant_id = ?",
            )
            .bind(&request.tag_id)
            .bind(&request.target_id)
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;

            if count == 0 {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                sqlx::query(
                    r#"
                    INSERT INTO tag_bindings (id, tag_id, target_id, target_type, tenant_id, created_by, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&id)
                .bind(&request.tag_id)
                .bind(&request.target_id)
                .bind(&request.target_type)
                .bind(tenant_id)
                .bind(created_by)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                created_bindings.push(TagBinding {
                    id: id.clone(),
                    tag_id: request.tag_id.clone(),
                    target_id: request.target_id.clone(),
                    tenant_id: Some(tenant_id.to_string()),
                    created_by: Some(created_by.to_string()),
                    created_at: now,
                });
            }
        }

        tx.commit().await?;
        Ok(created_bindings)
    }

    pub async fn delete_all_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE target_id = ? AND tenant_id = ?")
            .bind(target_id)
            .bind(tenant_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete_all_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ? AND tenant_id = ?")
            .bind(tag_id)
            .bind(tenant_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }
}
