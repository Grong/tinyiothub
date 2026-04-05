// Workspace DTO entity

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::infrastructure::persistence::database::Database;

/// Workspace entity for API responses
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub agent_config: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Workspace with device count (for list responses)
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceWithDeviceCount {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Create workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Update workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agent_config: Option<String>,
}

/// Assign device request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssignDeviceRequest {
    pub device_id: String,
}

/// Workspace query params
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Workspace {
    /// Find workspaces by tenant_id
    pub async fn find_by_tenant(
        db: &Database,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>, sqlx::Error> {
        let page = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(20).min(100);
        let offset = (page - 1) * page_size;

        let workspaces = sqlx::query_as::<_, WorkspaceWithDeviceCount>(
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
        .fetch_all(db.pool())
        .await?;

        Ok(workspaces)
    }

    /// Find workspace by ID
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<WorkspaceWithDeviceCount>, sqlx::Error> {
        let workspace = sqlx::query_as::<_, WorkspaceWithDeviceCount>(
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
        .fetch_optional(db.pool())
        .await?;

        Ok(workspace)
    }

    /// Create a new workspace
    pub async fn create(
        db: &Database,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace, sqlx::Error> {
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
        .execute(db.pool())
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

    /// Update workspace
    pub async fn update(
        db: &Database,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Option<WorkspaceWithDeviceCount>, sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();

        // Build dynamic update
        let mut query = String::from("UPDATE workspaces SET updated_at = ?");
        let mut bindings: Vec<String> = vec![now.clone()];

        if let Some(n) = name {
            query.push_str(", name = ?");
            bindings.push(n.to_string());
        }
        if let Some(d) = description {
            query.push_str(", description = ?");
            bindings.push(d.to_string());
        }
        if let Some(c) = agent_config {
            query.push_str(", agent_config = ?");
            bindings.push(c.to_string());
        }

        query.push_str(" WHERE id = ?");

        let mut q = sqlx::query(&query);
        q = q.bind(&now);
        if let Some(n) = name {
            q = q.bind(n);
        }
        if let Some(d) = description {
            q = q.bind(d);
        }
        if let Some(c) = agent_config {
            q = q.bind(c);
        }
        q = q.bind(id);

        q.execute(db.pool()).await?;

        Self::find_by_id(db, id).await
    }

    /// Delete workspace
    pub async fn delete(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// Assign device to workspace (with conflict check)
    /// Returns error message if device is already in another workspace
    pub async fn assign_device(
        db: &Database,
        device_id: &str,
        workspace_id: &str,
    ) -> Result<(), String> {
        // Check current workspace assignment
        let device: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT id, workspace_id FROM devices WHERE id = ?",
        )
        .bind(device_id)
        .fetch_optional(db.pool())
        .await
        .map_err(|e| format!("database error: {}", e))?;

        let (current_id, current_ws) = device.ok_or("device not found")?;

        if let Some(current_workspace) = current_ws {
            if current_workspace != workspace_id {
                return Err(format!(
                    "device already assigned to workspace {}",
                    current_workspace
                ));
            }
            // Already assigned to this workspace — no-op success
            return Ok(());
        }

        // Assign the device
        sqlx::query("UPDATE devices SET workspace_id = ?, updated_at = ? WHERE id = ?")
            .bind(workspace_id)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(device_id)
            .execute(db.pool())
            .await
            .map_err(|e| format!("failed to assign device: {}", e))?;

        Ok(())
    }
}
