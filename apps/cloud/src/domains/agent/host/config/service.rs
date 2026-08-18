// 数据实现，留 cloud（D2）
// ConfigService — AgentRuntimeConfig DB read/write + agent CRUD.
//
// Task 7 fix round 1: these were `impl AgentPool` methods in
// `host/agent/config.rs`; they are db-backed data operations, so they live
// here as cloud-side free functions. Callers that change config must
// invalidate the pooled agent themselves (`AgentPool::invalidate`).

use sqlx::SqlitePool;

use tinyiothub_agent::AgentError;
use tinyiothub_agent::config::{
    AgentConfig, AgentInfo, AgentRuntimeConfig, compute_hash, default_agent_config,
};

// ============================================================================
// Agent CRUD (moved from host/agent/config.rs)
// ============================================================================

/// Insert an agent row; returns the new agent_id.
pub async fn create_agent(db_pool: &SqlitePool, config: &AgentConfig) -> Result<String, AgentError> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&agent_id)
    .bind(&config.workspace_id)
    .bind(&config.name)
    .execute(db_pool)
    .await
    .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    Ok(agent_id)
}

/// Delete an agent and its config/tool rows. Caller invalidates the pool.
pub async fn delete_agent(db_pool: &SqlitePool, agent_id: &str) -> Result<(), AgentError> {
    let result = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
        .bind(agent_id)
        .execute(db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if result.rows_affected() == 0 {
        return Err(AgentError::NotFound(agent_id.to_string()));
    }
    let _ = sqlx::query("DELETE FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .execute(db_pool)
        .await;
    let _ = sqlx::query("DELETE FROM agent_tools WHERE agent_id = ?")
        .bind(agent_id)
        .execute(db_pool)
        .await;
    Ok(())
}

pub async fn get_agent(db_pool: &SqlitePool, agent_id: &str) -> Result<AgentInfo, AgentError> {
    let row: Option<(String, String, String, String)> =
        sqlx::query_as("SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?")
            .bind(agent_id)
            .fetch_optional(db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

    match row {
        Some((id, _workspace, name, status)) => Ok(AgentInfo {
            id,
            name,
            status,
            created_at: None,
        }),
        None => Err(AgentError::NotFound(agent_id.to_string())),
    }
}

pub async fn list_agents(db_pool: &SqlitePool, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT agent_id, workspace_id, name, status FROM agents WHERE workspace_id = ? ORDER BY created_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(db_pool)
    .await
    .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, _ws, name, status)| serde_json::json!({"id": id, "name": name, "status": status, "workspaceId": _ws}),
        )
        .collect();

    Ok(serde_json::json!({"agents": items}))
}

/// Workspace-checked config read (for API responses).
pub async fn get_agent_config_json(
    db_pool: &SqlitePool,
    agent_id: &str,
    workspace_id: &str,
) -> Result<serde_json::Value, AgentError> {
    verify_agent_workspace(db_pool, agent_id, workspace_id).await?;
    get_config_json(db_pool, agent_id).await
}

/// Workspace-checked config write. Caller invalidates the pool on success.
pub async fn set_agent_config(
    db_pool: &SqlitePool,
    agent_id: &str,
    config: &str,
    workspace_id: &str,
) -> Result<(), AgentError> {
    verify_agent_workspace(db_pool, agent_id, workspace_id).await?;
    set_config(db_pool, agent_id, config).await
}

/// Add/remove a tool in the agent's denylist. Caller invalidates the pool.
pub async fn toggle_tool(
    db_pool: &SqlitePool,
    agent_id: &str,
    tool_name: &str,
    enabled: bool,
    workspace_id: &str,
) -> Result<(), AgentError> {
    verify_agent_workspace(db_pool, agent_id, workspace_id).await?;
    let mut config = get_config(db_pool, agent_id).await?;
    if enabled {
        config.tool_denylist.retain(|t| t != tool_name);
    } else if !config.tool_denylist.contains(&tool_name.to_string()) {
        config.tool_denylist.push(tool_name.to_string());
    }
    let config_str = serde_json::to_string(&config).map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    set_config(db_pool, agent_id, &config_str).await
}

// ============================================================================
// AgentRuntimeConfig read/write
// ============================================================================

/// Read agent runtime config from DB. Falls back to default if not found.
pub async fn get_config(db_pool: &SqlitePool, agent_id: &str) -> Result<AgentRuntimeConfig, AgentError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT config FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if let Some((config_str,)) = row
        && let Ok(config) = serde_json::from_str::<AgentRuntimeConfig>(&config_str)
    {
        return Ok(config);
    }
    Ok(AgentRuntimeConfig::default())
}

/// Read agent config as JSON (for API responses).
pub async fn get_config_json(db_pool: &SqlitePool, agent_id: &str) -> Result<serde_json::Value, AgentError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT config, config_hash FROM agent_configs WHERE agent_id = ?")
            .bind(agent_id)
            .fetch_optional(db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if let Some((config_str, config_hash)) = row {
        let config: serde_json::Value = serde_json::from_str(&config_str).unwrap_or_else(|_| default_agent_config());
        return Ok(serde_json::json!({"config": config, "baseHash": config_hash}));
    }
    Ok(serde_json::json!({"config": default_agent_config(), "baseHash": null}))
}

/// Write agent config to DB.
pub async fn set_config(db_pool: &SqlitePool, agent_id: &str, config: &str) -> Result<(), AgentError> {
    let _: serde_json::Value =
        serde_json::from_str(config).map_err(|e| AgentError::RequestFailed(format!("Invalid config: {}", e)))?;
    let config_hash = compute_hash(config);
    sqlx::query(
        "INSERT INTO agent_configs (agent_id, config, config_hash, updated_at)
         VALUES (?, ?, ?, datetime('now'))
         ON CONFLICT(agent_id) DO UPDATE SET config = excluded.config, config_hash = excluded.config_hash, updated_at = datetime('now')",
    )
    .bind(agent_id)
    .bind(config)
    .bind(&config_hash)
    .execute(db_pool)
    .await
    .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    Ok(())
}

/// Verify that an agent belongs to a workspace.
pub async fn verify_agent_workspace(
    db_pool: &SqlitePool,
    agent_id: &str,
    workspace_id: &str,
) -> Result<(), AgentError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT workspace_id FROM agents WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    match row {
        Some((ws,)) if ws == workspace_id => Ok(()),
        Some(_) | None => Err(AgentError::NotFound(agent_id.to_string())),
    }
}
