// AgentPool — agent config management: agent CRUD, config get/set (delegated
// to ConfigService), tools catalog/effective/toggle (delegated to ToolService).

use super::pool::AgentPool;
use crate::domains::agent::host::shared::config::{AgentConfig, AgentError, AgentInfo};
use crate::domains::agent::host::{config::service as config_service, tools::service as tool_service};

impl AgentPool {
    // ========================================================================
    // Agent CRUD
    // ========================================================================

    pub async fn create_agent(&self, config: &AgentConfig) -> Result<String, AgentError> {
        let workspace_id = config.workspace_id.clone();
        let name = config.name.clone();
        let agent_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
             VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
        )
        .bind(&agent_id)
        .bind(&workspace_id)
        .bind(&name)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(agent_id)
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<(), AgentError> {
        let agent_id = agent_id.to_string();
        let result = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(AgentError::NotFound(agent_id));
        }
        let _ = sqlx::query("DELETE FROM agent_configs WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await;
        let _ = sqlx::query("DELETE FROM agent_tools WHERE agent_id = ?")
            .bind(&agent_id)
            .execute(&self.db_pool)
            .await;
        self.invalidate(&agent_id);
        Ok(())
    }

    pub async fn get_agent(&self, agent_id: &str) -> Result<AgentInfo, AgentError> {
        let agent_id = agent_id.to_string();
        let row: Option<(String, String, String, String)> =
            sqlx::query_as("SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?")
                .bind(&agent_id)
                .fetch_optional(&self.db_pool)
                .await
                .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        match row {
            Some((id, _workspace, name, status)) => Ok(AgentInfo {
                id,
                name,
                status,
                created_at: None,
            }),
            None => Err(AgentError::NotFound(agent_id)),
        }
    }

    pub async fn list_agents(&self, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT agent_id, workspace_id, name, status FROM agents WHERE workspace_id = ? ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        let items: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|(id, _ws, name, status)| {
                serde_json::json!({"id": id, "name": name, "status": status, "workspaceId": _ws})
            })
            .collect();

        Ok(serde_json::json!({"agents": items}))
    }

    // ========================================================================
    // Config (delegated to ConfigService)
    // ========================================================================

    pub async fn get_agent_config(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        config_service::get_config_json(&self.db_pool, agent_id).await
    }

    pub async fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        base_hash: Option<&str>,
        workspace_id: &str,
    ) -> Result<(), AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        config_service::set_config(&self.db_pool, agent_id, config).await?;
        self.invalidate(agent_id);
        // Silently ignore base_hash mismatch — last write wins
        let _ = base_hash;
        Ok(())
    }

    // ========================================================================
    // Tools (delegated to ToolService)
    // ========================================================================

    pub async fn tools_catalog(&self, _agent_id: &str) -> Result<serde_json::Value, AgentError> {
        Ok(tool_service::build_catalog().await)
    }

    pub async fn tools_effective(&self, agent_id: &str, workspace_id: &str) -> Result<serde_json::Value, AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let all_tools = {
            let runtime = self.runtime.read().await.clone();
            tool_service::load_all_tools(workspace_id, Some(self.db_pool.clone()), &runtime).await
        };
        let effective = tool_service::filter_by_denylist(all_tools, &config.tool_denylist);
        let names: Vec<&str> = effective.iter().map(|t| t.name()).collect();
        Ok(serde_json::json!({ "tools": names }))
    }

    pub async fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
        workspace_id: &str,
    ) -> Result<(), AgentError> {
        config_service::verify_agent_workspace(&self.db_pool, agent_id, workspace_id).await?;
        let mut config = config_service::get_config(&self.db_pool, agent_id).await?;
        if enabled {
            config.tool_denylist.retain(|t| t != tool_name);
        } else if !config.tool_denylist.contains(&tool_name.to_string()) {
            config.tool_denylist.push(tool_name.to_string());
        }
        let config_str = serde_json::to_string(&config).map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        config_service::set_config(&self.db_pool, agent_id, &config_str).await?;
        self.invalidate(agent_id);
        Ok(())
    }
}
