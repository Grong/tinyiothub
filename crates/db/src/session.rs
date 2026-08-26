//! Session 持久化：会话索引（P-集中化 E6b，自 agent crate 迁入）。
//!
//! 类型随 repo 住 db（方案 B）：Session 行类型 + SessionError 契约错误，
//! agent crate 经 re-export 兼容。

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use thiserror::Error;

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 契约错误）— 自 agent/host/types.rs 迁入
// ──────────────────────────────────────────────

// --- Session types ---

/// Errors that can occur during session operations
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Invalid session data: {0}")]
    InvalidData(String),
}

/// A chat session representing a conversation between user and agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session key: "agent:{workspace_id}:{agent_id}/{session_uuid}"
    pub session_key: String,
    /// Associated workspace ID
    pub workspace_id: String,
    /// Associated agent ID
    pub agent_id: String,
    /// Optional session label/title
    pub label: Option<String>,
    /// Session creation timestamp (Unix millis)
    pub created_at: i64,
    /// Last update timestamp (Unix millis)
    pub updated_at: i64,
    /// Session metadata (arbitrary JSON)
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Session {
    /// Create a new session
    pub fn new(session_key: String, workspace_id: String, agent_id: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            session_key,
            workspace_id,
            agent_id,
            label: None,
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Update the label
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Update metadata
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        if let Some(obj) = self.metadata.as_object_mut() {
            obj.insert(key.into(), value);
        }
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Touch the session (update updated_at)
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

// ──────────────────────────────────────────────
// 持久化函数（SQLite, session index only）
// ──────────────────────────────────────────────

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
            parse_timestamp(&s).ok_or_else(|| sqlx::Error::ColumnDecode {
                index: "created_at".into(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid datetime")),
            })
        })
    })?;
    let updated_at: i64 = row.try_get::<i64, _>("updated_at").or_else(|_| {
        row.try_get::<String, _>("updated_at").and_then(|s| {
            parse_timestamp(&s).ok_or_else(|| sqlx::Error::ColumnDecode {
                index: "updated_at".into(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid datetime")),
            })
        })
    })?;

    let metadata_str: String = row.try_get("metadata").unwrap_or_else(|_| "{}".to_string());
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or_else(|_| serde_json::json!({}));

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

pub(crate) async fn get_or_create_session(pool: &SqlitePool, session_key: &str) -> Result<Session, SessionError> {
    if let Some(session) = get_session(pool, session_key).await? {
        return Ok(session);
    }

    let parts: Vec<&str> = session_key.split('/').collect();
    if parts.len() != 2 {
        return Err(SessionError::InvalidData(format!(
            "Invalid session key format: {}",
            session_key
        )));
    }

    let prefix_parts: Vec<&str> = parts[0].split(':').collect();
    if prefix_parts.len() != 3 || prefix_parts[0] != "agent" {
        return Err(SessionError::InvalidData(format!(
            "Invalid session key prefix: {}",
            session_key
        )));
    }

    let workspace_id = prefix_parts[1].to_string();
    let agent_id = prefix_parts[2].to_string();
    let session = Session::new(session_key.to_string(), workspace_id, agent_id);

    match create_session(pool, &session).await {
        Ok(()) => Ok(session),
        Err(SessionError::RepositoryError(ref e)) if e.contains("UNIQUE") => get_session(pool, session_key)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_key.to_string())),
        Err(e) => Err(e),
    }
}

pub(crate) async fn get_session(pool: &SqlitePool, session_key: &str) -> Result<Option<Session>, SessionError> {
    let row = sqlx::query(
        "SELECT session_key, workspace_id, agent_id, label, created_at, updated_at, metadata \
         FROM chat_sessions WHERE session_key = ?",
    )
    .bind(session_key)
    .fetch_optional(pool)
    .await
    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

    match row {
        Some(r) => map_session_row(r)
            .map(Some)
            .map_err(|e| SessionError::RepositoryError(e.to_string())),
        None => Ok(None),
    }
}

pub(crate) async fn create_session(pool: &SqlitePool, session: &Session) -> Result<(), SessionError> {
    let metadata_str =
        serde_json::to_string(&session.metadata).map_err(|e| SessionError::RepositoryError(e.to_string()))?;

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
    .execute(pool)
    .await
    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

    Ok(())
}

pub(crate) async fn update_session(pool: &SqlitePool, session: &Session) -> Result<(), SessionError> {
    let metadata_str =
        serde_json::to_string(&session.metadata).map_err(|e| SessionError::RepositoryError(e.to_string()))?;

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
    .execute(pool)
    .await
    .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(SessionError::NotFound(session.session_key.clone()));
    }

    Ok(())
}

pub(crate) async fn delete_session(pool: &SqlitePool, session_key: &str) -> Result<(), SessionError> {
    // chat_messages has an FK to chat_sessions without ON DELETE CASCADE
    // in the original schema, so messages must go first.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
    sqlx::query("DELETE FROM chat_messages WHERE session_key = ?")
        .bind(session_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
    let result = sqlx::query("DELETE FROM chat_sessions WHERE session_key = ?")
        .bind(session_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(SessionError::NotFound(session_key.to_string()));
    }

    Ok(())
}

pub(crate) async fn list_sessions(
    pool: &SqlitePool,
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
        .fetch_all(pool)
        .await
        .map_err(|e| SessionError::RepositoryError(e.to_string()))?;

    let mut sessions = Vec::with_capacity(rows.len());
    for row in rows {
        sessions.push(map_session_row(row).map_err(|e| SessionError::RepositoryError(e.to_string()))?);
    }

    Ok(sessions)
}

// ── chat_messages 持久化（自 cloud agent/host/chat/history.rs 收编，D4b）──

/// Create the chat_sessions row if missing. chat_messages has an FK to it,
/// and foreign_keys is ON in production pools.
pub(crate) async fn ensure_session(
    pool: &SqlitePool,
    session_key: &str,
    workspace_id: &str,
    agent_id: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT OR IGNORE INTO chat_sessions \
         (session_key, workspace_id, agent_id, label, created_at, updated_at, metadata) \
         VALUES (?, ?, ?, NULL, ?, ?, '{}')",
    )
    .bind(session_key)
    .bind(workspace_id)
    .bind(agent_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Append one message to a session. Caller must ensure_session first.
pub(crate) async fn append_message(
    pool: &SqlitePool,
    session_key: &str,
    role: &str,
    content: &str,
    run_id: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(session_key)
    .bind(role)
    .bind(content)
    .bind(now)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load the most recent `limit` messages of a session, chronological order.
pub(crate) async fn list_messages(
    pool: &SqlitePool,
    session_key: &str,
    limit: u32,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let mut rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM chat_messages WHERE session_key = ? \
         ORDER BY id DESC LIMIT ?",
    )
    .bind(session_key)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// Whether the chat_sessions row exists (cheap EXISTS probe for push paths).
pub(crate) async fn session_exists(pool: &SqlitePool, session_key: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE session_key = ?)")
        .bind(session_key)
        .fetch_one(pool)
        .await
}

/// Most recently active session in the workspace with a message at or after
/// `active_since_ms` (自 thing_agent_host::recent_active_admin_session 收编).
pub(crate) async fn find_recent_active_session(
    pool: &SqlitePool,
    workspace_id: &str,
    active_since_ms: i64,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT cs.session_key, MAX(cm.timestamp) AS last_ts \
         FROM chat_sessions cs \
         JOIN chat_messages cm ON cm.session_key = cs.session_key \
         WHERE cs.workspace_id = ? \
         GROUP BY cs.session_key \
         HAVING last_ts >= ? \
         ORDER BY last_ts DESC \
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(active_since_ms)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get("session_key")))
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 按键查询会话（不存在返回 None）。
    pub async fn get_session(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        get_session(self.pool(), session_key).await
    }

    /// 按键查询会话，不存在则按 key 解析 workspace/agent 并创建。
    pub async fn get_or_create_session(&self, session_key: &str) -> Result<Session, SessionError> {
        get_or_create_session(self.pool(), session_key).await
    }

    /// 创建会话索引行。
    pub async fn create_session(&self, session: &Session) -> Result<(), SessionError> {
        create_session(self.pool(), session).await
    }

    /// 更新会话索引行（label/metadata/updated_at）。
    pub async fn update_session(&self, session: &Session) -> Result<(), SessionError> {
        update_session(self.pool(), session).await
    }

    /// 删除会话及其消息（内部事务：先删 chat_messages 再删 chat_sessions）。
    pub async fn delete_session(&self, session_key: &str) -> Result<(), SessionError> {
        delete_session(self.pool(), session_key).await
    }

    /// 分页列出会话（可按 workspace/agent 过滤）。
    pub async fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        list_sessions(self.pool(), workspace_id, agent_id, limit, offset).await
    }

    /// 会话行不存在则创建（INSERT OR IGNORE；chat_messages FK 前置）。
    pub async fn ensure_session(
        &self,
        session_key: &str,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<(), sqlx::Error> {
        ensure_session(self.pool(), session_key, workspace_id, agent_id).await
    }

    /// 追加一条会话消息（调用方须先 ensure_session）。
    pub async fn append_session_message(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
        run_id: &str,
    ) -> Result<(), sqlx::Error> {
        append_message(self.pool(), session_key, role, content, run_id).await
    }

    /// 读取会话最近 `limit` 条消息（按时间正序）。
    pub async fn list_session_messages(
        &self,
        session_key: &str,
        limit: u32,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        list_messages(self.pool(), session_key, limit).await
    }

    /// 会话行是否已存在（EXISTS 探测）。
    pub async fn session_exists(&self, session_key: &str) -> Result<bool, sqlx::Error> {
        session_exists(self.pool(), session_key).await
    }

    /// 工作区内 `active_since_ms` 之后仍有消息的最近活跃会话。
    pub async fn find_recent_active_session(
        &self,
        workspace_id: &str,
        active_since_ms: i64,
    ) -> Result<Option<String>, sqlx::Error> {
        find_recent_active_session(self.pool(), workspace_id, active_since_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    pub async fn create_test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    #[tokio::test]
    pub async fn test_session_crud() {
        let db = create_test_db().await;
        let session = Session::new(
            "agent:ws:agent1/sess1".to_string(),
            "ws".to_string(),
            "agent1".to_string(),
        );

        // Create
        db.create_session(&session).await.unwrap();

        // Get
        let found = db.get_session(&session.session_key).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.session_key, session.session_key);
        assert_eq!(found.workspace_id, session.workspace_id);
        assert_eq!(found.agent_id, session.agent_id);

        // Update label
        let mut updated = found;
        updated.set_label("Test Label");
        db.update_session(&updated).await.unwrap();

        let found = db.get_session(&session.session_key).await.unwrap().unwrap();
        assert_eq!(found.label, Some("Test Label".to_string()));

        // List
        let sessions = db.list_sessions(Some("ws"), Some("agent1"), 10, 0).await.unwrap();
        assert_eq!(sessions.len(), 1);

        // Delete
        db.delete_session(&session.session_key).await.unwrap();
        let found = db.get_session(&session.session_key).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    pub async fn test_get_or_create() {
        let db = create_test_db().await;
        let key = "agent:ws:agent1/sess2";

        let session = db.get_or_create_session(key).await.unwrap();
        assert_eq!(session.session_key, key);
        assert_eq!(session.workspace_id, "ws");
        assert_eq!(session.agent_id, "agent1");

        // Second call should return existing
        let session2 = db.get_or_create_session(key).await.unwrap();
        assert_eq!(session2.session_key, key);
    }

    #[tokio::test]
    pub async fn test_get_nonexistent_session() {
        let db = create_test_db().await;
        let result = db.get_session("nonexistent:key/session").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    pub async fn test_delete_session_also_removes_messages() {
        // FK is ON in production pools; chat_messages has no ON DELETE CASCADE
        // in the original schema, so delete must remove messages first.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        let db = Db::new(pool.clone());

        let session = Session::new(
            "agent:ws:agent1/sess_msgs".to_string(),
            "ws".to_string(),
            "agent1".to_string(),
        );
        db.create_session(&session).await.unwrap();
        sqlx::query("INSERT INTO chat_messages (session_key, role, content, timestamp) VALUES (?, 'user', 'hi', 1)")
            .bind(&session.session_key)
            .execute(&pool)
            .await
            .unwrap();

        db.delete_session(&session.session_key).await.unwrap();

        let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chat_messages WHERE session_key = ?")
            .bind(&session.session_key)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining.0, 0, "messages must be deleted with the session");
    }

    #[tokio::test]
    pub async fn test_update_nonexistent_session() {
        let db = create_test_db().await;
        let session = Session::new("nonexistent:key".to_string(), "ws".to_string(), "agent".to_string());
        let result = db.update_session(&session).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[tokio::test]
    pub async fn test_delete_nonexistent_session() {
        let db = create_test_db().await;
        let result = db.delete_session("nonexistent:key").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }
}
