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

/// A data source declared by a Thing's active driver manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDeviceSource {
    pub id: String,
    pub data_kind: String,
}

/// The current-workspace reference surface checked before a Workflow from a
/// template is created. `found` distinguishes a missing Thing from a Thing
/// whose driver has not published a declaration yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowDeviceReference {
    pub found: bool,
    pub sources: Vec<WorkflowDeviceSource>,
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
    async fn insert_workflow(
        &self,
        workspace_id: &str,
        revision: &WorkflowRevision,
        enabled: bool,
    ) -> Result<WorkflowRevision> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO agent_workflows (workflow_id, enabled, current_version, updated_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&revision.workflow_id)
        .bind(enabled)
        .bind(revision.version)
        .bind(revision.created_at)
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(sqlx::Error::Database(database)) if database.is_unique_violation() => {
                return Err(DbError::Conflict {
                    message: format!("workflow already exists: {}", revision.workflow_id),
                });
            }
            Err(error) => return Err(error.into()),
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
        tx.commit().await?;
        Ok(WorkflowRevision {
            enabled,
            ..revision.clone()
        })
    }

    /// Blank create (R30): the first revision is always disabled.
    pub async fn create_workflow(&self, workspace_id: &str, revision: &WorkflowRevision) -> Result<WorkflowRevision> {
        self.insert_workflow(workspace_id, revision, false).await
    }

    /// Template instantiation (DR4=B): a validated template creates its first
    /// revision directly enabled. The enabled bit is fixed by this entry point
    /// so a caller cannot accidentally invert the template contract. No
    /// occurrence or wake is queued here; the normal trigger scheduler owns all
    /// future wakes.
    pub async fn create_enabled_workflow(
        &self,
        workspace_id: &str,
        revision: &WorkflowRevision,
    ) -> Result<WorkflowRevision> {
        self.insert_workflow(workspace_id, revision, true).await
    }

    /// Resolve a Thing plus its active driver manifest inside one workspace.
    /// Template instantiation uses this to reject unknown devices and sources
    /// before any Workflow row is written.
    ///
    /// Selectors may name a Thing by ID or display name. The canonical
    /// `things.id` is resolved first; declarations and manifests are keyed by
    /// that ID, never by the raw selector.
    pub async fn workflow_device_reference(
        &self,
        workspace_id: &str,
        device_id: &str,
    ) -> Result<WorkflowDeviceReference> {
        let thing_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM things WHERE workspace_id = ? AND (id = ? OR name = ?)
             ORDER BY CASE WHEN id = ? THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(workspace_id)
        .bind(device_id)
        .bind(device_id)
        .bind(device_id)
        .fetch_optional(self.pool())
        .await?;
        let Some(thing_id) = thing_id else {
            return Ok(WorkflowDeviceReference::default());
        };

        let manifest_json: Option<String> = sqlx::query_scalar(
            "SELECT m.manifest_json FROM device_declarations d
             JOIN driver_declarations m
               ON m.workspace_id = d.workspace_id
              AND m.driver_name = d.driver_name
              AND m.driver_version = d.driver_version
             WHERE d.workspace_id = ? AND d.thing_id = ?
               AND d.status = 'active' AND m.status = 'active'
             ORDER BY d.id DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(&thing_id)
        .fetch_optional(self.pool())
        .await?;

        let mut reference = WorkflowDeviceReference {
            found: true,
            ..WorkflowDeviceReference::default()
        };
        let Some(manifest_json) = manifest_json else {
            return Ok(reference);
        };
        let manifest: Value = serde_json::from_str(&manifest_json)?;
        for source in manifest.get("sources").and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = source.get("id").and_then(Value::as_str) else {
                continue;
            };
            reference.sources.push(WorkflowDeviceSource {
                id: id.to_string(),
                data_kind: source
                    .get("dataKind")
                    .and_then(Value::as_str)
                    .unwrap_or("values")
                    .to_string(),
            });
        }
        Ok(reference)
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

// ── Harness Task workflow control (moved from runtime_task.rs, Task 19C4) ──
//
// These functions operate on the owning workspace's Harness generation. They
// are the surviving Task writers after the legacy platform Task plane was
// retired; the platform `agent_tasks`/`task_wake_outbox` tables have no
// production reader or writer.

use sqlx::SqliteConnection;
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

use crate::error::Result as DbResult;

/// Read-only projection used by the Harness overview and Subagent page.
#[derive(Debug, Clone)]
pub struct HarnessTaskRow {
    pub id: String,
    pub run_id: Option<String>,
    pub state: String,
    pub objective: Option<String>,
    /// Latest `task.outcome` payload, if any; parsed by the HTTP projection.
    pub outcome_json: Option<String>,
    pub updated_at: Option<String>,
    pub parent_task_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct HarnessTaskDbRow {
    id: String,
    run_id: Option<String>,
    state: String,
    objective: Option<String>,
    outcome_json: Option<String>,
    updated_at: Option<String>,
    parent_task_id: Option<String>,
}

impl From<HarnessTaskDbRow> for HarnessTaskRow {
    fn from(row: HarnessTaskDbRow) -> Self {
        Self {
            id: row.id,
            run_id: row.run_id,
            state: row.state,
            objective: row.objective,
            outcome_json: row.outcome_json,
            updated_at: row.updated_at,
            parent_task_id: row.parent_task_id,
        }
    }
}

fn harness_task_event(
    workspace_id: &str,
    task_id: &str,
    run_id: Option<&str>,
    event_id: String,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    payload_json: Value,
) -> DbResult<HarnessEventInput> {
    let payload = serde_json::to_string(&payload_json)?;
    Ok(HarnessEventInput {
        event_id,
        workspace_id: workspace_id.to_string(),
        session_id: run_id
            .map(|run_id| format!("task:{task_id}/run:{run_id}"))
            .unwrap_or_else(|| format!("task:{task_id}")),
        execution_mode: ExecutionMode::Production,
        run_id: run_id.map(str::to_string),
        task_id: Some(task_id.to_string()),
        event_type: event_type.to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", sha2::Sha256::digest(payload.as_bytes())),
        payload_json,
    })
}

pub(crate) async fn drain_cancelling_tasks(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    now: DateTime<Utc>,
) -> DbResult<()> {
    let tasks = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
        "SELECT id, parent_task_id, run_id, state_version FROM agent_tasks
         WHERE state = 'cancelling' AND execution_mode = 'production'
         ORDER BY CASE WHEN parent_task_id IS NULL THEN 1 ELSE 0 END, id",
    )
    .fetch_all(&mut *connection)
    .await?;
    for (task_id, _, run_id, expected_version) in tasks {
        let blocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM agent_tasks child
                 WHERE child.parent_task_id = ?
                   AND child.state NOT IN ('completed', 'failed', 'cancelled')
             ) OR EXISTS(
                 SELECT 1 FROM effect_intents effect
                 JOIN agent_tasks owner ON owner.id = effect.task_id
                 WHERE (owner.id = ? OR owner.parent_task_id = ?)
                   AND effect.state NOT IN
                       ('completed', 'failed', 'not_applied', 'cancelled', 'cancelled_unknown')
             ) OR EXISTS(
                 -- A cancelling Task must stay cancelling while a physical Job
                 -- is still accepted/running: finalizing it here would hide the
                 -- later cancelled_unknown -> failed decision.
                 SELECT 1 FROM device_jobs job
                 WHERE (job.task_id = ?
                    OR job.task_id IN (
                        SELECT child.id FROM agent_tasks child WHERE child.parent_task_id = ?
                    ))
                   AND job.state IN ('accepted', 'running')
             )",
        )
        .bind(&task_id)
        .bind(&task_id)
        .bind(&task_id)
        .bind(&task_id)
        .bind(&task_id)
        .fetch_one(&mut *connection)
        .await?;
        if blocked {
            continue;
        }
        let cancel_unknown = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM effect_intents effect
                 JOIN agent_tasks owner ON owner.id = effect.task_id
                 WHERE (owner.id = ? OR owner.parent_task_id = ?)
                   AND effect.state = 'cancelled_unknown'
             )",
        )
        .bind(&task_id)
        .bind(&task_id)
        .fetch_one(&mut *connection)
        .await?;
        let next = if cancel_unknown { "failed" } else { "cancelled" };
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = 'cancelling' AND state_version = ?",
        )
        .bind(next)
        .bind(&task_id)
        .bind(expected_version)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if updated != 1 {
            continue;
        }
        sqlx::query("UPDATE agent_runs SET state = ? WHERE task_id = ?")
            .bind(next)
            .bind(&task_id)
            .execute(&mut *connection)
            .await?;
        let reason = if cancel_unknown { "cancel_unknown" } else { "cancelled" };
        let event = harness_task_event(
            workspace_id,
            &task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:{next}", expected_version + 1),
            &format!("task.{next}"),
            now,
            json!({"taskId":task_id,"state":next,"reason":reason}),
        )?;
        crate::harness_effect::insert_event(connection, &event).await?;
    }
    Ok(())
}

impl Db {
    pub async fn pause_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT state, run_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, version) = current;
        if state == "paused" {
            return Ok(());
        }
        if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
            return Err(DbError::Conflict {
                message: format!("task {task_id} cannot be paused from {state}"),
            });
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = 'paused', state_version = state_version + 1
             WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .bind(&state)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("task {task_id} changed while pausing"),
            });
        }
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:paused", version + 1),
            "task.paused",
            Utc::now(),
            json!({"taskId":task_id,"state":"paused","previousState":state}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn resume_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<String> {
        // N1: a reconciliation pause must not be lifted before the Effect is
        // resolved. The periodic recovery pass consumes an answered Decision;
        // until then the ordinary resume route keeps the Task paused.
        if self.task_reconciliation_wait(workspace_id, task_id).await? {
            return Err(DbError::Validation {
                message: format!(
                    "task {task_id} is waiting for effect reconciliation; answer the reconciliation before resuming"
                ),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT state, run_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, version) = current;
        if state != "paused" {
            return Ok(state);
        }
        let previous = sqlx::query_scalar::<_, Option<String>>(
            "SELECT json_extract(payload_json, '$.previousState') FROM session_events
             WHERE task_id = ? AND type IN ('task.paused', 'task.pause_waiting')
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten()
        .filter(|value| !value.is_empty())
        // A Task paused by startup recovery may not carry an explicit pause
        // relation; restoring Active is the safe default.
        .unwrap_or_else(|| "running".to_string());
        if !matches!(
            previous.as_str(),
            "pending" | "running" | "waiting_job" | "waiting_watch" | "needs_approval"
        ) {
            return Err(DbError::Internal(format!(
                "paused task {task_id} has invalid previous state {previous}"
            )));
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = 'paused' AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(&previous)
        .bind(task_id)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("task {task_id} changed while resuming"),
            });
        }
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:resumed", version + 1),
            "task.resumed",
            Utc::now(),
            json!({"taskId":task_id,"state":previous}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(previous)
    }

    pub async fn cancel_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<Vec<String>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let root_state = sqlx::query_scalar::<_, String>(
            "SELECT state FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        if matches!(root_state.as_str(), "completed" | "failed" | "cancelled") {
            return Ok(Vec::new());
        }
        let mut task_ids = vec![task_id.to_string()];
        task_ids.extend(
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM agent_tasks WHERE parent_task_id = ? AND execution_mode = 'production' ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&mut *tx)
            .await?,
        );
        let now = Utc::now();
        for id in &task_ids {
            let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
                "SELECT state, run_id, state_version FROM agent_tasks
                 WHERE id = ? AND execution_mode = 'production'",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let (state, run_id, version) = current;
            if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
                continue;
            }
            let updated = sqlx::query(
                "UPDATE agent_tasks SET state = 'cancelling', state_version = state_version + 1
                 WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
            )
            .bind(id)
            .bind(&state)
            .bind(version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(DbError::CasConflict {
                    message: format!("task {id} changed while cancelling"),
                });
            }
            sqlx::query(
                "UPDATE task_wake_outbox
                 SET dispatch_state = 'dead', resolved_at = ?, failure_reason = 'cancelled'
                 WHERE task_id = ? AND dispatch_state = 'pending'",
            )
            .bind(now.to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
            let event = harness_task_event(
                workspace_id,
                id,
                run_id.as_deref(),
                format!("task:{id}:v{}:cancelling", version + 1),
                "task.cancelling",
                now,
                json!({"taskId":id,"state":"cancelling","reason":"cancel_requested"}),
            )?;
            crate::harness_effect::insert_event(&mut tx, &event).await?;
        }
        drain_cancelling_tasks(&mut tx, workspace_id, now).await?;
        tx.commit().await?;
        for id in &task_ids {
            if let Ok(Some(task)) = self.harness_get_task(workspace_id, id).await
                && task.status.is_terminal()
            {
                self.prune_runtime_device_state(workspace_id, id).await;
            }
        }
        Ok(task_ids)
    }

    /// Non-terminal direct Subagents of a Task. Subagents never nest, so one
    /// level is the whole tree; this is the cancel complement used when the
    /// live coordinator, not the workspace cascade, transitions the root.
    pub async fn harness_active_child_task_ids(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
    ) -> DbResult<Vec<String>> {
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT id FROM agent_tasks
             WHERE parent_task_id = ? AND execution_mode = 'production'
               AND state NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY id",
        )
        .bind(parent_task_id)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn drain_workflow_cancellation(&self, workspace_id: &str) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let candidates: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM agent_tasks WHERE state = 'cancelling' AND execution_mode = 'production'",
        )
        .fetch_all(&pool)
        .await?;
        let mut tx = pool.begin().await?;
        drain_cancelling_tasks(&mut tx, workspace_id, Utc::now()).await?;
        tx.commit().await?;
        for id in candidates {
            if let Ok(Some(task)) = self.harness_get_task(workspace_id, &id).await
                && task.status.is_terminal()
            {
                self.prune_runtime_device_state(workspace_id, &id).await;
            }
        }
        Ok(())
    }

    /// Nonterminal root Tasks for the Harness overview. Subagents are loaded
    /// separately so a Main with many children stays bounded.
    pub async fn harness_main_tasks(&self, workspace_id: &str) -> DbResult<Vec<HarnessTaskRow>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, HarnessTaskDbRow>(
            "SELECT t.id, t.run_id, t.state,
                    json_extract(r.definition_json, '$.objective') AS objective,
                    (SELECT e.payload_json FROM session_events e
                      WHERE e.task_id = t.id AND e.type = 'task.outcome'
                      ORDER BY e.seq DESC LIMIT 1) AS outcome_json,
                    (SELECT MAX(e2.occurred_at) FROM session_events e2
                      WHERE e2.task_id = t.id) AS updated_at,
                    t.parent_task_id
             FROM agent_tasks t
             LEFT JOIN workflow_revisions r
               ON r.workflow_id = t.workflow_id AND r.version = t.workflow_version
             WHERE t.parent_task_id IS NULL
               AND t.execution_mode = 'production'
               AND t.state NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY t.id",
        )
        .fetch_all(&pool)
        .await?;
        Ok(rows.into_iter().map(HarnessTaskRow::from).collect())
    }

    /// One bounded page of children for a Main Task. A missing Main returns
    /// NotFound; tasks from other parents are never included.
    pub async fn harness_subagents(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
        limit: i64,
        offset: i64,
    ) -> DbResult<(Vec<HarnessTaskRow>, i64)> {
        let pool = self.workspace_pool(workspace_id).await?;
        let exists = sqlx::query_scalar::<_, String>(
            "SELECT id FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_optional(&pool)
        .await?;
        if exists.is_none() {
            return Err(DbError::NotFound {
                id: parent_task_id.to_string(),
            });
        }
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_tasks WHERE parent_task_id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_one(&pool)
        .await?;
        let rows = sqlx::query_as::<_, HarnessTaskDbRow>(
            "SELECT t.id, t.run_id, t.state,
                    json_extract(r.definition_json, '$.objective') AS objective,
                    (SELECT e.payload_json FROM session_events e
                      WHERE e.task_id = t.id AND e.type = 'task.outcome'
                      ORDER BY e.seq DESC LIMIT 1) AS outcome_json,
                    (SELECT MAX(e2.occurred_at) FROM session_events e2
                      WHERE e2.task_id = t.id) AS updated_at,
                    t.parent_task_id
             FROM agent_tasks t
             LEFT JOIN workflow_revisions r
               ON r.workflow_id = t.workflow_id AND r.version = t.workflow_version
             WHERE t.parent_task_id = ?
               AND t.execution_mode = 'production'
             ORDER BY t.id LIMIT ? OFFSET ?",
        )
        .bind(parent_task_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await?;
        Ok((rows.into_iter().map(HarnessTaskRow::from).collect(), total))
    }

    pub async fn create_subagent_task(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
        task_id: &str,
        run_id: &str,
        now: DateTime<Utc>,
    ) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let parent = sqlx::query_as::<_, (Option<String>, Option<String>, Option<i64>, Option<String>, String)>(
            "SELECT parent_task_id, workflow_id, workflow_version, run_id, state
             FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: parent_task_id.to_string(),
        })?;
        let (grandparent, workflow_id, workflow_version, parent_run_id, parent_state) = parent;
        if grandparent.is_some() {
            return Err(DbError::Validation {
                message: "v1 subagents cannot create child tasks".to_string(),
            });
        }
        if matches!(
            parent_state.as_str(),
            "paused" | "cancelling" | "completed" | "failed" | "cancelled"
        ) {
            return Err(DbError::Conflict {
                message: format!("parent task {parent_task_id} cannot dispatch a child from {parent_state}"),
            });
        }
        let workflow_id = workflow_id.ok_or_else(|| DbError::Validation {
            message: "parent task has no pinned workflow".to_string(),
        })?;
        let workflow_version = workflow_version.ok_or_else(|| DbError::Validation {
            message: "parent task has no pinned workflow version".to_string(),
        })?;
        sqlx::query(
            "INSERT INTO agent_tasks
                (id, run_id, state, state_version, workflow_id, workflow_version, parent_task_id)
             VALUES (?, ?, 'pending', 1, ?, ?, ?)",
        )
        .bind(task_id)
        .bind(run_id)
        .bind(&workflow_id)
        .bind(workflow_version)
        .bind(parent_task_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO agent_runs (id, task_id, state, parent_run_id) VALUES (?, ?, 'pending', ?)")
            .bind(run_id)
            .bind(task_id)
            .bind(parent_run_id)
            .execute(&mut *tx)
            .await?;
        let signal = json!({
            "workspace_id":workspace_id,
            "task_id":task_id,
            "run_id":run_id,
            "workflow_id":workflow_id,
            "workflow_version":workflow_version,
            "parent_task_id":parent_task_id,
        });
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, 'pending', ?, NULL, NULL)",
        )
        .bind(task_id)
        .bind(format!("task:{task_id}:created"))
        .bind(serde_json::to_string(&signal)?)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        let event = harness_task_event(
            workspace_id,
            task_id,
            Some(run_id),
            format!("task:{task_id}:created"),
            "task.created",
            now,
            json!({"taskId":task_id,"parentTaskId":parent_task_id,"workflowVersion":workflow_version}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_subagent_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        outcome_status: &str,
        outcome: Value,
        now: DateTime<Utc>,
    ) -> DbResult<bool> {
        if !matches!(outcome_status, "completed" | "failed") {
            return Err(DbError::Validation {
                message: format!("invalid subagent outcome status {outcome_status}"),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
            "SELECT state, run_id, parent_task_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, parent_task_id, version) = current;
        if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
            return Ok(false);
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(outcome_status)
        .bind(task_id)
        .bind(&state)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("subagent task {task_id} changed while completing"),
            });
        }
        if let Some(run_id) = &run_id {
            sqlx::query("UPDATE agent_runs SET state = ? WHERE id = ? AND task_id = ?")
                .bind(outcome_status)
                .bind(run_id)
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
        }
        let event_id = format!("task:{task_id}:outcome");
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            event_id.clone(),
            "task.outcome",
            now,
            json!({"taskId":task_id,"outcome":outcome}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        if let Some(parent_task_id) = parent_task_id {
            let signal = json!({"workspace_id":workspace_id,"taskId":parent_task_id,"outcomeEventId":event_id});
            sqlx::query(
                "INSERT INTO task_wake_outbox
                    (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
                 SELECT ?, ?, ?, 'pending', ?, NULL, NULL FROM agent_tasks parent
                 WHERE parent.id = ? AND parent.state NOT IN ('cancelling', 'completed', 'failed', 'cancelled')
                 ON CONFLICT(task_id, event_id) DO NOTHING",
            )
            .bind(&parent_task_id)
            .bind(format!("{event_id}:parent-wake"))
            .bind(serde_json::to_string(&signal)?)
            .bind(now.to_rfc3339())
            .bind(parent_task_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        // `complete_subagent_task` only accepts completed/failed.
        self.prune_runtime_device_state(workspace_id, task_id).await;
        Ok(true)
    }
}
