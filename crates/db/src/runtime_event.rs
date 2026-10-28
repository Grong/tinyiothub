use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::agent_task::{AgentTask, WakeDispatchState};
use tinyiothub_core::device_job::DeviceJob;
use tinyiothub_core::media::EvidenceRef;

use crate::Db;
use crate::error::{DbError, Result};

pub const RUNTIME_EVENT_KIND: &str = "runtime.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub cursor: i64,
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
    pub approval_id: Option<String>,
    pub device_id: Option<String>,
    pub source_id: Option<String>,
    pub source_seq: Option<i64>,
    pub evidence_id: Option<String>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventInput {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
    pub approval_id: Option<String>,
    pub device_id: Option<String>,
    pub source_id: Option<String>,
    pub source_seq: Option<i64>,
    pub evidence_id: Option<String>,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunProjection {
    pub id: String,
    pub status: String,
    pub outcome: String,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskWatchProjection {
    pub id: String,
    pub condition: Value,
    pub persistent: bool,
    pub timeout_secs: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProjection {
    pub task: AgentTask,
    pub runs: Vec<TaskRunProjection>,
    pub jobs: Vec<DeviceJob>,
    pub watches: Vec<TaskWatchProjection>,
    pub evidence: Vec<EvidenceRef>,
    pub events: Vec<RuntimeEvent>,
    pub summary: Option<String>,
    pub cursor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWake {
    pub workspace_id: String,
    pub task_id: String,
    pub event_id: String,
    pub signal_json: Value,
    pub dispatch_state: WakeDispatchState,
    pub created_at: DateTime<Utc>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RuntimeLedgerRow {
    id: i64,
    workspace_id: String,
    ts: String,
    payload: String,
}

fn internal(error: impl std::fmt::Display) -> DbError {
    DbError::Internal(error.to_string())
}

fn decode_event(row: RuntimeLedgerRow) -> Result<RuntimeEvent> {
    let mut payload: Value = serde_json::from_str(&row.payload)?;
    if payload.get("v").and_then(Value::as_i64) != Some(1) {
        return Err(DbError::Validation {
            message: format!("runtime ledger row {} has no v=1 envelope", row.id),
        });
    }
    let object = payload.as_object_mut().ok_or_else(|| DbError::Validation {
        message: format!("runtime ledger row {} is not an object", row.id),
    })?;
    object.remove("v");
    object.insert("cursor".into(), Value::from(row.id));
    object.insert("occurredAt".into(), Value::from(row.ts));
    object.insert("workspaceId".into(), Value::from(row.workspace_id));
    let event: RuntimeEvent = serde_json::from_value(payload)?;
    if event.cursor != row.id {
        return Err(DbError::Validation {
            message: "runtime cursor does not match ledger identity".into(),
        });
    }
    Ok(event)
}

impl Db {
    pub async fn append_runtime_event(&self, input: RuntimeEventInput) -> Result<RuntimeEvent> {
        if input.event_id.trim().is_empty()
            || input.event_type.trim().is_empty()
            || input.workspace_id.trim().is_empty()
        {
            return Err(DbError::Validation {
                message: "runtime event id, type, and workspace are required".into(),
            });
        }
        let event = RuntimeEvent {
            cursor: 0,
            event_id: input.event_id,
            event_type: input.event_type,
            occurred_at: input.occurred_at,
            workspace_id: input.workspace_id,
            task_id: input.task_id,
            run_id: input.run_id,
            tool_call_id: input.tool_call_id,
            job_id: input.job_id,
            approval_id: input.approval_id,
            device_id: input.device_id,
            source_id: input.source_id,
            source_seq: input.source_seq,
            evidence_id: input.evidence_id,
            data: input.data,
        };
        let mut payload = serde_json::to_value(&event)?;
        let object = payload
            .as_object_mut()
            .ok_or_else(|| DbError::Internal("event was not an object".into()))?;
        object.remove("cursor");
        object.insert("v".into(), Value::from(1));
        let payload = serde_json::to_string(&payload)?;
        let mut tx = self.pool().begin().await?;
        let inserted = sqlx::query_as::<_, RuntimeLedgerRow>(
            "INSERT INTO hub_ledger (workspace_id, ts, kind, thing_id, payload)
             SELECT ?, ?, ?, ?, ?
             WHERE NOT EXISTS (
                 SELECT 1 FROM hub_ledger
                 WHERE workspace_id = ? AND kind = ?
                   AND json_extract(payload, '$.eventId') = ?
             )
             RETURNING id, workspace_id, ts, payload",
        )
        .bind(&event.workspace_id)
        .bind(event.occurred_at.to_rfc3339())
        .bind(RUNTIME_EVENT_KIND)
        .bind(&event.device_id)
        .bind(&payload)
        .bind(&event.workspace_id)
        .bind(RUNTIME_EVENT_KIND)
        .bind(&event.event_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = match inserted {
            Some(row) => row,
            None => {
                sqlx::query_as::<_, RuntimeLedgerRow>(
                    "SELECT id, workspace_id, ts, payload FROM hub_ledger
                 WHERE workspace_id = ? AND kind = ? AND json_extract(payload, '$.eventId') = ?
                 ORDER BY id LIMIT 1",
                )
                .bind(&event.workspace_id)
                .bind(RUNTIME_EVENT_KIND)
                .bind(&event.event_id)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        decode_event(row)
    }

    pub async fn runtime_events_after(
        &self,
        workspace_id: &str,
        after: Option<i64>,
        task_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<RuntimeEvent>> {
        let rows = sqlx::query_as::<_, RuntimeLedgerRow>(
            "SELECT id, workspace_id, ts, payload FROM hub_ledger
             WHERE workspace_id = ? AND kind = ? AND id > COALESCE(?, 0)
               AND (? IS NULL OR json_extract(payload, '$.taskId') = ?)
             ORDER BY id ASC LIMIT ?",
        )
        .bind(workspace_id)
        .bind(RUNTIME_EVENT_KIND)
        .bind(after)
        .bind(task_id)
        .bind(task_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(decode_event).collect()
    }

    pub async fn runtime_cursor_known(&self, workspace_id: &str, cursor: i64) -> Result<bool> {
        if cursor == 0 {
            return Ok(true);
        }
        if cursor < 0 {
            return Ok(false);
        }
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM hub_ledger WHERE workspace_id = ? AND id = ?)")
            .bind(workspace_id)
            .bind(cursor)
            .fetch_one(self.pool())
            .await
            .map_err(Into::into)
    }

    pub async fn runtime_projection(&self, workspace_id: &str, task_id: &str) -> Result<Option<TaskProjection>> {
        let Some(task) = self.get_task(workspace_id, task_id).await.map_err(internal)? else {
            return Ok(None);
        };
        let runs = sqlx::query_as::<_, TaskRunProjection>(
            "SELECT id, status, outcome, summary, created_at FROM agent_runs
             WHERE workspace_id = ? AND task_id = ? ORDER BY created_at, rowid",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(self.pool())
        .await?;
        let jobs = sqlx::query_as::<_, crate::device_job::DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                    timeout_ms, started_at, ended_at
             FROM device_jobs WHERE workspace_id = ? AND task_id = ? ORDER BY started_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(DeviceJob::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(internal)?;
        let watch_rows = sqlx::query_as::<_, crate::watch::WatchRuleRow>(
            "SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id
             FROM watch_rules WHERE workspace_id = ? AND owner_task_id = ? ORDER BY created_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(self.pool())
        .await?;
        let watches = watch_rows
            .into_iter()
            .map(|row| {
                Ok(TaskWatchProjection {
                    id: row.id,
                    condition: serde_json::from_str(&row.condition)?,
                    persistent: row.persistent != 0,
                    timeout_secs: row.timeout_secs,
                    created_at: row.created_at,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let evidence_rows = sqlx::query_as::<_, (String, String, i64, DateTime<Utc>)>(
            "SELECT id, mime, source_seq, media_time FROM media_evidence
             WHERE workspace_id = ? AND task_id = ? ORDER BY created_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(self.pool())
        .await?;
        let evidence = evidence_rows
            .into_iter()
            .map(|(evidence_id, mime, source_seq, media_time)| {
                Ok(EvidenceRef {
                    evidence_id,
                    mime,
                    source_seq: u64::try_from(source_seq).map_err(internal)?,
                    media_time,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let events = self
            .runtime_events_after(workspace_id, None, Some(task_id), i64::MAX)
            .await?;
        let summary = runs
            .iter()
            .rev()
            .find(|run| run.status == "completed")
            .map(|run| run.summary.clone());
        let cursor = crate::watch::max_ledger_id(self.pool(), workspace_id).await?;
        Ok(Some(TaskProjection {
            task,
            runs,
            jobs,
            watches,
            evidence,
            events,
            summary,
            cursor,
        }))
    }

    pub async fn runtime_outbox(
        &self,
        workspace_id: Option<&str>,
        state: WakeDispatchState,
        limit: i64,
    ) -> Result<Vec<AdminWake>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                DateTime<Utc>,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            "SELECT t.workspace_id, w.task_id, w.event_id, w.signal_json, w.created_at, w.dispatched_at, w.resolved_at
             FROM task_wake_outbox w JOIN agent_tasks t ON t.id = w.task_id
             WHERE w.dispatch_state = ? AND (? IS NULL OR t.workspace_id = ?)
             ORDER BY w.created_at, w.task_id, w.event_id LIMIT ?",
        )
        .bind(state.as_str())
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(
                |(workspace_id, task_id, event_id, signal_json, created_at, dispatched_at, resolved_at)| {
                    Ok(AdminWake {
                        workspace_id,
                        task_id,
                        event_id,
                        signal_json: serde_json::from_str(&signal_json)?,
                        dispatch_state: state,
                        created_at,
                        dispatched_at,
                        resolved_at,
                    })
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};
    use tinyiothub_core::media::MediaEvidenceRecord;

    use crate::Db;
    use crate::runtime_event::RuntimeEventInput;

    async fn seed_workspace(pool: &SqlitePool, workspace_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(pool)
        .await
        .expect("seed plan");
        sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(pool)
            .await
            .expect("seed tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 't1', 'now', 'now')",
        )
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    }

    async fn seed_task(db: &Db, workspace_id: &str, task_id: &str) {
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: task_id.into(),
                workspace_id: workspace_id.into(),
                idempotency_key: format!("key-{task_id}"),
                objective: "inspect pump".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: task_id.into(),
                event_id: format!("create-{task_id}"),
                signal_json: json!({"workspace_id":workspace_id}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("seed task");
    }

    fn event(event_id: &str, task_id: &str) -> RuntimeEventInput {
        RuntimeEventInput {
            event_id: event_id.into(),
            event_type: "task.updated".into(),
            occurred_at: Utc::now(),
            workspace_id: "ws-1".into(),
            task_id: Some(task_id.into()),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: json!({"stateVersion":1}),
        }
    }

    #[tokio::test]
    async fn runtime_events_replay_by_cursor_with_stable_ids_and_task_scope() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        seed_workspace(&pool, "ws-2").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        seed_task(&db, "ws-1", "task-2").await;

        let first = db
            .append_runtime_event(event("event-1", "task-1"))
            .await
            .expect("first");
        let second = db
            .append_runtime_event(event("event-2", "task-1"))
            .await
            .expect("second");
        db.append_runtime_event(event("event-3", "task-2"))
            .await
            .expect("other task");
        let replayed = db
            .append_runtime_event(event("event-2", "task-1"))
            .await
            .expect("dedupe retry");
        assert_eq!(replayed.cursor, second.cursor);
        crate::watch::append_ledger(db.pool(), "ws-1", &Utc::now().to_rfc3339(), "state_change", None, "{}")
            .await
            .expect("non-runtime ledger row");

        let events = db
            .runtime_events_after("ws-1", Some(first.cursor), Some("task-1"), 50)
            .await
            .expect("replay");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "event-2");
        assert_eq!(events[0].cursor, second.cursor);
        assert_eq!(events[0].event_type, "task.updated");
        assert_eq!(events[0].workspace_id, "ws-1");
        assert_eq!(events[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(events[0].data["stateVersion"], 1);
        assert!(
            db.runtime_cursor_known("ws-1", second.cursor)
                .await
                .expect("known cursor")
        );
        assert!(
            !db.runtime_cursor_known("ws-2", second.cursor)
                .await
                .expect("foreign cursor")
        );
        assert!(
            !db.runtime_cursor_known("ws-1", second.cursor + 100)
                .await
                .expect("future cursor")
        );
    }

    #[tokio::test]
    async fn projection_combines_ordered_runtime_facts_without_media_bytes() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        let task = db.get_task("ws-1", "task-1").await.expect("get").expect("task");
        assert!(db.begin_run(&task, "create-task-1", "run-1").await.expect("run"));
        db.finish_run(&tinyiothub_core::agent_runs::RunReport {
            run_id: "run-1".into(),
            workspace_id: "ws-1".into(),
            trigger: "task:create-task-1".into(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "pump is healthy".into(),
            actions: vec![],
            verified: true,
            duration_ms: 10,
            tool_calls: 0,
            tokens: 2,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some("task-1".into()),
            wake_event_id: Some("create-task-1".into()),
        })
        .await
        .expect("finish run");
        db.create_job(&DeviceJob {
            id: "job-1".into(),
            workspace_id: "ws-1".into(),
            device_id: "device-1".into(),
            tool: "inspect".into(),
            args_json: json!({}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some("task-1".into()),
            run_id: Some("run-1".into()),
            tool_call_id: Some("call-1".into()),
            driver_call_id: None,
            timeout_ms: 1000,
            started_at: Utc::now(),
            ended_at: None,
        })
        .await
        .expect("job");
        crate::watch::insert_rule(
            db.pool(),
            &crate::watch::WatchRuleRow {
                id: "watch-1".into(),
                workspace_id: "ws-1".into(),
                kind: "availability".into(),
                condition: json!({"type":"availability","thing_id":"device-1","online":true}).to_string(),
                persistent: 0,
                timeout_secs: 600,
                created_at: Utc::now().to_rfc3339(),
                owner_task_id: Some("task-1".into()),
            },
        )
        .await
        .expect("watch");
        let now = Utc::now();
        db.put_media_evidence(&MediaEvidenceRecord {
            id: "evidence-1".into(),
            workspace_id: "ws-1".into(),
            task_id: Some("task-1".into()),
            run_id: Some("run-1".into()),
            tool_call_id: Some("call-1".into()),
            job_id: Some("job-1".into()),
            device_id: "device-1".into(),
            source_id: "camera".into(),
            source_seq: 4,
            media_time: now,
            mime: "image/png".into(),
            size_bytes: 12,
            sha256: "abc".into(),
            storage_path: "private/path".into(),
            dedup_key: "dedup-1".into(),
            created_at: now,
            expires_at: now + chrono::Duration::days(1),
        })
        .await
        .expect("evidence");
        let appended = db
            .append_runtime_event(event("event-1", "task-1"))
            .await
            .expect("event");

        let projection = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task");
        assert_eq!(projection.task.id, "task-1");
        assert_eq!(projection.runs.len(), 1);
        assert_eq!(projection.runs[0].id, "run-1");
        assert_eq!(projection.jobs.len(), 1);
        assert_eq!(projection.watches.len(), 1);
        assert_eq!(projection.evidence.len(), 1);
        assert_eq!(projection.evidence[0].evidence_id, "evidence-1");
        assert_eq!(projection.events.len(), 1);
        assert_eq!(projection.summary.as_deref(), Some("pump is healthy"));
        assert_eq!(projection.cursor, appended.cursor);
        let serialized = serde_json::to_value(&projection).expect("serialize");
        assert!(serialized.get("storagePath").is_none());
        assert!(serialized["evidence"][0].get("storagePath").is_none());
        assert!(
            db.runtime_projection("ws-1", "missing")
                .await
                .expect("missing")
                .is_none()
        );
    }

    #[tokio::test]
    async fn dead_outbox_retry_preserves_original_id_and_never_replaces_rows() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        seed_workspace(&pool, "ws-2").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        db.mark_wake("task-1", "create-task-1", WakeDispatchState::Dead)
            .await
            .expect("dead");

        let listed = db
            .runtime_outbox(Some("ws-1"), WakeDispatchState::Dead, 50)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].event_id, "create-task-1");
        assert!(
            !db.retry_dead_wake("ws-2", "create-task-1")
                .await
                .expect("foreign retry")
        );
        assert!(db.retry_dead_wake("ws-1", "create-task-1").await.expect("retry"));
        assert!(
            !db.retry_dead_wake("ws-1", "create-task-1")
                .await
                .expect("repeat is current state")
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("count");
        assert_eq!(count, 1);
        let wake = db
            .wake_by_id("task-1", "create-task-1")
            .await
            .expect("wake")
            .expect("original");
        assert_eq!(wake.event_id, "create-task-1");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
    }
}
