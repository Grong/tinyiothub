//! Assistant Schedule — 每个 workspace 一行，只保存“什么时候唤醒”。
//!
//! 任务内容来自主 prompt、技能或（迁移期）legacy heartbeat tasks；本表不存任务
//! 正文。时间统一以 UTC RFC3339（秒精度）文本存储，保证字典序 == 时间序。

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::Db;
use crate::error::{DbError, Result};
use crate::policy::PermissionMode;

pub const DEFAULT_ASSISTANT_INTERVAL_SECONDS: u32 = 300;
pub const MIN_ASSISTANT_INTERVAL_SECONDS: u32 = 60;
pub const MAX_ASSISTANT_INTERVAL_SECONDS: u32 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantSchedule {
    pub workspace_id: String,
    pub assistant_id: String,
    pub interval_seconds: u32,
    pub enabled: bool,
    pub permission_mode: PermissionMode,
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

/// Workspace-level schedules are never allowed to carry `FullAccess` (that is
/// session-explicit only); normalize it to `Ask` on read and write.
fn workspace_scoped(mode: PermissionMode) -> PermissionMode {
    match mode {
        PermissionMode::FullAccess => PermissionMode::Ask,
        other => other,
    }
}

fn map_row(row: &SqliteRow) -> Result<AssistantSchedule> {
    Ok(AssistantSchedule {
        workspace_id: row.try_get("workspace_id")?,
        assistant_id: row.try_get("assistant_id")?,
        interval_seconds: row.try_get::<i64, _>("interval_seconds")?.max(0) as u32,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        permission_mode: workspace_scoped(
            row.try_get::<String, _>("permission_mode")
                .ok()
                .and_then(|value| PermissionMode::parse(&value))
                // Unknown/corrupt values fall back to the column default.
                .unwrap_or(PermissionMode::Ask),
        ),
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
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, permission_mode, prompt, next_wake_at, last_wake_at, last_outcome \
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

    // One-time legacy mapping (spec 2026-11-07 §5.4.4): the retired
    // `workspace_autonomy_policy.mode` maps onto Assistant pause state and
    // schedule permission mode. Read before INSERT OR IGNORE so a new row
    // inherits the legacy intent; existing rows are left untouched.
    let (enabled, permission_mode) =
        match sqlx::query_scalar::<_, String>("SELECT mode FROM workspace_autonomy_policy WHERE workspace_id = ?")
            .bind(workspace_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(mode)) if mode.eq_ignore_ascii_case("off") => (false, "ask"),
            Ok(Some(mode)) if mode.eq_ignore_ascii_case("diagnose") => (true, "read_only"),
            Ok(Some(_)) => (true, "ask"),
            Ok(None) => (true, "ask"),
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "load legacy autonomy mode failed; defaulting to ask");
                (true, "ask")
            }
        };

    sqlx::query(
        "INSERT OR IGNORE INTO assistant_schedules \
         (workspace_id, assistant_id, interval_seconds, enabled, permission_mode, prompt, next_wake_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, '', ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(assistant_id)
    .bind(i64::from(DEFAULT_ASSISTANT_INTERVAL_SECONDS))
    .bind(if enabled { 1i64 } else { 0i64 })
    .bind(permission_mode)
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
        "SELECT workspace_id, assistant_id, interval_seconds, enabled, permission_mode, prompt, next_wake_at, last_wake_at, last_outcome \
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
    permission_mode: Option<PermissionMode>,
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
         permission_mode = COALESCE(?, permission_mode), \
         updated_at = ? \
         WHERE workspace_id = ?",
    )
    .bind(enabled.map(|value| if value { 1i64 } else { 0i64 }))
    .bind(interval_seconds.map(i64::from))
    .bind(prompt)
    .bind(next_wake_at.map(format_ts))
    .bind(permission_mode.map(|mode| workspace_scoped(mode).as_str()))
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
        permission_mode: Option<PermissionMode>,
    ) -> Result<Option<AssistantSchedule>> {
        update(
            self.pool(),
            workspace_id,
            enabled,
            interval_seconds,
            prompt,
            next_wake_at,
            permission_mode,
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
            .ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        assert_eq!(created.interval_seconds, DEFAULT_ASSISTANT_INTERVAL_SECONDS);
        assert!(created.enabled);
        assert_eq!(created.permission_mode, PermissionMode::Ask);
        assert_eq!(created.prompt, "");
        assert_eq!(created.next_wake_at, Some(now));

        let later = ts(1_800_000_100);
        let again = db
            .ensure_assistant_schedule("ws-1", "__assistant__ws-1", later)
            .await
            .unwrap();
        assert_eq!(again.next_wake_at, Some(now), "ensure must not overwrite existing row");
    }

    #[tokio::test]
    async fn due_scan_respects_enabled_and_time() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-due", "__assistant__ws-due", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.ensure_assistant_schedule(
            "ws-future",
            "__assistant__ws-future",
            now + chrono::Duration::seconds(300),
        )
        .await
        .unwrap();
        db.ensure_assistant_schedule("ws-off", "__assistant__ws-off", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.update_assistant_schedule("ws-off", Some(false), None, None, None, None)
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
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
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
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let next = now + chrono::Duration::seconds(600);
        let updated = db
            .update_assistant_schedule(
                "ws-1",
                Some(false),
                Some(600),
                Some("巡检 3 号楼"),
                Some(next),
                Some(PermissionMode::ReadOnly),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.interval_seconds, 600);
        assert_eq!(updated.permission_mode, PermissionMode::ReadOnly);
        assert_eq!(updated.prompt, "巡检 3 号楼");
        assert_eq!(updated.next_wake_at, Some(next));

        let untouched = db
            .update_assistant_schedule("ws-1", None, None, None, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(untouched.interval_seconds, 600);
        assert_eq!(untouched.permission_mode, PermissionMode::ReadOnly);
        assert_eq!(untouched.prompt, "巡检 3 号楼");

        let error = db
            .update_assistant_schedule("ws-1", None, Some(5), None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(error, DbError::Validation { .. }));
    }

    async fn seed_legacy_mode(db: &Db, workspace_id: &str, mode: &str) {
        sqlx::query("INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES (?, ?)")
            .bind(workspace_id)
            .bind(mode)
            .execute(db.pool())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn ensure_maps_legacy_autonomy_mode() {
        let db = test_db().await;
        let now = ts(1_800_000_000);

        seed_legacy_mode(&db, "ws-off", "off").await;
        let off = db
            .ensure_assistant_schedule("ws-off", "__assistant__ws-off", now)
            .await
            .unwrap();
        assert!(!off.enabled, "legacy off pauses the Assistant");
        assert_eq!(off.permission_mode, PermissionMode::Ask);

        seed_legacy_mode(&db, "ws-diagnose", "diagnose").await;
        let diagnose = db
            .ensure_assistant_schedule("ws-diagnose", "__assistant__ws-diagnose", now)
            .await
            .unwrap();
        assert!(diagnose.enabled);
        assert_eq!(diagnose.permission_mode, PermissionMode::ReadOnly);

        seed_legacy_mode(&db, "ws-act", "act").await;
        let act = db
            .ensure_assistant_schedule("ws-act", "__assistant__ws-act", now)
            .await
            .unwrap();
        assert!(act.enabled);
        assert_eq!(act.permission_mode, PermissionMode::Ask);

        // No legacy row -> enabled + Ask.
        let none = db
            .ensure_assistant_schedule("ws-none", "__assistant__ws-none", now)
            .await
            .unwrap();
        assert!(none.enabled);
        assert_eq!(none.permission_mode, PermissionMode::Ask);
    }

    #[tokio::test]
    async fn ensure_does_not_override_existing_schedule() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        db.update_assistant_schedule("ws-1", None, None, None, None, Some(PermissionMode::ReadOnly))
            .await
            .unwrap();

        // A legacy policy row appearing later must not rewrite the existing row.
        seed_legacy_mode(&db, "ws-1", "act").await;
        let again = db
            .ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        assert_eq!(again.permission_mode, PermissionMode::ReadOnly);
    }

    #[tokio::test]
    async fn permission_mode_round_trips_through_due_scan() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-due", "__assistant__ws-due", now - chrono::Duration::seconds(10))
            .await
            .unwrap();
        db.update_assistant_schedule("ws-due", None, None, None, None, Some(PermissionMode::ReadOnly))
            .await
            .unwrap();
        let due = db.list_due_assistant_schedules(now, 10).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].permission_mode, PermissionMode::ReadOnly);
    }

    /// Simulate rows that predate the permission migration: insert directly so
    /// `ensure`'s legacy mapping is not involved.
    async fn seed_existing_schedule(db: &Db, workspace_id: &str, enabled: bool, permission_mode: &str) {
        sqlx::query(
            "INSERT INTO assistant_schedules \
             (workspace_id, assistant_id, interval_seconds, enabled, permission_mode) \
             VALUES (?, ?, 300, ?, ?)",
        )
        .bind(workspace_id)
        .bind(format!("__assistant__{workspace_id}"))
        .bind(if enabled { 1i64 } else { 0i64 })
        .bind(permission_mode)
        .execute(db.pool())
        .await
        .unwrap();
    }

    /// Proves the real backfill migration SQL works on pre-existing rows and
    /// is idempotent (no duplicated copy in the test).
    #[tokio::test]
    async fn backfill_migration_maps_pre_existing_schedule_rows_once() {
        let db = test_db().await;

        seed_existing_schedule(&db, "ws-off", true, "ask").await;
        seed_existing_schedule(&db, "ws-diagnose", true, "ask").await;
        seed_existing_schedule(&db, "ws-act", true, "ask").await;
        seed_legacy_mode(&db, "ws-off", "off").await;
        seed_legacy_mode(&db, "ws-diagnose", "diagnose").await;
        seed_legacy_mode(&db, "ws-act", "act").await;

        // Migrations already ran in `test_db`; replay the real backfill SQL
        // exactly as the migration runner would have.
        let backfill = include_str!("../migrations/20261108000003_assistant_schedule_backfill.sql");
        sqlx::raw_sql(backfill).execute(db.pool()).await.unwrap();

        let off = db.get_assistant_schedule("ws-off").await.unwrap().unwrap();
        assert!(!off.enabled, "legacy off must pause pre-existing schedules");
        assert_eq!(off.permission_mode, PermissionMode::Ask);
        let diagnose = db.get_assistant_schedule("ws-diagnose").await.unwrap().unwrap();
        assert!(diagnose.enabled, "diagnose keeps the Assistant enabled");
        assert_eq!(diagnose.permission_mode, PermissionMode::ReadOnly);
        let act = db.get_assistant_schedule("ws-act").await.unwrap().unwrap();
        assert!(act.enabled, "legacy act must stay enabled");
        assert_eq!(act.permission_mode, PermissionMode::Ask);

        // Idempotent: a second replay changes nothing.
        sqlx::raw_sql(backfill).execute(db.pool()).await.unwrap();
        let off = db.get_assistant_schedule("ws-off").await.unwrap().unwrap();
        assert!(!off.enabled);
        let diagnose = db.get_assistant_schedule("ws-diagnose").await.unwrap().unwrap();
        assert!(diagnose.enabled);
        assert_eq!(diagnose.permission_mode, PermissionMode::ReadOnly);
        let act = db.get_assistant_schedule("ws-act").await.unwrap().unwrap();
        assert!(act.enabled);
        assert_eq!(act.permission_mode, PermissionMode::Ask);
    }

    #[tokio::test]
    async fn full_access_is_normalized_for_workspace_schedules() {
        let db = test_db().await;
        let now = ts(1_800_000_000);
        seed_existing_schedule(&db, "ws-full", true, "full_access").await;
        let row = db.get_assistant_schedule("ws-full").await.unwrap().unwrap();
        assert_eq!(
            row.permission_mode,
            PermissionMode::Ask,
            "read must not expose workspace full_access"
        );

        db.ensure_assistant_schedule("ws-update", "__assistant__ws-update", now)
            .await
            .unwrap();
        db.update_assistant_schedule("ws-update", None, None, None, None, Some(PermissionMode::FullAccess))
            .await
            .unwrap();
        let row = db.get_assistant_schedule("ws-update").await.unwrap().unwrap();
        assert_eq!(
            row.permission_mode,
            PermissionMode::Ask,
            "write must not persist full_access"
        );
    }
}
