//! Per-session chat message persistence (chat_messages table).
//!
//! History is stored per session_key so the history API is session-scoped
//! (the zeroclaw in-memory agent history is shared across all sessions of a
//! workspace agent and cannot isolate them).

use sqlx::SqlitePool;

/// Upper bound on prior messages re-seeded into the LLM context per turn.
pub const SESSION_CONTEXT_MESSAGE_LIMIT: u32 = 50;

/// Create the chat_sessions row if missing. chat_messages has an FK to it,
/// and foreign_keys is ON in production pools.
pub async fn ensure_session(
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
pub async fn append_message(
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
pub async fn list_messages(
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

/// Format persisted messages for the chat history API response.
pub fn messages_to_history_json(messages: Vec<(String, String)>, session_key: &str) -> serde_json::Value {
    let msgs: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "content": [{ "type": "text", "text": content }],
            })
        })
        .collect();
    serde_json::json!({ "messages": msgs, "sessionKey": session_key })
}

/// Session-scoped history for the chat API (formerly
/// `AgentPool::chat_history` — moved off the pool in Task 7 fix round 1
/// because it is a pure db read). Empty `authorized_workspace` = unscoped
/// (admin) token; nothing to check against.
pub async fn session_history_json(
    db_pool: &SqlitePool,
    session_key: &str,
    limit: u32,
    authorized_workspace: &str,
) -> Result<serde_json::Value, crate::domains::agent::host::shared::config::AgentError> {
    use crate::domains::agent::host::shared::config::AgentError;

    let parsed = crate::domains::agent::host::session::SessionKey::parse(session_key)?;
    if !authorized_workspace.is_empty() {
        parsed.verify_workspace(authorized_workspace)?;
    }

    // DB-backed, session-scoped history. The zeroclaw in-memory agent
    // history is shared across all sessions of the workspace agent and
    // cannot isolate them.
    let messages = list_messages(db_pool, session_key, limit)
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
    Ok(messages_to_history_json(messages, session_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn session_history_rejects_session_from_other_workspace() {
        let pool = test_pool().await;
        let err = session_history_json(&pool, "agent:ws_other:agent_main/s1", 50, "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            crate::domains::agent::host::shared::config::AgentError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn session_history_with_unscoped_token_reads_persisted_messages() {
        let pool = test_pool().await;
        let key = "agent:ws1:agent_main/s1";
        ensure_session(&pool, key, "ws1", "agent_main").await.unwrap();
        append_message(&pool, key, "user", "hello", "r1").await.unwrap();

        // Empty authorized_workspace = unscoped (admin) token: no workspace
        // check, history served straight from the DB.
        let out = session_history_json(&pool, key, 50, "").await.unwrap();
        assert!(out.to_string().contains("hello"));
    }

    #[tokio::test]
    async fn append_and_list_roundtrip_chronological() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
        append_message(&pool, "agent:ws:a/s1", "user", "hello", "r1")
            .await
            .unwrap();
        append_message(&pool, "agent:ws:a/s1", "assistant", "hi there", "r1")
            .await
            .unwrap();

        let msgs = list_messages(&pool, "agent:ws:a/s1", 200).await.unwrap();
        assert_eq!(
            msgs,
            vec![
                ("user".to_string(), "hello".to_string()),
                ("assistant".to_string(), "hi there".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn list_is_scoped_to_session() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
        ensure_session(&pool, "agent:ws:a/s2", "ws", "a").await.unwrap();
        append_message(&pool, "agent:ws:a/s1", "user", "session one", "r1")
            .await
            .unwrap();
        append_message(&pool, "agent:ws:a/s2", "user", "session two", "r2")
            .await
            .unwrap();

        let s1 = list_messages(&pool, "agent:ws:a/s1", 200).await.unwrap();
        assert_eq!(s1.len(), 1);
        assert_eq!(s1[0].1, "session one");

        let s2 = list_messages(&pool, "agent:ws:a/s2", 200).await.unwrap();
        assert_eq!(s2.len(), 1);
        assert_eq!(s2[0].1, "session two");
    }

    #[tokio::test]
    async fn list_honors_limit_returning_most_recent() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
        for i in 0..5 {
            append_message(&pool, "agent:ws:a/s1", "user", &format!("msg {i}"), "r1")
                .await
                .unwrap();
        }

        let msgs = list_messages(&pool, "agent:ws:a/s1", 2).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].1, "msg 3");
        assert_eq!(msgs[1].1, "msg 4");
    }

    #[tokio::test]
    async fn append_without_session_row_fails_fk() {
        let pool = test_pool().await;
        let result = append_message(&pool, "agent:ws:a/missing", "user", "hello", "r1").await;
        assert!(result.is_err(), "FK must reject messages for unknown sessions");

        ensure_session(&pool, "agent:ws:a/missing", "ws", "a").await.unwrap();
        append_message(&pool, "agent:ws:a/missing", "user", "hello", "r1")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn ensure_session_is_idempotent() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
    }

    #[test]
    fn history_json_matches_frontend_shape() {
        let json = messages_to_history_json(
            vec![("user".into(), "你好".into()), ("assistant".into(), "你好！".into())],
            "agent:ws:a/s1",
        );
        assert_eq!(json["sessionKey"], "agent:ws:a/s1");
        let msgs = json["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"][0]["type"], "text");
        assert_eq!(msgs[0]["content"][0]["text"], "你好");
        assert_eq!(msgs[1]["role"], "assistant");
    }
}
