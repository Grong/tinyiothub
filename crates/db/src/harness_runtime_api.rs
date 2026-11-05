//! Workspace-Harness read/write adapters used by the Runtime HTTP surface
//! (Task 19B).
//!
//! Everything here reads or writes the owning workspace's Harness SQLite
//! generation. No platform `runtime_events`/`task_approvals` row is touched.
//! The returned values are the existing Runtime DTOs so the web contract does
//! not change in the cutover.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tinyiothub_core::agent_task::{AgentTask, TaskStatus};
use tinyiothub_core::device_job::DeviceJob;
use tinyiothub_core::harness::{
    Decision, DecisionKind, DecisionState, EffectIntent, EffectState, ExecutionMode, HarnessEvent, HarnessEventInput,
};
use tinyiothub_core::task_approval::{ApprovalDecision, ApprovalState, TaskApproval};

use crate::error::DbError;
use crate::runtime_event::{
    RuntimeEvent, TaskEvidenceProjection, TaskJobProjection, TaskProjection, TaskRunProjection, TaskWatchProjection,
};

const TASK_SELECT: &str = "SELECT id, run_id, state, state_version,
            runtime_idempotency_key AS idempotency_key,
            runtime_objective AS objective,
            runtime_session_key AS session_key,
            runtime_created_by AS created_by,
            runtime_created_at AS created_at,
            runtime_updated_at AS updated_at,
            runtime_ended_at AS ended_at
     FROM agent_tasks WHERE id = ? AND execution_mode = 'production'";

const DECISION_SELECT: &str = "SELECT decision_id, kind, state, session_id, run_id, task_id, effect_id,
            subject_json, options_json, created_at, updated_at, expires_at, answer, answer_actor,
            answer_event_id, answer_evidence_json, consumed_at, consumed_by
     FROM decisions";

const EVENT_COLUMNS: &str = "seq, event_id, session_id, execution_mode, run_id, task_id, type,
            occurred_at, cause_id, schema_v, payload_digest, payload_json";

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    id: String,
    run_id: Option<String>,
    state: String,
    state_version: i64,
    idempotency_key: String,
    objective: String,
    session_key: Option<String>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl TaskRow {
    fn into_task(self, workspace_id: &str) -> Result<AgentTask, DbError> {
        let status = status_from_state(&self.state)
            .ok_or_else(|| DbError::Internal(format!("unknown task state: {}", self.state)))?;
        Ok(AgentTask {
            id: self.id,
            workspace_id: workspace_id.to_string(),
            idempotency_key: self.idempotency_key,
            objective: self.objective,
            session_key: self.session_key,
            created_by: self.created_by,
            status,
            current_run_id: self.run_id,
            state_version: self.state_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ended_at: self.ended_at,
        })
    }
}

fn status_from_state(state: &str) -> Option<TaskStatus> {
    match state {
        "pending" | "running" | "model_calling" | "interrupted" => Some(TaskStatus::Active),
        other => TaskStatus::from_db(other),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DecisionRow {
    decision_id: String,
    kind: String,
    state: String,
    session_id: String,
    run_id: String,
    task_id: String,
    effect_id: Option<String>,
    subject_json: String,
    options_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    answer: Option<String>,
    answer_actor: Option<String>,
    answer_event_id: Option<String>,
    answer_evidence_json: Option<String>,
    consumed_at: Option<DateTime<Utc>>,
    consumed_by: Option<String>,
}

impl DecisionRow {
    fn into_decision(self) -> Result<Decision, DbError> {
        Ok(Decision {
            decision_id: self.decision_id,
            kind: DecisionKind::from_db(&self.kind)
                .ok_or_else(|| DbError::Internal(format!("unknown decision kind: {}", self.kind)))?,
            state: DecisionState::from_db(&self.state)
                .ok_or_else(|| DbError::Internal(format!("unknown decision state: {}", self.state)))?,
            session_id: self.session_id,
            run_id: self.run_id,
            task_id: self.task_id,
            effect_id: self.effect_id,
            subject: serde_json::from_str(&self.subject_json)?,
            options: serde_json::from_str(&self.options_json)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            expires_at: self.expires_at,
            answer: self.answer,
            answer_actor: self.answer_actor,
            answer_event_id: self.answer_event_id,
            answer_evidence: self
                .answer_evidence_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            consumed_at: self.consumed_at,
            consumed_by: self.consumed_by,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
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
    fn into_event(self, workspace_id: &str) -> Result<HarnessEvent, DbError> {
        let execution_mode = ExecutionMode::from_db(&self.execution_mode)
            .ok_or_else(|| DbError::Internal(format!("unknown execution mode: {}", self.execution_mode)))?;
        let occurred_at = DateTime::parse_from_rfc3339(&self.occurred_at)
            .map_err(|error| DbError::Internal(format!("invalid event occurred_at: {error}")))?
            .with_timezone(&Utc);
        let payload_json: Value = serde_json::from_str(&self.payload_json)?;
        crate::harness_event::validate_schema(self.schema_v)?;
        crate::harness_event::validate_payload(&self.event_type, &payload_json)?;
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

/// Result of answering one Harness tool-approval decision over HTTP.
pub struct ApprovalAnswer {
    pub decision: Decision,
    pub deduplicated: bool,
}

/// Job plus the Effect intent that produced it (if the workspace recorded one).
pub struct JobEffect {
    pub job: DeviceJob,
    pub effect: Option<EffectIntent>,
}

fn payload_str(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn approval_from_decision(
    workspace_id: &str,
    decision: &Decision,
    effect: Option<&EffectIntent>,
) -> Result<TaskApproval, DbError> {
    let subject = &decision.subject;
    let (state, approval_decision) = match decision.state {
        DecisionState::Requested => (ApprovalState::Pending, None),
        DecisionState::Answered | DecisionState::Consumed => match decision.answer.as_deref() {
            Some("approved") => (ApprovalState::Approved, Some(ApprovalDecision::Approve)),
            Some("denied") => (ApprovalState::Rejected, Some(ApprovalDecision::Reject)),
            _ => (ApprovalState::Pending, None),
        },
        DecisionState::Expired => (ApprovalState::Expired, None),
        DecisionState::Withdrawn => (ApprovalState::Unavailable, None),
    };
    let decided_at = matches!(
        decision.state,
        DecisionState::Answered | DecisionState::Consumed | DecisionState::Expired | DecisionState::Withdrawn
    )
    .then_some(decision.updated_at);
    let device_id = payload_str(subject, &["deviceId", "device_id", "device"])
        .or_else(|| effect.and_then(|effect| payload_str(&effect.request_json, &["deviceId", "device_id"])))
        .unwrap_or_default();
    let tool_call_id = payload_str(subject, &["toolCallId", "tool_call_id"])
        .or_else(|| effect.map(|effect| effect.tool_call_id.clone()))
        .unwrap_or_default();
    let tool = payload_str(subject, &["tool", "toolName"]).or_else(|| effect.map(|effect| effect.tool_name.clone()));
    let args_json = subject
        .get("argsJson")
        .or_else(|| subject.get("args"))
        .cloned()
        .or_else(|| effect.map(|effect| effect.request_json.clone()))
        .unwrap_or(Value::Null);
    let reason = payload_str(subject, &["reason", "summary"]).unwrap_or_default();
    Ok(TaskApproval {
        id: decision.decision_id.clone(),
        workspace_id: workspace_id.to_string(),
        task_id: decision.task_id.clone(),
        run_id: decision.run_id.clone(),
        tool_call_id,
        device_id,
        tool: tool.unwrap_or_default(),
        args_json,
        reason,
        state,
        decision: approval_decision,
        decider_id: decision.answer_actor.clone(),
        requested_at: decision.created_at,
        decided_at,
        // TaskApproval predates optional expiry; a decision without an expiry
        // keeps the request time so the serialized contract stays total.
        expires_at: decision.expires_at.unwrap_or(decision.created_at),
        decision_event_id: decision
            .answer_event_id
            .clone()
            .unwrap_or_else(|| decision.decision_id.clone()),
        updated_at: decision.updated_at,
    })
}

pub(crate) fn harness_event_to_runtime(event: &HarnessEvent) -> RuntimeEvent {
    let payload = &event.payload_json;
    RuntimeEvent {
        cursor: event.seq,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        occurred_at: event.occurred_at,
        workspace_id: event.workspace_id.clone(),
        task_id: event.task_id.clone(),
        run_id: event.run_id.clone(),
        tool_call_id: payload_str(payload, &["toolCallId", "tool_call_id"]),
        job_id: payload_str(payload, &["jobId", "job_id"]),
        approval_id: payload_str(payload, &["approvalId", "approval_id", "decisionId"]),
        device_id: payload_str(payload, &["deviceId", "device_id"]),
        source_id: payload_str(payload, &["sourceId", "source_id"]),
        source_seq: payload
            .get("sourceSeq")
            .or_else(|| payload.get("source_seq"))
            .and_then(Value::as_i64),
        evidence_id: payload_str(payload, &["evidenceId", "evidence_id"]),
        data: payload.clone(),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EffectRow {
    effect_id: String,
    session_id: String,
    task_id: String,
    run_id: String,
    tool_call_id: String,
    tool_name: String,
    request_json: String,
    request_digest: String,
    idempotency_key: String,
    external_ref: Option<String>,
    state: String,
    recovery_from: Option<String>,
    state_version: i64,
    result_json: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl EffectRow {
    fn into_intent(self) -> Result<EffectIntent, DbError> {
        Ok(EffectIntent {
            effect_id: self.effect_id,
            session_id: self.session_id,
            task_id: self.task_id,
            run_id: self.run_id,
            tool_call_id: self.tool_call_id,
            tool_name: self.tool_name,
            request_json: serde_json::from_str(&self.request_json)?,
            request_digest: self.request_digest,
            idempotency_key: self.idempotency_key,
            external_ref: self.external_ref,
            state: EffectState::from_db(&self.state)
                .ok_or_else(|| DbError::Internal(format!("unknown effect state: {}", self.state)))?,
            recovery_from: self.recovery_from.as_deref().and_then(EffectState::from_db),
            state_version: self.state_version,
            result_json: self.result_json.as_deref().map(serde_json::from_str).transpose()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

async fn effect_by_tool_call(
    pool: &SqlitePool,
    task_id: &str,
    tool_call_id: &str,
) -> Result<Option<EffectIntent>, DbError> {
    sqlx::query_as::<_, EffectRow>(
        "SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                request_digest, idempotency_key, external_ref, state, recovery_from, state_version,
                result_json, created_at, updated_at
         FROM effect_intents WHERE task_id = ? AND tool_call_id = ?",
    )
    .bind(task_id)
    .bind(tool_call_id)
    .fetch_optional(pool)
    .await?
    .map(EffectRow::into_intent)
    .transpose()
}

impl crate::Db {
    /// Full Runtime projection backed by the workspace Harness facts. Jobs,
    /// approvals, and runs are read in one transaction with the Task so the
    /// snapshot is coherent. Harness owns no watch/evidence projection yet, so
    /// those arrays stay present but empty for contract compatibility.
    pub async fn harness_runtime_projection(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<Option<TaskProjection>, DbError> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let task = sqlx::query_as::<_, TaskRow>(TASK_SELECT)
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| row.into_task(workspace_id))
            .transpose()?;
        let Some(task) = task else {
            return Ok(None);
        };
        let cursor = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq), 0) FROM session_events")
            .fetch_one(&mut *tx)
            .await?;
        let runs = sqlx::query_as::<_, TaskRunProjection>(
            "SELECT id, state AS status, COALESCE(runtime_outcome, '') AS outcome,
                    runtime_summary AS summary, runtime_created_at AS created_at
             FROM agent_runs WHERE task_id = ? AND execution_mode = 'production'
             ORDER BY runtime_created_at, rowid",
        )
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
        let job_rows = sqlx::query_as::<_, crate::device_job::DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                    timeout_ms, started_at, ended_at
             FROM device_jobs WHERE task_id = ? ORDER BY started_at, id",
        )
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
        let jobs = job_rows
            .into_iter()
            .map(DeviceJob::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DbError::Internal(error.to_string()))?
            .into_iter()
            .map(TaskJobProjection::from)
            .collect::<Vec<_>>();
        let decisions = sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(format!(
            "{DECISION_SELECT} WHERE task_id = ? AND kind = 'tool_approval'
             ORDER BY created_at, decision_id"
        )))
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(DecisionRow::into_decision)
        .collect::<Result<Vec<_>, _>>()?;
        let mut approvals = Vec::with_capacity(decisions.len());
        for decision in &decisions {
            let effect = match decision.effect_id.as_deref() {
                Some(effect_id) => sqlx::query_as::<_, EffectRow>(
                    "SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                            request_digest, idempotency_key, external_ref, state, recovery_from,
                            state_version, result_json, created_at, updated_at
                     FROM effect_intents WHERE effect_id = ?",
                )
                .bind(effect_id)
                .fetch_optional(&mut *tx)
                .await?
                .map(EffectRow::into_intent)
                .transpose()?,
                None => None,
            };
            approvals.push(approval_from_decision(workspace_id, decision, effect.as_ref())?);
        }
        let summary = runs
            .iter()
            .rev()
            .find(|run| !run.summary.trim().is_empty())
            .map(|run| run.summary.clone());
        tx.commit().await?;
        Ok(Some(TaskProjection {
            task,
            runs,
            jobs,
            approvals,
            watches: Vec::<TaskWatchProjection>::new(),
            evidence: Vec::<TaskEvidenceProjection>::new(),
            associations: Vec::new(),
            devices: Vec::new(),
            summary,
            cursor,
        }))
    }

    /// Ascending event page strictly after `after`, bounded by the existing
    /// Harness page contract (256 rows).
    pub async fn harness_task_events_after(
        &self,
        workspace_id: &str,
        task_id: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<RuntimeEvent>, DbError> {
        let page = self
            .session_event_page(
                workspace_id,
                &crate::harness_event::EventFilter {
                    session_id: None,
                    run_id: None,
                    task_id: Some(task_id.to_string()),
                    execution_mode: ExecutionMode::Production,
                },
                after,
                limit,
            )
            .await?;
        Ok(page.events.iter().map(harness_event_to_runtime).collect())
    }

    /// Workspace high-watermark used by the SSE cursor contract.
    pub async fn harness_event_high_watermark(&self, workspace_id: &str) -> Result<i64, DbError> {
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq), 0) FROM session_events")
                .fetch_one(&pool)
                .await?,
        )
    }

    /// A cursor is known only when it names a persisted production Event.
    /// `0` is the empty-ledger sentinel.
    pub async fn harness_cursor_known(&self, workspace_id: &str, cursor: i64) -> Result<bool, DbError> {
        if cursor == 0 {
            return Ok(true);
        }
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM session_events WHERE seq = ? AND execution_mode = 'production')",
        )
        .bind(cursor)
        .fetch_one(&pool)
        .await?)
    }

    /// Newest page for the bounded initial timeline. Rows are returned
    /// ascending; `has_more` reports whether an older page exists.
    pub async fn harness_task_events_before(
        &self,
        workspace_id: &str,
        task_id: &str,
        before: Option<i64>,
        limit: i64,
    ) -> Result<(Vec<RuntimeEvent>, bool), DbError> {
        let limit = limit.clamp(1, 200);
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, SessionEventRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EVENT_COLUMNS} FROM session_events
             WHERE execution_mode = 'production' AND task_id = ?
               AND (? IS NULL OR seq < ?)
             ORDER BY seq DESC LIMIT ?"
        )))
        .bind(task_id)
        .bind(before)
        .bind(before)
        .bind(limit + 1)
        .fetch_all(&pool)
        .await?;
        let mut rows = rows;
        let has_more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let mut events = rows
            .into_iter()
            .map(|row| row.into_event(workspace_id))
            .collect::<Result<Vec<_>, _>>()?;
        events.reverse();
        Ok((events.iter().map(harness_event_to_runtime).collect(), has_more))
    }

    /// Answer a tool-approval decision in the workspace. Returns the current
    /// decision even when the answer was already recorded, matching the legacy
    /// idempotent HTTP contract.
    pub async fn harness_answer_tool_approval(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
        answer: &str,
        actor: &str,
        now: DateTime<Utc>,
    ) -> Result<ApprovalAnswer, DbError> {
        if !matches!(answer, "approved" | "denied") || actor.trim().is_empty() {
            return Err(DbError::Validation {
                message: format!("invalid approval answer: {answer}"),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(format!(
            "{DECISION_SELECT} WHERE decision_id = ? AND task_id = ? AND kind = 'tool_approval'"
        )))
        .bind(approval_id)
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(DecisionRow::into_decision)
        .transpose()?
        .ok_or_else(|| DbError::NotFound {
            id: approval_id.to_string(),
        })?;
        match current.state {
            DecisionState::Answered | DecisionState::Consumed => {
                if current.answer.as_deref() == Some(answer) {
                    tx.commit().await?;
                    return Ok(ApprovalAnswer {
                        decision: current,
                        deduplicated: true,
                    });
                }
                return Err(DbError::Conflict {
                    message: "approval already has a different answer".to_string(),
                });
            }
            DecisionState::Expired | DecisionState::Withdrawn => {
                tx.commit().await?;
                return Ok(ApprovalAnswer {
                    decision: current,
                    deduplicated: true,
                });
            }
            DecisionState::Requested => {}
        }
        let event_id = format!("approval:{approval_id}:{answer}");
        let payload = json!({
            "decisionId": approval_id,
            "approvalId": approval_id,
            "effectId": current.effect_id,
            "taskId": task_id,
            "runId": current.run_id,
            "answer": answer,
            "answerActor": actor,
        });
        let payload_json = serde_json::to_string(&payload)?;
        let updated = sqlx::query(
            "UPDATE decisions SET state = 'answered', answer = ?, answer_actor = ?,
                 answer_event_id = ?, updated_at = ?
             WHERE decision_id = ? AND state = 'requested'",
        )
        .bind(answer)
        .bind(actor)
        .bind(&event_id)
        .bind(now)
        .bind(approval_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: "approval answer lost the CAS race".to_string(),
            });
        }
        let event = HarnessEventInput {
            event_id,
            workspace_id: workspace_id.to_string(),
            session_id: current.session_id.clone(),
            execution_mode: ExecutionMode::Production,
            run_id: Some(current.run_id.clone()),
            task_id: Some(task_id.to_string()),
            event_type: if answer == "approved" {
                "approval.approved".to_string()
            } else {
                "approval.denied".to_string()
            },
            occurred_at: now,
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("{:x}", Sha256::digest(payload_json.as_bytes())),
            payload_json: payload,
        };
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, 'pending', ?, NULL, NULL)
             ON CONFLICT(task_id, event_id) DO NOTHING",
        )
        .bind(task_id)
        .bind(&event.event_id)
        .bind(
            json!({
                "kind":"approval_decided",
                "workspace_id":workspace_id,
                "task_id":task_id,
                "approval_id":approval_id,
                "decision_event_id":event.event_id,
            })
            .to_string(),
        )
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let decision =
            sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(format!("{DECISION_SELECT} WHERE decision_id = ?")))
                .bind(approval_id)
                .fetch_one(&pool)
                .await?
                .into_decision()?;
        Ok(ApprovalAnswer {
            decision,
            deduplicated: false,
        })
    }

    /// Resolve one Device Job scoped to its Task, plus the Effect intent that
    /// created it (matched by `tool_call_id`).
    pub async fn harness_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<Option<JobEffect>, DbError> {
        let pool = self.workspace_pool(workspace_id).await?;
        let row = sqlx::query_as::<_, crate::device_job::DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                    timeout_ms, started_at, ended_at
             FROM device_jobs WHERE id = ? AND task_id = ?",
        )
        .bind(job_id)
        .bind(task_id)
        .fetch_optional(&pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let job = DeviceJob::try_from(row).map_err(|error| DbError::Internal(error.to_string()))?;
        let effect = match job.tool_call_id.as_deref() {
            Some(tool_call_id) => effect_by_tool_call(&pool, task_id, tool_call_id).await?,
            None => None,
        };
        Ok(Some(JobEffect { job, effect }))
    }
}
