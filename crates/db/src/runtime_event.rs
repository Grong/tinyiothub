use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::agent_task::{AgentTask, WakeDispatchState};
use tinyiothub_core::device_job::DeviceJob;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::DriverManifest;
use tinyiothub_core::media::DataKind;

use crate::Db;
use crate::error::{DbError, Result};

pub const RUNTIME_EVENT_KIND: &str = "runtime.v1";

#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
pub struct RuntimeProjectionProbe {
    armed: std::sync::atomic::AtomicBool,
    attempts: std::sync::atomic::AtomicUsize,
    task_read: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[cfg(any(test, feature = "testing"))]
impl RuntimeProjectionProbe {
    pub fn arm(&self) {
        self.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn wait_for_task_read(&self) {
        self.task_read.notified().await;
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn release(&self) {
        self.release.notify_one();
    }

    async fn after_task_read(&self) {
        self.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.task_read.notify_one();
            self.release.notified().await;
        }
    }
}

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

pub(crate) fn task_event(task: &AgentTask, event_type: &str, event_id: String) -> RuntimeEventInput {
    RuntimeEventInput {
        event_id,
        event_type: event_type.to_string(),
        occurred_at: Utc::now(),
        workspace_id: task.workspace_id.clone(),
        task_id: Some(task.id.clone()),
        run_id: task.current_run_id.clone(),
        tool_call_id: None,
        job_id: None,
        approval_id: None,
        device_id: None,
        source_id: None,
        source_seq: None,
        evidence_id: None,
        data: serde_json::json!({"stateVersion": task.state_version}),
    }
}

pub(crate) fn job_event(job: &DeviceJob, suffix: &str, data: Value) -> RuntimeEventInput {
    let mut payload = serde_json::json!({
        "id": job.id,
        "workspaceId": job.workspace_id,
        "deviceId": job.device_id,
        "tool": job.tool,
        "argsJson": job.args_json,
        "state": job.state.as_str(),
        "stopRequestedAt": job.stop_requested_at,
        "progressJson": job.progress_json,
        "resultJson": job.result_json,
        "taskId": job.task_id,
        "runId": job.run_id,
        "toolCallId": job.tool_call_id,
        "driverCallId": job.driver_call_id,
        "timeoutMs": job.timeout_ms,
        "startedAt": job.started_at,
        "endedAt": job.ended_at,
    });
    if let (Some(payload), Some(data)) = (payload.as_object_mut(), data.as_object()) {
        for (key, value) in data {
            payload.insert(key.clone(), value.clone());
        }
    }
    RuntimeEventInput {
        event_id: format!("job:{}:{suffix}", job.id),
        event_type: format!("job.{suffix}"),
        occurred_at: Utc::now(),
        workspace_id: job.workspace_id.clone(),
        task_id: job.task_id.clone(),
        run_id: job.run_id.clone(),
        tool_call_id: job.tool_call_id.clone(),
        job_id: Some(job.id.clone()),
        approval_id: None,
        device_id: Some(job.device_id.clone()),
        source_id: None,
        source_seq: None,
        evidence_id: None,
        data: payload,
    }
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
pub struct TaskEvidenceProjection {
    pub evidence_id: String,
    pub mime: String,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub device_id: String,
    pub source_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeviceAssociation {
    pub device_id: String,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeviceSource {
    pub id: String,
    pub data_kind: DataKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeviceField {
    pub name: String,
    pub value: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeviceProjection {
    pub device_id: String,
    pub health: String,
    pub zone: Option<String>,
    pub position: Option<String>,
    pub sources: Vec<RuntimeDeviceSource>,
    pub fields: Vec<RuntimeDeviceField>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskJobProjection {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub state: tinyiothub_core::device_job::DeviceJobState,
    pub stop_requested_at: Option<DateTime<Utc>>,
    pub progress_json: Option<Value>,
    pub result_json: Option<Value>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub driver_call_id: Option<String>,
    pub timeout_ms: u64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl From<DeviceJob> for TaskJobProjection {
    fn from(job: DeviceJob) -> Self {
        Self {
            id: job.id,
            workspace_id: job.workspace_id,
            device_id: job.device_id,
            tool: job.tool,
            args_json: job.args_json,
            state: job.state,
            stop_requested_at: job.stop_requested_at,
            progress_json: job.progress_json,
            result_json: job.result_json,
            task_id: job.task_id,
            run_id: job.run_id,
            tool_call_id: job.tool_call_id,
            driver_call_id: job.driver_call_id,
            timeout_ms: job.timeout_ms,
            started_at: job.started_at,
            ended_at: job.ended_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProjection {
    pub task: AgentTask,
    pub runs: Vec<TaskRunProjection>,
    pub jobs: Vec<TaskJobProjection>,
    pub approvals: Vec<tinyiothub_core::task_approval::TaskApproval>,
    pub watches: Vec<TaskWatchProjection>,
    pub evidence: Vec<TaskEvidenceProjection>,
    pub associations: Vec<RuntimeDeviceAssociation>,
    pub devices: Vec<RuntimeDeviceProjection>,
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

#[derive(sqlx::FromRow)]
struct RuntimeThingFactsRow {
    id: String,
    state: i32,
    zone_ref: Option<String>,
    position: Option<String>,
}

fn add_device_reference(
    associations: &mut Vec<RuntimeDeviceAssociation>,
    positions: &mut std::collections::HashMap<String, usize>,
    device_id: &str,
    source_id: Option<&str>,
) {
    if device_id.trim().is_empty() {
        return;
    }
    let index = match positions.get(device_id).copied() {
        Some(index) => index,
        None => {
            let index = associations.len();
            positions.insert(device_id.to_string(), index);
            associations.push(RuntimeDeviceAssociation {
                device_id: device_id.to_string(),
                source_ids: Vec::new(),
            });
            index
        }
    };
    if let Some(source_id) = source_id.filter(|id| !id.trim().is_empty()) {
        let sources = &mut associations[index].source_ids;
        if !sources.iter().any(|existing| existing == source_id) {
            sources.push(source_id.to_string());
        }
    }
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

fn encode_event_payload(event: &RuntimeEvent) -> Result<String> {
    let mut payload = serde_json::to_value(event)?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| DbError::Internal("event was not an object".into()))?;
    object.remove("cursor");
    object.insert("v".into(), Value::from(1));
    Ok(serde_json::to_string(&payload)?)
}

/// Authoritative device facts for a live event: declaration ordering, health,
/// zone/position, and selected current values. The SQL stays in `db` and is
/// evaluated inside the event-append transaction so published events match
/// persisted facts without a follow-up fetch.
async fn load_device_facts(
    connection: &mut sqlx::SqliteConnection,
    workspace_id: &str,
    task_id: Option<&str>,
    device_id: &str,
    cursor: i64,
) -> Result<RuntimeDeviceProjection> {
    let facts = sqlx::query_as::<_, RuntimeThingFactsRow>(
        "SELECT id, state, zone_ref, position FROM things WHERE workspace_id = ? AND id = ?",
    )
    .bind(workspace_id)
    .bind(device_id)
    .fetch_optional(&mut *connection)
    .await?;
    let declaration = sqlx::query_scalar::<_, String>(
        "SELECT declaration_json FROM device_declarations
         WHERE workspace_id = ? AND thing_id = ? AND status = 'active'
         ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(device_id)
    .fetch_optional(&mut *connection)
    .await?
    .and_then(|raw| serde_json::from_str::<DeviceDeclaration>(&raw).ok());
    let manifest = match declaration.as_ref() {
        Some(declaration) => sqlx::query_scalar::<_, String>(
            "SELECT manifest_json FROM driver_declarations
             WHERE workspace_id = ? AND driver_name = ? AND driver_version = ? AND status = 'active'
             LIMIT 1",
        )
        .bind(workspace_id)
        .bind(&declaration.driver)
        .bind(&declaration.driver_version)
        .fetch_optional(&mut *connection)
        .await?
        .and_then(|raw| serde_json::from_str::<DriverManifest>(&raw).ok()),
        None => None,
    };
    let selected_fields = declaration
        .as_ref()
        .map(|declaration| {
            let key_fields = declaration
                .state_fields
                .iter()
                .filter(|field| field.key.is_some())
                .collect::<Vec<_>>();
            if key_fields.is_empty() {
                declaration.state_fields.iter().take(3).collect::<Vec<_>>()
            } else {
                key_fields
            }
        })
        .unwrap_or_default();
    let mut current_values = std::collections::HashMap::new();
    if let Some(task_id) = task_id {
        let state_rows = sqlx::query_as::<_, RuntimeLedgerRow>(
            "SELECT id, workspace_id, ts, payload FROM hub_ledger
             WHERE workspace_id = ? AND kind = ? AND id <= ?
               AND json_extract(payload, '$.taskId') = ?
               AND json_extract(payload, '$.deviceId') = ?
               AND json_extract(payload, '$.data.state') IS NOT NULL
             ORDER BY id",
        )
        .bind(workspace_id)
        .bind(RUNTIME_EVENT_KIND)
        .bind(cursor)
        .bind(task_id)
        .bind(device_id)
        .fetch_all(&mut *connection)
        .await?;
        for row in state_rows {
            let event = decode_event(row)?;
            if let Some(state) = event.data.get("state").and_then(Value::as_object) {
                for (name, value) in state {
                    let value = match value {
                        Value::String(value) => Some(value.clone()),
                        Value::Null => None,
                        value => Some(value.to_string()),
                    };
                    if let Some(value) = value {
                        current_values.insert((device_id.to_string(), name.clone()), value);
                    }
                }
            }
        }
    }
    let health = match facts.as_ref().map(|facts| facts.state) {
        Some(1) => "ok",
        Some(0) => "attention",
        Some(2 | 3) => "bad",
        _ => "unknown",
    }
    .to_string();
    let sources = manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .sources
                .iter()
                .map(|source| RuntimeDeviceSource {
                    id: source.id.clone(),
                    data_kind: source.data_kind.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let fields = selected_fields
        .into_iter()
        .map(|field| RuntimeDeviceField {
            name: field.name.clone(),
            value: current_values
                .get(&(device_id.to_string(), field.name.clone()))
                .cloned(),
            unit: field.unit.clone(),
        })
        .collect();
    Ok(RuntimeDeviceProjection {
        device_id: device_id.to_string(),
        health,
        zone: facts.as_ref().and_then(|facts| facts.zone_ref.clone()),
        position: facts.and_then(|facts| facts.position),
        sources,
        fields,
    })
}

pub(crate) async fn append_runtime_event_tx(
    connection: &mut sqlx::SqliteConnection,
    input: RuntimeEventInput,
) -> Result<RuntimeEvent> {
    if input.event_id.trim().is_empty() || input.event_type.trim().is_empty() || input.workspace_id.trim().is_empty() {
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
    let payload = encode_event_payload(&event)?;
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
    .fetch_optional(&mut *connection)
    .await?;
    let inserted_new = inserted.is_some();
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
            .fetch_one(&mut *connection)
            .await?
        }
    };
    let mut event = decode_event(row)?;
    if inserted_new && let Some(device_id) = event.device_id.clone() {
        let facts = load_device_facts(
            &mut *connection,
            &event.workspace_id,
            event.task_id.as_deref(),
            &device_id,
            event.cursor,
        )
        .await?;
        if let Some(data) = event.data.as_object_mut() {
            data.insert("deviceFacts".into(), serde_json::to_value(&facts)?);
            let payload = encode_event_payload(&event)?;
            sqlx::query("UPDATE hub_ledger SET payload = ? WHERE id = ? AND workspace_id = ? AND kind = ?")
                .bind(payload)
                .bind(event.cursor)
                .bind(&event.workspace_id)
                .bind(RUNTIME_EVENT_KIND)
                .execute(&mut *connection)
                .await?;
        }
    }
    Ok(event)
}

impl Db {
    pub async fn append_runtime_event(&self, input: RuntimeEventInput) -> Result<RuntimeEvent> {
        let mut tx = self.pool().begin().await?;
        let event = append_runtime_event_tx(&mut tx, input).await?;
        tx.commit().await?;
        Ok(event)
    }

    pub async fn runtime_event_by_id(&self, workspace_id: &str, event_id: &str) -> Result<Option<RuntimeEvent>> {
        sqlx::query_as::<_, RuntimeLedgerRow>(
            "SELECT id, workspace_id, ts, payload FROM hub_ledger
             WHERE workspace_id = ? AND kind = ? AND json_extract(payload, '$.eventId') = ?
             ORDER BY id LIMIT 1",
        )
        .bind(workspace_id)
        .bind(RUNTIME_EVENT_KIND)
        .bind(event_id)
        .fetch_optional(self.pool())
        .await?
        .map(decode_event)
        .transpose()
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
        let mut tx = self.pool().begin().await?;
        let cursor = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(id), 0) FROM hub_ledger WHERE workspace_id = ?")
            .bind(workspace_id)
            .fetch_one(&mut *tx)
            .await?;
        let Some(task) = crate::runtime_task::find_task(&mut tx, workspace_id, task_id)
            .await
            .map_err(internal)?
        else {
            return Ok(None);
        };
        #[cfg(any(test, feature = "testing"))]
        if let Some(probe) = &self.runtime_projection_probe {
            probe.after_task_read().await;
        }
        let runs = sqlx::query_as::<_, TaskRunProjection>(
            "SELECT id, status, outcome, summary, created_at FROM agent_runs
             WHERE workspace_id = ? AND task_id = ? ORDER BY created_at, rowid",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
        let jobs = sqlx::query_as::<_, crate::device_job::DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                    timeout_ms, started_at, ended_at
             FROM device_jobs WHERE workspace_id = ? AND task_id = ? ORDER BY started_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(DeviceJob::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(internal)?;
        let approvals = sqlx::query_as::<_, crate::task_approval::TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM task_approvals WHERE workspace_id = ? AND task_id = ? ORDER BY requested_at, id",
            crate::task_approval::APPROVAL_COLUMNS
        )))
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(tinyiothub_core::task_approval::TaskApproval::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(internal)?;
        let watch_rows = sqlx::query_as::<_, crate::watch::WatchRuleRow>(
            "SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id
             FROM watch_rules WHERE workspace_id = ? AND owner_task_id = ? ORDER BY created_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(&mut *tx)
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
        let evidence_rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                i64,
                DateTime<Utc>,
                DateTime<Utc>,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            "SELECT id, mime, source_seq, media_time, expires_at, device_id, source_id, task_id, run_id, tool_call_id, job_id
             FROM media_evidence WHERE workspace_id = ? AND task_id = ? ORDER BY created_at, id",
        )
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
        let evidence = evidence_rows
            .into_iter()
            .map(
                |(
                    evidence_id,
                    mime,
                    source_seq,
                    media_time,
                    expires_at,
                    device_id,
                    source_id,
                    task_id,
                    run_id,
                    tool_call_id,
                    job_id,
                )| {
                    Ok(TaskEvidenceProjection {
                        evidence_id,
                        mime,
                        source_seq: u64::try_from(source_seq).map_err(internal)?,
                        media_time,
                        expires_at,
                        device_id,
                        source_id,
                        task_id,
                        run_id,
                        tool_call_id,
                        job_id,
                    })
                },
            )
            .collect::<Result<Vec<_>>>()?;

        let event_rows = sqlx::query_as::<_, RuntimeLedgerRow>(
            "SELECT id, workspace_id, ts, payload FROM hub_ledger
             WHERE workspace_id = ? AND kind = ? AND json_extract(payload, '$.taskId') = ? ORDER BY id",
        )
        .bind(workspace_id)
        .bind(RUNTIME_EVENT_KIND)
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut associations = Vec::new();
        let mut association_positions = std::collections::HashMap::new();
        let mut current_values = std::collections::HashMap::new();
        for row in event_rows {
            let event = decode_event(row)?;
            if let Some(device_id) = event.device_id.as_deref() {
                add_device_reference(
                    &mut associations,
                    &mut association_positions,
                    device_id,
                    event.source_id.as_deref(),
                );
                if let Some(source_ids) = event.data.get("sourceIds").and_then(Value::as_array) {
                    for source_id in source_ids.iter().filter_map(Value::as_str) {
                        add_device_reference(
                            &mut associations,
                            &mut association_positions,
                            device_id,
                            Some(source_id),
                        );
                    }
                }
                if let Some(state) = event.data.get("state").and_then(Value::as_object) {
                    for (name, value) in state {
                        let value = match value {
                            Value::String(value) => Some(value.clone()),
                            Value::Null => None,
                            value => Some(value.to_string()),
                        };
                        if let Some(value) = value {
                            current_values.insert((device_id.to_string(), name.clone()), value);
                        }
                    }
                }
            }
        }
        // Compatibility fallback for facts written before their reference events existed.
        for job in &jobs {
            add_device_reference(&mut associations, &mut association_positions, &job.device_id, None);
        }
        for approval in &approvals {
            add_device_reference(&mut associations, &mut association_positions, &approval.device_id, None);
        }
        for watch in &watches {
            let device_id = watch
                .condition
                .get("thing_id")
                .and_then(Value::as_str)
                .or_else(|| watch.condition.get("thingId").and_then(Value::as_str));
            if let Some(device_id) = device_id {
                add_device_reference(&mut associations, &mut association_positions, device_id, None);
            }
        }
        for item in &evidence {
            add_device_reference(
                &mut associations,
                &mut association_positions,
                &item.device_id,
                Some(&item.source_id),
            );
        }

        let thing_rows = sqlx::query_as::<_, RuntimeThingFactsRow>(
            "SELECT id, state, zone_ref, position FROM things WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_all(&mut *tx)
        .await?;
        let thing_facts = thing_rows
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect::<std::collections::HashMap<_, _>>();
        let declaration_rows = sqlx::query_as::<_, (String, String)>(
            "SELECT thing_id, declaration_json FROM device_declarations WHERE workspace_id = ? AND status = 'active' ORDER BY thing_id, updated_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut declarations = std::collections::HashMap::new();
        for (device_id, raw) in declaration_rows {
            if let std::collections::hash_map::Entry::Vacant(entry) = declarations.entry(device_id)
                && let Ok(declaration) = serde_json::from_str::<DeviceDeclaration>(&raw)
            {
                entry.insert(declaration);
            }
        }
        let manifest_rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT driver_name, driver_version, manifest_json FROM driver_declarations WHERE workspace_id = ? AND status = 'active'",
        )
        .bind(workspace_id)
        .fetch_all(&mut *tx)
        .await?;
        let manifests = manifest_rows
            .into_iter()
            .filter_map(|(driver, version, raw)| {
                serde_json::from_str::<DriverManifest>(&raw)
                    .ok()
                    .map(|manifest| ((driver, version), manifest))
            })
            .collect::<std::collections::HashMap<_, _>>();
        let devices = associations
            .iter()
            .map(|association| {
                let facts = thing_facts.get(&association.device_id);
                let health = match facts.map(|facts| facts.state) {
                    Some(1) => "ok",
                    Some(0) => "attention",
                    Some(2 | 3) => "bad",
                    _ => "unknown",
                }
                .to_string();
                let declaration = declarations.get(&association.device_id);
                let sources = declaration
                    .and_then(|declaration| {
                        manifests.get(&(declaration.driver.clone(), declaration.driver_version.clone()))
                    })
                    .map(|manifest| {
                        manifest
                            .sources
                            .iter()
                            .map(|source| RuntimeDeviceSource {
                                id: source.id.clone(),
                                data_kind: source.data_kind.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selected_fields = declaration
                    .map(|declaration| {
                        let key_fields = declaration
                            .state_fields
                            .iter()
                            .filter(|field| field.key.is_some())
                            .collect::<Vec<_>>();
                        if key_fields.is_empty() {
                            declaration.state_fields.iter().take(3).collect::<Vec<_>>()
                        } else {
                            key_fields
                        }
                    })
                    .unwrap_or_default();
                let fields = selected_fields
                    .into_iter()
                    .map(|field| RuntimeDeviceField {
                        name: field.name.clone(),
                        value: current_values
                            .get(&(association.device_id.clone(), field.name.clone()))
                            .cloned(),
                        unit: field.unit.clone(),
                    })
                    .collect();
                RuntimeDeviceProjection {
                    device_id: association.device_id.clone(),
                    health,
                    zone: facts.and_then(|facts| facts.zone_ref.clone()),
                    position: facts.and_then(|facts| facts.position.clone()),
                    sources,
                    fields,
                }
            })
            .collect();
        let summary = runs
            .iter()
            .rev()
            .find(|run| run.status == "completed")
            .map(|run| run.summary.clone());
        tx.commit().await?;
        Ok(Some(TaskProjection {
            task,
            runs,
            jobs: jobs.into_iter().map(TaskJobProjection::from).collect(),
            approvals,
            watches,
            evidence,
            associations,
            devices,
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
        assert_eq!(projection.summary.as_deref(), Some("pump is healthy"));
        assert_eq!(projection.cursor, appended.cursor);
        let serialized = serde_json::to_value(&projection).expect("serialize");
        assert!(serialized.get("events").is_none());
        assert!(serialized.get("storagePath").is_none());
        assert!(serialized["evidence"][0].get("storagePath").is_none());
        assert!(serialized["evidence"][0].get("expiresAt").is_some());
        assert!(
            db.runtime_projection("ws-1", "missing")
                .await
                .expect("missing")
                .is_none()
        );
    }

    #[tokio::test]
    async fn projection_enriches_declaration_cards_and_preserves_cross_type_first_references() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        for (device_id, state, zone, position) in [
            ("dev-direct", 1, "North", "rack-1"),
            ("dev-watch", 0, "South", "rack-2"),
            ("dev-approval", 1, "West", "rack-3"),
            ("dev-job", 2, "East", "rack-4"),
            ("dev-evidence", 1, "West", "rack-5"),
        ] {
            sqlx::query(
                "INSERT INTO things (id, name, workspace_id, thing_type, state, zone_ref, position, driver_name)
                 VALUES (?, ?, 'ws-1', 'device', ?, ?, ?, 'camera')",
            )
            .bind(device_id)
            .bind(device_id)
            .bind(state)
            .bind(zone)
            .bind(position)
            .execute(db.pool())
            .await
            .expect("device");
        }
        let manifest = json!({
            "manifestVersion":"1.0",
            "identity":{"driver":"camera","version":"1.0.0","protocol":"demo","abiVersion":2},
            "config":{"type":"object"},
            "tools":[],
            "stateFields":[
                {"name":"temperature","type":"number","unit":"C","key":"primary"},
                {"name":"humidity","type":"number"}
            ],
            "sources":[
                {"id":"telemetry","kind":"poll","fields":["temperature"],"dataKind":"values"},
                {"id":"preview","kind":"poll","fields":[],"dataKind":"image"},
                {"id":"live","kind":"push","fields":[],"dataKind":"video"}
            ],
            "resources":[],"placement":{"mode":"runtime"},
            "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
        });
        db.upsert_driver_declaration(&tinyiothub_core::driver::declaration::DriverDeclarationRecord {
            workspace_id: "ws-1".into(),
            driver_name: "camera".into(),
            driver_version: "1.0.0".into(),
            manifest_version: "1.0".into(),
            manifest_json: manifest.to_string(),
            manifest_hash: "hash".into(),
            binary_hash: None,
            status: "active".into(),
            installed_at: "now".into(),
        })
        .await
        .expect("manifest");
        let declaration = tinyiothub_core::driver::declaration::DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "camera".into(),
            driver_version: "1.0.0".into(),
            state_fields: serde_json::from_value(manifest["stateFields"].clone()).expect("fields"),
            tools: vec![],
        };
        for device_id in ["dev-direct", "dev-watch", "dev-approval", "dev-job", "dev-evidence"] {
            db.upsert_device_declaration(&tinyiothub_core::driver::declaration::DeviceDeclarationRecord {
                workspace_id: "ws-1".into(),
                thing_id: device_id.into(),
                driver_name: "camera".into(),
                driver_version: "1.0.0".into(),
                declaration_json: serde_json::to_string(&declaration).expect("declaration"),
                declaration_hash: "hash".into(),
                status: "active".into(),
                updated_at: "now".into(),
            })
            .await
            .expect("device declaration");
        }

        let mut direct = event("direct-read", "task-1");
        direct.event_type = "read_device".into();
        direct.device_id = Some("dev-direct".into());
        direct.source_id = Some("telemetry".into());
        direct.data = json!({"state":{"temperature":"21.5"}});
        db.append_runtime_event(direct).await.expect("direct read");
        let mut watch = event("watch-reference", "task-1");
        watch.event_type = "watch.created".into();
        watch.device_id = Some("dev-watch".into());
        watch.data = json!({"watchId":"watch-1"});
        db.append_runtime_event(watch).await.expect("watch reference");
        let mut approval = event("approval-reference", "task-1");
        approval.event_type = "approval.requested".into();
        approval.approval_id = Some("approval-1".into());
        approval.device_id = Some("dev-approval".into());
        db.append_runtime_event(approval).await.expect("approval reference");
        db.create_job(&DeviceJob {
            id: "job-first-reference".into(),
            workspace_id: "ws-1".into(),
            device_id: "dev-job".into(),
            tool: "inspect".into(),
            args_json: json!({"mode":"brief"}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some("task-1".into()),
            run_id: None,
            tool_call_id: None,
            driver_call_id: None,
            timeout_ms: 1000,
            started_at: Utc::now(),
            ended_at: None,
        })
        .await
        .expect("job");
        let now = Utc::now();
        db.put_media_evidence(&MediaEvidenceRecord {
            id: "evidence-only".into(),
            workspace_id: "ws-1".into(),
            task_id: Some("task-1".into()),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            device_id: "dev-evidence".into(),
            source_id: "preview".into(),
            source_seq: 8,
            media_time: now,
            mime: "image/jpeg".into(),
            size_bytes: 12,
            sha256: "abc".into(),
            storage_path: "private/path".into(),
            dedup_key: "dedup-evidence-only".into(),
            created_at: now,
            expires_at: now + chrono::Duration::days(1),
        })
        .await
        .expect("evidence");

        let projection = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task");
        let device_ids = projection
            .associations
            .iter()
            .map(|association| association.device_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            device_ids,
            ["dev-direct", "dev-watch", "dev-approval", "dev-job", "dev-evidence"]
        );
        assert_eq!(projection.associations[0].source_ids, ["telemetry"]);
        assert_eq!(projection.associations[4].source_ids, ["preview"]);
        assert_eq!(projection.evidence[0].device_id, "dev-evidence");
        assert_eq!(projection.evidence[0].source_id, "preview");
        assert_eq!(projection.evidence[0].task_id.as_deref(), Some("task-1"));
        let direct = projection
            .devices
            .iter()
            .find(|device| device.device_id == "dev-direct")
            .expect("facts");
        assert_eq!(direct.health, "ok");
        assert_eq!(direct.zone.as_deref(), Some("North"));
        assert_eq!(direct.position.as_deref(), Some("rack-1"));
        assert_eq!(
            direct
                .sources
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            ["telemetry", "preview", "live"]
        );
        assert_eq!(direct.fields.len(), 1);
        assert_eq!(direct.fields[0].name, "temperature");
        assert_eq!(direct.fields[0].value.as_deref(), Some("21.5"));
        let persisted = db
            .runtime_event_by_id("ws-1", "evidence:evidence-only:created")
            .await
            .expect("event")
            .expect("persisted");
        assert_eq!(persisted.event_type, "evidence.created");
        assert_eq!(persisted.device_id.as_deref(), Some("dev-evidence"));
        assert_eq!(persisted.source_id.as_deref(), Some("preview"));
        assert_eq!(persisted.evidence_id.as_deref(), Some("evidence-only"));
        assert!(persisted.data.get("storagePath").is_none());
    }

    #[tokio::test]
    async fn live_device_events_carry_authoritative_declaration_and_state_facts() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        sqlx::query(
            "INSERT INTO things (id, name, workspace_id, thing_type, state, zone_ref, position, driver_name)
             VALUES ('dev-live', 'dev-live', 'ws-1', 'device', 1, 'North', 'rack-1', 'camera')",
        )
        .execute(db.pool())
        .await
        .expect("device");
        let manifest = json!({
            "manifestVersion":"1.0",
            "identity":{"driver":"camera","version":"1.0.0","protocol":"demo","abiVersion":2},
            "config":{"type":"object"},
            "tools":[],
            "stateFields":[
                {"name":"temperature","type":"number","unit":"C","key":"primary"},
                {"name":"humidity","type":"number"}
            ],
            "sources":[
                {"id":"telemetry","kind":"poll","fields":["temperature"],"dataKind":"values"},
                {"id":"preview","kind":"poll","fields":[],"dataKind":"image"},
                {"id":"live","kind":"push","fields":[],"dataKind":"video"}
            ],
            "resources":[],"placement":{"mode":"runtime"},
            "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
        });
        db.upsert_driver_declaration(&tinyiothub_core::driver::declaration::DriverDeclarationRecord {
            workspace_id: "ws-1".into(),
            driver_name: "camera".into(),
            driver_version: "1.0.0".into(),
            manifest_version: "1.0".into(),
            manifest_json: manifest.to_string(),
            manifest_hash: "hash".into(),
            binary_hash: None,
            status: "active".into(),
            installed_at: "now".into(),
        })
        .await
        .expect("manifest");
        let declaration = tinyiothub_core::driver::declaration::DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "camera".into(),
            driver_version: "1.0.0".into(),
            state_fields: serde_json::from_value(manifest["stateFields"].clone()).expect("fields"),
            tools: vec![],
        };
        db.upsert_device_declaration(&tinyiothub_core::driver::declaration::DeviceDeclarationRecord {
            workspace_id: "ws-1".into(),
            thing_id: "dev-live".into(),
            driver_name: "camera".into(),
            driver_version: "1.0.0".into(),
            declaration_json: serde_json::to_string(&declaration).expect("declaration"),
            declaration_hash: "hash".into(),
            status: "active".into(),
            updated_at: "now".into(),
        })
        .await
        .expect("device declaration");

        let mut job_reference = event("job-live-reference", "task-1");
        job_reference.event_type = "job.accepted".into();
        job_reference.device_id = Some("dev-live".into());
        job_reference.data = json!({"state":"accepted","tool":"inspect"});
        let first = db.append_runtime_event(job_reference).await.expect("job reference");
        let facts = first.data.get("deviceFacts").expect("live event carries device facts");
        assert_eq!(facts["deviceId"], "dev-live");
        assert_eq!(facts["health"], "ok");
        assert_eq!(facts["zone"], "North");
        assert_eq!(facts["position"], "rack-1");
        assert_eq!(
            facts["sources"]
                .as_array()
                .expect("sources")
                .iter()
                .map(|source| source["id"].as_str())
                .collect::<Vec<_>>(),
            [Some("telemetry"), Some("preview"), Some("live")]
        );
        assert_eq!(facts["fields"][0]["name"], "temperature");
        assert_eq!(facts["fields"][0]["value"], serde_json::Value::Null);

        let mut read = event("live-read", "task-1");
        read.event_type = "read_device".into();
        read.device_id = Some("dev-live".into());
        read.source_id = Some("telemetry".into());
        read.data = json!({
            "sourceIds":["telemetry","preview"],
            "state":{"temperature":"21.5","unselected":"kept-private"},
        });
        let second = db.append_runtime_event(read).await.expect("live read");
        let fields = second.data["deviceFacts"]["fields"].as_array().expect("fields");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "temperature");
        assert_eq!(fields[0]["value"], "21.5");
        assert_eq!(fields[0]["unit"], "C");
        assert!(!second.data["deviceFacts"].to_string().contains("kept-private"));

        let persisted = db
            .runtime_event_by_id("ws-1", "live-read")
            .await
            .expect("event")
            .expect("persisted");
        assert_eq!(persisted, second);
        let projection = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task");
        let projected = serde_json::to_value(
            projection
                .devices
                .iter()
                .find(|device| device.device_id == "dev-live")
                .expect("projected device"),
        )
        .expect("projected facts");
        assert_eq!(second.data["deviceFacts"], projected);
    }

    #[tokio::test]
    async fn legacy_watch_only_associations_accept_production_thing_id_condition() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        for (id, condition, created_at) in [
            (
                "watch-legacy",
                json!({"type":"availability","thing_id":"dev-legacy","online":true}),
                "2026-10-27T10:00:00Z",
            ),
            (
                "watch-camel",
                json!({"type":"availability","thingId":"dev-camel","online":true}),
                "2026-10-27T10:00:01Z",
            ),
        ] {
            sqlx::query(
                "INSERT INTO watch_rules (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
                 VALUES (?, 'ws-1', 'availability', ?, 1, 60, ?, 'task-1')",
            )
            .bind(id)
            .bind(condition.to_string())
            .bind(created_at)
            .execute(db.pool())
            .await
            .expect("watch rule");
        }

        let projection = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task");
        assert_eq!(
            projection
                .associations
                .iter()
                .map(|association| association.device_id.as_str())
                .collect::<Vec<_>>(),
            ["dev-legacy", "dev-camel"]
        );
    }

    #[tokio::test]
    async fn projection_snapshots_exclude_history_while_cursor_replay_paginates() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        for index in 0..300 {
            db.append_runtime_event(event(&format!("history-{index}"), "task-1"))
                .await
                .expect("append history");
        }

        let projection = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task");
        let snapshot_json = serde_json::to_value(&projection).expect("serialize snapshot");
        assert!(snapshot_json.get("events").is_none());
        assert!(!snapshot_json.to_string().contains("history-299"));

        let mut replayed = Vec::new();
        let mut after = 0;
        loop {
            let page = db
                .runtime_events_after("ws-1", Some(after), Some("task-1"), 64)
                .await
                .expect("history page");
            let count = page.len();
            let next_cursor = page.last().map(|event| event.cursor);
            replayed.extend(page.into_iter().map(|event| event.event_id));
            if count < 64 {
                break;
            }
            after = next_cursor.expect("full page has a cursor");
        }
        assert_eq!(replayed.len(), 301);
        assert_eq!(replayed.last().map(String::as_str), Some("history-299"));
    }

    #[tokio::test]
    async fn projection_uses_one_consistent_snapshot_during_sustained_unrelated_writes() {
        let path = std::env::temp_dir().join(format!("tinyiothub-projection-{}.db", uuid::Uuid::new_v4()));
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .expect("wal pool");
        crate::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        seed_task(&db, "ws-1", "task-1").await;
        let cursor = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("initial projection")
            .expect("task")
            .cursor;
        let probe = std::sync::Arc::new(crate::runtime_event::RuntimeProjectionProbe::default());
        let db = db.with_runtime_projection_probe(probe.clone());
        probe.arm();
        let projecting = tokio::spawn({
            let db = db.clone();
            async move { db.runtime_projection("ws-1", "task-1").await }
        });
        probe.wait_for_task_read().await;
        for index in 0..64 {
            db.append_runtime_event(event(&format!("unrelated-{index}"), "other-task"))
                .await
                .expect("unrelated write");
        }
        probe.release();
        let projection = tokio::time::timeout(std::time::Duration::from_secs(1), projecting)
            .await
            .expect("projection completes")
            .expect("join")
            .expect("projection")
            .expect("task");
        assert_eq!(probe.attempts(), 1);
        assert_eq!(projection.cursor, cursor);
        assert_eq!(projection.task.status, TaskStatus::Active);
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
