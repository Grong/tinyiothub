use async_trait::async_trait;
use sqlx::Row;

use crate::{
    modules::agent::types::{Session, SessionError, SessionRepository},
    shared::persistence::Database,
};

/// SQLite implementation of SessionRepository (session index only)
#[derive(Debug, Clone)]
pub struct SqliteSessionRepository {
    database: Database,
}

impl SqliteSessionRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    fn parse_timestamp(s: &str) -> Option<i64> {
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
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn get(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        let row = sqlx::query(
            "SELECT session_key, workspace_id, agent_id, label, created_at, updated_at, metadata \
             FROM chat_sessions WHERE session_key = ?",
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
            "INSERT INTO chat_sessions (session_key, workspace_id, agent_id, label, created_at, updated_at, metadata) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
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
            "UPDATE chat_sessions \
             SET workspace_id = ?, agent_id = ?, label = ?, updated_at = ?, metadata = ? \
             WHERE session_key = ?",
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
    async fn test_get_or_create() {
        let repo = create_test_repo().await;
        let key = "agent:ws:agent1/sess2";

        let session = repo.get_or_create(key).await.unwrap();
        assert_eq!(session.session_key, key);
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent1");

        // Second call should return existing
        let session2 = repo.get_or_create(key).await.unwrap();
        assert_eq!(session2.session_key, key);
    }

    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let repo = create_test_repo().await;
        let result = repo.get("nonexistent:key/session").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_nonexistent_session() {
        let repo = create_test_repo().await;
        let session =
            Session::new("nonexistent:key".to_string(), "ws".to_string(), "agent".to_string());
        let result = repo.update(&session).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_session() {
        let repo = create_test_repo().await;
        let result = repo.delete("nonexistent:key").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }
}
