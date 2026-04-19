use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder};

use crate::{
    domain::workspace::repository::WorkspaceRepository,
    dto::entity::workspace::{Workspace, WorkspaceWithDeviceCount},
    infrastructure::persistence::Database,
    shared::error::{Error, Result},
};

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

    async fn assign_device(
        &self, device_id: &str, workspace_id: &str) -> Result<()> {
        let device: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT id, workspace_id FROM devices WHERE id = ?",
        )
        .bind(device_id)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(format!("database error: {}", e)))?;

        let (_current_id, current_ws) = device.ok_or_else(|| Error::NotFound)?;

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
}
