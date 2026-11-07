//! Assistant Schedule — 每个 workspace 一行，只保存“什么时候唤醒”。
//!
//! 任务内容来自主 prompt、技能或（迁移期）legacy heartbeat tasks；本表不存任务
//! 正文。时间统一以 UTC RFC3339（秒精度）文本存储，保证字典序 == 时间序。

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::Db;
use crate::error::{DbError, Result};

pub const DEFAULT_ASSISTANT_INTERVAL_SECONDS: u32 = 300;
pub const MIN_ASSISTANT_INTERVAL_SECONDS: u32 = 60;
pub const MAX_ASSISTANT_INTERVAL_SECONDS: u32 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantSchedule {
    pub workspace_id: String,
    pub assistant_id: String,
    pub interval_seconds: u32,
    pub enabled: bool,
    pub prompt: String,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn map_row(row: &SqliteRow) -> Result<AssistantSchedule> {
    Ok(AssistantSchedule {
        workspace_id: row.try_get("workspace_id")?,
        assistant_id: row.try_get("assistant_id")?,
        interval_seconds: row.try_get::<i64, _>("interval_seconds")?.max(0) as u32,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        prompt: row.try_get("prompt")?,
        next_wake_at: row
            .try_get::<Option<String>, _>("next_wake_at")?
            .as_deref()
            .and_then(parse_ts),
        last_wake_at: row
            .try_get::<Option<String>, _>("last_wake_at")?
            .as_deref()
            .and_then(parse_ts),
        last_outcome: row.try_get("last_outcome")?,
    })
}

async fn get_row(pool: &sqlx::SqlitePool, workspace_id: &str) -> Result<Option<AssistantSchedule>> {
    let row = sqlx::query(
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, last_wake_at, last_outcome \
         FROM assistant_schedules WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(map_row).transpose()
}

pub(crate) async fn ensure(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    assistant_id: &str,
    now: DateTime<Utc>,
) -> Result<AssistantSchedule> {
    let stamp = format_ts(now);
    sqlx::query(
        "INSERT OR IGNORE INTO assistant_schedules \
         (workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, created_at, updated_at) \
         VALUES (?, ?, ?, 1, '', ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(assistant_id)
    .bind(i64::from(DEFAULT_ASSISTANT_INTERVAL_SECONDS))
    .bind(&stamp)
    .bind(&stamp)
    .bind(&stamp)
    .execute(pool)
    .await?;
    get_row(pool, workspace_id).await?.ok_or_else(|| DbError::NotFound {
        id: workspace_id.to_string(),
    })
}

pub(crate) async fn due(pool: &sqlx::SqlitePool, now: DateTime<Utc>, limit: i64) -> Result<Vec<AssistantSchedule>> {
    let limit = if limit <= 0 { 100 } else { limit.min(500) };
    let rows = sqlx::query(
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, prompt, next_wake_at, last_wake_at, last_outcome \
         FROM assistant_schedules \
         WHERE enabled = 1 AND next_wake_at IS NOT NULL AND next_wake_at <= ? \
         ORDER BY next_wake_at ASC LIMIT ?",
    )
    .bind(format_ts(now))
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter().map(map_row).collect()
}

pub(crate) async fn update(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    enabled: Option<bool>,
    interval_seconds: Option<u32>,
    prompt: Option<&str>,
    next_wake_at: Option<DateTime<Utc>>,
) -> Result<Option<AssistantSchedule>> {
    if let Some(interval) = interval_seconds
        && !(MIN_ASSISTANT_INTERVAL_SECONDS..=MAX_ASSISTANT_INTERVAL_SECONDS).contains(&interval)
    {
        return Err(DbError::Validation {
            message: format!(
                "interval_seconds must be within {MIN_ASSISTANT_INTERVAL_SECONDS}..={MAX_ASSISTANT_INTERVAL_SECONDS}"
            ),
        });
    }
    sqlx::query(
        "UPDATE assistant_schedules SET \
         enabled = COALESCE(?, enabled), \
         interval_seconds = COALESCE(?, interval_seconds), \
         prompt = COALESCE(?, prompt), \
         next_wake_at = COALESCE(?, next_wake_at), \
         updated_at = ? \
         WHERE workspace_id = ?",
    )
    .bind(enabled.map(|value| if value { 1i64 } else { 0i64 }))
    .bind(interval_seconds.map(i64::from))
    .bind(prompt)
    .bind(next_wake_at.map(format_ts))
    .bind(format_ts(Utc::now()))
    .bind(workspace_id)
    .execute(pool)
    .await?;
    get_row(pool, workspace_id).await
}

pub(crate) async fn mark_wake(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    next_wake_at: DateTime<Utc>,
    last_wake_at: DateTime<Utc>,
    last_outcome: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE assistant_schedules SET next_wake_at = ?, last_wake_at = ?, last_outcome = ?, updated_at = ? \
         WHERE workspace_id = ?",
    )
    .bind(format_ts(next_wake_at))
    .bind(format_ts(last_wake_at))
    .bind(last_outcome)
    .bind(format_ts(Utc::now()))
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn ensure_assistant_schedule(
        &self,
        workspace_id: &str,
        assistant_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AssistantSchedule> {
        ensure(self.pool(), workspace_id, assistant_id, now).await
    }

    pub async fn get_assistant_schedule(&self, workspace_id: &str) -> Result<Option<AssistantSchedule>> {
        get_row(self.pool(), workspace_id).await
    }

    pub async fn list_due_assistant_schedules(&self, now: DateTime<Utc>, limit: i64) -> Result<Vec<AssistantSchedule>> {
        due(self.pool(), now, limit).await
    }

    pub async fn update_assistant_schedule(
        &self,
        workspace_id: &str,
        enabled: Option<bool>,
        interval_seconds: Option<u32>,
        prompt: Option<&str>,
        next_wake_at: Option<DateTime<Utc>>,
    ) -> Result<Option<AssistantSchedule>> {
        update(
            self.pool(),
            workspace_id,
            enabled,
            interval_seconds,
            prompt,
            next_wake_at,
        )
        .await
    }

    pub async fn mark_assistant_wake(
        &self,
        workspace_id: &str,
        next_wake_at: DateTime<Utc>,
        last_wake_at: DateTime<Utc>,
        last_outcome: &str,
    ) -> Result<()> {
        mark_wake(self.pool(), workspace_id, next_wake_at, last_wake_at, last_outcome).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::test_helpers::run_all_migrations(&pool).await.unwrap();
        Db::new(pool)
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[tokio::test]
    async fn ensure_creates_default_row_once() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        let created = db
            .ensure_assistant_schedule("ws-1", "__assistant__:ws-1", now)
            .await
            .unwrap();
        assert_eq!(created.interval_seconds, DEFAULT_ASSISTANT_INTERVAL_SECONDS);
        assert!(created.enabled);
        assert_eq!(created.prompt, "");
        assert_eq!(created.next_wake_at, Some(now));

        let later = ts(1_800_000_100);
        let again = db
            .ensure_assistant_schedule("ws-1", "__assistant__:ws-1", later)
            .await
            .unwrap();
        assert_eq!(again.next_wake_at, Some(now), "ensure must not overwrite existing row");
    }

    #[tokio::test]
    async fn due_scan_respects_enabled_and_time() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-due", "__assistant__:ws-due", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.ensure_assistant_schedule(
            "ws-future",
            "__assistant__:ws-future",
            now + chrono::Duration::seconds(300),
        )
        .await
        .unwrap();
        db.ensure_assistant_schedule("ws-off", "__assistant__:ws-off", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.update_assistant_schedule("ws-off", Some(false), None, None, None)
            .await
            .unwrap();

        let due = db.list_due_assistant_schedules(now, 10).await.unwrap();
        let ids: Vec<&str> = due.iter().map(|row| row.workspace_id.as_str()).collect();
        assert_eq!(ids, vec!["ws-due"]);
    }

    #[tokio::test]
    async fn mark_wake_advances_and_records_outcome() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__:ws-1", now)
            .await
            .unwrap();
        let next = now + chrono::Duration::seconds(300);
        db.mark_assistant_wake("ws-1", next, now, "completed").await.unwrap();
        let row = db.get_assistant_schedule("ws-1").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(next));
        assert_eq!(row.last_wake_at, Some(now));
        assert_eq!(row.last_outcome.as_deref(), Some("completed"));
    }

    #[tokio::test]
    async fn update_with_none_leaves_fields_and_validates_interval() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__:ws-1", now)
            .await
            .unwrap();
        let next = now + chrono::Duration::seconds(600);
        let updated = db
            .update_assistant_schedule("ws-1", Some(false), Some(600), Some("巡检 3 号楼"), Some(next))
            .await
            .unwrap()
            .unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.interval_seconds, 600);
        assert_eq!(updated.prompt, "巡检 3 号楼");
        assert_eq!(updated.next_wake_at, Some(next));

        let untouched = db
            .update_assistant_schedule("ws-1", None, None, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(untouched.interval_seconds, 600);
        assert_eq!(untouched.prompt, "巡检 3 号楼");

        let error = db
            .update_assistant_schedule("ws-1", None, Some(5), None, None)
            .await
            .unwrap_err();
        assert!(matches!(error, DbError::Validation { .. }));
    }
}
