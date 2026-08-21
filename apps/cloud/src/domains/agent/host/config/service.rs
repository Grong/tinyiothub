// ConfigService — AgentRuntimeConfig DB read/write + agent CRUD.
//
// Task 7 fix round 1: these were `impl AgentPool` methods in
// `host/agent/config.rs`; they are db-backed data operations, so they live
// here as cloud-side free functions. Callers that change config must
// invalidate the pooled agent themselves (`AgentPool::invalidate`).

use sqlx::SqlitePool;
use tinyiothub_storage::Db;

use tinyiothub_agent::AgentError;
use tinyiothub_agent::config::{
    AgentConfig, AgentInfo, AgentRuntimeConfig, compute_hash, default_agent_config,
};

// ============================================================================
// Agent CRUD (moved from host/agent/config.rs)
// ============================================================================

/// Insert an agent row; returns the new agent_id.
pub async fn create_agent(db_pool: &SqlitePool, config: &AgentConfig) -> Result<String, AgentError> {
    Db::new(db_pool.clone())
        .insert_agent(&config.workspace_id, &config.name)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))
}

/// Delete an agent and its config/tool rows. Caller invalidates the pool.
pub async fn delete_agent(db_pool: &SqlitePool, agent_id: &str) -> Result<(), AgentError> {
    let db = Db::new(db_pool.clone());
    let affected = db
        .delete_agent_row(agent_id)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if affected == 0 {
        return Err(AgentError::NotFound(agent_id.to_string()));
    }
    let _ = db.delete_agent_config_rows(agent_id).await;
    let _ = db.delete_agent_tool_rows(agent_id).await;
    Ok(())
}

pub async fn get_agent(db_pool: &SqlitePool, agent_id: &str) -> Result<AgentInfo, AgentError> {
    let row = Db::new(db_pool.clone())
        .find_agent_row(agent_id)
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
    let rows = Db::new(db_pool.clone())
        .list_agent_rows(workspace_id)
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
    let row = Db::new(db_pool.clone())
        .find_agent_config(agent_id)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if let Some(config_str) = row
        && let Ok(config) = serde_json::from_str::<AgentRuntimeConfig>(&config_str)
    {
        return Ok(config);
    }
    Ok(AgentRuntimeConfig::default())
}

/// Read agent config as JSON (for API responses).
pub async fn get_config_json(db_pool: &SqlitePool, agent_id: &str) -> Result<serde_json::Value, AgentError> {
    let row = Db::new(db_pool.clone())
        .find_agent_config_with_hash(agent_id)
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
    Db::new(db_pool.clone())
        .upsert_agent_config(agent_id, config, &config_hash)
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
    let row = Db::new(db_pool.clone())
        .find_agent_workspace(agent_id)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    match row {
        Some(ws) if ws == workspace_id => Ok(()),
        Some(_) | None => Err(AgentError::NotFound(agent_id.to_string())),
    }
}
