use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// Tag entity - 标签实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tag {
    pub id: String,
    #[serde(rename = "type")]
    pub tag_type: String, // "device" or "app"
    pub name: String,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Tag binding entity - 标签绑定实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TagBinding {
    pub id: String,
    pub tag_id: String,
    pub target_id: String,
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
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub tag_type: String, // "device" or "app"
}

/// Request for updating a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTagRequest {
    pub name: Option<String>,
}

/// Request for creating a tag binding
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTagBindingRequest {
    pub tag_id: String,
    pub target_id: String,
}

/// Request for batch creating tag bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchTagBindingRequest {
    pub tag_ids: Vec<String>,
    pub target_id: String,
}

impl Tag {
    /// 根据 ID 查找标签
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Tag>, sqlx::Error> {
        let tag = sqlx::query_as::<_, Tag>(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(tag)
    }

    /// 根据名称和类型查找标签
    pub async fn find_by_name_and_type(
        db: &Database,
        name: &str,
        tag_type: &str,
    ) -> Result<Option<Tag>, sqlx::Error> {
        let tag = sqlx::query_as::<_, Tag>(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE name = ? AND type = ?",
        )
        .bind(name)
        .bind(tag_type)
        .fetch_optional(db.pool())
        .await?;

        Ok(tag)
    }

    /// 创建新标签
    pub async fn create(
        db: &Database,
        request: &CreateTagRequest,
        created_by: &str,
    ) -> Result<Tag, sqlx::Error> {
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
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新标签信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateTagRequest,
    ) -> Result<Tag, sqlx::Error> {
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
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除标签
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        // 使用事务删除标签及其绑定关系
        let mut tx = db.pool().begin().await?;

        // 先删除标签绑定关系
        sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ?").bind(id).execute(&mut *tx).await?;

        // 删除标签本身
        let result =
            sqlx::query("DELETE FROM tags WHERE id = ?").bind(id).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// 查询标签列表（支持分页和筛选）
    pub async fn find_all(db: &Database, params: &TagQuery) -> Result<Vec<Tag>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT id, type as tag_type, name, created_by, created_at FROM tags WHERE 1=1",
        );

        // 动态添加查询条件
        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(tag_type) = &params.tag_type {
            query.push(" AND type = ").push_bind(tag_type);
        }

        // 添加排序
        query.push(" ORDER BY created_at DESC");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let tags = query.build_query_as::<Tag>().fetch_all(db.pool()).await?;

        Ok(tags)
    }

    /// 统计标签数量
    pub async fn count(db: &Database, params: &TagQuery) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM tags WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(tag_type) = &params.tag_type {
            query.push(" AND type = ").push_bind(tag_type);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 根据目标ID获取关联的标签
    pub async fn find_by_target_id(
        db: &Database,
        target_id: &str,
    ) -> Result<Vec<Tag>, sqlx::Error> {
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
        .fetch_all(db.pool())
        .await?;

        Ok(tags)
    }

    /// 检查标签名称是否存在（在同一类型下）
    pub async fn exists_by_name_and_type(
        db: &Database,
        name: &str,
        tag_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ?")
                .bind(name)
                .bind(tag_type)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }

    /// 检查标签名称是否存在（排除指定 ID）
    pub async fn exists_by_name_and_type_exclude_id(
        db: &Database,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ? AND type = ? AND id != ?")
                .bind(name)
                .bind(tag_type)
                .bind(exclude_id)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }
}

impl TagBinding {
    /// 根据 ID 查找标签绑定
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<TagBinding>, sqlx::Error> {
        let binding = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(binding)
    }

    /// 创建新标签绑定
    pub async fn create(
        db: &Database,
        request: &CreateTagBindingRequest,
        created_by: &str,
    ) -> Result<TagBinding, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            r#"
            INSERT INTO tag_bindings (id, tag_id, target_id, created_by, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.tag_id)
        .bind(&request.target_id)
        .bind(created_by)
        .bind(&now)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除标签绑定
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 删除指定标签和目标的绑定
    pub async fn delete_by_tag_and_target(
        db: &Database,
        tag_id: &str,
        target_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ? AND target_id = ?")
            .bind(tag_id)
            .bind(target_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 根据标签 ID 查询绑定
    pub async fn find_by_tag_id(
        db: &Database,
        tag_id: &str,
    ) -> Result<Vec<TagBinding>, sqlx::Error> {
        let bindings = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE tag_id = ? ORDER BY created_at DESC"
        )
        .bind(tag_id)
        .fetch_all(db.pool())
        .await?;

        Ok(bindings)
    }

    /// 根据目标 ID 查询绑定
    pub async fn find_by_target_id(
        db: &Database,
        target_id: &str,
    ) -> Result<Vec<TagBinding>, sqlx::Error> {
        let bindings = sqlx::query_as::<_, TagBinding>(
            "SELECT id, tag_id, target_id, created_by, created_at FROM tag_bindings WHERE target_id = ? ORDER BY created_at DESC"
        )
        .bind(target_id)
        .fetch_all(db.pool())
        .await?;

        Ok(bindings)
    }

    /// 检查标签绑定是否存在
    pub async fn exists(db: &Database, tag_id: &str, target_id: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tag_bindings WHERE tag_id = ? AND target_id = ?",
        )
        .bind(tag_id)
        .bind(target_id)
        .fetch_one(db.pool())
        .await?;

        Ok(count > 0)
    }

    /// 批量创建标签绑定
    pub async fn create_batch(
        db: &Database,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
    ) -> Result<Vec<TagBinding>, sqlx::Error> {
        if bindings.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = db.pool().begin().await?;
        let mut created_bindings = Vec::new();

        for request in bindings {
            // 检查绑定是否已存在
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
                    INSERT INTO tag_bindings (id, tag_id, target_id, created_by, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&id)
                .bind(&request.tag_id)
                .bind(&request.target_id)
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

    /// 删除目标的所有标签绑定
    pub async fn delete_all_by_target_id(
        db: &Database,
        target_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE target_id = ?")
            .bind(target_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 删除标签的所有绑定
    pub async fn delete_all_by_tag_id(db: &Database, tag_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tag_bindings WHERE tag_id = ?")
            .bind(tag_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }
}
