use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::domain::tag::repository::{TagBindingRepository, TagRepository};
use crate::dto::entity::tag::{
    CreateTagBindingRequest, CreateTagRequest, Tag, TagBinding, TagQuery, UpdateTagRequest,
};
use crate::infrastructure::persistence::database::Database;
use crate::shared::error::Result;

pub struct SqliteTagRepository {
    database: Database,
}

impl SqliteTagRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl TagRepository for SqliteTagRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Tag>> {
        let tag = sqlx::query_as::<_, Tag>(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(tag)
    }

    async fn find_by_name_and_type(&self, name: &str, tag_type: &str) -> Result<Option<Tag>> {
        let tag = sqlx::query_as::<_, Tag>(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE name = ? AND type = ?",
        )
        .bind(name)
        .bind(tag_type)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(tag)
    }

    async fn create(&self, request: &CreateTagRequest, created_by: &str) -> Result<Tag> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO tags (id, type, name, created_by, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.tag_type)
        .bind(&request.name)
        .bind(created_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdateTagRequest) -> Result<Tag> {
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
            return self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(crate::shared::error::Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let mut tx = self.database.pool().begin().await?;

        sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(result.rows_affected())
    }

    async fn find_all(&self, params: &TagQuery) -> Result<Vec<Tag>> {
        let mut query = QueryBuilder::new(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE 1=1",
        );

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

        let tags = query.build_query_as::<Tag>().fetch_all(self.database.pool()).await?;

        Ok(tags)
    }

    async fn count(&self, params: &TagQuery) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM tags WHERE 1=1");

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

    async fn find_by_target_id(&self, target_id: &str) -> Result<Vec<Tag>> {
        let tags = sqlx::query_as::<_, Tag>(
            r#"
            SELECT t.id, t.type as tag_type, t.name, t.created_by, t.created_at
            FROM tags t
            INNER JOIN tag_bindings tb ON t.id = tb.tag_id
            WHERE tb.target_id = ?
            ORDER BY t.created_at DESC
            "#,
        )
        .bind(target_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(tags)
    }

    async fn exists_by_name_and_type(&self, name: &str, tag_type: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ?")
            .bind(name)
            .bind(tag_type)
            .fetch_one(self.database.pool())
            .await?;

        Ok(count > 0)
    }

    async fn exists_by_name_and_type_exclude_id(
        &self,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
    ) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ? AND id != ?")
                .bind(name)
                .bind(tag_type)
                .bind(exclude_id)
                .fetch_one(self.database.pool())
                .await?;

        Ok(count > 0)
    }
}

pub struct SqliteTagBindingRepository {
    database: Database,
}

impl SqliteTagBindingRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl TagBindingRepository for SqliteTagBindingRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<TagBinding>> {
        let binding = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(binding)
    }

    async fn create(
        &self,
        request: &CreateTagBindingRequest,
        created_by: &str,
    ) -> Result<TagBinding> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO tag_bindings (id, tag_id, target_id, target_type, created_by, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.tag_id)
        .bind(&request.target_id)
        .bind(&request.target_type)
        .bind(created_by)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    async fn delete_by_tag_and_target(&self, tag_id: &str, target_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ? AND target_id = ?")
            .bind(tag_id)
            .bind(target_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    async fn find_by_tag_id(&self, tag_id: &str) -> Result<Vec<TagBinding>> {
        let bindings = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE tag_id = ? ORDER BY created_at DESC"
        )
        .bind(tag_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(bindings)
    }

    async fn find_by_target_id(&self, target_id: &str) -> Result<Vec<TagBinding>> {
        let bindings = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE target_id = ? ORDER BY created_at DESC"
        )
        .bind(target_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(bindings)
    }

    async fn count_by_tag_id(&self, tag_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ?")
            .bind(tag_id)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count)
    }

    async fn count_by_target_id(&self, target_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_bindings WHERE target_id = ?")
            .bind(target_id)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count)
    }

    async fn exists(&self, tag_id: &str, target_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND target_id = ?",
        )
        .bind(tag_id)
        .bind(target_id)
        .fetch_one(self.database.pool())
        .await?;

        Ok(count > 0)
    }

    async fn find_by_tag_and_target(&self, tag_id: &str, target_id: &str) -> Result<Option<TagBinding>> {
        let binding = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE tag_id = ? AND target_id = ? LIMIT 1",
        )
        .bind(tag_id)
        .bind(target_id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(binding)
    }

    async fn create_batch(
        &self,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
    ) -> Result<Vec<TagBinding>> {
        if bindings.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = self.database.pool().begin().await?;
        let mut created_bindings = Vec::new();

        for request in bindings {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND target_id = ?",
            )
            .bind(&request.tag_id)
            .bind(&request.target_id)
            .fetch_one(&mut *tx)
            .await?;

            if count == 0 {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                sqlx::query(
                    r#"
                    INSERT INTO tag_bindings (id, tag_id, target_id, target_type, created_by, created_at)
                    VALUES (?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&id)
                .bind(&request.tag_id)
                .bind(&request.target_id)
                .bind(&request.target_type)
                .bind(created_by)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                created_bindings.push(TagBinding {
                    id: id.clone(),
                    tag_id: request.tag_id.clone(),
                    target_id: request.target_id.clone(),
                    created_by: Some(created_by.to_string()),
                    created_at: now,
                });
            }
        }

        tx.commit().await?;
        Ok(created_bindings)
    }

    async fn delete_all_by_target_id(&self, target_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE target_id = ?")
            .bind(target_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    async fn delete_all_by_tag_id(&self, tag_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ?")
            .bind(tag_id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }
}
