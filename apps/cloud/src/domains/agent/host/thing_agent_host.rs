//! Cloud-side `ThingAgentHost` implementation.
//!
//! Event-plane capabilities (T6):
//! - `subscribe_events` — subscribe to the global [`ThingEventBus`];
//! - `replay_events_since` — cursor compensation against the `events` table
//!   (rowid cursor + `min_level` filter, thing-sourced rows only, O27).
//!
//! Pushback capabilities (T13):
//! - `push_chat_message` — SQLite 直写 assistant 消息（O12，零 LLM 成本）；
//! - `recent_active_admin_session` — 工作区 30 天内最近活跃会话（chat_sessions
//!   无 admin 维度，见方法注释）；
//! - `notify_alert` — 写 events 表（agent 来源告警行，前端事件流既有通道）。

use std::sync::Arc;

use tinyiothub_agent::runtime::thing_agent::{ThingAgentHost, ThingEventSignal};

use crate::domains::event::bus::ThingEventBus;

pub struct CloudThingAgentHost {
    pool: sqlx::SqlitePool,
    bus: Arc<ThingEventBus>,
}

impl CloudThingAgentHost {
    pub fn new(pool: sqlx::SqlitePool, bus: Arc<ThingEventBus>) -> Self {
        Self { pool, bus }
    }
}

#[async_trait::async_trait]
impl ThingAgentHost for CloudThingAgentHost {
    fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
        self.bus.subscribe()
    }

    async fn replay_events_since(&self, cursor: i64, min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
        // The UUID `events.id` is not orderable — the cursor is the implicit
        // SQLite rowid, which is monotonic for appends (retention deletes
        // never lower max(rowid)). SQL lives in db::event (Task 10).
        let rows = tinyiothub_storage::Db::new(self.pool.clone())
            .replay_thing_events_since(cursor, min_level)
            .await?;

        let signals = rows
            .into_iter()
            .filter_map(|row| {
                let is_unknown = row
                    .metadata
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
                    .and_then(|v| v.get("unknown_event")?.as_bool())
                    .unwrap_or(false);
                Some(ThingEventSignal {
                    workspace_id: row.workspace_id?,
                    thing_id: row.device_id?,
                    event_name: row.event_subtype,
                    event_id: row.rid,
                    level: row.event_level,
                    data: serde_json::from_str(&row.content).unwrap_or(serde_json::Value::Null),
                    is_unknown,
                    actor: row.actor,
                })
            })
            .collect();
        Ok(signals)
    }

    /// O12：SQLite 直写 assistant 消息，零 LLM 成本。会话行必须已存在
    /// （用户会话在打开时由 chat service ensure_session 创建）——append_message
    /// 的 FK 会拒绝未知 session_key，这里先显式检查以给出可读错误。
    async fn push_chat_message(&self, session_key: &str, content: &str, run_id: &str) -> anyhow::Result<()> {
        let db = tinyiothub_storage::Db::new(self.pool.clone());
        let exists = db.session_exists(session_key).await?;
        anyhow::ensure!(exists, "unknown chat session: {session_key}");
        db.append_session_message(session_key, "assistant", content, run_id)
            .await?;
        Ok(())
    }

    /// 挂既有通知发布点：写入 events 表（source_type/actor = 'agent'，
    /// event_subtype = 'thing_agent_alert'，Error 级），前端事件流/SSE 已消费该表。
    async fn notify_alert(&self, workspace_id: &str, payload: serde_json::Value) -> anyhow::Result<()> {
        let title = payload
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("thing agent alert")
            .to_string();
        let content = serde_json::to_string(&payload)?;
        // SQL lives in db::event (Task 10)
        tinyiothub_storage::Db::new(self.pool.clone())
            .insert_agent_alert_event(workspace_id, &title, &content)
            .await?;
        Ok(())
    }

    /// chat_sessions 没有 admin/owner 维度（user_id 列从未被写入），用最接近的
    /// 既有机制：工作区内 30 天内有消息的最近活跃会话（O28）。
    ///
    /// NOTE: 当前实现返回工作区内任意用户的最近活跃会话，不区分 admin 角色——
    /// `chat_sessions.user_id` 列存在但写入路径（history.rs / session_repository_impl.rs）
    /// 均不填值，schema 暂无 admin 维度。单用户工作区形态下可接受（CEO 0E 决议）；
    /// 多用户防泄漏的 admin 收窄已入 TODOS（"chat 会话 admin 维度"）。
    async fn recent_active_admin_session(&self, workspace_id: &str) -> anyhow::Result<Option<String>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).timestamp_millis();
        let session_key = tinyiothub_storage::Db::new(self.pool.clone())
            .find_recent_active_session(workspace_id, cutoff)
            .await?;
        Ok(session_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::host::chat::history::{append_message, ensure_session, list_messages};
    use sqlx::Row;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn host(pool: sqlx::SqlitePool) -> CloudThingAgentHost {
        CloudThingAgentHost::new(pool, Arc::new(ThingEventBus::new()))
    }

    #[tokio::test]
    async fn push_chat_message_appends_assistant_message_readable() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();

        host(pool.clone())
            .push_chat_message("agent:ws:a/s1", "[acted] done · ✓ 已验证", "run_1")
            .await
            .expect("push");

        let msgs = list_messages(&pool, "agent:ws:a/s1", 10).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, "assistant");
        assert_eq!(msgs[0].1, "[acted] done · ✓ 已验证");
    }

    #[tokio::test]
    async fn push_chat_message_unknown_session_errors() {
        let pool = test_pool().await;
        let result = host(pool).push_chat_message("agent:ws:a/missing", "hi", "run_1").await;
        assert!(result.is_err(), "unknown session must error, got {result:?}");
    }

    #[tokio::test]
    async fn recent_active_admin_session_picks_most_recent_in_window() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/old", "ws", "a").await.unwrap();
        ensure_session(&pool, "agent:ws:a/new", "ws", "a").await.unwrap();
        ensure_session(&pool, "agent:other:a/s", "other", "a").await.unwrap();
        // 直接写时间戳控制先后（append_message 用当前时间，无法排序）。
        let now = chrono::Utc::now().timestamp_millis();
        for (key, ts) in [
            ("agent:ws:a/old", now - 60_000),
            ("agent:ws:a/new", now),
            ("agent:other:a/s", now),
        ] {
            sqlx::query("INSERT INTO chat_messages (session_key, role, content, timestamp, run_id) VALUES (?, 'user', 'm', ?, 'r')")
                .bind(key)
                .bind(ts)
                .execute(&pool)
                .await
                .unwrap();
        }

        let got = host(pool).recent_active_admin_session("ws").await.expect("query");
        assert_eq!(got.as_deref(), Some("agent:ws:a/new"));
    }

    #[tokio::test]
    async fn recent_active_admin_session_ignores_messages_older_than_30_days() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/stale", "ws", "a").await.unwrap();
        let stale = (chrono::Utc::now() - chrono::Duration::days(31)).timestamp_millis();
        sqlx::query(
            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id) VALUES (?, 'user', 'm', ?, 'r')",
        )
        .bind("agent:ws:a/stale")
        .bind(stale)
        .execute(&pool)
        .await
        .unwrap();

        let got = host(pool).recent_active_admin_session("ws").await.expect("query");
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn recent_active_admin_session_none_without_any_sessions() {
        let pool = test_pool().await;
        let got = host(pool).recent_active_admin_session("ws").await.expect("query");
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn notify_alert_persists_agent_event_row() {
        let pool = test_pool().await;
        let payload = serde_json::json!({
            "type": "thing_agent_run_alert",
            "reason": "run_failed",
            "run_id": "run_1",
            "summary": "调低设定值失败",
        });
        host(pool.clone())
            .notify_alert("ws", payload.clone())
            .await
            .expect("alert");

        let row = sqlx::query(
            "SELECT event_type, event_subtype, event_level, source_type, actor, title, content, workspace_id \
             FROM events WHERE event_subtype = 'thing_agent_alert'",
        )
        .fetch_one(&pool)
        .await
        .expect("row");
        assert_eq!(row.get::<String, _>("event_type"), "agent");
        assert_eq!(row.get::<i32, _>("event_level"), 4);
        assert_eq!(row.get::<String, _>("source_type"), "agent");
        assert_eq!(row.get::<String, _>("actor"), "agent");
        assert_eq!(row.get::<String, _>("title"), "调低设定值失败");
        assert_eq!(row.get::<String, _>("workspace_id"), "ws");
        let content: String = row.get("content");
        let back: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(back["reason"], "run_failed");
        assert_eq!(back["run_id"], "run_1");
    }

    // append_message 参与回推路径，保留一个直连冒烟用例防 FK 回归。
    #[tokio::test]
    async fn append_message_smoke_via_history_module() {
        let pool = test_pool().await;
        ensure_session(&pool, "agent:ws:a/s1", "ws", "a").await.unwrap();
        append_message(&pool, "agent:ws:a/s1", "assistant", "pong", "run_2")
            .await
            .unwrap();
        let msgs = list_messages(&pool, "agent:ws:a/s1", 10).await.unwrap();
        assert_eq!(msgs[0].1, "pong");
    }
}
