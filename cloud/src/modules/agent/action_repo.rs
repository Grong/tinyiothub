// AgentActionRepository — audit log for autonomous AI actions

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::agent::config::AgentError;

/// A single autonomous agent action (reasoning, tool call, result, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub alarm_id: Option<String>,
    pub device_id: Option<String>,
    pub event_type: String,
    pub action_type: String,
    pub content: String,
    pub created_at: String,
}

impl AgentAction {
    pub fn new(
        workspace_id: String,
        agent_id: String,
        alarm_id: Option<String>,
        device_id: Option<String>,
        event_type: String,
        action_type: String,
        content: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id,
            agent_id,
            alarm_id,
            device_id,
            event_type,
            action_type,
            content,
            created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// Query criteria for agent actions
#[derive(Debug, Clone, Default)]
pub struct AgentActionQuery {
    pub workspace_id: Option<String>,
    pub alarm_id: Option<String>,
    pub agent_id: Option<String>,
    pub device_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Repository trait for agent action audit log
#[async_trait]
pub trait AgentActionRepository: Send + Sync {
    async fn insert(&self, action: &AgentAction) -> Result<(), AgentError>;
    async fn find_by_alarm(
        &self,
        alarm_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<AgentAction>, AgentError>;
    async fn find_by_query(&self, query: &AgentActionQuery)
    -> Result<Vec<AgentAction>, AgentError>;
    async fn find_recent_by_workspace(
        &self,
        workspace_id: &str,
        event_types: &[&str],
        limit: u32,
    ) -> Result<Vec<AgentAction>, AgentError>;
    async fn delete_old(&self, before: DateTime<Utc>) -> Result<usize, AgentError>;
}

// ============================================================================
// SQLite implementation
// ============================================================================

use tinyiothub_storage::sqlite::Database;

pub struct SqliteAgentActionRepository {
    db: Arc<Database>,
}

type ActionRow =
    (String, String, String, Option<String>, Option<String>, String, String, String, String);

fn row_to_action((id, ws, ag, al, dev, et, at, c, ca): ActionRow) -> AgentAction {
    AgentAction {
        id,
        workspace_id: ws,
        agent_id: ag,
        alarm_id: al,
        device_id: dev,
        event_type: et,
        action_type: at,
        content: c,
        created_at: ca,
    }
}

impl SqliteAgentActionRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AgentActionRepository for SqliteAgentActionRepository {
    async fn insert(&self, action: &AgentAction) -> Result<(), AgentError> {
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&action.id)
        .bind(&action.workspace_id)
        .bind(&action.agent_id)
        .bind(&action.alarm_id)
        .bind(&action.device_id)
        .bind(&action.event_type)
        .bind(&action.action_type)
        .bind(&action.content)
        .bind(&action.created_at)
        .execute(self.db.pool())
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(())
    }

    async fn find_by_alarm(
        &self,
        alarm_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<AgentAction>, AgentError> {
        let limit = limit.unwrap_or(100) as i64;
        let rows: Vec<(
            String, String, String, Option<String>, Option<String>,
            String, String, String, String,
        )> = sqlx::query_as(
            "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
             FROM agent_actions WHERE alarm_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(alarm_id)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(id, ws, ag, al, dev, et, at, c, ca)| AgentAction {
                id,
                workspace_id: ws,
                agent_id: ag,
                alarm_id: al,
                device_id: dev,
                event_type: et,
                action_type: at,
                content: c,
                created_at: ca,
            })
            .collect())
    }

    async fn find_by_query(
        &self,
        query: &AgentActionQuery,
    ) -> Result<Vec<AgentAction>, AgentError> {
        let limit = query.limit.unwrap_or(100) as i64;
        let offset = query.offset.unwrap_or(0) as i64;

        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
        )> = if query.workspace_id.is_some() {
            sqlx::query_as(
                "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
                 FROM agent_actions WHERE workspace_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(query.workspace_id.as_deref().unwrap_or(""))
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?
        } else if query.alarm_id.is_some() {
            sqlx::query_as(
                "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
                 FROM agent_actions WHERE alarm_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(query.alarm_id.as_deref().unwrap_or(""))
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?
        } else if query.agent_id.is_some() {
            sqlx::query_as(
                "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
                 FROM agent_actions WHERE agent_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(query.agent_id.as_deref().unwrap_or(""))
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?
        } else if query.device_id.is_some() {
            sqlx::query_as(
                "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
                 FROM agent_actions WHERE device_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(query.device_id.as_deref().unwrap_or(""))
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?
        } else {
            sqlx::query_as(
                "SELECT id, workspace_id, agent_id, alarm_id, device_id, event_type, action_type, content, created_at
                 FROM agent_actions ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?
        };

        Ok(rows
            .into_iter()
            .map(|(id, ws, ag, al, dev, et, at, c, ca)| AgentAction {
                id,
                workspace_id: ws,
                agent_id: ag,
                alarm_id: al,
                device_id: dev,
                event_type: et,
                action_type: at,
                content: c,
                created_at: ca,
            })
            .collect())
    }

    async fn find_recent_by_workspace(
        &self,
        workspace_id: &str,
        event_types: &[&str],
        limit: u32,
    ) -> Result<Vec<AgentAction>, AgentError> {
        use sqlx::QueryBuilder;

        let mut qb = QueryBuilder::new(
            "SELECT id, workspace_id, agent_id, alarm_id, device_id, \
             event_type, action_type, content, created_at \
             FROM agent_actions WHERE workspace_id = ",
        );
        qb.push_bind(workspace_id.to_string());

        if !event_types.is_empty() {
            qb.push(" AND event_type IN (");
            let mut separated = qb.separated(", ");
            for et in event_types {
                separated.push_bind(*et);
            }
            separated.push_unseparated(") ");
        }

        qb.push(" ORDER BY created_at DESC LIMIT ");
        qb.push_bind(limit as i64);

        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
        )> = qb
            .build_query_as()
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_action).collect())
    }

    async fn delete_old(&self, before: DateTime<Utc>) -> Result<usize, AgentError> {
        let before_str = before.format("%Y-%m-%d %H:%M:%S").to_string();
        let result = sqlx::query("DELETE FROM agent_actions WHERE created_at < ?")
            .bind(&before_str)
            .execute(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(result.rows_affected() as usize)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use tinyiothub_storage::sqlite::Database;

    use super::*;

    async fn setup_db(pool: &sqlx::SqlitePool) {
        sqlx::query("DROP TABLE IF EXISTS agent_actions").execute(pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE agent_actions (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                alarm_id TEXT,
                device_id TEXT,
                event_type TEXT NOT NULL,
                action_type TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_action(ws_id: &str, event_type: &str, action_type: &str, content: &str) -> AgentAction {
        AgentAction {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: ws_id.to_string(),
            agent_id: "default".into(),
            alarm_id: None,
            device_id: None,
            event_type: event_type.to_string(),
            action_type: action_type.to_string(),
            content: content.to_string(),
            created_at: "2026-06-17 10:00:00".to_string(),
        }
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_empty() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db);

        let result = repo.find_recent_by_workspace("ws1", &["heartbeat"], 10).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_filters_event_types() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        // Insert mix of heartbeat and alarm actions
        repo.insert(&make_action("ws1", "heartbeat", "summary", "hb1")).await.unwrap();
        repo.insert(&make_action("ws1", "alarm", "analysis", "alarm1")).await.unwrap();
        repo.insert(&make_action("ws1", "heartbeat", "summary", "hb2")).await.unwrap();
        repo.insert(&make_action("ws2", "heartbeat", "summary", "other_ws")).await.unwrap();

        // Query only heartbeat events for ws1
        let result = repo.find_recent_by_workspace("ws1", &["heartbeat"], 10).await.unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|a| a.event_type == "heartbeat" && a.workspace_id == "ws1"));
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_multiple_event_types() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        repo.insert(&make_action("ws1", "heartbeat", "summary", "hb1")).await.unwrap();
        repo.insert(&make_action("ws1", "alarm", "analysis", "alarm1")).await.unwrap();
        repo.insert(&make_action("ws1", "heartbeat", "error", "hb_err")).await.unwrap();

        // Query both event types
        let result =
            repo.find_recent_by_workspace("ws1", &["heartbeat", "alarm"], 10).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_respects_limit() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        for i in 0..5 {
            repo.insert(&make_action("ws1", "heartbeat", "summary", &format!("hb_{}", i)))
                .await
                .unwrap();
        }

        let result = repo.find_recent_by_workspace("ws1", &["heartbeat"], 3).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_no_event_types_returns_all() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        repo.insert(&make_action("ws1", "heartbeat", "summary", "hb1")).await.unwrap();
        repo.insert(&make_action("ws1", "alarm", "analysis", "alarm1")).await.unwrap();

        // Empty event_types = no filter
        let result = repo.find_recent_by_workspace("ws1", &[], 10).await.unwrap();
        assert_eq!(result.len(), 2);
    }
}
