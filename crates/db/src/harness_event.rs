use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use sqlx::SqlSafeStr;
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent, HarnessEventInput};

use crate::{
    Db,
    error::{DbError, Result},
};

const CURRENT_EVENT_SCHEMA: i64 = 1;
const DEFAULT_EVENT_PAGE_SIZE: i64 = 256;

struct SessionEventPageQueries {
    session: &'static str,
    run: &'static str,
    task: &'static str,
}

const SESSION_EVENT_PAGE_SQL: SessionEventPageQueries = SessionEventPageQueries {
    session: "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json FROM session_events
        WHERE execution_mode = ? AND session_id = ?
          AND (? IS NULL OR run_id = ?) AND (? IS NULL OR task_id = ?)
          AND seq > ? ORDER BY seq ASC LIMIT ?",
    run: "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json FROM session_events
        WHERE execution_mode = ? AND run_id = ? AND (? IS NULL OR task_id = ?)
          AND seq > ? ORDER BY seq ASC LIMIT ?",
    task: "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json FROM session_events
        WHERE execution_mode = ? AND task_id = ? AND seq > ? ORDER BY seq ASC LIMIT ?",
};

const EVENT_BY_ID_SQL: &str = "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json
    FROM session_events WHERE event_id = ?";

const MODE_EVENT_PAGE_SQL: &str = "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json FROM session_events
    WHERE execution_mode = ? AND seq > ? ORDER BY seq ASC LIMIT ?";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFilter {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub execution_mode: ExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    pub events: Vec<HarnessEvent>,
    pub high_watermark: i64,
}

#[derive(sqlx::FromRow)]
struct SessionEventRow {
    seq: i64,
    event_id: String,
    session_id: String,
    execution_mode: String,
    run_id: Option<String>,
    task_id: Option<String>,
    #[sqlx(rename = "type")]
    event_type: String,
    occurred_at: String,
    cause_id: Option<i64>,
    schema_v: i64,
    payload_digest: String,
    payload_json: String,
}

impl SessionEventRow {
    fn into_event(self, workspace_id: &str) -> Result<HarnessEvent> {
        validate_schema(self.schema_v)?;
        let execution_mode = ExecutionMode::from_db(&self.execution_mode).ok_or_else(|| DbError::Validation {
            message: format!("unknown execution mode: {}", self.execution_mode),
        })?;
        let occurred_at = DateTime::parse_from_rfc3339(&self.occurred_at)
            .map_err(|error| DbError::Internal(format!("invalid event occurred_at: {error}")))?
            .with_timezone(&Utc);
        let payload_json: Value = serde_json::from_str(&self.payload_json)?;
        validate_payload(&self.event_type, &payload_json)?;
        Ok(HarnessEvent {
            seq: self.seq,
            event_id: self.event_id,
            workspace_id: workspace_id.to_string(),
            session_id: self.session_id,
            execution_mode,
            run_id: self.run_id,
            task_id: self.task_id,
            event_type: self.event_type,
            occurred_at,
            cause_id: self.cause_id,
            schema_v: self.schema_v,
            payload_digest: self.payload_digest,
            payload_json,
        })
    }
}

pub(crate) fn validate_schema(schema_v: i64) -> Result<()> {
    if schema_v != CURRENT_EVENT_SCHEMA {
        return Err(DbError::Validation {
            message: format!("unknown harness event schema: {schema_v}"),
        });
    }
    Ok(())
}

pub(crate) fn validate_payload(event_type: &str, payload: &Value) -> Result<()> {
    let segment = match event_type {
        "step.thinking" => Some("thinking"),
        "step.text" => Some("text"),
        _ => None,
    };
    if let Some(segment) = segment {
        let empty = payload
            .get(segment)
            .and_then(Value::as_str)
            .is_none_or(|text| text.trim().is_empty());
        if empty {
            return Err(DbError::Validation {
                message: format!("empty {event_type} segment is not persisted"),
            });
        }
    }
    validate_media_value(payload)
}

fn validate_media_value(value: &Value) -> Result<()> {
    validate_media_value_inner(value, false)
}

fn validate_media_value_inner(value: &Value, in_media: bool) -> Result<()> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
                if matches!(
                    normalized.as_str(),
                    "bytes"
                        | "base64"
                        | "signedurl"
                        | "imagebytes"
                        | "videobytes"
                        | "mediabytes"
                        | "imagebase64"
                        | "videobase64"
                        | "mediabase64"
                ) || (in_media && matches!(normalized.as_str(), "data" | "content"))
                    || (matches!(normalized.as_str(), "url" | "uri")
                        && value.as_str().is_some_and(|url| {
                            is_signed_url(url)
                                || (in_media && url.get(..5).is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:")))
                        }))
                {
                    return Err(DbError::Validation {
                        message: "event media may contain only metadata, hashes, and evidence references".to_string(),
                    });
                }
                let nested_media = in_media
                    || matches!(
                        normalized.as_str(),
                        "media"
                            | "image"
                            | "video"
                            | "attachment"
                            | "attachments"
                            | "providerattachment"
                            | "providerattachments"
                            | "evidence"
                    );
                validate_media_value_inner(value, nested_media)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_media_value_inner(value, in_media)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_signed_url(value: &str) -> bool {
    let query = value.split_once('?').map(|(_, query)| query).unwrap_or_default();
    query.split('&').any(|parameter| {
        let name = parameter
            .split_once('=')
            .map(|(name, _)| name)
            .unwrap_or(parameter)
            .to_ascii_lowercase();
        matches!(
            name.as_str(),
            "signature" | "sig" | "x-amz-signature" | "x-goog-signature"
        )
    })
}

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000002,
        "harness_events".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000002_harness_events.sql").into_sql_str(),
        false,
    )
}

impl Db {
    pub async fn append_session_event(&self, input: &HarnessEventInput) -> Result<HarnessEvent> {
        let pool = self.workspace_pool(&input.workspace_id).await?;
        if let Some(row) = sqlx::query_as::<_, SessionEventRow>(EVENT_BY_ID_SQL)
            .bind(&input.event_id)
            .fetch_optional(&pool)
            .await?
        {
            if row.payload_digest != input.payload_digest {
                return Err(DbError::Conflict {
                    message: format!("event {} already exists with another digest", input.event_id),
                });
            }
            return row.into_event(&input.workspace_id);
        }

        validate_schema(input.schema_v)?;
        validate_payload(&input.event_type, &input.payload_json)?;
        let mut tx = pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO session_events (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(event_id) DO NOTHING",
        )
        .bind(&input.event_id)
        .bind(&input.session_id)
        .bind(input.execution_mode.as_str())
        .bind(&input.run_id)
        .bind(&input.task_id)
        .bind(&input.event_type)
        .bind(input.occurred_at.to_rfc3339())
        .bind(input.cause_id)
        .bind(input.schema_v)
        .bind(&input.payload_digest)
        .bind(serde_json::to_string(&input.payload_json)?)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 1 {
            let event = HarnessEvent {
                seq: inserted.last_insert_rowid(),
                event_id: input.event_id.clone(),
                workspace_id: input.workspace_id.clone(),
                session_id: input.session_id.clone(),
                execution_mode: input.execution_mode,
                run_id: input.run_id.clone(),
                task_id: input.task_id.clone(),
                event_type: input.event_type.clone(),
                occurred_at: input.occurred_at,
                cause_id: input.cause_id,
                schema_v: input.schema_v,
                payload_digest: input.payload_digest.clone(),
                payload_json: input.payload_json.clone(),
            };
            tx.commit().await?;
            return Ok(event);
        }

        let row = sqlx::query_as::<_, SessionEventRow>(EVENT_BY_ID_SQL)
            .bind(&input.event_id)
            .fetch_one(&mut *tx)
            .await?;
        if row.payload_digest != input.payload_digest {
            return Err(DbError::Conflict {
                message: format!("event {} already exists with another digest", input.event_id),
            });
        }
        let event = row.into_event(&input.workspace_id)?;
        tx.commit().await?;
        Ok(event)
    }

    /// Startup/crash recovery for one Harness run. The run status fact and its
    /// `run.interrupted` event commit together; the event names the last
    /// persisted run seq so a replay never presents a lost tail as complete.
    pub async fn interrupt_harness_run(
        &self,
        workspace_id: &str,
        task_id: &str,
        run_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> Result<Option<HarnessEventInput>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        // The run's own execution mode decides the event mode. Recovery must
        // never relabel an isolated simulation run as production history.
        let execution_mode = sqlx::query_scalar::<_, String>(
            "SELECT execution_mode FROM agent_runs
             WHERE id = ? AND task_id = ? AND state IN ('running', 'model_calling')",
        )
        .bind(run_id)
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(execution_mode) = execution_mode else {
            tx.commit().await?;
            return Ok(None);
        };
        let updated = sqlx::query(
            "UPDATE agent_runs SET state = 'interrupted'
             WHERE id = ? AND task_id = ? AND state IN ('running', 'model_calling')",
        )
        .bind(run_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        // The Task projection must not stay "running" forever after a crash.
        // Only an in-flight Task changes; paused/terminal facts are retained.
        sqlx::query(
            "UPDATE agent_tasks SET state = 'interrupted', state_version = state_version + 1
             WHERE id = ? AND state IN ('running', 'model_calling')",
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await?;
        let last_persisted_seq =
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE run_id = ?")
                .bind(run_id)
                .fetch_one(&mut *tx)
                .await?;
        let payload_json = serde_json::json!({
            "taskId": task_id,
            "runId": run_id,
            "state": "interrupted",
            "incomplete_tail": true,
            "last_persisted_seq": last_persisted_seq,
        });
        let payload = serde_json::to_string(&payload_json)?;
        let input = HarnessEventInput {
            event_id: format!("run:{run_id}:interrupted"),
            workspace_id: workspace_id.to_string(),
            session_id: format!("task:{task_id}"),
            execution_mode: if execution_mode == "simulation" {
                ExecutionMode::Simulation
            } else {
                ExecutionMode::Production
            },
            run_id: Some(run_id.to_string()),
            task_id: Some(task_id.to_string()),
            event_type: "run.interrupted".to_string(),
            occurred_at,
            cause_id: None,
            schema_v: CURRENT_EVENT_SCHEMA,
            payload_digest: format!("{:x}", sha2::Sha256::digest(payload.as_bytes())),
            payload_json,
        };
        crate::harness_effect::insert_event(&mut tx, &input).await?;
        tx.commit().await?;
        Ok(Some(input))
    }

    /// Startup crash recovery for every in-flight Harness run in one
    /// workspace store. A workspace without a Harness store is skipped so
    /// startup reconciliation never creates empty stores.
    pub async fn recover_interrupted_harness_runs(
        &self,
        workspace_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> Result<Vec<HarnessEventInput>> {
        crate::harness_workspace::validate_workspace_id(workspace_id)?;
        let path = self
            .workspace_store_root
            .join(workspace_id)
            .join("harness")
            .join("harness.db");
        let pending_restore = self.restore_journal_row(workspace_id).await?.is_some();
        if !pending_restore && !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vec::new());
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let runs = sqlx::query_as::<_, (String, String)>(
            "SELECT id, task_id FROM agent_runs
             WHERE state IN ('running', 'model_calling')
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await?;
        let mut events = Vec::new();
        for (run_id, task_id) in runs {
            if let Some(event) = self
                .interrupt_harness_run(workspace_id, &task_id, &run_id, occurred_at)
                .await?
            {
                events.push(event);
            }
        }
        Ok(events)
    }

    /// Fetch one retained event by id. Used by the brain feed's
    /// `session:<event_id>` detail path.
    pub async fn session_event_by_id(&self, workspace_id: &str, event_id: &str) -> Result<Option<HarnessEvent>> {
        let path = self
            .workspace_store_root
            .join(workspace_id)
            .join("harness")
            .join("harness.db");
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(None);
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let row = sqlx::query_as::<_, SessionEventRow>(EVENT_BY_ID_SQL)
            .bind(event_id)
            .fetch_optional(&pool)
            .await?;
        row.map(|row| row.into_event(workspace_id)).transpose()
    }

    /// Page of recent Production events strictly older than a merged-feed
    /// cursor. The cursor id is the brain-feed id (`session:<event_id>`), so
    /// the comparison applies the same prefix the feed merges under.
    pub async fn recent_session_events_before(
        &self,
        workspace_id: &str,
        cursor_created_at: &str,
        cursor_id: &str,
        limit: i64,
    ) -> Result<Vec<HarnessEvent>> {
        let limit = if limit <= 0 { 20 } else { limit.min(200) };
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, SessionEventRow>(
            "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, \
             cause_id, schema_v, payload_digest, payload_json \
             FROM session_events WHERE execution_mode = ? \
               AND (occurred_at < ? OR (occurred_at = ? AND ('session:' || event_id) < ?)) \
             ORDER BY occurred_at DESC, ('session:' || event_id) DESC LIMIT ?",
        )
        .bind(ExecutionMode::Production.as_str())
        .bind(cursor_created_at)
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(|row| row.into_event(workspace_id)).collect()
    }

    /// 最近事件（跨会话，Production），按发生时间倒序。
    /// 只用于 HUD/概览；会话回放仍走 `session_event_page`。
    pub async fn recent_session_events(&self, workspace_id: &str, limit: i64) -> Result<Vec<HarnessEvent>> {
        let limit = if limit <= 0 { 20 } else { limit.min(200) };
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, SessionEventRow>(
            "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, \
             cause_id, schema_v, payload_digest, payload_json \
             FROM session_events WHERE execution_mode = ? \
             ORDER BY occurred_at DESC, ('session:' || event_id) DESC LIMIT ?",
        )
        .bind(ExecutionMode::Production.as_str())
        .bind(limit)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(|row| row.into_event(workspace_id)).collect()
    }

    pub async fn projection_tail(
        &self,
        subject: &crate::harness_snapshot::ProjectionSubject,
        after: i64,
        limit: i64,
    ) -> Result<EventPage> {
        self.session_event_page(
            &subject.workspace_id,
            &EventFilter {
                session_id: subject.session_id.clone(),
                run_id: subject.run_id.clone(),
                task_id: subject.task_id.clone(),
                execution_mode: subject.execution_mode,
            },
            after,
            limit,
        )
        .await
    }

    pub async fn session_event_page(
        &self,
        workspace_id: &str,
        filter: &EventFilter,
        after: i64,
        limit: i64,
    ) -> Result<EventPage> {
        let limit = if limit <= 0 {
            DEFAULT_EVENT_PAGE_SIZE
        } else {
            limit.min(DEFAULT_EVENT_PAGE_SIZE)
        };
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let rows = if let Some(session_id) = &filter.session_id {
            sqlx::query_as::<_, SessionEventRow>(SESSION_EVENT_PAGE_SQL.session)
                .bind(filter.execution_mode.as_str())
                .bind(session_id)
                .bind(&filter.run_id)
                .bind(&filter.run_id)
                .bind(&filter.task_id)
                .bind(&filter.task_id)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await?
        } else if let Some(run_id) = &filter.run_id {
            sqlx::query_as::<_, SessionEventRow>(SESSION_EVENT_PAGE_SQL.run)
                .bind(filter.execution_mode.as_str())
                .bind(run_id)
                .bind(&filter.task_id)
                .bind(&filter.task_id)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await?
        } else if let Some(task_id) = &filter.task_id {
            sqlx::query_as::<_, SessionEventRow>(SESSION_EVENT_PAGE_SQL.task)
                .bind(filter.execution_mode.as_str())
                .bind(task_id)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await?
        } else {
            sqlx::query_as::<_, SessionEventRow>(MODE_EVENT_PAGE_SQL)
                .bind(filter.execution_mode.as_str())
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await?
        };
        let high_watermark = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq), 0) FROM session_events")
            .fetch_one(&mut *tx)
            .await?;
        let events = rows
            .into_iter()
            .map(|row| row.into_event(workspace_id))
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(EventPage { events, high_watermark })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};
    use tinyiothub_core::harness::{DeviceFactsSnapshot, ExecutionMode, HarnessEventInput};

    use super::{EventFilter, SESSION_EVENT_PAGE_SQL};
    use crate::{Db, DbError};

    struct HarnessEventFixture {
        root: PathBuf,
        db: Db,
    }

    impl HarnessEventFixture {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-event-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-events', 'workspace', 'tenant-default-001', 'now', 'now')",
            )
            .execute(db.pool())
            .await
            .expect("insert workspace");
            db.create_workspace_store("ws-events")
                .await
                .expect("create workspace store");
            Self { root, db }
        }

        async fn workspace_pool(&self) -> sqlx::SqlitePool {
            self.db.workspace_pool("ws-events").await.expect("workspace pool")
        }

        async fn event_count(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
                .fetch_one(&self.workspace_pool().await)
                .await
                .expect("event count")
        }

        async fn close(self) {
            self.db
                .close_workspace_store("ws-events")
                .await
                .expect("close workspace store");
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn occurred_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn input(event_id: &str, execution_mode: ExecutionMode) -> HarnessEventInput {
        HarnessEventInput {
            event_id: event_id.to_string(),
            workspace_id: "ws-events".to_string(),
            session_id: "session-1".to_string(),
            execution_mode,
            run_id: Some("run-1".to_string()),
            task_id: Some("task-1".to_string()),
            event_type: "step.tool_result".to_string(),
            occurred_at: occurred_at(),
            cause_id: None,
            schema_v: 1,
            payload_digest: "digest-1".to_string(),
            payload_json: json!({"result": {"ok": true}}),
        }
    }

    fn filter(execution_mode: ExecutionMode) -> EventFilter {
        EventFilter {
            session_id: Some("session-1".to_string()),
            run_id: None,
            task_id: None,
            execution_mode,
        }
    }

    #[tokio::test]
    async fn harness_event_identity_dedupes_and_conflicts() {
        let fixture = HarnessEventFixture::new().await;
        let original = fixture
            .db
            .append_session_event(&input("event-1", ExecutionMode::Production))
            .await
            .expect("append event");
        assert_eq!(original.seq, 1);
        assert_eq!(original.workspace_id, "ws-events");
        assert_eq!(original.event_type, "step.tool_result");

        let mut retry = input("event-1", ExecutionMode::Production);
        retry.payload_json = json!({"media": {"bytes": "retry must not be validated or stored"}});
        let deduped = fixture
            .db
            .append_session_event(&retry)
            .await
            .expect("same ID and digest returns original");
        assert_eq!(deduped, original);

        let mut conflicting = input("event-1", ExecutionMode::Production);
        conflicting.payload_digest = "digest-2".to_string();
        let error = fixture
            .db
            .append_session_event(&conflicting)
            .await
            .expect_err("same ID with another digest must conflict");
        assert!(matches!(error, DbError::Conflict { .. }));
        assert_eq!(fixture.event_count().await, 1);

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_event_production_pages_exclude_simulation_and_return_high_watermark() {
        let fixture = HarnessEventFixture::new().await;
        let production = fixture
            .db
            .append_session_event(&input("event-production", ExecutionMode::Production))
            .await
            .expect("production event");
        fixture
            .db
            .append_session_event(&input("event-simulation", ExecutionMode::Simulation))
            .await
            .expect("simulation event");
        let mut other_session = input("event-other", ExecutionMode::Production);
        other_session.session_id = "session-2".to_string();
        let latest = fixture
            .db
            .append_session_event(&other_session)
            .await
            .expect("other session event");

        let page = fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Production), 0, 10)
            .await
            .expect("production page");
        assert_eq!(page.events, vec![production.clone()]);
        assert_eq!(page.high_watermark, latest.seq);

        let empty = fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Production), production.seq, 10)
            .await
            .expect("filtered gap is an empty page, not a lost frame");
        assert!(empty.events.is_empty());
        assert_eq!(empty.high_watermark, latest.seq);

        let simulation = fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Simulation), 0, 10)
            .await
            .expect("simulation remains separately queryable");
        assert_eq!(simulation.events.len(), 1);
        assert_eq!(simulation.events[0].event_id, "event-simulation");

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_event_replays_stamped_device_facts_without_platform_catalog() {
        let fixture = HarnessEventFixture::new().await;
        let snapshot = DeviceFactsSnapshot {
            observed_at: occurred_at(),
            declaration_hash: "declaration-v1".to_string(),
            driver_version: "driver-v1".to_string(),
            facts: json!({"device-1": {"temperature": 21.5}}),
        };
        let mut stamped = input("event-facts", ExecutionMode::Production);
        stamped.event_type = "run.started".to_string();
        stamped.payload_json = json!({"deviceFacts": snapshot});

        sqlx::query("DROP TABLE things")
            .execute(fixture.db.pool())
            .await
            .expect("remove current platform catalog");
        fixture
            .db
            .append_session_event(&stamped)
            .await
            .expect("commit must not read the platform catalog");
        let page = fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Production), 0, 10)
            .await
            .expect("replay must not read the platform catalog");
        let replayed: DeviceFactsSnapshot =
            serde_json::from_value(page.events[0].payload_json["deviceFacts"].clone()).expect("stamped device facts");
        assert_eq!(replayed, snapshot);
        assert_eq!(replayed.facts["device-1"]["temperature"], 21.5);

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_event_rejects_unknown_schema_empty_segments_and_media_payloads() {
        let fixture = HarnessEventFixture::new().await;
        let mut unknown = input("event-unknown", ExecutionMode::Production);
        unknown.schema_v = 2;
        fixture
            .db
            .append_session_event(&unknown)
            .await
            .expect_err("unknown event schema must be rejected");

        for (event_id, event_type, payload) in [
            ("event-thinking", "step.thinking", json!({"thinking": "  "})),
            ("event-text", "step.text", json!({"text": ""})),
            (
                "event-media-bytes",
                "step.tool_result",
                json!({"media": {"bytes": "AAEC"}}),
            ),
            (
                "event-media-base64",
                "step.tool_result",
                json!({"media": {"base64": "AAEC"}}),
            ),
            (
                "event-media-data",
                "step.tool_result",
                json!({"media": {"data": "AAEC"}}),
            ),
            (
                "event-media-data-url",
                "step.tool_result",
                json!({"media": {"url": "data:image/png;base64,AAEC"}}),
            ),
            (
                "event-media-uppercase-data-url",
                "step.tool_result",
                json!({"media": {"url": "DATA:image/png;base64,AAEC"}}),
            ),
            (
                "event-media-mixed-case-data-url",
                "step.tool_result",
                json!({"evidence": {"uri": "DaTa:video/mp4;base64,AAEC"}}),
            ),
            (
                "event-signed-url",
                "step.tool_result",
                json!({"media": {"signedUrl": "https://example.invalid/evidence?signature=secret"}}),
            ),
        ] {
            let mut invalid = input(event_id, ExecutionMode::Production);
            invalid.event_type = event_type.to_string();
            invalid.payload_json = payload;
            fixture
                .db
                .append_session_event(&invalid)
                .await
                .expect_err("invalid event payload must not be persisted");
        }
        assert_eq!(fixture.event_count().await, 0);

        sqlx::query(
            "INSERT INTO session_events (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json) VALUES ('event-future', 'session-1', 'production', 'run-1', 'task-1', 'run.started', '2026-11-03T00:00:00Z', NULL, 99, 'digest', '{}')",
        )
        .execute(&fixture.workspace_pool().await)
        .await
        .expect("insert future schema fixture");
        fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Production), 0, 10)
            .await
            .expect_err("read path must reject unknown schema instead of downgrading");

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_event_schema_and_composite_indexes_match_read_paths() {
        let fixture = HarnessEventFixture::new().await;
        let pool = fixture.workspace_pool().await;
        let columns =
            sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('session_events') ORDER BY cid")
                .fetch_all(&pool)
                .await
                .expect("event columns");
        assert_eq!(
            columns,
            [
                "seq",
                "event_id",
                "session_id",
                "execution_mode",
                "run_id",
                "task_id",
                "type",
                "occurred_at",
                "cause_id",
                "schema_v",
                "payload_digest",
                "payload_json",
            ]
        );

        for (sql, index, optional_filters) in [
            (SESSION_EVENT_PAGE_SQL.session, "idx_session_events_mode_session_seq", 2),
            (SESSION_EVENT_PAGE_SQL.run, "idx_session_events_mode_run_seq", 1),
            (SESSION_EVENT_PAGE_SQL.task, "idx_session_events_mode_task_seq", 0),
        ] {
            let query =
                sqlx::query_as::<_, (i64, i64, i64, String)>(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
                    .bind("production")
                    .bind("subject-1");
            let query = match optional_filters {
                2 => query
                    .bind(None::<String>)
                    .bind(None::<String>)
                    .bind(None::<String>)
                    .bind(None::<String>),
                1 => query.bind(None::<String>).bind(None::<String>),
                _ => query,
            };
            let plan = query
                .bind(0_i64)
                .bind(256_i64)
                .fetch_all(&pool)
                .await
                .expect("event query plan")
                .into_iter()
                .map(|(_, _, _, detail)| detail)
                .collect::<Vec<_>>();
            assert!(
                plan.iter().any(|detail| detail.contains(index)),
                "expected {index} in {plan:?} for {sql}"
            );
        }

        let platform_has_events: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_events')",
        )
        .fetch_one(fixture.db.pool())
        .await
        .expect("platform schema");
        assert!(!platform_has_events, "Harness events belong only in workspace DBs");
        let platform_versions = crate::migrations::platform_migrator()
            .iter()
            .map(|migration| migration.version)
            .collect::<Vec<_>>();
        assert!(platform_versions.contains(&20261103000001));
        assert!(!platform_versions.contains(&20261103000002));

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_event_pages_are_bounded_and_resume_by_sequence() {
        let fixture = HarnessEventFixture::new().await;
        for number in 0..257 {
            let mut event = input(&format!("event-{number}"), ExecutionMode::Production);
            event.payload_digest = format!("digest-{number}");
            fixture
                .db
                .append_session_event(&event)
                .await
                .expect("append page fixture");
        }

        let first = fixture
            .db
            .session_event_page("ws-events", &filter(ExecutionMode::Production), 0, 0)
            .await
            .expect("default page size");
        assert_eq!(first.events.len(), 256);
        assert_eq!(first.high_watermark, 257);
        let second = fixture
            .db
            .session_event_page(
                "ws-events",
                &filter(ExecutionMode::Production),
                first.events.last().expect("last event").seq,
                1000,
            )
            .await
            .expect("next page");
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].seq, 257);
        assert_eq!(second.high_watermark, 257);

        fixture.close().await;
    }

    #[tokio::test]
    async fn recent_session_events_returns_newest_first() {
        use crate::test_helpers::fixture_workspace;
        use chrono::TimeZone;
        use serde_json::json;
        use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

        let fixture = fixture_workspace("ws-recent").await;
        for (index, text) in ["first", "second", "third"].iter().enumerate() {
            let occurred_at = Utc.timestamp_opt(1_800_000_000 + index as i64, 0).single().unwrap();
            fixture
                .db
                .append_session_event(&HarnessEventInput {
                    event_id: format!("recent-{index}"),
                    workspace_id: "ws-recent".to_string(),
                    session_id: "agent:ws-recent:a/s1".to_string(),
                    execution_mode: ExecutionMode::Production,
                    run_id: None,
                    task_id: None,
                    event_type: "message.created".to_string(),
                    occurred_at,
                    cause_id: None,
                    schema_v: 1,
                    payload_digest: format!("digest-{index}"),
                    payload_json: json!({ "text": text }),
                })
                .await
                .unwrap();
        }
        let events = fixture.db.recent_session_events("ws-recent", 2).await.unwrap();
        let payloads: Vec<&str> = events
            .iter()
            .map(|event| event.payload_json["text"].as_str().unwrap())
            .collect();
        assert_eq!(payloads, vec!["third", "second"]);
    }

    #[test]
    fn harness_event_device_facts_snapshot_has_json_value_facts() {
        let snapshot = DeviceFactsSnapshot {
            observed_at: occurred_at(),
            declaration_hash: "hash".to_string(),
            driver_version: "1.0.0".to_string(),
            facts: Value::Null,
        };
        assert_eq!(serde_json::to_value(snapshot).expect("snapshot")["facts"], Value::Null);
    }
}
