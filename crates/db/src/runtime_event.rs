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

/// Exact production page query. Shared with the EXPLAIN regression test so an
/// edit that makes the bounded page non-indexable cannot pass unnoticed.
const RUNTIME_EVENTS_PAGE_SQL: &str = "SELECT id, workspace_id, ts, payload FROM hub_ledger
             WHERE workspace_id = ? AND kind = ?
               AND (? IS NULL OR id < ?)
               AND json_extract(payload, '$.taskId') = ?
             ORDER BY id DESC LIMIT ?";

/// Exact event-id dedupe predicate used inside `append_runtime_event_tx`.
const RUNTIME_EVENT_DEDUPE_SQL: &str = "SELECT 1 FROM hub_ledger
             WHERE workspace_id = ? AND kind = ?
               AND json_extract(payload, '$.eventId') = ?";

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

/// One bounded timeline page (ascending cursor) plus whether older rows exist.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEventPage {
    pub events: Vec<RuntimeEvent>,
    pub has_more: bool,
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

/// Legacy platform Job event builder. The production Job writer was removed in
/// Task 19C3; only the test-only platform fixture store still emits these.
#[cfg(any(test, feature = "testing"))]
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
    state: i32,
    zone_ref: Option<String>,
    position: Option<String>,
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
        "SELECT state, zone_ref, position FROM things WHERE workspace_id = ? AND id = ?",
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
        // Incremental projection first: the append transaction applies only the
        // new event's state delta, so this read is a single row instead of a
        // full replay of the Task's ledger history. The replay remains as a
        // one-time migration fallback and seeds the projection afterwards.
        let projected: Option<String> = sqlx::query_scalar(
            "SELECT state_json FROM runtime_device_state
             WHERE workspace_id = ? AND task_id = ? AND device_id = ?",
        )
        .bind(workspace_id)
        .bind(task_id)
        .bind(device_id)
        .fetch_optional(&mut *connection)
        .await?;
        let state = match projected {
            Some(raw) => serde_json::from_str::<Value>(&raw).ok(),
            None => {
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
                let mut merged = serde_json::Map::new();
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
                                merged.insert(name.clone(), Value::String(value));
                            }
                        }
                    }
                }
                // Seed even an empty projection: a device whose first events
                // carry only null values still needs a row, otherwise every new
                // event replays the Task's whole ledger and `merge_device_state_delta`
                // keeps treating the projection as unseeded. Task lifecycle is a
                // workspace Harness fact (Task 19C4), so this telemetry writer
                // never reads the retired platform `agent_tasks` table.
                sqlx::query(
                    "INSERT INTO runtime_device_state
                         (workspace_id, task_id, device_id, state_json, updated_at)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT(workspace_id, task_id, device_id)
                     DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
                )
                .bind(workspace_id)
                .bind(task_id)
                .bind(device_id)
                .bind(Value::Object(merged.clone()).to_string())
                .bind(Utc::now().to_rfc3339())
                .execute(&mut *connection)
                .await?;
                Some(Value::Object(merged))
            }
        };
        if let Some(state) = state.as_ref().and_then(Value::as_object) {
            for (name, value) in state {
                if let Some(value) = value.as_str() {
                    current_values.insert((device_id.to_string(), name.clone()), value.to_string());
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

/// Apply the new event's `data.state` delta to the incremental projection.
/// A missing projection is left to `load_device_facts`, which replays the
/// historical ledger once and seeds the row.
async fn merge_device_state_delta(connection: &mut sqlx::SqliteConnection, event: &RuntimeEvent) -> Result<()> {
    let (Some(task_id), Some(device_id)) = (event.task_id.as_deref(), event.device_id.as_deref()) else {
        return Ok(());
    };
    let Some(state) = event.data.get("state").and_then(Value::as_object) else {
        return Ok(());
    };
    let projected: Option<String> = sqlx::query_scalar(
        "SELECT state_json FROM runtime_device_state
         WHERE workspace_id = ? AND task_id = ? AND device_id = ?",
    )
    .bind(&event.workspace_id)
    .bind(task_id)
    .bind(device_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(projected) = projected else {
        return Ok(());
    };
    let mut merged = serde_json::from_str::<Value>(&projected)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    for (name, value) in state {
        match value {
            Value::String(value) => {
                merged.insert(name.clone(), Value::String(value.clone()));
            }
            Value::Null => {}
            value => {
                merged.insert(name.clone(), Value::String(value.to_string()));
            }
        }
    }
    sqlx::query(
        "INSERT INTO runtime_device_state
             (workspace_id, task_id, device_id, state_json, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(workspace_id, task_id, device_id)
         DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
    )
    .bind(&event.workspace_id)
    .bind(task_id)
    .bind(device_id)
    .bind(Value::Object(merged).to_string())
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *connection)
    .await?;
    Ok(())
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
    let insert_sql = format!(
        "INSERT INTO hub_ledger (workspace_id, ts, kind, thing_id, payload)
         SELECT ?, ?, ?, ?, ?
         WHERE NOT EXISTS (
             {RUNTIME_EVENT_DEDUPE_SQL}
         )
         RETURNING id, workspace_id, ts, payload"
    );
    let inserted = sqlx::query_as::<_, RuntimeLedgerRow>(sqlx::AssertSqlSafe(insert_sql))
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
        merge_device_state_delta(&mut *connection, &event).await?;
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

    /// Bounded newest-first page of one Task's persisted events, returned in
    /// ascending cursor order. `before` is exclusive; `limit` is clamped to
    /// `1..=200` so callers can never replay the whole ledger in one query.
    pub async fn runtime_events_page(
        &self,
        workspace_id: &str,
        task_id: &str,
        before: Option<i64>,
        limit: i64,
    ) -> Result<RuntimeEventPage> {
        let limit = limit.clamp(1, 200);
        let mut rows = sqlx::query_as::<_, RuntimeLedgerRow>(RUNTIME_EVENTS_PAGE_SQL)
            .bind(workspace_id)
            .bind(RUNTIME_EVENT_KIND)
            .bind(before)
            .bind(before)
            .bind(task_id)
            .bind(limit + 1)
            .fetch_all(self.pool())
            .await?;
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let events = rows.into_iter().map(decode_event).collect::<Result<Vec<_>>>()?;
        let events = events.into_iter().rev().collect();
        Ok(RuntimeEventPage { events, has_more })
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
}
#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use sqlx::SqlitePool;

    use crate::Db;
    use crate::runtime_event::{RUNTIME_EVENT_DEDUPE_SQL, RUNTIME_EVENTS_PAGE_SQL, RuntimeEventInput};

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
    async fn task_page_and_event_dedupe_queries_use_their_bounded_indexes() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let page = sqlx::query_as::<_, (i64, i64, i64, String)>(sqlx::AssertSqlSafe(format!(
            "EXPLAIN QUERY PLAN {RUNTIME_EVENTS_PAGE_SQL}"
        )))
        .bind("ws-1")
        .bind(super::RUNTIME_EVENT_KIND)
        .bind(None::<i64>)
        .bind(None::<i64>)
        .bind("task-1")
        .bind(51_i64)
        .fetch_all(&pool)
        .await
        .expect("page query plan")
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>();
        assert!(
            page.iter()
                .any(|detail| detail.contains("idx_hub_ledger_runtime_task_page")),
            "{page:?}"
        );
        let dedupe = sqlx::query_as::<_, (i64, i64, i64, String)>(sqlx::AssertSqlSafe(format!(
            "EXPLAIN QUERY PLAN {RUNTIME_EVENT_DEDUPE_SQL}"
        )))
        .bind("ws-1")
        .bind(super::RUNTIME_EVENT_KIND)
        .bind("event-1")
        .fetch_all(&pool)
        .await
        .expect("dedupe query plan")
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>();
        assert!(
            dedupe
                .iter()
                .any(|detail| detail.contains("idx_hub_ledger_runtime_event_id_full")),
            "{dedupe:?}"
        );
    }

    #[tokio::test]
    async fn null_only_state_seeds_the_device_projection() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);

        let mut read = event("null-state", "task-1");
        read.event_type = "read_device".into();
        read.device_id = Some("dev-null".into());
        read.data = json!({"state":{"temperature":null}});
        db.append_runtime_event(read).await.expect("append");

        let state: String = sqlx::query_scalar(
            "SELECT state_json FROM runtime_device_state
             WHERE workspace_id = 'ws-1' AND task_id = 'task-1' AND device_id = 'dev-null'",
        )
        .fetch_one(db.pool())
        .await
        .expect("seeded projection");
        assert_eq!(state, "{}");

        // The follow-up delta must update the seeded row instead of replaying
        // the ledger from scratch.
        let mut next = event("null-state-next", "task-1");
        next.event_type = "read_device".into();
        next.device_id = Some("dev-null".into());
        next.data = json!({"state":{"temperature":"22"}});
        db.append_runtime_event(next).await.expect("next append");
        let state: String = sqlx::query_scalar(
            "SELECT state_json FROM runtime_device_state
             WHERE workspace_id = 'ws-1' AND task_id = 'task-1' AND device_id = 'dev-null'",
        )
        .fetch_one(db.pool())
        .await
        .expect("updated projection");
        assert_eq!(state, r#"{"temperature":"22"}"#);
    }

    #[tokio::test]
    async fn live_device_events_carry_authoritative_declaration_and_state_facts() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
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

        // Incremental projection: a later state-only event merges into the
        // stored projection rather than replaying (and trusting) all history.
        // Corrupting the older ledger payload makes the two paths observably
        // different: replay would report "poisoned", the projection keeps 21.5.
        sqlx::query(
            "UPDATE hub_ledger SET payload = json_set(payload, '$.data.state.temperature', 'poisoned')
             WHERE workspace_id = 'ws-1' AND kind = 'runtime.v1'
               AND json_extract(payload, '$.eventId') = 'live-read'",
        )
        .execute(db.pool())
        .await
        .expect("mutate historical payload");
        let mut delta = event("live-humidity-only", "task-1");
        delta.event_type = "read_device".into();
        delta.device_id = Some("dev-live".into());
        delta.source_id = Some("telemetry".into());
        delta.data = json!({"state":{"humidity":"40"}});
        let third = db.append_runtime_event(delta).await.expect("incremental delta");
        let fields = third.data["deviceFacts"]["fields"].as_array().expect("fields");
        assert_eq!(fields[0]["name"], "temperature");
        assert_eq!(
            fields[0]["value"], "21.5",
            "device facts must merge into the incremental projection, not replay history"
        );
    }

    #[tokio::test]
    async fn live_facts_apply_the_newest_declaration_tie_break() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        for (driver, field) in [("camera_a", "temperature"), ("camera_b", "humidity")] {
            let declaration = json!({
                "manifestVersion":"1.0",
                "driver":driver,
                "driverVersion":"1.0.0",
                "stateFields":[{"name":field,"type":"number","key":"primary"}],
                "tools":[],
            });
            sqlx::query(
                "INSERT INTO device_declarations
                     (workspace_id, thing_id, driver_name, driver_version, declaration_json,
                      declaration_hash, status, updated_at)
                 VALUES ('ws-1', 'dev-tie', ?, '1.0.0', ?, 'hash', 'active', '2026-10-29T00:00:00Z')",
            )
            .bind(driver)
            .bind(declaration.to_string())
            .execute(db.pool())
            .await
            .expect("declaration");
        }
        let mut read = event("tie-read", "task-1");
        read.event_type = "read_device".into();
        read.device_id = Some("dev-tie".into());
        read.source_id = Some("telemetry".into());
        read.data = json!({"state":{"temperature":"21","humidity":"40"}});
        let live = db.append_runtime_event(read).await.expect("live read");
        let live_fields = live.data["deviceFacts"]["fields"].as_array().expect("fields");
        assert_eq!(
            live_fields[0]["name"], "humidity",
            "the newest declaration id wins the tie-break"
        );
        assert_eq!(live_fields[0]["value"], "40");
    }

    #[tokio::test]
    async fn cursor_replay_paginates_without_a_preloaded_snapshot() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws-1").await;
        let db = Db::new(pool);
        for index in 0..300 {
            db.append_runtime_event(event(&format!("history-{index}"), "task-1"))
                .await
                .expect("append history");
        }

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
        assert_eq!(replayed.len(), 300);
        assert_eq!(replayed.last().map(String::as_str), Some("history-299"));
    }
}
