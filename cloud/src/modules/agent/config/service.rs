// ConfigService — AgentRuntimeConfig DB read/write + pool invalidation

use sqlx::SqlitePool;

use crate::shared::agent::config::{compute_hash, default_agent_config, AgentError, AgentRuntimeConfig};

/// Read agent runtime config from DB. Falls back to default if not found.
pub async fn get_config(db_pool: &SqlitePool, agent_id: &str) -> Result<AgentRuntimeConfig, AgentError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT config FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if let Some((config_str,)) = row {
        if let Ok(config) = serde_json::from_str::<AgentRuntimeConfig>(&config_str) {
            return Ok(config);
        }
    }
    Ok(AgentRuntimeConfig::default())
}

/// Read agent config as JSON (for API responses).
pub async fn get_config_json(db_pool: &SqlitePool, agent_id: &str) -> Result<serde_json::Value, AgentError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT config, config_hash FROM agent_configs WHERE agent_id = ?",
    )
    .bind(agent_id)
    .fetch_optional(db_pool)
    .await
    .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    if let Some((config_str, config_hash)) = row {
        let config: serde_json::Value = serde_json::from_str(&config_str)
            .unwrap_or_else(|_| default_agent_config());
        return Ok(serde_json::json!({"config": config, "baseHash": config_hash}));
    }
    Ok(serde_json::json!({"config": default_agent_config(), "baseHash": null}))
}

/// Write agent config to DB.
pub async fn set_config(db_pool: &SqlitePool, agent_id: &str, config: &str) -> Result<(), AgentError> {
    let _: serde_json::Value = serde_json::from_str(config)
        .map_err(|e| AgentError::RequestFailed(format!("Invalid config: {}", e)))?;
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
