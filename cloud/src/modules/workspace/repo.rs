use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder};
use tinyiothub_core::error::{Error, Result};
use tinyiothub_storage::sqlite::Database;

use super::types::{ResourceSearchResult, Workspace, WorkspaceResource, WorkspaceWithDeviceCount};

/// Repository interface for workspace persistence
#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>>;
    async fn find_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>>;
    async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace>;
    async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Option<WorkspaceWithDeviceCount>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn assign_device(&self, device_id: &str, workspace_id: &str) -> Result<()>;
    async fn list_resources(
        &self,
        workspace_id: &str,
        resource_type: Option<&str>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceResource>>;
    async fn find_resource_by_id(
        &self,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<Option<WorkspaceResource>>;
    async fn create_resource(
        &self,
        workspace_id: &str,
        resource_type: &str,
        name: &str,
        description: Option<&str>,
        file_path: &str,
        tags: &[String],
        metadata: Option<&str>,
    ) -> Result<WorkspaceResource>;
    async fn update_resource(
        &self,
        workspace_id: &str,
        resource_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&[String]>,
        metadata: Option<&str>,
    ) -> Result<Option<WorkspaceResource>>;
    async fn delete_resource(&self, workspace_id: &str, resource_id: &str) -> Result<()>;
    async fn search_resources(
        &self,
        workspace_id: &str,
        query: &str,
        resource_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ResourceSearchResult>>;
}

// --- SQLite implementation ---

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct WorkspaceWithDeviceCountRow {
    id: String,
    name: String,
    description: Option<String>,
    tenant_id: String,
    agent_id: Option<String>,
    created_at: String,
    updated_at: String,
    device_count: Option<i64>,
    #[sqlx(default)]
    warning: Option<String>,
}

impl From<WorkspaceWithDeviceCountRow> for WorkspaceWithDeviceCount {
    fn from(row: WorkspaceWithDeviceCountRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            tenant_id: row.tenant_id,
            agent_id: row.agent_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            device_count: row.device_count,
            warning: row.warning,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct WorkspaceResourceRow {
    id: String,
    workspace_id: String,
    resource_type: String,
    name: String,
    description: Option<String>,
    file_path: String,
    tags: String,
    metadata: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<WorkspaceResourceRow> for WorkspaceResource {
    fn from(row: WorkspaceResourceRow) -> Self {
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            resource_type: row.resource_type,
            name: row.name,
            description: row.description,
            file_path: row.file_path,
            tags,
            metadata: row.metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct ResourceSearchResultRow {
    id: String,
    workspace_id: String,
    resource_type: String,
    name: String,
    description: Option<String>,
    file_path: String,
    tags: String,
    metadata: Option<String>,
    created_at: String,
    updated_at: String,
    relevance: i64,
}

impl From<ResourceSearchResultRow> for ResourceSearchResult {
    fn from(row: ResourceSearchResultRow) -> Self {
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            resource_type: row.resource_type,
            name: row.name,
            description: row.description,
            file_path: row.file_path,
            tags,
            metadata: row.metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
            relevance: row.relevance,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqliteWorkspaceRepository {
    database: Database,
}

impl SqliteWorkspaceRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl WorkspaceRepository for SqliteWorkspaceRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
        let row = sqlx::query_as::<_, WorkspaceWithDeviceCountRow>(
            r#"
            SELECT
                w.id,
                w.name,
                w.description,
                w.tenant_id,
                w.agent_id,
                w.created_at,
                w.updated_at,
                COUNT(d.id) as device_count
            FROM workspaces w
            LEFT JOIN devices d ON d.workspace_id = w.id
            WHERE w.id = ?
            GROUP BY w.id
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>> {
        let page = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(20).min(100);
        let offset = (page - 1) * page_size;

        let rows = sqlx::query_as::<_, WorkspaceWithDeviceCountRow>(
            r#"
            SELECT
                w.id,
                w.name,
                w.description,
                w.tenant_id,
                w.agent_id,
                w.created_at,
                w.updated_at,
                COUNT(d.id) as device_count
            FROM workspaces w
            LEFT JOIN devices d ON d.workspace_id = w.id
            WHERE w.tenant_id = ?
            GROUP BY w.id
            ORDER BY w.created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(page_size as i64)
        .bind(offset as i64)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace> {
        let id = format!("ws-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO workspaces (id, name, description, tenant_id, agent_id, agent_config, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(tenant_id)
        .bind(agent_id)
        .bind(agent_config)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        Ok(Workspace {
            id,
            name: name.to_string(),
            description: description.map(String::from),
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.map(String::from),
            agent_config: agent_config.map(String::from),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Option<WorkspaceWithDeviceCount>> {
        let mut builder = QueryBuilder::new("UPDATE workspaces SET ");
        let mut has_updates = false;
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(n) = name {
            if has_updates {
                builder.push(", ");
            }
            builder.push("name = ").push_bind(n);
            has_updates = true;
        }

        if let Some(d) = description {
            if has_updates {
                builder.push(", ");
            }
            builder.push("description = ").push_bind(d);
            has_updates = true;
        }

        if let Some(aid) = agent_id {
            if has_updates {
                builder.push(", ");
            }
            builder.push("agent_id = ").push_bind(aid);
            has_updates = true;
        }

        if let Some(c) = agent_config {
            if has_updates {
                builder.push(", ");
            }
            builder.push("agent_config = ").push_bind(c);
            has_updates = true;
        }

        if !has_updates {
            return self.find_by_id(id).await;
        }

        builder.push(", updated_at = ").push_bind(&now);
        builder.push(" WHERE id = ").push_bind(id);

        let result = builder.build().execute(self.database.pool()).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.find_by_id(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(())
    }

    async fn assign_device(&self, device_id: &str, workspace_id: &str) -> Result<()> {
        let device: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT id, workspace_id FROM devices WHERE id = ?")
                .bind(device_id)
                .fetch_optional(self.database.pool())
                .await
                .map_err(|e| Error::DatabaseError(format!("database error: {}", e)))?;

        let (_current_id, current_ws) = device.ok_or(Error::NotFound)?;

        if let Some(current_workspace) = current_ws {
            if current_workspace != workspace_id {
                return Err(Error::InvalidArgument(format!(
                    "device already assigned to workspace {}",
                    current_workspace
                )));
            }
            return Ok(());
        }

        sqlx::query("UPDATE devices SET workspace_id = ?, updated_at = ? WHERE id = ?")
            .bind(workspace_id)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(device_id)
            .execute(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(format!("failed to assign device: {}", e)))?;

        Ok(())
    }

    async fn list_resources(
        &self,
        workspace_id: &str,
        resource_type: Option<&str>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceResource>> {
        let page = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(20).min(100);
        let offset = (page - 1) * page_size;

        let rows = if let Some(rt) = resource_type {
            sqlx::query_as::<_, WorkspaceResourceRow>(
                r#"
                SELECT id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at
                FROM workspace_resources
                WHERE workspace_id = ? AND resource_type = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(workspace_id)
            .bind(rt)
            .bind(page_size as i64)
            .bind(offset as i64)
            .fetch_all(self.database.pool())
            .await?
        } else {
            sqlx::query_as::<_, WorkspaceResourceRow>(
                r#"
                SELECT id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at
                FROM workspace_resources
                WHERE workspace_id = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(workspace_id)
            .bind(page_size as i64)
            .bind(offset as i64)
            .fetch_all(self.database.pool())
            .await?
        };

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_resource_by_id(
        &self,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<Option<WorkspaceResource>> {
        let row = sqlx::query_as::<_, WorkspaceResourceRow>(
            r#"
            SELECT id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at
            FROM workspace_resources
            WHERE workspace_id = ? AND id = ?
            "#,
        )
        .bind(workspace_id)
        .bind(resource_id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn create_resource(
        &self,
        workspace_id: &str,
        resource_type: &str,
        name: &str,
        description: Option<&str>,
        file_path: &str,
        tags: &[String],
        metadata: Option<&str>,
    ) -> Result<WorkspaceResource> {
        let id = format!("res-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO workspace_resources (id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(resource_type)
        .bind(name)
        .bind(description)
        .bind(file_path)
        .bind(&tags_json)
        .bind(metadata)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        Ok(WorkspaceResource {
            id,
            workspace_id: workspace_id.to_string(),
            resource_type: resource_type.to_string(),
            name: name.to_string(),
            description: description.map(String::from),
            file_path: file_path.to_string(),
            tags: tags.to_vec(),
            metadata: metadata.map(String::from),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn update_resource(
        &self,
        workspace_id: &str,
        resource_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&[String]>,
        metadata: Option<&str>,
    ) -> Result<Option<WorkspaceResource>> {
        let mut builder = QueryBuilder::new("UPDATE workspace_resources SET ");
        let mut has_updates = false;
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(n) = name {
            if has_updates {
                builder.push(", ");
            }
            builder.push("name = ").push_bind(n);
            has_updates = true;
        }

        if let Some(d) = description {
            if has_updates {
                builder.push(", ");
            }
            builder.push("description = ").push_bind(d);
            has_updates = true;
        }

        if let Some(t) = tags {
            if has_updates {
                builder.push(", ");
            }
            let tags_json = serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string());
            builder.push("tags = ").push_bind(tags_json);
            has_updates = true;
        }

        if let Some(m) = metadata {
            if has_updates {
                builder.push(", ");
            }
            builder.push("metadata = ").push_bind(m);
            has_updates = true;
        }

        if !has_updates {
            return self.find_resource_by_id(workspace_id, resource_id).await;
        }

        builder.push(", updated_at = ").push_bind(&now);
        builder.push(" WHERE workspace_id = ").push_bind(workspace_id);
        builder.push(" AND id = ").push_bind(resource_id);

        let result = builder.build().execute(self.database.pool()).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.find_resource_by_id(workspace_id, resource_id).await
    }

    async fn delete_resource(&self, workspace_id: &str, resource_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workspace_resources WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(resource_id)
            .execute(self.database.pool())
            .await?;
        Ok(())
    }

    async fn search_resources(
        &self,
        workspace_id: &str,
        query: &str,
        resource_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ResourceSearchResult>> {
        let keywords: Vec<&str> = query.split_whitespace().collect();
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let mut builder = QueryBuilder::new(
            "SELECT id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at, SUM(relevance) as relevance FROM (",
        );

        for (i, keyword) in keywords.iter().enumerate() {
            if i > 0 {
                builder.push(" UNION ALL ");
            }
            builder.push(
                "SELECT *, (
                    (CASE WHEN name LIKE ",
            );
            builder.push_bind(format!("%{}%", keyword));
            builder.push(
                " THEN 3 ELSE 0 END) +
                    (CASE WHEN description LIKE ",
            );
            builder.push_bind(format!("%{}%", keyword));
            builder.push(
                " THEN 2 ELSE 0 END) +
                    (CASE WHEN EXISTS (SELECT 1 FROM json_each(tags) WHERE value LIKE ",
            );
            builder.push_bind(format!("%{}%", keyword));
            builder.push(
                ") THEN 2 ELSE 0 END)
                ) as relevance
                FROM workspace_resources
                WHERE workspace_id = ",
            );
            builder.push_bind(workspace_id);
            if let Some(rt) = resource_type {
                builder.push(" AND resource_type = ");
                builder.push_bind(rt);
            }
            builder.push(" AND (name LIKE ");
            builder.push_bind(format!("%{}%", keyword));
            builder.push(" OR description LIKE ");
            builder.push_bind(format!("%{}%", keyword));
            builder.push(
                " OR EXISTS (
                    SELECT 1 FROM json_each(tags) WHERE value LIKE ",
            );
            builder.push_bind(format!("%{}%", keyword));
            builder.push("))");
        }

        builder.push(") GROUP BY id HAVING relevance > 0 ORDER BY relevance DESC LIMIT ");
        builder.push_bind(limit);

        let rows = builder
            .build_query_as::<ResourceSearchResultRow>()
            .fetch_all(self.database.pool())
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
