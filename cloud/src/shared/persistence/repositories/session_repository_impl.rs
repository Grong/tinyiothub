use async_trait::async_trait;
use sqlx::Row;

use crate::{
    modules::agent::types::{
        ChatMessage, CompactedSession, Session, SessionError, SessionRepository,
    },
    shared::persistence::Database,
};

/// SQLite implementation of SessionRepository
#[derive(Debug, Clone)]
pub struct SqliteSessionRepository {
    database: Database,
}

impl SqliteSessionRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    fn parse_timestamp(s: &str) -> Option<i64> {
        // SQLite may store integer millis as plain text when column type is TEXT
        if let Ok(ts) = s.parse::<i64>() {
            return Some(ts);
        }
        let formats = [
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M:%S%.3f",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%dT%H:%M:%S%.3f",
            "%Y-%m-%dT%H:%M:%S%:z",
            "%Y-%m-%d %H:%M:%S%:z",
        ];
        for fmt in &formats {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
                return Some(dt.and_utc().timestamp() * 1000);
            }
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp_millis());
        }
        None
    }

    fn map_session_row(row: sqlx::sqlite::SqliteRow) -> Result<Session, sqlx::Error> {
        let session_key: String = row.try_get("session_key")?;
        let workspace_id: Option<String> = row.try_get("workspace_id").ok();
        let agent_id: String = row.try_get("agent_id")?;
        let label: Option<String> = row.try_get("label").ok();

        // Handle both integer and text timestamps for compatibility
        let created_at: i64 = row.try_get::<i64, _>("created_at").or_else(|_| {
            row.try_get::<String, _>("created_at").and_then(|s| {
                Self::parse_timestamp(&s).ok_or_else(|| sqlx::Error::ColumnDecode {
                    index: "created_at".into(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid datetime",
                    )),
                })
            })
        })?;
        let updated_at: i64 = row.try_get::<i64, _>("updated_at").or_else(|_| {
            row.try_get::<String, _>("updated_at").and_then(|s| {
                Self::parse_timestamp(&s).ok_or_else(|| sqlx::Error::ColumnDecode {
                    index: "updated_at".into(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid datetime",
                    )),
                })
            })
        })?;

        let metadata_str: String = row.try_get("metadata").unwrap_or_else(|_| "{}".to_string());
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or_else(|_| serde_json::json!({}));

        Ok(Session {
            session_key,
            workspace_id: workspace_id.unwrap_or_default(),
            agent_id,
            label,
            created_at,
            updated_at,
            metadata,
        })
    }

    fn map_message_row(row: sqlx::sqlite::SqliteRow) -> Result<ChatMessage, sqlx::Error> {
        let role: String = row.try_get("role")?;
        let content: String = row.try_get("content")?;
        let timestamp: i64 = row.try_get("timestamp")?;
        let run_id: Option<String> = row.try_get("run_id").ok();
        let tool_call_id: Option<String> = row.try_get("tool_call_id").ok();
        let tool_name: Option<String> = row.try_get("tool_name").ok();

        Ok(ChatMessage {
            role,
            content,
            timestamp: Some(timestamp),
            run_id,
            tool_call_id,
            tool_name,
        })
    }
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn get(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        let row = sqlx::query(
            r#"
            SELECT session_key, workspace_id, agent_id, label, created_at, updated_at, metadata
            FROM chat_sessions WHERE session_key = ?
            "#,
        )
        .bind(session_key)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        match row {
            Some(r) => Self::map_session_row(r)
                .map(Some)
                .map_err(|e| SessionError::RepositoryError(e.to_string())),
            None => Ok(None),
        }
    }

    async fn create(&self, session: &Session) -> Result<(), SessionError> {
        let metadata_str = serde_json::to_string(&session.metadata)
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO chat_sessions (session_key, workspace_id, agent_id, label, created_at, updated_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.session_key)
        .bind(&session.workspace_id)
        .bind(&session.agent_id)
        .bind(&session.label)
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(&metadata_str)
        .execute(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, session: &Session) -> Result<(), SessionError> {
        let metadata_str = serde_json::to_string(&session.metadata)
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE chat_sessions
            SET workspace_id = ?, agent_id = ?, label = ?, updated_at = ?, metadata = ?
            WHERE session_key = ?
            "#,
        )
        .bind(&session.workspace_id)
        .bind(&session.agent_id)
        .bind(&session.label)
        .bind(session.updated_at)
        .bind(&metadata_str)
        .bind(&session.session_key)
        .execute(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(SessionError::NotFound(session.session_key.clone()));
        }

        Ok(())
    }

    async fn delete(&self, session_key: &str) -> Result<(), SessionError> {
        let result = sqlx::query("DELETE FROM chat_sessions WHERE session_key = ?")
            .bind(session_key)
            .execute(self.database.pool())
            .await
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(SessionError::NotFound(session_key.to_string()));
        }

        Ok(())
    }

    async fn list(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        use sqlx::QueryBuilder;

        let mut builder = QueryBuilder::new(
            "SELECT session_key, workspace_id, agent_id, label, created_at, updated_at, metadata \
             FROM chat_sessions WHERE 1=1",
        );

        if let Some(ws) = workspace_id {
            builder.push(" AND workspace_id = ").push_bind(ws);
        }
        if let Some(agent) = agent_id {
            builder.push(" AND agent_id = ").push_bind(agent);
        }
        builder.push(" ORDER BY updated_at DESC LIMIT ").push_bind(limit as i64);
        builder.push(" OFFSET ").push_bind(offset as i64);

        let rows = builder
            .build()
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            sessions.push(
                Self::map_session_row(row)
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?,
            );
        }

        Ok(sessions)
    }

    async fn add_message(
        &self,
        session_key: &str,
        message: ChatMessage,
    ) -> Result<(), SessionError> {
        sqlx::query(
            r#"
            INSERT INTO chat_messages (session_key, role, content, timestamp, run_id, tool_call_id, tool_name)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(session_key)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis()))
        .bind(&message.run_id)
        .bind(&message.tool_call_id)
        .bind(&message.tool_name)
        .execute(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn get_messages(
        &self,
        session_key: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ChatMessage>, SessionError> {
        let rows = sqlx::query(
            r#"
            SELECT role, content, timestamp, run_id, tool_call_id, tool_name
            FROM chat_messages
            WHERE session_key = ?
            ORDER BY timestamp ASC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(session_key)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            messages.push(
                Self::map_message_row(row)
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?,
            );
        }

        Ok(messages)
    }

    async fn get_message_count(&self, session_key: &str) -> Result<usize, SessionError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_key = ?")
                .bind(session_key)
                .fetch_one(self.database.pool())
                .await
                .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        #[allow(clippy::cast_sign_loss)]
        Ok(count as usize)
    }

    async fn delete_messages_before(
        &self,
        session_key: &str,
        timestamp: i64,
    ) -> Result<usize, SessionError> {
        let result =
            sqlx::query("DELETE FROM chat_messages WHERE session_key = ? AND timestamp < ?")
                .bind(session_key)
                .bind(timestamp)
                .execute(self.database.pool())
                .await
                .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        #[allow(clippy::cast_sign_loss)]
        Ok(result.rows_affected() as usize)
    }

    async fn save_compacted(&self, compacted: &CompactedSession) -> Result<(), SessionError> {
        let system_messages = serde_json::to_string(&compacted.system_messages)
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
        let summary_message = compacted
            .summary_message
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
        let recent_messages = serde_json::to_string(&compacted.recent_messages)
            .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO chat_compacted_sessions
                (session_key, system_messages, summary_message, recent_messages, compacted_at, original_message_count)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_key) DO UPDATE SET
                system_messages = excluded.system_messages,
                summary_message = excluded.summary_message,
                recent_messages = excluded.recent_messages,
                compacted_at = excluded.compacted_at,
                original_message_count = excluded.original_message_count
            "#,
        )
        .bind(&compacted.session_key)
        .bind(&system_messages)
        .bind(&summary_message)
        .bind(&recent_messages)
        .bind(compacted.compacted_at)
        .bind(compacted.original_message_count as i64)
        .execute(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn get_compacted(
        &self,
        session_key: &str,
    ) -> Result<Option<CompactedSession>, SessionError> {
        let row = sqlx::query(
            r#"
            SELECT session_key, system_messages, summary_message, recent_messages, compacted_at, original_message_count
            FROM chat_compacted_sessions WHERE session_key = ?
            "#,
        )
        .bind(session_key)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

        match row {
            Some(r) => {
                let session_key: String = r
                    .try_get("session_key")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let system_messages_str: String = r
                    .try_get("system_messages")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let summary_message_str: Option<String> = r
                    .try_get("summary_message")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let recent_messages_str: String = r
                    .try_get("recent_messages")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let compacted_at: i64 = r
                    .try_get("compacted_at")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let original_message_count: i64 = r
                    .try_get("original_message_count")
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

                let system_messages: Vec<ChatMessage> = serde_json::from_str(&system_messages_str)
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let summary_message: Option<ChatMessage> = summary_message_str
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
                let recent_messages: Vec<ChatMessage> = serde_json::from_str(&recent_messages_str)
                    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

                Ok(Some(CompactedSession {
                    session_key,
                    system_messages,
                    summary_message,
                    recent_messages,
                    compacted_at,
                    original_message_count: original_message_count as usize,
                }))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::agent::types::{Session, SessionRepository};

    async fn create_test_repo() -> SqliteSessionRepository {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::shared::persistence::test_helpers::run_all_migrations(&pool).await.unwrap();
        SqliteSessionRepository::new(Database::new(pool))
    }

    #[tokio::test]
    async fn test_session_crud() {
        let repo = create_test_repo().await;
        let session = Session::new(
            "agent:ws:agent1/sess1".to_string(),
            "ws".to_string(),
            "agent1".to_string(),
        );

        // Create
        repo.create(&session).await.unwrap();

        // Get
        let found = repo.get(&session.session_key).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.session_key, session.session_key);
        assert_eq!(found.workspace_id, session.workspace_id);
        assert_eq!(found.agent_id, session.agent_id);

        // Update label
        let mut updated = found;
        updated.set_label("Test Label");
        repo.update(&updated).await.unwrap();

        let found = repo.get(&session.session_key).await.unwrap().unwrap();
        assert_eq!(found.label, Some("Test Label".to_string()));

        // List
        let sessions = repo.list(Some("ws"), Some("agent1"), 10, 0).await.unwrap();
        assert_eq!(sessions.len(), 1);

        // Delete
        repo.delete(&session.session_key).await.unwrap();
        let found = repo.get(&session.session_key).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_messages() {
        let repo = create_test_repo().await;
        let session = Session::new(
            "agent:ws:agent1/sess2".to_string(),
            "ws".to_string(),
            "agent1".to_string(),
        );
        repo.create(&session).await.unwrap();

        let msg1 = ChatMessage::user("Hello");
        let msg2 = ChatMessage::assistant("Hi there");

        repo.add_message(&session.session_key, msg1).await.unwrap();
        repo.add_message(&session.session_key, msg2).await.unwrap();

        let count = repo.get_message_count(&session.session_key).await.unwrap();
        assert_eq!(count, 2);

        let messages = repo.get_messages(&session.session_key, 10, 0).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[tokio::test]
    async fn test_compacted_session() {
        let repo = create_test_repo().await;
        let session = Session::new(
            "agent:ws:agent1/sess3".to_string(),
            "ws".to_string(),
            "agent1".to_string(),
        );
        repo.create(&session).await.unwrap();

        let compacted = CompactedSession {
            session_key: session.session_key.clone(),
            system_messages: vec![ChatMessage::system("You are helpful")],
            summary_message: Some(ChatMessage::assistant("Summary")),
            recent_messages: vec![ChatMessage::user("Recent")],
            compacted_at: chrono::Utc::now().timestamp_millis(),
            original_message_count: 10,
        };

        repo.save_compacted(&compacted).await.unwrap();

        let found = repo.get_compacted(&session.session_key).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.original_message_count, 10);
        assert_eq!(found.system_messages.len(), 1);
    }

    #[tokio::test]
    async fn test_get_or_create() {
        let repo = create_test_repo().await;
        let key = "agent:ws:agent1/sess4";

        let session = repo.get_or_create(key).await.unwrap();
        assert_eq!(session.session_key, key);
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent1");

        // Second call should return existing
        let session2 = repo.get_or_create(key).await.unwrap();
        assert_eq!(session2.session_key, key);
    }
}
