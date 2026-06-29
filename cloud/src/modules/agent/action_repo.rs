// AgentActionRepository — audit log for autonomous AI actions

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::agent::config::AgentError;

// ============================================================================
// Action type enums — typed replacements for raw strings
// ============================================================================

/// The kind of event that triggered an AI action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(rename = "alarm")]
    Alarm,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Heartbeat => "heartbeat",
            EventType::Alarm => "alarm",
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The type of AI action performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    #[serde(rename = "summary")]
    Summary,
    #[serde(rename = "auto_executed")]
    AutoExecuted,
    #[serde(rename = "proposal")]
    Proposal,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "analysis")]
    Analysis,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Summary => "summary",
            ActionType::AutoExecuted => "auto_executed",
            ActionType::Proposal => "proposal",
            ActionType::Error => "error",
            ActionType::Analysis => "analysis",
        }
    }
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single autonomous agent action (reasoning, tool call, result, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub alarm_id: Option<String>,
    pub device_id: Option<String>,
    pub event_type: EventType,
    pub action_type: ActionType,
    pub content: String,
    pub created_at: String,
}

impl AgentAction {
    pub fn new(
        workspace_id: String,
        agent_id: String,
        alarm_id: Option<String>,
        device_id: Option<String>,
        event_type: EventType,
        action_type: ActionType,
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
        event_types: &[EventType],
        limit: u32,
    ) -> Result<Vec<AgentAction>, AgentError>;
    async fn delete_old(&self, before: DateTime<Utc>) -> Result<usize, AgentError>;
    async fn update_content(&self, id: &str, content: &str) -> Result<(), AgentError>;
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

fn parse_event_type(s: &str) -> EventType {
    match s {
        "alarm" => EventType::Alarm,
        "heartbeat" => EventType::Heartbeat,
        other => {
            tracing::warn!(%other, "Unknown event_type in agent_actions, defaulting to Heartbeat");
            EventType::Heartbeat
        }
    }
}

fn parse_action_type(s: &str) -> ActionType {
    match s {
        "auto_executed" => ActionType::AutoExecuted,
        "proposal" => ActionType::Proposal,
        "error" => ActionType::Error,
        "analysis" => ActionType::Analysis,
        "summary" => ActionType::Summary,
        other => {
            tracing::warn!(%other, "Unknown action_type in agent_actions, defaulting to Summary");
            ActionType::Summary
        }
    }
}

fn row_to_action((id, ws, ag, al, dev, et, at, c, ca): ActionRow) -> AgentAction {
    AgentAction {
        id,
        workspace_id: ws,
        agent_id: ag,
        alarm_id: al,
        device_id: dev,
        event_type: parse_event_type(&et),
        action_type: parse_action_type(&at),
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
        .bind(&action.event_type.as_str())
        .bind(&action.action_type.as_str())
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

        Ok(rows.into_iter().map(row_to_action).collect())
    }

    async fn find_by_query(
        &self,
        query: &AgentActionQuery,
    ) -> Result<Vec<AgentAction>, AgentError> {
        use sqlx::QueryBuilder;

        let limit = query.limit.unwrap_or(100) as i64;
        let offset = query.offset.unwrap_or(0) as i64;

        let mut qb = QueryBuilder::new(
            "SELECT id, workspace_id, agent_id, alarm_id, device_id, \
             event_type, action_type, content, created_at \
             FROM agent_actions",
        );

        let mut has_where = false;
        if let Some(ref ws_id) = query.workspace_id {
            qb.push(" WHERE workspace_id = ");
            qb.push_bind(ws_id.clone());
            has_where = true;
        }
        if let Some(ref alarm_id) = query.alarm_id {
            qb.push(if has_where { " AND " } else { " WHERE " });
            qb.push("alarm_id = ");
            qb.push_bind(alarm_id.clone());
            has_where = true;
        }
        if let Some(ref agent_id) = query.agent_id {
            qb.push(if has_where { " AND " } else { " WHERE " });
            qb.push("agent_id = ");
            qb.push_bind(agent_id.clone());
            has_where = true;
        }
        if let Some(ref device_id) = query.device_id {
            qb.push(if has_where { " AND " } else { " WHERE " });
            qb.push("device_id = ");
            qb.push_bind(device_id.clone());
        }

        qb.push(" ORDER BY created_at DESC LIMIT ");
        qb.push_bind(limit);
        qb.push(" OFFSET ");
        qb.push_bind(offset);

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

    async fn find_recent_by_workspace(
        &self,
        workspace_id: &str,
        event_types: &[EventType],
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
                separated.push_bind(et.as_str());
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

    async fn update_content(&self, id: &str, content: &str) -> Result<(), AgentError> {
        sqlx::query("UPDATE agent_actions SET content = ? WHERE id = ?")
            .bind(content)
            .bind(id)
            .execute(self.db.pool())
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(())
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

    fn make_action(
        ws_id: &str,
        event_type: EventType,
        action_type: ActionType,
        content: &str,
    ) -> AgentAction {
        AgentAction {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: ws_id.to_string(),
            agent_id: "default".into(),
            alarm_id: None,
            device_id: None,
            event_type,
            action_type,
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

        let result =
            repo.find_recent_by_workspace("ws1", &[EventType::Heartbeat], 10).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_filters_event_types() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        // Insert mix of heartbeat and alarm actions
        repo.insert(&make_action("ws1", EventType::Heartbeat, ActionType::Summary, "hb1"))
            .await
            .unwrap();
        repo.insert(&make_action("ws1", EventType::Alarm, ActionType::Analysis, "alarm1"))
            .await
            .unwrap();
        repo.insert(&make_action("ws1", EventType::Heartbeat, ActionType::Summary, "hb2"))
            .await
            .unwrap();
        repo.insert(&make_action("ws2", EventType::Heartbeat, ActionType::Summary, "other_ws"))
            .await
            .unwrap();

        // Query only heartbeat events for ws1
        let result =
            repo.find_recent_by_workspace("ws1", &[EventType::Heartbeat], 10).await.unwrap();
        assert_eq!(result.len(), 2);
        assert!(
            result.iter().all(|a| a.event_type == EventType::Heartbeat && a.workspace_id == "ws1")
        );
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_multiple_event_types() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        repo.insert(&make_action("ws1", EventType::Heartbeat, ActionType::Summary, "hb1"))
            .await
            .unwrap();
        repo.insert(&make_action("ws1", EventType::Alarm, ActionType::Analysis, "alarm1"))
            .await
            .unwrap();
        repo.insert(&make_action("ws1", EventType::Heartbeat, ActionType::Error, "hb_err"))
            .await
            .unwrap();

        // Query both event types
        let result = repo
            .find_recent_by_workspace("ws1", &[EventType::Heartbeat, EventType::Alarm], 10)
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_respects_limit() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        for i in 0..5 {
            repo.insert(&make_action(
                "ws1",
                EventType::Heartbeat,
                ActionType::Summary,
                &format!("hb_{}", i),
            ))
            .await
            .unwrap();
        }

        let result =
            repo.find_recent_by_workspace("ws1", &[EventType::Heartbeat], 3).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_find_recent_by_workspace_no_event_types_returns_all() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        setup_db(&pool).await;
        let db = Arc::new(Database::new(pool));
        let repo = SqliteAgentActionRepository::new(db.clone());

        repo.insert(&make_action("ws1", EventType::Heartbeat, ActionType::Summary, "hb1"))
            .await
            .unwrap();
        repo.insert(&make_action("ws1", EventType::Alarm, ActionType::Analysis, "alarm1"))
            .await
            .unwrap();

        // Empty event_types = no filter
        let result = repo.find_recent_by_workspace("ws1", &[], 10).await.unwrap();
        assert_eq!(result.len(), 2);
    }
}
