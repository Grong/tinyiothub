use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{SqlSafeStr, Sqlite, Transaction};
use tinyiothub_core::harness::{WorkflowOccurrence, WorkflowRevision};

use crate::{
    Db,
    error::{DbError, Result},
};

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000005,
        "harness_workflows".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000005_harness_workflows.sql").into_sql_str(),
        false,
    )
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn source_key(workflow_id: &str, trigger_id: &str, occurrence_id: &str) -> String {
    fn escape(component: &str) -> String {
        component.replace('%', "%25").replace(':', "%3A")
    }
    format!(
        "{}:{}:{}",
        escape(workflow_id),
        escape(trigger_id),
        escape(occurrence_id)
    )
}

fn digest(payload: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(payload)?)))
}

#[derive(sqlx::FromRow)]
struct WorkflowRevisionRow {
    workflow_id: String,
    version: i64,
    enabled: bool,
    definition_json: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<WorkflowRevisionRow> for WorkflowRevision {
    type Error = DbError;

    fn try_from(row: WorkflowRevisionRow) -> Result<Self> {
        Ok(Self {
            workflow_id: row.workflow_id,
            version: row.version,
            enabled: row.enabled,
            definition: serde_json::from_str(&row.definition_json)?,
            created_at: row.created_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowOccurrenceRow {
    source_key: String,
    workflow_id: String,
    trigger_id: String,
    occurrence_id: String,
    workflow_version: i64,
    task_id: String,
    run_id: String,
    session_id: String,
    source_json: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<WorkflowOccurrenceRow> for WorkflowOccurrence {
    type Error = DbError;

    fn try_from(row: WorkflowOccurrenceRow) -> Result<Self> {
        Ok(Self {
            source_key: row.source_key,
            workflow_id: row.workflow_id,
            trigger_id: row.trigger_id,
            occurrence_id: row.occurrence_id,
            workflow_version: row.workflow_version,
            task_id: row.task_id,
            run_id: row.run_id,
            session_id: row.session_id,
            source: serde_json::from_str(&row.source_json)?,
            created_at: row.created_at,
        })
    }
}

async fn revision_by_version_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workflow_id: &str,
    version: i64,
) -> Result<Option<WorkflowRevision>> {
    sqlx::query_as::<_, WorkflowRevisionRow>(
        "SELECT r.workflow_id, r.version, w.enabled, r.definition_json, r.created_at
         FROM workflow_revisions r
         JOIN agent_workflows w ON w.workflow_id = r.workflow_id
         WHERE r.workflow_id = ? AND r.version = ?",
    )
    .bind(workflow_id)
    .bind(version)
    .fetch_optional(&mut **tx)
    .await?
    .map(WorkflowRevision::try_from)
    .transpose()
}

async fn occurrence_by_source_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_key: &str,
) -> Result<Option<WorkflowOccurrence>> {
    sqlx::query_as::<_, WorkflowOccurrenceRow>(
        "SELECT source_key, workflow_id, trigger_id, occurrence_id, workflow_version,
                task_id, run_id, session_id, source_json, created_at
         FROM workflow_occurrences WHERE source_key = ?",
    )
    .bind(source_key)
    .fetch_optional(&mut **tx)
    .await?
    .map(WorkflowOccurrence::try_from)
    .transpose()
}

async fn append_workflow_event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event_id: &str,
    session_id: &str,
    task_id: Option<&str>,
    run_id: Option<&str>,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    payload: &Value,
) -> Result<()> {
    crate::harness_event::validate_payload(event_type, payload)?;
    let payload_digest = digest(payload)?;
    let inserted = sqlx::query(
        "INSERT INTO session_events
            (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at,
             cause_id, schema_v, payload_digest, payload_json)
         VALUES (?, ?, 'production', ?, ?, ?, ?, NULL, 1, ?, ?)
         ON CONFLICT(event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(session_id)
    .bind(run_id)
    .bind(task_id)
    .bind(event_type)
    .bind(occurred_at)
    .bind(&payload_digest)
    .bind(serde_json::to_string(payload)?)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        let existing = sqlx::query_scalar::<_, String>("SELECT payload_digest FROM session_events WHERE event_id = ?")
            .bind(event_id)
            .fetch_one(&mut **tx)
            .await?;
        if existing != payload_digest {
            return Err(DbError::Conflict {
                message: format!("event {event_id} already exists with another digest"),
            });
        }
    }
    Ok(())
}

async fn schedule_occurrence_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    workflow_id: &str,
    trigger_id: &str,
    occurrence_id: &str,
    source: &Value,
    now: DateTime<Utc>,
) -> Result<Option<WorkflowOccurrence>> {
    let key = source_key(workflow_id, trigger_id, occurrence_id);
    if let Some(existing) = occurrence_by_source_tx(tx, &key).await? {
        return Ok(Some(existing));
    }
    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    let run_id = format!("run-{}", uuid::Uuid::new_v4());
    let session_id = format!("task:{task_id}/run:{run_id}");
    let inserted = sqlx::query(
        "INSERT INTO workflow_occurrences
            (source_key, workflow_id, trigger_id, occurrence_id, workflow_version,
             task_id, run_id, session_id, source_json, created_at)
         SELECT ?, w.workflow_id, ?, ?, w.current_version, ?, ?, ?, ?, ?
         FROM agent_workflows w
         JOIN workflow_revisions r
           ON r.workflow_id = w.workflow_id AND r.version = w.current_version
         WHERE w.workflow_id = ? AND w.enabled = 1",
    )
    .bind(&key)
    .bind(trigger_id)
    .bind(occurrence_id)
    .bind(&task_id)
    .bind(&run_id)
    .bind(&session_id)
    .bind(serde_json::to_string(source)?)
    .bind(now)
    .bind(workflow_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return occurrence_by_source_tx(tx, &key).await;
    }

    let revision = sqlx::query_scalar::<_, i64>("SELECT current_version FROM agent_workflows WHERE workflow_id = ?")
        .bind(workflow_id)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO agent_tasks
            (id, run_id, state, state_version, workflow_id, workflow_version, parent_task_id)
         VALUES (?, ?, 'pending', 1, ?, ?, NULL)",
    )
    .bind(&task_id)
    .bind(&run_id)
    .bind(workflow_id)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO agent_runs (id, task_id, state, parent_run_id) VALUES (?, ?, 'pending', NULL)")
        .bind(&run_id)
        .bind(&task_id)
        .execute(&mut **tx)
        .await?;
    let signal = json!({
        "workspace_id": workspace_id,
        "task_id": task_id,
        "run_id": run_id,
        "workflow_id": workflow_id,
        "workflow_version": revision,
        "source_key": key,
        "source": source,
    });
    sqlx::query(
        "INSERT INTO task_wake_outbox
            (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
         VALUES (?, ?, ?, 'pending', ?, NULL, NULL)",
    )
    .bind(&task_id)
    .bind(format!("workflow:{key}"))
    .bind(serde_json::to_string(&signal)?)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    let payload = json!({
        "workflow_id": workflow_id,
        "workflow_version": revision,
        "trigger_id": trigger_id,
        "occurrence_id": occurrence_id,
        "source": source,
    });
    append_workflow_event_tx(
        tx,
        &format!("workflow:{key}:task-created"),
        &session_id,
        Some(&task_id),
        Some(&run_id),
        "task.created",
        now,
        &payload,
    )
    .await?;
    occurrence_by_source_tx(tx, &key).await
}

pub struct CronWindow<'a> {
    pub workflow_id: &'a str,
    pub trigger_id: &'a str,
    pub expected_slot: DateTime<Utc>,
    pub missed_slots: &'a [DateTime<Utc>],
    pub due_slot: Option<DateTime<Utc>>,
    pub next_slot: DateTime<Utc>,
    pub now: DateTime<Utc>,
}

impl Db {
    pub async fn create_workflow(&self, workspace_id: &str, revision: &WorkflowRevision) -> Result<WorkflowRevision> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO agent_workflows (workflow_id, enabled, current_version, updated_at)
             VALUES (?, 0, ?, ?)",
        )
        .bind(&revision.workflow_id)
        .bind(revision.version)
        .bind(revision.created_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO workflow_revisions (workflow_id, version, definition_json, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&revision.workflow_id)
        .bind(revision.version)
        .bind(serde_json::to_string(&revision.definition)?)
        .bind(revision.created_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(WorkflowRevision {
            enabled: false,
            ..revision.clone()
        })
    }

    pub async fn append_workflow_revision(
        &self,
        workspace_id: &str,
        revision: &WorkflowRevision,
        expected_version: i64,
    ) -> Result<WorkflowRevision> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (bool, i64)>(
            "SELECT enabled, current_version FROM agent_workflows WHERE workflow_id = ?",
        )
        .bind(&revision.workflow_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: revision.workflow_id.clone(),
        })?;
        if current.1 != expected_version || revision.version != expected_version + 1 {
            return Err(DbError::CasConflict {
                message: format!("expected workflow version {expected_version}"),
            });
        }
        sqlx::query(
            "INSERT INTO workflow_revisions (workflow_id, version, definition_json, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&revision.workflow_id)
        .bind(revision.version)
        .bind(serde_json::to_string(&revision.definition)?)
        .bind(revision.created_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE agent_workflows SET current_version = ?, updated_at = ?
             WHERE workflow_id = ? AND current_version = ?",
        )
        .bind(revision.version)
        .bind(revision.created_at)
        .bind(&revision.workflow_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
        let stored = revision_by_version_tx(&mut tx, &revision.workflow_id, revision.version)
            .await?
            .ok_or_else(|| DbError::Internal("inserted workflow revision is missing".to_string()))?;
        tx.commit().await?;
        Ok(stored)
    }

    pub async fn set_workflow_enabled(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Result<WorkflowRevision> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let updated = sqlx::query("UPDATE agent_workflows SET enabled = ?, updated_at = ? WHERE workflow_id = ?")
            .bind(enabled)
            .bind(now)
            .bind(workflow_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DbError::NotFound {
                id: workflow_id.to_string(),
            });
        }
        let version = sqlx::query_scalar::<_, i64>("SELECT current_version FROM agent_workflows WHERE workflow_id = ?")
            .bind(workflow_id)
            .fetch_one(&mut *tx)
            .await?;
        let revision = revision_by_version_tx(&mut tx, workflow_id, version)
            .await?
            .ok_or_else(|| DbError::Internal("current workflow revision is missing".to_string()))?;
        tx.commit().await?;
        Ok(revision)
    }

    /// Current revision plus the authoritative enabled pointer.
    pub async fn current_workflow(&self, workspace_id: &str, workflow_id: &str) -> Result<Option<WorkflowRevision>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, WorkflowRevisionRow>(
            "SELECT r.workflow_id, r.version, w.enabled, r.definition_json, r.created_at
             FROM agent_workflows w
             JOIN workflow_revisions r
               ON r.workflow_id = w.workflow_id AND r.version = w.current_version
             WHERE w.workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_optional(&pool)
        .await?
        .map(WorkflowRevision::try_from)
        .transpose()
    }

    pub async fn workflow_revision(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        version: i64,
    ) -> Result<Option<WorkflowRevision>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let revision = revision_by_version_tx(&mut tx, workflow_id, version).await?;
        tx.commit().await?;
        Ok(revision)
    }

    /// Every current Workflow revision for the overview's scheduling section.
    /// Unlike [`Self::list_enabled_workflows`], disabled Workflows are included.
    pub async fn list_workflows(&self, workspace_id: &str) -> Result<Vec<WorkflowRevision>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, WorkflowRevisionRow>(
            "SELECT r.workflow_id, r.version, w.enabled, r.definition_json, r.created_at
             FROM agent_workflows w
             JOIN workflow_revisions r
               ON r.workflow_id = w.workflow_id AND r.version = w.current_version
             ORDER BY w.workflow_id",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(WorkflowRevision::try_from)
        .collect()
    }

    pub async fn list_enabled_workflows(&self, workspace_id: &str) -> Result<Vec<WorkflowRevision>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, WorkflowRevisionRow>(
            "SELECT r.workflow_id, r.version, w.enabled, r.definition_json, r.created_at
             FROM agent_workflows w
             JOIN workflow_revisions r
               ON r.workflow_id = w.workflow_id AND r.version = w.current_version
             WHERE w.enabled = 1 ORDER BY w.workflow_id",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(WorkflowRevision::try_from)
        .collect()
    }

    pub async fn cron_cursor(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        trigger_id: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let value = sqlx::query_scalar::<_, String>(
            "SELECT next_slot FROM workflow_cron_cursors WHERE workflow_id = ? AND trigger_id = ?",
        )
        .bind(workflow_id)
        .bind(trigger_id)
        .fetch_optional(&pool)
        .await?;
        value
            .map(|value| {
                DateTime::parse_from_rfc3339(&value)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|error| DbError::Internal(format!("invalid cron cursor: {error}")))
            })
            .transpose()
    }

    pub async fn initialize_cron_cursor(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        trigger_id: &str,
        next_slot: DateTime<Utc>,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO workflow_cron_cursors (workflow_id, trigger_id, next_slot)
             SELECT w.workflow_id, ?, ? FROM agent_workflows w
             WHERE w.workflow_id = ? AND w.enabled = 1",
        )
        .bind(trigger_id)
        .bind(timestamp(next_slot))
        .bind(workflow_id)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn record_cron_window(
        &self,
        workspace_id: &str,
        window: &CronWindow<'_>,
    ) -> Result<Vec<WorkflowOccurrence>> {
        let CronWindow {
            workflow_id,
            trigger_id,
            expected_slot,
            missed_slots,
            due_slot,
            next_slot,
            now,
        } = window;
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE workflow_cron_cursors SET next_slot = ?
             WHERE workflow_id = ? AND trigger_id = ? AND next_slot = ?
               AND EXISTS (SELECT 1 FROM agent_workflows WHERE workflow_id = ? AND enabled = 1)",
        )
        .bind(timestamp(*next_slot))
        .bind(workflow_id)
        .bind(trigger_id)
        .bind(timestamp(*expected_slot))
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(Vec::new());
        }
        for slot in *missed_slots {
            let slot = timestamp(*slot);
            let payload = json!({
                "workflow_id": workflow_id,
                "trigger_id": trigger_id,
                "slot": slot,
                "next_slot": timestamp(*next_slot),
            });
            append_workflow_event_tx(
                &mut tx,
                &format!("trigger.missed:{workflow_id}:{trigger_id}:{slot}"),
                &format!("supervisor:{workspace_id}"),
                None,
                None,
                "trigger.missed",
                *now,
                &payload,
            )
            .await?;
        }
        let occurrence = match due_slot {
            Some(slot) => {
                let slot_string = timestamp(*slot);
                schedule_occurrence_tx(
                    &mut tx,
                    workspace_id,
                    workflow_id,
                    trigger_id,
                    &slot_string,
                    &json!({"slot": slot_string}),
                    *now,
                )
                .await?
            }
            None => None,
        };
        tx.commit().await?;
        Ok(occurrence.into_iter().collect())
    }

    pub async fn schedule_workflow_slot(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        trigger_id: &str,
        slot: DateTime<Utc>,
    ) -> Result<Option<WorkflowOccurrence>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let key = source_key(workflow_id, trigger_id, &timestamp(slot));
        let occurrence = occurrence_by_source_tx(&mut tx, &key).await?;
        tx.commit().await?;
        Ok(occurrence)
    }

    pub async fn schedule_workflow_event(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        trigger_id: &str,
        source_seq: i64,
        event_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<WorkflowOccurrence>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let last_seq = sqlx::query_scalar::<_, i64>(
            "SELECT last_seq FROM workflow_event_cursors WHERE workflow_id = ? AND trigger_id = ?",
        )
        .bind(workflow_id)
        .bind(trigger_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        if source_seq > last_seq + 1 {
            let payload = json!({
                "workflow_id": workflow_id,
                "trigger_id": trigger_id,
                "event_id": event_id,
                "expected_seq": last_seq + 1,
                "actual_seq": source_seq,
                "reason": "event_gap",
            });
            append_workflow_event_tx(
                &mut tx,
                &format!("trigger.unavailable:{workflow_id}:{trigger_id}:{event_id}:gap"),
                &format!("supervisor:{workspace_id}"),
                None,
                None,
                "trigger.unavailable",
                now,
                &payload,
            )
            .await?;
            tx.commit().await?;
            return Ok(None);
        }
        let occurrence = schedule_occurrence_tx(
            &mut tx,
            workspace_id,
            workflow_id,
            trigger_id,
            event_id,
            &json!({"event_id": event_id, "source_seq": source_seq}),
            now,
        )
        .await?;
        if occurrence.is_some() {
            sqlx::query(
                "INSERT INTO workflow_event_cursors (workflow_id, trigger_id, last_seq)
                 VALUES (?, ?, ?)
                 ON CONFLICT(workflow_id, trigger_id) DO UPDATE SET
                     last_seq = MAX(last_seq, excluded.last_seq)",
            )
            .bind(workflow_id)
            .bind(trigger_id)
            .bind(source_seq)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(occurrence)
    }

    pub async fn record_workflow_source_unavailable(
        &self,
        workspace_id: &str,
        workflow_id: &str,
        trigger_id: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let alert_id = uuid::Uuid::new_v4();
        let payload = json!({
            "workflow_id": workflow_id,
            "trigger_id": trigger_id,
            "reason": reason,
        });
        append_workflow_event_tx(
            &mut tx,
            &format!("trigger.unavailable:{workflow_id}:{trigger_id}:{alert_id}"),
            &format!("supervisor:{workspace_id}"),
            None,
            None,
            "trigger.unavailable",
            now,
            &payload,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn workflow_main_projection(&self, workspace_id: &str) -> Result<Value> {
        let pool = self.workspace_pool(workspace_id).await?;
        let workflows = sqlx::query_as::<_, WorkflowRevisionRow>(
            "SELECT r.workflow_id, r.version, w.enabled, r.definition_json, r.created_at
             FROM agent_workflows w
             JOIN workflow_revisions r
               ON r.workflow_id = w.workflow_id AND r.version = w.current_version
             ORDER BY w.workflow_id",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(WorkflowRevision::try_from)
        .collect::<Result<Vec<_>>>()?;
        let occurrences = sqlx::query_as::<_, WorkflowOccurrenceRow>(
            "SELECT source_key, workflow_id, trigger_id, occurrence_id, workflow_version,
                    task_id, run_id, session_id, source_json, created_at
             FROM workflow_occurrences ORDER BY created_at DESC, source_key DESC LIMIT 100",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(WorkflowOccurrence::try_from)
        .collect::<Result<Vec<_>>>()?;
        Ok(json!({"workflows": workflows, "recent_occurrences": occurrences}))
    }
}
