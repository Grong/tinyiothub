//! Isolated Workflow dry-run composition (X2/XD3, DR7=A).
//!
//! The composition root deliberately owns no device worker, driver dispatcher,
//! Effect sender, or wake outbox: a simulation can only resolve a fixed
//! Workflow revision, a caller-chosen historical snapshot, and one executor.
//! The workspace schema independently rejects simulation Effect/Decision/wake
//! writes. Production lists, recovery, and Main projections filter to
//! `execution_mode = 'production'`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tinyiothub_agent::harness::{SubagentExecutor, SubagentInput, TaskOutcome};
use tinyiothub_skills::registry::Capability;
use tinyiothub_storage::Db;
use tinyiothub_storage::DbError;
use tinyiothub_storage::harness_simulation::DeviceFactSnapshotRef;

use crate::domains::agent::tools::CAPABILITY_UNAVAILABLE_RECOVERY;

use super::runner::SubagentRunner;

/// Fixed dry-run identity: one revision plus one explicitly chosen
/// workspace-event snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunInput {
    pub workflow_id: String,
    pub workflow_version: i64,
    pub snapshot_seq: i64,
}

/// The isolated simulation Task/Run pair returned to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunResult {
    pub task_id: String,
    pub run_id: String,
}

/// Deterministic simulation provider. It may only surface values that are
/// already in the stamped snapshot; it never invents an observation and never
/// receives a device worker or Effect sender.
#[derive(Debug, Default)]
pub struct ScriptedDryRunExecutor;

#[async_trait]
impl SubagentExecutor for ScriptedDryRunExecutor {
    async fn execute(&self, input: SubagentInput) -> anyhow::Result<TaskOutcome> {
        let observations = input
            .facts
            .facts
            .as_object()
            .map(|facts| {
                facts
                    .iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let snapshot_seq = input
            .source
            .get("snapshotSeq")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        Ok(TaskOutcome {
            status: "completed".to_string(),
            summary: format!("模拟运行完成：{}", input.workflow.definition.objective),
            observations,
            evidence_refs: vec![format!("snapshot:{snapshot_seq}")],
            recommended_actions: input.workflow.definition.tool_ids.clone(),
        })
    }
}

#[derive(Clone)]
pub struct WorkflowDryRun {
    db: Arc<Db>,
    workspace_id: String,
    executor: Arc<dyn SubagentExecutor>,
    available_capabilities: Vec<Capability>,
}

impl WorkflowDryRun {
    pub fn new(
        db: Arc<Db>,
        workspace_id: impl Into<String>,
        executor: Arc<dyn SubagentExecutor>,
        available_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
            executor,
            available_capabilities,
        }
    }

    /// Snapshot picker entries for one fixed revision. A missing revision is
    /// NotFound before any snapshot is listed.
    pub async fn list_snapshots(
        &self,
        workflow_id: &str,
        workflow_version: i64,
        limit: i64,
    ) -> Result<Vec<DeviceFactSnapshotRef>, DbError> {
        self.db
            .workflow_revision(&self.workspace_id, workflow_id, workflow_version)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: format!("{workflow_id}@{workflow_version}"),
            })?;
        self.db.list_device_fact_snapshots(&self.workspace_id, limit).await
    }

    /// Start one isolated simulation. A missing/unreadable snapshot or
    /// revision fails before any Task/Run/event is written.
    pub async fn start(&self, input: DryRunInput) -> Result<DryRunResult, DbError> {
        let revision = self
            .db
            .workflow_revision(&self.workspace_id, &input.workflow_id, input.workflow_version)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: format!("{}@{}", input.workflow_id, input.workflow_version),
            })?;
        let resolved = self
            .db
            .device_fact_snapshot(&self.workspace_id, input.snapshot_seq)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: format!("snapshot:{}", input.snapshot_seq),
            })?;

        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        let run_id = format!("run-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        self.db
            .create_simulation_task(
                &self.workspace_id,
                &task_id,
                &run_id,
                &revision.workflow_id,
                revision.version,
                input.snapshot_seq,
                &resolved.snapshot,
                now,
            )
            .await?;

        let missing = missing_capabilities(&revision.definition.required_capabilities, &self.available_capabilities);
        if !missing.is_empty() {
            let tool = revision
                .definition
                .tool_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "workflow".to_string());
            self.db
                .record_simulation_capability_unavailable(
                    &self.workspace_id,
                    &task_id,
                    &run_id,
                    &missing,
                    &tool,
                    CAPABILITY_UNAVAILABLE_RECOVERY,
                    now,
                )
                .await?;
            let outcome = json!({
                "status": "failed",
                "summary": format!("缺少能力: {}", missing.join(", ")),
                "observations": [],
                "evidenceRefs": [],
                "recommendedActions": [CAPABILITY_UNAVAILABLE_RECOVERY],
            });
            self.db
                .complete_simulation_task(&self.workspace_id, &task_id, &run_id, "failed", &outcome, now)
                .await?;
            return Ok(DryRunResult { task_id, run_id });
        }

        let runner = SubagentRunner::new(self.executor.clone());
        let executed = runner
            .run(SubagentInput {
                session_id: String::new(),
                task_id: task_id.clone(),
                run_id: run_id.clone(),
                workflow: revision,
                facts: resolved.snapshot,
                source: json!({
                    "snapshotSeq": resolved.reference.seq,
                    "observedAt": resolved.reference.observed_at.to_rfc3339(),
                    "declarationHash": resolved.reference.declaration_hash,
                    "driverVersion": resolved.reference.driver_version,
                }),
                transcript: Vec::new(),
            })
            .await;
        let (status, outcome) = match executed {
            Ok(outcome) => {
                let status = if outcome.status == "completed" {
                    "completed"
                } else {
                    "failed"
                };
                (status, serde_json::to_value(&outcome)?)
            }
            Err(error) => (
                "failed",
                json!({
                    "status": "failed",
                    "summary": format!("模拟执行失败: {error}"),
                    "observations": [],
                    "evidenceRefs": [],
                    "recommendedActions": [],
                }),
            ),
        };
        self.db
            .complete_simulation_task(&self.workspace_id, &task_id, &run_id, status, &outcome, now)
            .await?;
        Ok(DryRunResult { task_id, run_id })
    }

    /// Read-only generic Run projection. Production Task IDs return None.
    pub async fn projection(&self, task_id: &str) -> Result<Option<Value>, DbError> {
        self.db.simulation_run_projection(&self.workspace_id, task_id).await
    }
}

/// Same fail-closed capability check used by production: `Capability::
/// is_satisfied_by` is the single authority, so video still requires vision.
fn missing_capabilities(required: &[String], available: &[Capability]) -> Vec<String> {
    required
        .iter()
        .filter(|raw| {
            Capability::all()
                .into_iter()
                .find(|capability| capability.as_str() == raw.as_str())
                .is_none_or(|capability| !capability.is_satisfied_by(available))
        })
        .cloned()
        .collect()
}
