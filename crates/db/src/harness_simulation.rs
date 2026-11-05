//! Isolated simulation Tasks/Runs (Workflow dry-run, X2/XD3).
//!
//! A simulation Task never owns an Effect or Decision: the schema triggers in
//! `20261103000007_harness_simulation.sql` abort those writes before a prepared
//! Effect can exist, and simulation never enqueues a production wake. All read
//! helpers here are filtered to `execution_mode = 'simulation'`; production
//! projections elsewhere filter to `execution_mode = 'production'`.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{SqlSafeStr, SqliteConnection};
use tinyiothub_core::harness::{DeviceFactsSnapshot, ExecutionMode};

use crate::{
    Db,
    error::{DbError, Result},
};

/// One selectable historical stamped device-fact snapshot. `seq` is the
/// workspace event sequence that carries the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFactSnapshotRef {
    pub seq: i64,
    pub observed_at: DateTime<Utc>,
    pub declaration_hash: String,
    pub driver_version: String,
}

/// A resolved snapshot plus its picker identity.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDeviceFactSnapshot {
    pub reference: DeviceFactSnapshotRef,
    pub snapshot: DeviceFactsSnapshot,
}

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000007,
        "harness_simulation".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000007_harness_simulation.sql").into_sql_str(),
        false,
    )
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn digest(payload: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(payload)?)))
}

fn simulation_event(
    workspace_id: &str,
    task_id: &str,
    run_id: &str,
    event_id: String,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    payload_json: Value,
) -> Result<tinyiothub_core::harness::HarnessEventInput> {
    Ok(tinyiothub_core::harness::HarnessEventInput {
        event_id,
        workspace_id: workspace_id.to_string(),
        session_id: format!("task:{task_id}/run:{run_id}"),
        execution_mode: ExecutionMode::Simulation,
        run_id: Some(run_id.to_string()),
        task_id: Some(task_id.to_string()),
        event_type: event_type.to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: digest(&payload_json)?,
        payload_json,
    })
}

async fn insert_event(
    connection: &mut SqliteConnection,
    event: &tinyiothub_core::harness::HarnessEventInput,
) -> Result<()> {
    crate::harness_effect::insert_event(connection, event).await
}

#[derive(sqlx::FromRow)]
struct SimulationTaskRow {
    id: String,
    run_id: Option<String>,
    state: String,
    workflow_id: Option<String>,
    workflow_version: Option<i64>,
    execution_mode: String,
    definition_json: Option<String>,
}

impl Db {
    /// Historical stamped snapshots available to the dry-run picker. Only
    /// production events qualify: a simulation must not seed another
    /// simulation from its own history.
    pub async fn list_device_fact_snapshots(
        &self,
        workspace_id: &str,
        limit: i64,
    ) -> Result<Vec<DeviceFactSnapshotRef>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let rows: Vec<(i64, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT seq,
                    json_extract(payload_json, '$.deviceFacts.observedAt'),
                    json_extract(payload_json, '$.deviceFacts.declarationHash'),
                    json_extract(payload_json, '$.deviceFacts.driverVersion')
             FROM session_events
             WHERE execution_mode = 'production'
               AND json_extract(payload_json, '$.deviceFacts') IS NOT NULL
             ORDER BY seq DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(&pool)
        .await?;
        let mut snapshots = Vec::new();
        for (seq, observed_at, declaration_hash, driver_version) in rows {
            let (Some(observed_at), Some(declaration_hash), Some(driver_version)) =
                (observed_at, declaration_hash, driver_version)
            else {
                continue;
            };
            let Ok(observed_at) = DateTime::parse_from_rfc3339(&observed_at) else {
                continue;
            };
            snapshots.push(DeviceFactSnapshotRef {
                seq,
                observed_at: observed_at.with_timezone(&Utc),
                declaration_hash,
                driver_version,
            });
        }
        Ok(snapshots)
    }

    /// Resolve exactly one production snapshot by event sequence. Missing or
    /// simulation-mode snapshots are absent; malformed payloads are a loud
    /// validation error rather than a silent fallback.
    pub async fn device_fact_snapshot(
        &self,
        workspace_id: &str,
        seq: i64,
    ) -> Result<Option<ResolvedDeviceFactSnapshot>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT execution_mode, payload_json FROM session_events WHERE seq = ?")
                .bind(seq)
                .fetch_optional(&pool)
                .await?;
        let Some((execution_mode, payload_json)) = row else {
            return Ok(None);
        };
        if execution_mode != ExecutionMode::Production.as_str() {
            return Ok(None);
        }
        let payload: Value = serde_json::from_str(&payload_json)?;
        let Some(facts) = payload.get("deviceFacts") else {
            return Err(DbError::Validation {
                message: format!("snapshot {seq} is unreadable: missing deviceFacts"),
            });
        };
        let snapshot: DeviceFactsSnapshot =
            serde_json::from_value(facts.clone()).map_err(|error| DbError::Validation {
                message: format!("snapshot {seq} is unreadable: {error}"),
            })?;
        Ok(Some(ResolvedDeviceFactSnapshot {
            reference: DeviceFactSnapshotRef {
                seq,
                observed_at: snapshot.observed_at,
                declaration_hash: snapshot.declaration_hash.clone(),
                driver_version: snapshot.driver_version.clone(),
            },
            snapshot,
        }))
    }

    /// Create the simulation Task/Run pair and its self-contained
    /// `run.started` event. No wake, occurrence, parent coordination, Effect,
    /// or Decision is written.
    #[allow(clippy::too_many_arguments)] // the event needs the full stamped identity
    pub async fn create_simulation_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        run_id: &str,
        workflow_id: &str,
        workflow_version: i64,
        snapshot_seq: i64,
        snapshot: &DeviceFactsSnapshot,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO agent_tasks
                (id, run_id, state, state_version, workflow_id, workflow_version, parent_task_id, execution_mode)
             VALUES (?, ?, 'running', 1, ?, ?, NULL, 'simulation')",
        )
        .bind(task_id)
        .bind(run_id)
        .bind(workflow_id)
        .bind(workflow_version)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO agent_runs (id, task_id, state, parent_run_id, execution_mode)
             VALUES (?, ?, 'running', NULL, 'simulation')",
        )
        .bind(run_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await?;
        let payload = json!({
            "workflowId": workflow_id,
            "workflowVersion": workflow_version,
            "snapshotSeq": snapshot_seq,
            "snapshotObservedAt": timestamp(snapshot.observed_at),
            "deviceFacts": snapshot,
        });
        let event = simulation_event(
            workspace_id,
            task_id,
            run_id,
            format!("simulation:{task_id}:run.started"),
            "run.started",
            now,
            payload,
        )?;
        insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Persist a pre-model capability failure using the same event type and
    /// recovery copy as production. No model call, Effect, or Decision follows.
    pub async fn record_simulation_capability_unavailable(
        &self,
        workspace_id: &str,
        task_id: &str,
        run_id: &str,
        missing: &[String],
        tool: &str,
        recovery: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let payload = json!({
            "missingCapabilities": missing,
            "tool": tool,
            "recovery": recovery,
        });
        let event = simulation_event(
            workspace_id,
            task_id,
            run_id,
            format!("capability-unavailable:{run_id}:{tool}"),
            "capability.unavailable",
            now,
            payload,
        )?;
        insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Finish a simulation Task/Run. This never enqueues a parent wake and
    /// never touches production Task/Effect state.
    pub async fn complete_simulation_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        run_id: &str,
        status: &str,
        outcome: &Value,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        if !matches!(status, "completed" | "failed") {
            return Err(DbError::Validation {
                message: format!("invalid simulation outcome status {status}"),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND execution_mode = 'simulation' AND state = 'running'",
        )
        .bind(status)
        .bind(task_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE agent_runs SET state = ? WHERE id = ? AND task_id = ? AND execution_mode = 'simulation'")
            .bind(status)
            .bind(run_id)
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        let payload = json!({"taskId": task_id, "outcome": outcome});
        let event = simulation_event(
            workspace_id,
            task_id,
            run_id,
            format!("task:{task_id}:outcome"),
            "task.outcome",
            now,
            payload,
        )?;
        insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Read-only generic Run projection for one simulation Task. Production
    /// Task IDs are never readable through this path.
    pub async fn simulation_run_projection(&self, workspace_id: &str, task_id: &str) -> Result<Option<Value>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let task = sqlx::query_as::<_, SimulationTaskRow>(
            "SELECT t.id, t.run_id, t.state, t.workflow_id, t.workflow_version, t.execution_mode,
                    r.definition_json
             FROM agent_tasks t
             LEFT JOIN workflow_revisions r
               ON r.workflow_id = t.workflow_id AND r.version = t.workflow_version
             WHERE t.id = ?",
        )
        .bind(task_id)
        .fetch_optional(&pool)
        .await?;
        let Some(task) = task else {
            return Ok(None);
        };
        if task.execution_mode != ExecutionMode::Simulation.as_str() {
            return Ok(None);
        }
        let run_state: Option<String> =
            sqlx::query_scalar("SELECT state FROM agent_runs WHERE task_id = ? AND execution_mode = 'simulation'")
                .bind(task_id)
                .fetch_optional(&pool)
                .await?;
        let events: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT seq, type, occurred_at, payload_json FROM session_events
             WHERE execution_mode = 'simulation' AND task_id = ? ORDER BY seq",
        )
        .bind(task_id)
        .fetch_all(&pool)
        .await?;

        let mut steps = Vec::new();
        let mut errors = Vec::new();
        let mut evidence: Vec<String> = Vec::new();
        let mut outcome: Option<Value> = None;
        let mut snapshot = json!({
            "seq": Value::Null,
            "observedAt": Value::Null,
            "declarationHash": Value::Null,
            "driverVersion": Value::Null,
        });
        for (seq, event_type, occurred_at, raw) in &events {
            let payload: Value = serde_json::from_str(raw)?;
            if *event_type == "run.started" {
                snapshot = json!({
                    "seq": payload.get("snapshotSeq").cloned().unwrap_or(Value::Null),
                    "observedAt": payload.get("snapshotObservedAt").cloned().unwrap_or(Value::Null),
                    "declarationHash": payload.pointer("/deviceFacts/declarationHash").cloned().unwrap_or(Value::Null),
                    "driverVersion": payload.pointer("/deviceFacts/driverVersion").cloned().unwrap_or(Value::Null),
                });
            }
            if *event_type == "task.outcome" {
                outcome = payload.get("outcome").cloned();
            } else {
                steps.push(json!({
                    "id": format!("{seq}:{event_type}"),
                    "name": event_type,
                    "args": {"seq": seq},
                    "output": raw,
                }));
            }
            if *event_type == "capability.unavailable" {
                let missing = payload
                    .get("missingCapabilities")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                let recovery = payload
                    .get("recovery")
                    .and_then(Value::as_str)
                    .unwrap_or("配置具备相应能力的 provider 后创建新的 Run");
                errors.push(json!({
                    "type": event_type,
                    "message": format!("capability.unavailable: 缺少能力 [{missing}]；{recovery}"),
                    "occurredAt": occurred_at,
                }));
            }
            collect_evidence(&payload, &mut evidence);
        }
        evidence.sort();
        evidence.dedup();

        let objective = task.definition_json.as_deref().and_then(|raw| {
            serde_json::from_str::<Value>(raw)
                .ok()
                .and_then(|definition| definition.get("objective").and_then(Value::as_str).map(str::to_string))
        });
        let status = if task.state == "running" {
            run_state.clone().unwrap_or(task.state.clone())
        } else {
            task.state.clone()
        };
        Ok(Some(json!({
            "executionMode": "simulation",
            "readOnly": true,
            "taskId": task.id,
            "runId": task.run_id,
            "status": status,
            "workflowId": task.workflow_id,
            "workflowVersion": task.workflow_version,
            "objective": objective,
            "snapshot": snapshot,
            "steps": steps,
            "evidence": evidence,
            "errors": errors,
            "outcome": outcome,
        })))
    }
}

fn collect_evidence(payload: &Value, evidence: &mut Vec<String>) {
    for value in payload
        .get("evidenceRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        evidence.push(value.to_string());
    }
}
