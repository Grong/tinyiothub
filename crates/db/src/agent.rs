//! Agent 配置/动作/死信持久化：agents / agent_configs / agent_tools /
//! agent_actions / agent_dead_letters 表（自 cloud agent/host 迁入，Task 12）。
//!
//! 注：AgentRuntimeConfig/AgentInfo 等类型属 agent crate，本文件只持有裸行
//! 数据；错误保持 sqlx::Error 以沿用 cloud 调用方既有错误文案。

use sqlx::SqlitePool;

use crate::database::Db;

// ──────────────────────────────────────────────
// agents / agent_configs / agent_tools（自 config/service.rs 迁入）
// ──────────────────────────────────────────────

/// 插入 agent 行；返回新 agent_id。
pub(crate) async fn insert_agent(
    pool: &SqlitePool,
    workspace_id: &str,
    name: &str,
) -> Result<String, sqlx::Error> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&agent_id)
    .bind(workspace_id)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(agent_id)
}

/// 删除 agent 行，返回受影响行数。
pub(crate) async fn delete_agent_row(pool: &SqlitePool, agent_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
        .bind(agent_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 删除 agent_configs 行（级联清理；调用方忽略错误）。
pub(crate) async fn delete_agent_config_rows(pool: &SqlitePool, agent_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 删除 agent_tools 行（级联清理；调用方忽略错误）。
pub(crate) async fn delete_agent_tool_rows(pool: &SqlitePool, agent_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM agent_tools WHERE agent_id = ?")
        .bind(agent_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 按 agent_id 查 (agent_id, workspace_id, name, status)。
pub(crate) async fn find_agent_row(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Option<(String, String, String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await
}

/// 按 workspace 列出 agent 行（新的在前）。
pub(crate) async fn list_agent_rows(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT agent_id, workspace_id, name, status FROM agents WHERE workspace_id = ? ORDER BY created_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

/// 读取 agent_configs.config。
pub(crate) async fn find_agent_config(pool: &SqlitePool, agent_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT config FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(config,)| config))
}

/// 读取 agent_configs.(config, config_hash)。
pub(crate) async fn find_agent_config_with_hash(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT config, config_hash FROM agent_configs WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await
}

/// 写入 agent config（upsert）。
pub(crate) async fn upsert_agent_config(
    pool: &SqlitePool,
    agent_id: &str,
    config: &str,
    config_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_configs (agent_id, config, config_hash, updated_at)
         VALUES (?, ?, ?, datetime('now'))
         ON CONFLICT(agent_id) DO UPDATE SET config = excluded.config, config_hash = excluded.config_hash, updated_at = datetime('now')",
    )
    .bind(agent_id)
    .bind(config)
    .bind(config_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询 agent 所属 workspace_id。
pub(crate) async fn find_agent_workspace(pool: &SqlitePool, agent_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT workspace_id FROM agents WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(ws,)| ws))
}

// ──────────────────────────────────────────────
// agent_actions（自 handler/workspace_heartbeat.rs 迁入）
// ──────────────────────────────────────────────

/// Heartbeat 动作行（action_type, content, created_at），最新 200 条。
pub(crate) async fn list_agent_heartbeat_actions(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND event_type = 'heartbeat' \
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

/// Proposal 动作行（action_type, content, created_at），最新 50 条。
pub(crate) async fn list_agent_proposal_actions(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

/// 按 proposalId 查最新 proposal 行（id, content）。
pub(crate) async fn find_agent_proposal(
    pool: &SqlitePool,
    workspace_id: &str,
    proposal_id: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, content FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         AND json_extract(content, '$.proposalId') = ? \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(proposal_id)
    .fetch_optional(pool)
    .await
}

/// 原子翻转 proposal 状态为 approved（仅 pending 行生效），返回受影响行数。
pub(crate) async fn flip_agent_proposal_approved(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE agent_actions SET content = json_set(content, '$.status', 'approved') \
         WHERE id = ? AND json_extract(content, '$.status') = 'pending'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 记录 proposal 执行结果（auto_executed 行；id 内部生成）。
pub(crate) async fn insert_agent_heartbeat_outcome(
    pool: &SqlitePool,
    workspace_id: &str,
    content: String,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
         VALUES (?, ?, ?, 'heartbeat', 'auto_executed', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(format!("__heartbeat__:{workspace_id}"))
    .bind(content)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// 更新 agent_actions.content。
pub(crate) async fn update_agent_action_content(
    pool: &SqlitePool,
    id: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agent_actions SET content = ? WHERE id = ?")
        .bind(content)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ──────────────────────────────────────────────
// agent_dead_letters（自 dlq_repo.rs 迁入）
// ──────────────────────────────────────────────

/// 死信行。
#[derive(Debug, sqlx::FromRow)]
pub struct AgentDeadLetterRow {
    pub id: String,
    pub workspace_id: String,
    pub event_type: String,
    pub payload_json: String,
    pub failure_reason: String,
    pub enqueued_at: String,
}

/// 入队死信，返回新 id。
pub(crate) async fn enqueue_agent_dead_letter(
    pool: &SqlitePool,
    workspace_id: &str,
    event_type: &str,
    payload_json: &str,
    failure_reason: &str,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO agent_dead_letters (id, workspace_id, event_type, payload_json, failure_reason, enqueued_at)
             VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(event_type)
    .bind(payload_json)
    .bind(failure_reason)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(id)
}

/// 按 workspace 列出死信（新的在前）。
pub(crate) async fn list_agent_dead_letters(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<AgentDeadLetterRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentDeadLetterRow>(
        "SELECT id, workspace_id, event_type, payload_json, failure_reason, enqueued_at
             FROM agent_dead_letters WHERE workspace_id = ? ORDER BY enqueued_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

/// 删除死信，返回受影响行数。
pub(crate) async fn delete_agent_dead_letter(pool: &SqlitePool, entry_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM agent_dead_letters WHERE id = ?")
        .bind(entry_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// 插入 agent 行；返回新 agent_id。
    pub async fn insert_agent(&self, workspace_id: &str, name: &str) -> Result<String, sqlx::Error> {
        insert_agent(self.pool(), workspace_id, name).await
    }

    /// 删除 agent 行，返回受影响行数。
    pub async fn delete_agent_row(&self, agent_id: &str) -> Result<u64, sqlx::Error> {
        delete_agent_row(self.pool(), agent_id).await
    }

    /// 删除 agent_configs 行（级联清理）。
    pub async fn delete_agent_config_rows(&self, agent_id: &str) -> Result<(), sqlx::Error> {
        delete_agent_config_rows(self.pool(), agent_id).await
    }

    /// 删除 agent_tools 行（级联清理）。
    pub async fn delete_agent_tool_rows(&self, agent_id: &str) -> Result<(), sqlx::Error> {
        delete_agent_tool_rows(self.pool(), agent_id).await
    }

    /// 按 agent_id 查 agent 行。
    pub async fn find_agent_row(
        &self,
        agent_id: &str,
    ) -> Result<Option<(String, String, String, String)>, sqlx::Error> {
        find_agent_row(self.pool(), agent_id).await
    }

    /// 按 workspace 列出 agent 行（新的在前）。
    pub async fn list_agent_rows(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        list_agent_rows(self.pool(), workspace_id).await
    }

    /// 读取 agent_configs.config。
    pub async fn find_agent_config(&self, agent_id: &str) -> Result<Option<String>, sqlx::Error> {
        find_agent_config(self.pool(), agent_id).await
    }

    /// 读取 agent_configs.(config, config_hash)。
    pub async fn find_agent_config_with_hash(
        &self,
        agent_id: &str,
    ) -> Result<Option<(String, String)>, sqlx::Error> {
        find_agent_config_with_hash(self.pool(), agent_id).await
    }

    /// 写入 agent config（upsert）。
    pub async fn upsert_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        config_hash: &str,
    ) -> Result<(), sqlx::Error> {
        upsert_agent_config(self.pool(), agent_id, config, config_hash).await
    }

    /// 查询 agent 所属 workspace_id。
    pub async fn find_agent_workspace(&self, agent_id: &str) -> Result<Option<String>, sqlx::Error> {
        find_agent_workspace(self.pool(), agent_id).await
    }

    /// Heartbeat 动作行（最新 200 条）。
    pub async fn list_agent_heartbeat_actions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<(String, String, String)>, sqlx::Error> {
        list_agent_heartbeat_actions(self.pool(), workspace_id).await
    }

    /// Proposal 动作行（最新 50 条）。
    pub async fn list_agent_proposal_actions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<(String, String, String)>, sqlx::Error> {
        list_agent_proposal_actions(self.pool(), workspace_id).await
    }

    /// 按 proposalId 查最新 proposal 行（id, content）。
    pub async fn find_agent_proposal(
        &self,
        workspace_id: &str,
        proposal_id: &str,
    ) -> Result<Option<(String, String)>, sqlx::Error> {
        find_agent_proposal(self.pool(), workspace_id, proposal_id).await
    }

    /// 原子翻转 proposal 状态为 approved，返回受影响行数。
    pub async fn flip_agent_proposal_approved(&self, id: &str) -> Result<u64, sqlx::Error> {
        flip_agent_proposal_approved(self.pool(), id).await
    }

    /// 记录 proposal 执行结果（auto_executed 行）。
    pub async fn insert_agent_heartbeat_outcome(
        &self,
        workspace_id: &str,
        content: String,
        now: &str,
    ) -> Result<(), sqlx::Error> {
        insert_agent_heartbeat_outcome(self.pool(), workspace_id, content, now).await
    }

    /// 更新 agent_actions.content。
    pub async fn update_agent_action_content(&self, id: &str, content: &str) -> Result<(), sqlx::Error> {
        update_agent_action_content(self.pool(), id, content).await
    }

    /// 入队死信，返回新 id。
    pub async fn enqueue_agent_dead_letter(
        &self,
        workspace_id: &str,
        event_type: &str,
        payload_json: &str,
        failure_reason: &str,
    ) -> Result<String, sqlx::Error> {
        enqueue_agent_dead_letter(self.pool(), workspace_id, event_type, payload_json, failure_reason).await
    }

    /// 按 workspace 列出死信（新的在前）。
    pub async fn list_agent_dead_letters(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<AgentDeadLetterRow>, sqlx::Error> {
        list_agent_dead_letters(self.pool(), workspace_id).await
    }

    /// 删除死信，返回受影响行数。
    pub async fn delete_agent_dead_letter(&self, entry_id: &str) -> Result<u64, sqlx::Error> {
        delete_agent_dead_letter(self.pool(), entry_id).await
    }
}
