//! Workspace 持久化：工作区与知识资源（P-集中化 E4，自 tenant crate 迁入）。

use tinyiothub_core::models::workspace::ResourceType;

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use tinyiothub_core::error::{Error, Result};

use crate::database::Db;
use crate::sql_security::escape_like_pattern;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自领域 crate 迁入
// ──────────────────────────────────────────────

/// Workspace entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub agent_config: Option<String>,
    pub require_action_confirm: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Workspace with device count (for list responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceWithDeviceCount {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub agent_id: Option<String>,
    pub require_action_confirm: Option<bool>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Unified workspace resource (replaces workspace_resources + knowledge_documents)
/// - type="document": content field is used
/// - type="file": file_path is used (uploaded binaries)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceResource {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: ResourceType,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceSearchResult {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: ResourceType,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub relevance: i64,
}

/// Resource query params
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ResourceQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub resource_type: Option<ResourceType>,
}

/// Workspace query params
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Extract a file path from document content when file_path is empty.
/// Handles markdown image syntax `![alt](path)` and code blocks `\`\`\`3d\npath\n\`\`\``.
pub fn extract_file_path_from_content(content: &str) -> String {
    // Try markdown 3d code block first
    if let Some(start) = content.find("```3d") {
        let after = &content[start + 5..];
        if let Some(nl) = after.find('\n') {
            let rest = &after[nl + 1..];
            if let Some(end) = rest.find("```") {
                return rest[..end].trim().to_string();
            }
        }
    }

    // Try markdown image: ![alt](path)
    if let Some(start) = content.find("![") {
        let after = &content[start + 2..];
        if let Some(close_bracket) = after.find("](") {
            let after_path = &after[close_bracket + 2..];
            if let Some(close_paren) = after_path.find(')') {
                return after_path[..close_paren].trim().to_string();
            }
        }
    }

    // Fallback: raw /uploads/ path
    if let Some(start) = content.find("/uploads/") {
        let rest = &content[start..];
        let end = rest.find(|c: char| c.is_whitespace() || c == ')').unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }

    String::new()
}

impl Workspace {
    pub fn new(id: String, name: String, description: Option<String>, tenant_id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            name,
            description,
            tenant_id,
            agent_id: None,
            agent_config: None,
            require_action_confirm: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn with_agent(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    pub fn with_config(mut self, config: String) -> Self {
        self.agent_config = Some(config);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new(
            "ws-1".to_string(),
            "Test Workspace".to_string(),
            Some("A test workspace".to_string()),
            "tenant-1".to_string(),
        );
        assert_eq!(ws.id, "ws-1");
        assert_eq!(ws.name, "Test Workspace");
        assert_eq!(ws.description, Some("A test workspace".to_string()));
        assert_eq!(ws.tenant_id, "tenant-1");
        assert!(ws.agent_id.is_none());
        assert!(ws.agent_config.is_none());
    }

    #[test]
    fn test_workspace_with_agent() {
        let ws = Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
            .with_agent("agent-1".to_string());
        assert_eq!(ws.agent_id, Some("agent-1".to_string()));
    }

    #[test]
    fn test_workspace_with_config() {
        let ws = Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
            .with_config(r#"{"model": "gpt-4"}"#.to_string());
        assert_eq!(ws.agent_config, Some(r#"{"model": "gpt-4"}"#.to_string()));
    }

    #[test]
    fn test_workspace_with_agent_and_config() {
        let ws = Workspace::new("ws-1".to_string(), "Test".to_string(), None, "tenant-1".to_string())
            .with_agent("agent-1".to_string())
            .with_config("config".to_string());
        assert_eq!(ws.agent_id, Some("agent-1".to_string()));
        assert_eq!(ws.agent_config, Some("config".to_string()));
    }
}

// ──────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────

// --- SQLite implementation ---

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct WorkspaceWithDeviceCountRow {
    id: String,
    name: String,
    description: Option<String>,
    tenant_id: String,
    agent_id: Option<String>,
    require_action_confirm: Option<bool>,
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
            require_action_confirm: row.require_action_confirm,
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
    content: Option<String>,
    file_path: String,
    file_size: Option<i64>,
    tags: String,
    metadata: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<WorkspaceResourceRow> for WorkspaceResource {
    fn from(row: WorkspaceResourceRow) -> Self {
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        let file_path = if row.file_path.is_empty() {
            extract_file_path_from_content(row.content.as_deref().unwrap_or(""))
        } else {
            row.file_path
        };
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            resource_type: ResourceType::from_string(&row.resource_type).unwrap_or(ResourceType::File),
            name: row.name,
            description: row.description,
            content: row.content,
            file_path,
            file_size: row.file_size,
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
    content: Option<String>,
    file_path: String,
    file_size: Option<i64>,
    tags: String,
    metadata: Option<String>,
    created_at: String,
    updated_at: String,
    relevance: i64,
}

impl From<ResourceSearchResultRow> for ResourceSearchResult {
    fn from(row: ResourceSearchResultRow) -> Self {
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        let file_path = if row.file_path.is_empty() {
            extract_file_path_from_content(row.content.as_deref().unwrap_or(""))
        } else {
            row.file_path
        };
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            resource_type: ResourceType::from_string(&row.resource_type).unwrap_or(ResourceType::File),
            name: row.name,
            description: row.description,
            content: row.content,
            file_path,
            file_size: row.file_size,
            tags,
            metadata: row.metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
            relevance: row.relevance,
        }
    }
}
// ──────────────────────────────────────────────
// 持久化函数（pub(crate) 自由函数 + Db 委托）
// ──────────────────────────────────────────────

pub(crate) async fn find_workspace_by_id(pool: &SqlitePool, id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
    let row = sqlx::query_as::<_, WorkspaceWithDeviceCountRow>(
        r#"
        SELECT
            w.id,
            w.name,
            w.description,
            w.tenant_id,
            w.agent_id,
            w.require_action_confirm,
            w.created_at,
            w.updated_at,
            COUNT(d.id) as device_count
        FROM workspaces w
        LEFT JOIN things d ON d.workspace_id = w.id
        WHERE w.id = ?
        GROUP BY w.id
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn find_workspaces_by_tenant(
    pool: &SqlitePool,
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
            w.require_action_confirm,
            w.created_at,
            w.updated_at,
            COUNT(d.id) as device_count
        FROM workspaces w
        LEFT JOIN things d ON d.workspace_id = w.id
        WHERE w.tenant_id = ?
        GROUP BY w.id
        ORDER BY w.created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(tenant_id)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn create_workspace(
    pool: &SqlitePool,
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
        INSERT INTO workspaces (id, name, description, tenant_id, agent_id, agent_config, require_action_confirm, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)
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
    .execute(pool)
    .await?;

    Ok(Workspace {
        id,
        name: name.to_string(),
        description: description.map(String::from),
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.map(String::from),
        agent_config: agent_config.map(String::from),
        require_action_confirm: true,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub(crate) async fn update_workspace(
    pool: &SqlitePool,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    agent_id: Option<&str>,
    agent_config: Option<&str>,
    require_action_confirm: Option<bool>,
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

    if let Some(rac) = require_action_confirm {
        if has_updates {
            builder.push(", ");
        }
        builder.push("require_action_confirm = ").push_bind(rac);
        has_updates = true;
    }

    if !has_updates {
        return find_workspace_by_id(pool, id).await;
    }

    builder.push(", updated_at = ").push_bind(&now);
    builder.push(" WHERE id = ").push_bind(id);

    let result = builder.build().execute(pool).await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }

    find_workspace_by_id(pool, id).await
}

pub(crate) async fn delete_workspace(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn assign_device_to_workspace(pool: &SqlitePool, thing_id: &str, workspace_id: &str) -> Result<()> {
    let device: Option<(String, Option<String>)> = sqlx::query_as("SELECT id, workspace_id FROM things WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
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

    sqlx::query("UPDATE things SET workspace_id = ?, updated_at = ? WHERE id = ?")
        .bind(workspace_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(thing_id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(format!("failed to assign device: {}", e)))?;

    Ok(())
}

pub(crate) async fn list_workspace_resources(
    pool: &SqlitePool,
    workspace_id: &str,
    resource_type: Option<ResourceType>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Vec<WorkspaceResource>> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let rows = if let Some(rt) = resource_type {
        sqlx::query_as::<_, WorkspaceResourceRow>(
            r#"
            SELECT id, workspace_id, resource_type, name, description, content, file_path, file_size, tags, metadata, created_at, updated_at
            FROM resources
            WHERE workspace_id = ? AND resource_type = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(workspace_id)
        .bind(rt.as_str())
        .bind(page_size as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, WorkspaceResourceRow>(
            r#"
            SELECT id, workspace_id, resource_type, name, description, content, file_path, file_size, tags, metadata, created_at, updated_at
            FROM resources
            WHERE workspace_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(workspace_id)
        .bind(page_size as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_workspace_resource(
    pool: &SqlitePool,
    workspace_id: &str,
    resource_id: &str,
) -> Result<Option<WorkspaceResource>> {
    let row = sqlx::query_as::<_, WorkspaceResourceRow>(
        r#"
        SELECT id, workspace_id, resource_type, name, description, content, file_path, file_size, tags, metadata, created_at, updated_at
        FROM resources
        WHERE workspace_id = ? AND id = ?
        "#,
    )
    .bind(workspace_id)
    .bind(resource_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn create_workspace_resource(
    pool: &SqlitePool,
    workspace_id: &str,
    resource_type: ResourceType,
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
        INSERT INTO resources (id, workspace_id, resource_type, name, description, file_path, tags, metadata, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(resource_type.as_str())
    .bind(name)
    .bind(description)
    .bind(file_path)
    .bind(&tags_json)
    .bind(metadata)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(WorkspaceResource {
        id,
        workspace_id: workspace_id.to_string(),
        resource_type,
        name: name.to_string(),
        description: description.map(String::from),
        content: None,
        file_path: file_path.to_string(),
        file_size: None,
        tags: tags.to_vec(),
        metadata: metadata.map(String::from),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub(crate) async fn update_workspace_resource(
    pool: &SqlitePool,
    workspace_id: &str,
    resource_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    tags: Option<&[String]>,
    metadata: Option<&str>,
) -> Result<Option<WorkspaceResource>> {
    let mut builder = QueryBuilder::new("UPDATE resources SET ");
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
        return find_workspace_resource(pool, workspace_id, resource_id).await;
    }

    builder.push(", updated_at = ").push_bind(&now);
    builder.push(" WHERE workspace_id = ").push_bind(workspace_id);
    builder.push(" AND id = ").push_bind(resource_id);

    let result = builder.build().execute(pool).await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }

    find_workspace_resource(pool, workspace_id, resource_id).await
}

pub(crate) async fn delete_workspace_resource(pool: &SqlitePool, workspace_id: &str, resource_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM resources WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(resource_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn search_workspace_resources(
    pool: &SqlitePool,
    workspace_id: &str,
    query: &str,
    resource_type: Option<ResourceType>,
    limit: i64,
) -> Result<Vec<ResourceSearchResult>> {
    let keywords: Vec<&str> = query.split_whitespace().collect();
    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    // Single-table search on unified resources table.
    // Relevance: name match = 3, description match = 2, tag match = 2, content match = 1
    let mut builder = QueryBuilder::new(
        "SELECT id, workspace_id, resource_type, name, description, \
             content, file_path, file_size, tags, metadata, \
             created_at, updated_at, SUM(relevance) as relevance FROM (",
    );

    for (i, keyword) in keywords.iter().enumerate() {
        if i > 0 {
            builder.push(" UNION ALL ");
        }

        let like = format!("%{}%", escape_like_pattern(keyword));

        builder.push("SELECT *, (");
        // Name match
        builder.push("CASE WHEN name LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' THEN 3 ELSE 0 END + ");
        // Description match
        builder.push("CASE WHEN description LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' THEN 2 ELSE 0 END + ");
        // Tag match
        builder.push("CASE WHEN EXISTS (SELECT 1 FROM json_each(tags) WHERE value LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\') THEN 2 ELSE 0 END + ");
        // Content match (for documents)
        builder.push("CASE WHEN content LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' THEN 1 ELSE 0 END");

        builder.push(") as relevance FROM resources WHERE workspace_id = ");
        builder.push_bind(workspace_id);

        if let Some(rt) = resource_type {
            builder.push(" AND resource_type = ");
            builder.push_bind(rt.as_str());
        }

        // WHERE match conditions
        builder.push(" AND (name LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' OR description LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' OR content LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\' OR EXISTS (SELECT 1 FROM json_each(tags) WHERE value LIKE ");
        builder.push_bind(&like);
        builder.push(" ESCAPE '\\'))");
    }

    builder.push(") GROUP BY id HAVING relevance > 0 ORDER BY relevance DESC LIMIT ");
    builder.push_bind(limit);

    let rows = builder
        .build_query_as::<ResourceSearchResultRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_all_workspace_ids(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM workspaces").fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// 租户下第一个 workspace id — 自 cloud auth 社交登录段收编。
pub(crate) async fn find_first_workspace_id_by_tenant(pool: &SqlitePool, tenant_id: &str) -> Result<Option<String>> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces WHERE tenant_id = ? LIMIT 1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;
    Ok(id)
}

/// 全库第一个 workspace id — 自 cloud auth 社交登录 mock 段收编。
pub(crate) async fn find_first_workspace_id(pool: &SqlitePool) -> Result<Option<String>> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
        .fetch_optional(pool)
        .await?;
    Ok(id)
}

/// 登录上下文 workspace：优先用户自己的 `ws-{user_id}`，否则租户下第一个
/// — 自 cloud auth login/sms 段收编（SQL 逐字迁移）。
pub(crate) async fn find_workspace_id_for_login(
    pool: &SqlitePool,
    user_ws_id: &str,
    tenant_id: &str,
) -> Result<Option<String>> {
    let id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? UNION ALL SELECT id FROM workspaces WHERE tenant_id = ? AND id != ? LIMIT 1"
    )
    .bind(user_ws_id)
    .bind(tenant_id)
    .bind(user_ws_id)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// 查询 workspace 的 require_action_confirm 开关（原始 i32 语义；
/// 自 cloud thing/actions 与 agent invoke_action 迁入，错误保持 sqlx::Error
/// 以沿用调用方既有错误文案）。
pub(crate) async fn get_workspace_require_action_confirm(
    pool: &SqlitePool,
    workspace_id: &str,
) -> std::result::Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
}

// ──────────────────────────────────────────────
// Db 委托（Workspace 领域）
// ──────────────────────────────────────────────

impl Db {
    /// 查询 workspace 的 require_action_confirm 开关（原始 i32 语义）。
    pub async fn get_workspace_require_action_confirm(
        &self,
        workspace_id: &str,
    ) -> std::result::Result<Option<i32>, sqlx::Error> {
        get_workspace_require_action_confirm(self.pool(), workspace_id).await
    }

    /// 按 ID 查询工作空间（含设备数）。
    pub async fn find_workspace_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
        find_workspace_by_id(self.pool(), id).await
    }

    /// 分页列出租户下工作空间（含设备数）。
    pub async fn find_workspaces_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>> {
        find_workspaces_by_tenant(self.pool(), tenant_id, page, page_size).await
    }

    /// 创建工作空间。
    pub async fn create_workspace(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace> {
        create_workspace(self.pool(), tenant_id, name, description, agent_id, agent_config).await
    }

    /// 更新工作空间（仅更新传入字段；无字段时直接回读）。
    pub async fn update_workspace(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
        require_action_confirm: Option<bool>,
    ) -> Result<Option<WorkspaceWithDeviceCount>> {
        update_workspace(
            self.pool(),
            id,
            name,
            description,
            agent_id,
            agent_config,
            require_action_confirm,
        )
        .await
    }

    /// 删除工作空间。
    pub async fn delete_workspace(&self, id: &str) -> Result<()> {
        delete_workspace(self.pool(), id).await
    }

    /// 把设备分配到工作空间（已分配给其他工作空间时报错）。
    pub async fn assign_device_to_workspace(&self, thing_id: &str, workspace_id: &str) -> Result<()> {
        assign_device_to_workspace(self.pool(), thing_id, workspace_id).await
    }

    /// 分页列出工作空间资源。
    pub async fn list_workspace_resources(
        &self,
        workspace_id: &str,
        resource_type: Option<ResourceType>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceResource>> {
        list_workspace_resources(self.pool(), workspace_id, resource_type, page, page_size).await
    }

    /// 按 ID 查询工作空间资源。
    pub async fn find_workspace_resource(
        &self,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<Option<WorkspaceResource>> {
        find_workspace_resource(self.pool(), workspace_id, resource_id).await
    }

    /// 创建工作空间资源。
    pub async fn create_workspace_resource(
        &self,
        workspace_id: &str,
        resource_type: ResourceType,
        name: &str,
        description: Option<&str>,
        file_path: &str,
        tags: &[String],
        metadata: Option<&str>,
    ) -> Result<WorkspaceResource> {
        create_workspace_resource(
            self.pool(),
            workspace_id,
            resource_type,
            name,
            description,
            file_path,
            tags,
            metadata,
        )
        .await
    }

    /// 更新工作空间资源（仅更新传入字段；无字段时直接回读）。
    pub async fn update_workspace_resource(
        &self,
        workspace_id: &str,
        resource_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&[String]>,
        metadata: Option<&str>,
    ) -> Result<Option<WorkspaceResource>> {
        update_workspace_resource(
            self.pool(),
            workspace_id,
            resource_id,
            name,
            description,
            tags,
            metadata,
        )
        .await
    }

    /// 删除工作空间资源。
    pub async fn delete_workspace_resource(&self, workspace_id: &str, resource_id: &str) -> Result<()> {
        delete_workspace_resource(self.pool(), workspace_id, resource_id).await
    }

    /// 全文检索工作空间资源（名称/描述/标签/内容加权相关度）。
    pub async fn search_workspace_resources(
        &self,
        workspace_id: &str,
        query: &str,
        resource_type: Option<ResourceType>,
        limit: i64,
    ) -> Result<Vec<ResourceSearchResult>> {
        search_workspace_resources(self.pool(), workspace_id, query, resource_type, limit).await
    }

    /// 列出全部工作空间 ID。
    pub async fn find_all_workspace_ids(&self) -> Result<Vec<String>> {
        find_all_workspace_ids(self.pool()).await
    }

    /// 租户下第一个 workspace id。
    pub async fn find_first_workspace_id_by_tenant(&self, tenant_id: &str) -> Result<Option<String>> {
        find_first_workspace_id_by_tenant(self.pool(), tenant_id).await
    }

    /// 全库第一个 workspace id。
    pub async fn find_first_workspace_id(&self) -> Result<Option<String>> {
        find_first_workspace_id(self.pool()).await
    }

    /// 登录上下文 workspace：优先 `user_ws_id`，否则租户下第一个。
    pub async fn find_workspace_id_for_login(&self, user_ws_id: &str, tenant_id: &str) -> Result<Option<String>> {
        find_workspace_id_for_login(self.pool(), user_ws_id, tenant_id).await
    }
}

#[cfg(test)]
mod extract_tests {
    use super::extract_file_path_from_content;

    #[test]
    fn test_extract_from_3d_code_block() {
        let content = "```3d\n/uploads/ws-001/uploads/factory.glb\n```\n这是描述";
        assert_eq!(
            extract_file_path_from_content(content),
            "/uploads/ws-001/uploads/factory.glb"
        );
    }

    #[test]
    fn test_extract_from_markdown_image() {
        let content = "![平面图.png](/uploads/ws-001/uploads/floor.png)\n这是平面图";
        assert_eq!(
            extract_file_path_from_content(content),
            "/uploads/ws-001/uploads/floor.png"
        );
    }

    #[test]
    fn test_extract_fallback_raw_path() {
        let content = "请查看 /uploads/ws-001/uploads/data.bin 文件";
        assert_eq!(
            extract_file_path_from_content(content),
            "/uploads/ws-001/uploads/data.bin"
        );
    }

    #[test]
    fn test_extract_no_path() {
        let content = "纯文本内容，没有文件路径";
        assert_eq!(extract_file_path_from_content(content), "");
    }

    #[test]
    fn test_extract_priority_code_block_over_image() {
        let content = "```3d\n/uploads/model.glb\n```\n![img](/uploads/image.png)";
        assert_eq!(extract_file_path_from_content(content), "/uploads/model.glb");
    }
}

// ──────────────────────────────────────────────
// 初始化引导查询（自 cloud shared/initialization.rs 迁入，Task 12；
// 错误保持 sqlx::Error 以沿用调用方既有错误文案）
// ──────────────────────────────────────────────

/// workspace 是否存在（按 id）。
pub(crate) async fn workspace_exists_by_id(pool: &SqlitePool, ws_id: &str) -> std::result::Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?)")
        .bind(ws_id)
        .fetch_one(pool)
        .await
}

/// 创建用户个人 workspace（默认租户）。
pub(crate) async fn insert_user_workspace(
    pool: &SqlitePool,
    ws_id: &str,
    name: String,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO workspaces
           (id, name, description, tenant_id, created_at, updated_at)
           VALUES (?, ?, '用户个人工作空间', 'tenant-default-001', datetime('now'), datetime('now'))"#,
    )
    .bind(ws_id)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// 回填 workspace 的 agent_id。
pub(crate) async fn set_workspace_agent_id(
    pool: &SqlitePool,
    agent_id: &str,
    ws_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE workspaces SET agent_id = ? WHERE id = ?")
        .bind(agent_id)
        .bind(ws_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 默认 workspace 是否存在。
pub(crate) async fn default_workspace_exists(pool: &SqlitePool) -> std::result::Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = 'ws-default-001')")
        .fetch_one(pool)
        .await
}

/// 创建默认 workspace（字面量行）。
pub(crate) async fn insert_default_workspace(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO workspaces
               (id, name, description, tenant_id, created_at, updated_at)
               VALUES
               ('ws-default-001', '默认工作空间', '系统自动创建的默认工作空间',
                'tenant-default-001', datetime('now'), datetime('now'))"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// 默认 workspace 是否缺少 agent_id。
pub(crate) async fn default_workspace_needs_agent(pool: &SqlitePool) -> std::result::Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT (agent_id IS NULL OR agent_id = '') FROM workspaces WHERE id = 'ws-default-001'",
    )
    .fetch_one(pool)
    .await
}

/// 回填默认 workspace 的 agent_id。
pub(crate) async fn set_default_workspace_agent_id(
    pool: &SqlitePool,
    agent_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE workspaces SET agent_id = ? WHERE id = 'ws-default-001'")
        .bind(agent_id)
        .execute(pool)
        .await?;
    Ok(())
}

impl Db {
    /// workspace 是否存在（按 id）。
    pub async fn workspace_exists_by_id(&self, ws_id: &str) -> std::result::Result<bool, sqlx::Error> {
        workspace_exists_by_id(self.pool(), ws_id).await
    }

    /// 创建用户个人 workspace（默认租户）。
    pub async fn insert_user_workspace(&self, ws_id: &str, name: String) -> std::result::Result<(), sqlx::Error> {
        insert_user_workspace(self.pool(), ws_id, name).await
    }

    /// 回填 workspace 的 agent_id。
    pub async fn set_workspace_agent_id(&self, agent_id: &str, ws_id: &str) -> std::result::Result<(), sqlx::Error> {
        set_workspace_agent_id(self.pool(), agent_id, ws_id).await
    }

    /// 默认 workspace 是否存在。
    pub async fn default_workspace_exists(&self) -> std::result::Result<bool, sqlx::Error> {
        default_workspace_exists(self.pool()).await
    }

    /// 创建默认 workspace（字面量行）。
    pub async fn insert_default_workspace(&self) -> std::result::Result<(), sqlx::Error> {
        insert_default_workspace(self.pool()).await
    }

    /// 默认 workspace 是否缺少 agent_id。
    pub async fn default_workspace_needs_agent(&self) -> std::result::Result<bool, sqlx::Error> {
        default_workspace_needs_agent(self.pool()).await
    }

    /// 回填默认 workspace 的 agent_id。
    pub async fn set_default_workspace_agent_id(&self, agent_id: &str) -> std::result::Result<(), sqlx::Error> {
        set_default_workspace_agent_id(self.pool(), agent_id).await
    }
}
