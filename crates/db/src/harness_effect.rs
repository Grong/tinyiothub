use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use sqlx::{SqlSafeStr, SqliteConnection, SqlitePool};
use tinyiothub_core::harness::{
    Decision, DecisionKind, DecisionMutation, DecisionState, EffectIntent, EffectState, Transition,
    TransitionPrecondition,
};

use crate::{
    Db,
    error::{DbError, Result},
};

const MAX_LOCK_RETRIES: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub transition_id: String,
    pub deduplicated: bool,
    pub state_version: i64,
}

#[derive(Debug, Clone)]
pub struct EffectRecoveryItem {
    pub intent: EffectIntent,
    pub decision: Option<Decision>,
    pub job_id: Option<String>,
    pub task_active: bool,
    pub task_state: String,
}

#[derive(Debug, sqlx::FromRow)]
struct EffectIntentRow {
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

impl TryFrom<EffectIntentRow> for EffectIntent {
    type Error = DbError;

    fn try_from(row: EffectIntentRow) -> Result<Self> {
        Ok(Self {
            effect_id: row.effect_id,
            session_id: row.session_id,
            task_id: row.task_id,
            run_id: row.run_id,
            tool_call_id: row.tool_call_id,
            tool_name: row.tool_name,
            request_json: serde_json::from_str(&row.request_json)?,
            request_digest: row.request_digest,
            idempotency_key: row.idempotency_key,
            external_ref: row.external_ref,
            state: EffectState::from_db(&row.state)
                .ok_or_else(|| DbError::Internal(format!("unknown effect state: {}", row.state)))?,
            recovery_from: row
                .recovery_from
                .as_deref()
                .map(|state| {
                    EffectState::from_db(state)
                        .ok_or_else(|| DbError::Internal(format!("unknown recovery state: {state}")))
                })
                .transpose()?,
            state_version: row.state_version,
            result_json: row.result_json.as_deref().map(serde_json::from_str).transpose()?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
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

impl TryFrom<DecisionRow> for Decision {
    type Error = DbError;

    fn try_from(row: DecisionRow) -> Result<Self> {
        Ok(Self {
            decision_id: row.decision_id,
            kind: DecisionKind::from_db(&row.kind)
                .ok_or_else(|| DbError::Internal(format!("unknown decision kind: {}", row.kind)))?,
            state: DecisionState::from_db(&row.state)
                .ok_or_else(|| DbError::Internal(format!("unknown decision state: {}", row.state)))?,
            session_id: row.session_id,
            run_id: row.run_id,
            task_id: row.task_id,
            effect_id: row.effect_id,
            subject: serde_json::from_str(&row.subject_json)?,
            options: serde_json::from_str(&row.options_json)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
            answer: row.answer,
            answer_actor: row.answer_actor,
            answer_event_id: row.answer_event_id,
            answer_evidence: row
                .answer_evidence_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            consumed_at: row.consumed_at,
            consumed_by: row.consumed_by,
        })
    }
}

fn sanitize_evidence(evidence: Option<Value>) -> Option<Value> {
    evidence.map(|value| {
        let refs = ["evidence", "evidenceRefs", "evidence_refs"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(Value::as_array))
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        serde_json::json!({"evidence": refs})
    })
}

enum AttemptError {
    Writes(DbError),
    Commit(DbError),
}

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000003,
        "harness_effects".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000003_harness_effects.sql").into_sql_str(),
        false,
    )
}

fn lock_message(error: &DbError) -> Option<String> {
    match error {
        DbError::Sqlx(sqlx::Error::Database(database))
            if matches!(database.code().as_deref(), Some("5") | Some("6"))
                || database.message().contains("database is locked")
                || database.message().contains("database is deadlocked") =>
        {
            Some(database.message().to_string())
        }
        _ => None,
    }
}

fn validate_transition(workspace_id: &str, expected: &TransitionPrecondition, transition: &Transition) -> Result<()> {
    if expected != &transition.precondition {
        return Err(DbError::Validation {
            message: "transition precondition differs from apply expectation".to_string(),
        });
    }
    match (&transition.effect, &transition.event) {
        (None, None)
            if transition.job.is_none()
                && transition.wake.is_none()
                && transition.additional_events.is_empty()
                && transition.decision.is_none()
                && transition.run_task.is_none() =>
        {
            Ok(())
        }
        (Some(effect), Some(event)) => {
            if effect.effect_id != expected.effect_id
                || effect.state_version != expected.expected_version + 1
                || event.workspace_id != workspace_id
                || event.task_id.as_deref() != transition.job.as_ref().map(|job| job.task_id.as_str())
                    && transition.job.is_some()
            {
                return Err(DbError::Validation {
                    message: "transition identities or next version are inconsistent".to_string(),
                });
            }
            if let Some(job) = &transition.job
                && event.run_id.as_deref() != Some(job.run_id.as_str())
            {
                return Err(DbError::Validation {
                    message: "job and event run identities differ".to_string(),
                });
            }
            crate::harness_event::validate_schema(event.schema_v)?;
            crate::harness_event::validate_payload(&event.event_type, &event.payload_json)
        }
        _ => Err(DbError::Validation {
            message: "effect and event writes must both be present or absent".to_string(),
        }),
    }
}

async fn current_version(connection: &mut SqliteConnection, effect_id: &str) -> Result<Option<(EffectState, i64)>> {
    let row = sqlx::query_as::<_, (String, i64)>("SELECT state, state_version FROM effect_intents WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_optional(connection)
        .await?;
    row.map(|(state, version)| {
        EffectState::from_db(&state)
            .map(|state| (state, version))
            .ok_or_else(|| DbError::Internal(format!("unknown effect state: {state}")))
    })
    .transpose()
}

async fn deduplicated_outcome(
    connection: &mut SqliteConnection,
    transition: &Transition,
    transition_json: &str,
) -> Result<Option<ApplyOutcome>> {
    let stored = sqlx::query_as::<_, (String, String)>(
        "SELECT effect_id, transition_json FROM harness_transitions WHERE transition_id = ?",
    )
    .bind(&transition.transition_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((effect_id, stored_json)) = stored else {
        return Ok(None);
    };
    if effect_id != transition.precondition.effect_id || stored_json != transition_json {
        return Err(DbError::Conflict {
            message: format!(
                "transition {} already exists with different content",
                transition.transition_id
            ),
        });
    }
    let event = transition
        .event
        .as_ref()
        .ok_or_else(|| DbError::Internal("persisted transition has no event".to_string()))?;
    let event_digest = sqlx::query_scalar::<_, String>("SELECT payload_digest FROM session_events WHERE event_id = ?")
        .bind(&event.event_id)
        .fetch_optional(&mut *connection)
        .await?;
    if event_digest.as_deref() != Some(event.payload_digest.as_str()) {
        return Err(DbError::Conflict {
            message: format!("event {} does not match committed transition", event.event_id),
        });
    }
    if let Some(wake) = &transition.wake {
        let signal = sqlx::query_scalar::<_, String>(
            "SELECT signal_json FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
        )
        .bind(&wake.task_id)
        .bind(&wake.wake_id)
        .fetch_optional(&mut *connection)
        .await?;
        if signal.as_deref() != Some(wake.signal_json.as_str()) {
            return Err(DbError::Conflict {
                message: format!("wake {} does not match committed transition", wake.wake_id),
            });
        }
    }
    if let Some(job) = &transition.job {
        let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM device_jobs WHERE id = ?)")
            .bind(&job.job_id)
            .fetch_one(&mut *connection)
            .await?;
        if !exists {
            return Err(DbError::Conflict {
                message: format!("job {} is missing from committed transition", job.job_id),
            });
        }
    }
    let (_, state_version) = current_version(connection, &effect_id)
        .await?
        .ok_or_else(|| DbError::Conflict {
            message: format!("effect {effect_id} is missing from committed transition"),
        })?;
    Ok(Some(ApplyOutcome {
        transition_id: transition.transition_id.clone(),
        deduplicated: true,
        state_version,
    }))
}

pub(crate) async fn insert_event(
    connection: &mut SqliteConnection,
    event: &tinyiothub_core::harness::HarnessEventInput,
) -> Result<()> {
    let inserted = sqlx::query(
        "INSERT INTO session_events
            (event_id, session_id, execution_mode, run_id, task_id, type, occurred_at,
             cause_id, schema_v, payload_digest, payload_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(event_id) DO NOTHING",
    )
    .bind(&event.event_id)
    .bind(&event.session_id)
    .bind(event.execution_mode.as_str())
    .bind(&event.run_id)
    .bind(&event.task_id)
    .bind(&event.event_type)
    .bind(event.occurred_at.to_rfc3339())
    .bind(event.cause_id)
    .bind(event.schema_v)
    .bind(&event.payload_digest)
    .bind(serde_json::to_string(&event.payload_json)?)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if inserted == 0 {
        let digest = sqlx::query_scalar::<_, String>("SELECT payload_digest FROM session_events WHERE event_id = ?")
            .bind(&event.event_id)
            .fetch_one(&mut *connection)
            .await?;
        if digest != event.payload_digest {
            return Err(DbError::Conflict {
                message: format!("event {} already exists with another digest", event.event_id),
            });
        }
    }
    Ok(())
}

async fn apply_decision(connection: &mut SqliteConnection, mutation: &DecisionMutation) -> Result<()> {
    let decision = &mutation.decision;
    if decision.kind == DecisionKind::EffectReconciliation && decision.expires_at.is_some() {
        return Err(DbError::Validation {
            message: "effect reconciliation decisions never expire".to_string(),
        });
    }
    if mutation.insert_if_missing {
        let inserted = sqlx::query(
            "INSERT INTO decisions
                (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                 options_json, created_at, updated_at, expires_at, answer, answer_actor,
                 answer_event_id, answer_evidence_json, consumed_at, consumed_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(decision_id) DO NOTHING",
        )
        .bind(&decision.decision_id)
        .bind(decision.kind.as_str())
        .bind(decision.state.as_str())
        .bind(&decision.session_id)
        .bind(&decision.run_id)
        .bind(&decision.task_id)
        .bind(&decision.effect_id)
        .bind(serde_json::to_string(&decision.subject)?)
        .bind(serde_json::to_string(&decision.options)?)
        .bind(decision.created_at.to_rfc3339())
        .bind(decision.updated_at.to_rfc3339())
        .bind(decision.expires_at.map(|value| value.to_rfc3339()))
        .bind(&decision.answer)
        .bind(&decision.answer_actor)
        .bind(&decision.answer_event_id)
        .bind(
            decision
                .answer_evidence
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
        )
        .bind(decision.consumed_at.map(|value| value.to_rfc3339()))
        .bind(&decision.consumed_by)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing: (String, String) = sqlx::query_as("SELECT kind, state FROM decisions WHERE decision_id = ?")
                .bind(&decision.decision_id)
                .fetch_one(&mut *connection)
                .await?;
            if existing != (decision.kind.as_str().to_string(), decision.state.as_str().to_string()) {
                return Err(DbError::Conflict {
                    message: format!("decision {} changed during recovery", decision.decision_id),
                });
            }
        }
        return Ok(());
    }
    let expected = mutation.expected_state.ok_or_else(|| DbError::Validation {
        message: "decision update requires an expected state".to_string(),
    })?;
    let updated = sqlx::query(
        "UPDATE decisions SET state = ?, updated_at = ?, answer = ?, answer_actor = ?,
             answer_event_id = ?, answer_evidence_json = ?, consumed_at = ?, consumed_by = ?
         WHERE decision_id = ? AND state = ?",
    )
    .bind(decision.state.as_str())
    .bind(decision.updated_at.to_rfc3339())
    .bind(&decision.answer)
    .bind(&decision.answer_actor)
    .bind(&decision.answer_event_id)
    .bind(
        decision
            .answer_evidence
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(decision.consumed_at.map(|value| value.to_rfc3339()))
    .bind(&decision.consumed_by)
    .bind(&decision.decision_id)
    .bind(expected.as_str())
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(DbError::CasConflict {
            message: format!("decision {} is no longer {}", decision.decision_id, expected.as_str()),
        });
    }
    Ok(())
}

async fn apply_writes(
    connection: &mut SqliteConnection,
    transition: &Transition,
    transition_json: &str,
) -> Result<ApplyOutcome> {
    if let Some(outcome) = deduplicated_outcome(connection, transition, transition_json).await? {
        return Ok(outcome);
    }
    let Some(effect) = &transition.effect else {
        let (current, state_version) = current_version(connection, &transition.precondition.effect_id)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: transition.precondition.effect_id.clone(),
            })?;
        if !current.is_terminal() {
            return Err(DbError::CasConflict {
                message: format!(
                    "effect {} is {} and cannot accept a late-fact no-op",
                    transition.precondition.effect_id,
                    current.as_str()
                ),
            });
        }
        return Ok(ApplyOutcome {
            transition_id: transition.transition_id.clone(),
            deduplicated: true,
            state_version,
        });
    };
    let event = transition.event.as_ref().ok_or_else(|| DbError::Validation {
        message: "effect transition requires an event".to_string(),
    })?;

    let updated = sqlx::query(
        "UPDATE effect_intents
         SET state = ?, recovery_from = ?, state_version = ?, external_ref = ?, result_json = ?, updated_at = ?
         WHERE effect_id = ? AND state = ? AND state_version = ?",
    )
    .bind(effect.state.as_str())
    .bind(effect.recovery_from.map(EffectState::as_str))
    .bind(effect.state_version)
    .bind(&effect.external_ref)
    .bind(effect.result.as_ref().map(serde_json::to_string).transpose()?)
    .bind(effect.updated_at.to_rfc3339())
    .bind(&effect.effect_id)
    .bind(transition.precondition.expected_state.as_str())
    .bind(transition.precondition.expected_version)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated != 1 {
        let current = current_version(connection, &effect.effect_id).await?;
        return Err(DbError::CasConflict {
            message: format!(
                "effect {} expected {}/{} but found {current:?}",
                effect.effect_id,
                transition.precondition.expected_state.as_str(),
                transition.precondition.expected_version
            ),
        });
    }

    insert_event(connection, event).await?;
    for additional in &transition.additional_events {
        insert_event(connection, additional).await?;
    }
    if let Some(decision) = &transition.decision {
        apply_decision(connection, decision).await?;
    }
    if let Some(projection) = &transition.run_task {
        let run_updated = sqlx::query("UPDATE agent_runs SET state = ? WHERE id = ? AND task_id = ?")
            .bind(&projection.run_state)
            .bind(&projection.run_id)
            .bind(&projection.task_id)
            .execute(&mut *connection)
            .await?
            .rows_affected();
        let task_updated =
            sqlx::query("UPDATE agent_tasks SET state = ?, state_version = state_version + 1 WHERE id = ?")
                .bind(&projection.task_state)
                .bind(&projection.task_id)
                .execute(&mut *connection)
                .await?
                .rows_affected();
        if run_updated != 1 || task_updated != 1 {
            return Err(DbError::Conflict {
                message: format!("run/task projection is missing for {}", projection.task_id),
            });
        }
    }

    if let Some(job) = &transition.job {
        let updated = sqlx::query(
            "UPDATE device_jobs
             SET state = ?, result_json = ?, driver_call_id = ?, ended_at = ?
             WHERE id = ? AND task_id = ? AND run_id = ?",
        )
        .bind(&job.state)
        .bind(job.result_json.as_ref().map(serde_json::to_string).transpose()?)
        .bind(&job.external_ref)
        .bind(job.ended_at.map(|ended| ended.to_rfc3339()))
        .bind(&job.job_id)
        .bind(&job.task_id)
        .bind(&job.run_id)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::Conflict {
                message: format!("job {} does not match transition identity", job.job_id),
            });
        }
    }

    if let Some(wake) = &transition.wake {
        if Some(wake.task_id.as_str()) != event.task_id.as_deref() {
            return Err(DbError::Validation {
                message: "wake and event task identities differ".to_string(),
            });
        }
        let inserted = sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, 'pending', ?, NULL, NULL)
             ON CONFLICT(task_id, event_id) DO NOTHING",
        )
        .bind(&wake.task_id)
        .bind(&wake.wake_id)
        .bind(&wake.signal_json)
        .bind(wake.created_at.to_rfc3339())
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if inserted == 0 {
            let signal = sqlx::query_scalar::<_, String>(
                "SELECT signal_json FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
            )
            .bind(&wake.task_id)
            .bind(&wake.wake_id)
            .fetch_one(&mut *connection)
            .await?;
            if signal != wake.signal_json {
                return Err(DbError::Conflict {
                    message: format!("wake {} already exists with another signal", wake.wake_id),
                });
            }
        }
    }

    let occurred_at = event.occurred_at.to_rfc3339();
    sqlx::query(
        "INSERT INTO harness_transitions
            (transition_id, effect_id, state_version, transition_json, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&transition.transition_id)
    .bind(&effect.effect_id)
    .bind(effect.state_version)
    .bind(transition_json)
    .bind(occurred_at)
    .execute(&mut *connection)
    .await?;

    Ok(ApplyOutcome {
        transition_id: transition.transition_id.clone(),
        deduplicated: false,
        state_version: effect.state_version,
    })
}

impl Db {
    pub async fn effect_intent(&self, workspace_id: &str, effect_id: &str) -> Result<Option<EffectIntent>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, EffectIntentRow>("SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json, request_digest, idempotency_key, external_ref, state, recovery_from, state_version, result_json, created_at, updated_at FROM effect_intents WHERE effect_id = ?")
            .bind(effect_id)
            .fetch_optional(&pool)
            .await?
            .map(EffectIntent::try_from)
            .transpose()
    }

    pub async fn effect_decision(&self, workspace_id: &str, effect_id: &str) -> Result<Option<Decision>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, DecisionRow>("SELECT decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json, options_json, created_at, updated_at, expires_at, answer, answer_actor, answer_event_id, answer_evidence_json, consumed_at, consumed_by FROM decisions WHERE effect_id = ?
             ORDER BY CASE kind WHEN 'effect_reconciliation' THEN 0 ELSE 1 END, created_at DESC LIMIT 1")
        .bind(effect_id)
        .fetch_optional(&pool)
        .await?
        .map(Decision::try_from)
        .transpose()
    }

    pub async fn list_effect_recovery(&self, workspace_id: &str) -> Result<Vec<EffectRecoveryItem>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let intents = sqlx::query_as::<_, EffectIntentRow>("SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json, request_digest, idempotency_key, external_ref, state, recovery_from, state_version, result_json, created_at, updated_at FROM effect_intents
             WHERE state IN ('prepared', 'dispatching', 'cancelling', 'unknown') ORDER BY created_at, effect_id")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(EffectIntent::try_from)
        .collect::<Result<Vec<_>>>()?;
        let mut items = Vec::with_capacity(intents.len());
        for intent in intents {
            let decision = sqlx::query_as::<_, DecisionRow>("SELECT decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json, options_json, created_at, updated_at, expires_at, answer, answer_actor, answer_event_id, answer_evidence_json, consumed_at, consumed_by FROM decisions WHERE effect_id = ?
                 ORDER BY CASE kind WHEN 'effect_reconciliation' THEN 0 ELSE 1 END, created_at DESC LIMIT 1")
            .bind(&intent.effect_id)
            .fetch_optional(&pool)
            .await?
            .map(Decision::try_from)
            .transpose()?;
            let job_id = sqlx::query_scalar::<_, String>("SELECT id FROM device_jobs WHERE tool_call_id = ?")
                .bind(&intent.tool_call_id)
                .fetch_optional(&pool)
                .await?;
            let task_state = sqlx::query_scalar::<_, String>("SELECT state FROM agent_tasks WHERE id = ?")
                .bind(&intent.task_id)
                .fetch_one(&pool)
                .await?;
            let task_active = task_state == "running";
            items.push(EffectRecoveryItem {
                intent,
                decision,
                job_id,
                task_active,
                task_state,
            });
        }
        Ok(items)
    }

    pub async fn answer_effect_reconciliation(
        &self,
        workspace_id: &str,
        effect_id: &str,
        answer: &str,
        actor: &str,
        evidence: Option<Value>,
    ) -> Result<Decision> {
        if !matches!(answer, "applied" | "not_applied") || actor.is_empty() {
            return Err(DbError::Validation {
                message: "reconciliation answer or actor is invalid".to_string(),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let evidence = sanitize_evidence(evidence);
        let mut tx = pool.begin().await?;
        let effect = sqlx::query_as::<_, EffectIntentRow>("SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json, request_digest, idempotency_key, external_ref, state, recovery_from, state_version, result_json, created_at, updated_at FROM effect_intents WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(EffectIntent::try_from)
        .transpose()?
        .ok_or_else(|| DbError::NotFound {
            id: effect_id.to_string(),
        })?;
        if effect.state != EffectState::Unknown {
            return Err(DbError::Conflict {
                message: format!("effect {effect_id} is already {}", effect.state.as_str()),
            });
        }
        let current = sqlx::query_as::<_, DecisionRow>("SELECT decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json, options_json, created_at, updated_at, expires_at, answer, answer_actor, answer_event_id, answer_evidence_json, consumed_at, consumed_by FROM decisions
             WHERE effect_id = ? AND kind = 'effect_reconciliation'")
        .bind(effect_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(Decision::try_from)
        .transpose()?
        .ok_or_else(|| DbError::NotFound {
            id: format!("reconciliation:{effect_id}"),
        })?;
        if current.state == DecisionState::Answered {
            if current.answer.as_deref() == Some(answer) {
                tx.commit().await?;
                return Ok(current);
            }
            return Err(DbError::Conflict {
                message: "reconciliation already has a different answer".to_string(),
            });
        }
        if current.state != DecisionState::Requested {
            return Err(DbError::Conflict {
                message: format!("reconciliation is {}", current.state.as_str()),
            });
        }
        let now = Utc::now();
        let event_id = format!("decision-answer:{}", uuid::Uuid::new_v4());
        let payload = serde_json::json!({
            "decisionId": current.decision_id,
            "kind": "effect_reconciliation",
            "answer": answer,
            "answerActor": actor,
            "evidence": evidence,
        });
        let payload_json = serde_json::to_string(&payload)?;
        let updated = sqlx::query(
            "UPDATE decisions SET state = 'answered', answer = ?, answer_actor = ?,
                 answer_event_id = ?, answer_evidence_json = ?, updated_at = ?
             WHERE decision_id = ? AND state = 'requested'",
        )
        .bind(answer)
        .bind(actor)
        .bind(&event_id)
        .bind(evidence.as_ref().map(serde_json::to_string).transpose()?)
        .bind(now.to_rfc3339())
        .bind(&current.decision_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: "reconciliation answer lost the CAS race".to_string(),
            });
        }
        insert_event(
            &mut tx,
            &tinyiothub_core::harness::HarnessEventInput {
                event_id: event_id.clone(),
                workspace_id: workspace_id.to_string(),
                session_id: current.session_id.clone(),
                execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
                run_id: Some(current.run_id.clone()),
                task_id: Some(current.task_id.clone()),
                event_type: "decision.answered".to_string(),
                occurred_at: now,
                cause_id: None,
                schema_v: 1,
                payload_digest: format!("{:x}", sha2::Sha256::digest(payload_json.as_bytes())),
                payload_json: payload,
            },
        )
        .await?;
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, 'pending', ?, NULL, NULL)
             ON CONFLICT(task_id, event_id) DO NOTHING",
        )
        .bind(&current.task_id)
        .bind(&event_id)
        .bind(serde_json::json!({"effectId":effect_id,"kind":"reconciliation_answered"}).to_string())
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.effect_decision(workspace_id, effect_id)
            .await?
            .ok_or_else(|| DbError::Internal("committed reconciliation answer disappeared".to_string()))
    }

    pub async fn record_effect_reconcile_attempt(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        reason: &str,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let event_id = format!("reconcile-attempt:{}", uuid::Uuid::new_v4());
        let payload = serde_json::json!({
            "effectId": intent.effect_id,
            "reason": "reconciliation_unknown",
            "reasonDigest": format!("{:x}", sha2::Sha256::digest(reason.as_bytes())),
        });
        let event = tinyiothub_core::harness::HarnessEventInput {
            event_id,
            workspace_id: workspace_id.to_string(),
            session_id: intent.session_id.clone(),
            execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
            run_id: Some(intent.run_id.clone()),
            task_id: Some(intent.task_id.clone()),
            event_type: "effect.reconcile_attempted".to_string(),
            occurred_at: Utc::now(),
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("{:x}", sha2::Sha256::digest(payload.to_string().as_bytes())),
            payload_json: payload,
        };
        let mut connection = pool.acquire().await?.detach();
        insert_event(&mut connection, &event).await
    }

    pub async fn expire_harness_decisions(&self, workspace_id: &str, now: DateTime<Utc>) -> Result<u64> {
        let pool = self.workspace_pool(workspace_id).await?;
        let due = sqlx::query_as::<_, DecisionRow>(
            "SELECT decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                    options_json, created_at, updated_at, expires_at, answer, answer_actor,
                    answer_event_id, answer_evidence_json, consumed_at, consumed_by
             FROM decisions
             WHERE kind = 'tool_approval' AND state = 'requested' AND expires_at IS NOT NULL AND expires_at <= ?
             ORDER BY expires_at, decision_id",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(Decision::try_from)
        .collect::<Result<Vec<_>>>()?;
        let mut expired = 0;
        for decision in due {
            let Some(effect_id) = decision.effect_id.clone() else {
                continue;
            };
            let mut tx = pool.begin().await?;
            let intent = sqlx::query_as::<_, EffectIntentRow>(
                "SELECT effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                        request_digest, idempotency_key, external_ref, state, recovery_from, state_version, result_json,
                        created_at, updated_at FROM effect_intents WHERE effect_id = ?",
            )
            .bind(&effect_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(EffectIntent::try_from)
            .transpose()?
            .ok_or_else(|| DbError::NotFound { id: effect_id.clone() })?;
            if intent.state != EffectState::Prepared {
                return Err(DbError::Conflict {
                    message: format!("only a prepared effect can expire with its approval: {effect_id}"),
                });
            }
            let decision_updated = sqlx::query(
                "UPDATE decisions SET state = 'expired', updated_at = ?
                 WHERE decision_id = ? AND state = 'requested'",
            )
            .bind(now.to_rfc3339())
            .bind(&decision.decision_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if decision_updated == 0 {
                tx.rollback().await?;
                continue;
            }
            let next_version = intent.state_version + 1;
            let effect_updated = sqlx::query(
                "UPDATE effect_intents SET state = 'not_applied', state_version = ?,
                     result_json = '{\"reason\":\"approval_expired\"}', updated_at = ?
                 WHERE effect_id = ? AND state = 'prepared' AND state_version = ?",
            )
            .bind(next_version)
            .bind(now.to_rfc3339())
            .bind(&effect_id)
            .bind(intent.state_version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if effect_updated != 1 {
                return Err(DbError::CasConflict {
                    message: format!("effect {effect_id} changed during approval expiry"),
                });
            }
            let event = |suffix: &str, event_type: &str, payload: Value| tinyiothub_core::harness::HarnessEventInput {
                event_id: format!("approval-expired:{effect_id}:{suffix}"),
                workspace_id: workspace_id.to_string(),
                session_id: intent.session_id.clone(),
                execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
                run_id: Some(intent.run_id.clone()),
                task_id: Some(intent.task_id.clone()),
                event_type: event_type.to_string(),
                occurred_at: now,
                cause_id: None,
                schema_v: 1,
                payload_digest: format!("{:x}", sha2::Sha256::digest(payload.to_string().as_bytes())),
                payload_json: payload,
            };
            insert_event(
                &mut tx,
                &event(
                    "decision",
                    "decision.expired",
                    serde_json::json!({"decisionId":decision.decision_id,"kind":"tool_approval"}),
                ),
            )
            .await?;
            let effect_event = event(
                "effect",
                "effect.not_applied",
                serde_json::json!({"effectId":effect_id,"state":"not_applied","reason":"approval_expired"}),
            );
            insert_event(&mut tx, &effect_event).await?;
            insert_event(
                &mut tx,
                &event(
                    "tool-result",
                    "step.tool_result",
                    serde_json::json!({"effectId":effect_id,"state":"not_applied","result":{"reason":"approval_expired"}}),
                ),
            )
            .await?;
            sqlx::query(
                "UPDATE device_jobs SET state = 'rejected', result_json = '{\"reason\":\"approval_expired\"}', ended_at = ?
                 WHERE tool_call_id = ?",
            )
            .bind(now.to_rfc3339())
            .bind(&intent.tool_call_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE agent_runs SET state = 'running' WHERE id = ?")
                .bind(&intent.run_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE agent_tasks SET state = 'running', state_version = state_version + 1 WHERE id = ?")
                .bind(&intent.task_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO task_wake_outbox
                    (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
                 VALUES (?, ?, ?, 'pending', ?, NULL, NULL)
                 ON CONFLICT(task_id, event_id) DO NOTHING",
            )
            .bind(&intent.task_id)
            .bind(&effect_event.event_id)
            .bind(serde_json::json!({"effectId":effect_id,"state":"not_applied"}).to_string())
            .bind(now.to_rfc3339())
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            expired += 1;
        }
        Ok(expired)
    }

    pub async fn apply_workspace_transition(
        &self,
        workspace_id: &str,
        expected: &TransitionPrecondition,
        transition: &Transition,
    ) -> Result<ApplyOutcome> {
        validate_transition(workspace_id, expected, transition)?;
        let transition_json = serde_json::to_string(transition)?;
        let pool = self.workspace_pool(workspace_id).await?;
        let mut retries = 0;
        loop {
            match self
                .apply_workspace_transition_once(&pool, transition, &transition_json)
                .await
            {
                Ok(outcome) => return Ok(outcome),
                Err(AttemptError::Writes(error)) => {
                    if let Some(message) = lock_message(&error) {
                        if retries < MAX_LOCK_RETRIES {
                            retries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(5 * u64::from(retries))).await;
                            continue;
                        }
                        return Err(DbError::StorageLocked { message });
                    }
                    return Err(error);
                }
                Err(AttemptError::Commit(error)) => {
                    let mut connection = pool.acquire().await?.detach();
                    if let Some(outcome) = deduplicated_outcome(&mut connection, transition, &transition_json).await? {
                        return Ok(outcome);
                    }
                    drop(connection);
                    if let Some(message) = lock_message(&error) {
                        if retries < MAX_LOCK_RETRIES {
                            retries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(5 * u64::from(retries))).await;
                            continue;
                        }
                        return Err(DbError::StorageLocked { message });
                    }
                    return Err(DbError::CommitUnknown {
                        transition_id: transition.transition_id.clone(),
                        message: error.to_string(),
                    });
                }
            }
        }
    }

    async fn apply_workspace_transition_once(
        &self,
        pool: &SqlitePool,
        transition: &Transition,
        transition_json: &str,
    ) -> std::result::Result<ApplyOutcome, AttemptError> {
        #[cfg(test)]
        if let Some(hooks) = &self.harness_transition_test_hooks {
            hooks.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let mut connection = pool
            .acquire()
            .await
            .map_err(|error| AttemptError::Writes(error.into()))?
            .detach();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut connection)
            .await
            .map_err(|error| AttemptError::Writes(error.into()))?;
        let result = async {
            let outcome = apply_writes(&mut connection, transition, transition_json).await?;
            #[cfg(test)]
            if let Some(hooks) = &self.harness_transition_test_hooks {
                let code = hooks
                    .fail_after_effect_with_code
                    .swap(0, std::sync::atomic::Ordering::SeqCst);
                if code != 0 {
                    return Err(tests::injected_error(code, "injected mid-transaction lock"));
                }
            }
            Ok(outcome)
        }
        .await;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
                return Err(AttemptError::Writes(error));
            }
        };
        #[cfg(test)]
        if self.harness_transition_test_hooks.as_ref().is_some_and(|hooks| {
            hooks
                .fail_commit_before
                .swap(false, std::sync::atomic::Ordering::SeqCst)
        }) {
            let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
            return Err(AttemptError::Commit(tests::injected_error(
                1,
                "injected commit failure before commit",
            )));
        }
        if let Err(error) = sqlx::query("COMMIT").execute(&mut connection).await {
            return Err(AttemptError::Commit(error.into()));
        }
        #[cfg(test)]
        if self
            .harness_transition_test_hooks
            .as_ref()
            .is_some_and(|hooks| hooks.fail_commit_after.swap(false, std::sync::atomic::Ordering::SeqCst))
        {
            return Err(AttemptError::Commit(tests::injected_error(
                5,
                "injected unknown result after commit",
            )));
        }
        Ok(outcome)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        },
    };

    use chrono::{DateTime, Utc};
    use serde_json::json;
    use tinyiothub_core::harness::{
        EffectMutation, EffectState, ExecutionMode, HarnessEventInput, JobProjection, Transition,
        TransitionPrecondition, WakeProjection,
    };

    use crate::{Db, DbError};

    #[derive(Debug, Default)]
    pub(crate) struct HarnessTransitionTestHooks {
        pub fail_after_effect_with_code: AtomicU8,
        pub fail_commit_after: AtomicBool,
        pub fail_commit_before: AtomicBool,
        pub attempts: AtomicUsize,
    }

    #[derive(Debug)]
    struct InjectedDatabaseError {
        code: String,
        message: &'static str,
    }

    impl std::fmt::Display for InjectedDatabaseError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl std::error::Error for InjectedDatabaseError {}

    impl sqlx::error::DatabaseError for InjectedDatabaseError {
        fn message(&self) -> &str {
            self.message
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(std::borrow::Cow::Borrowed(&self.code))
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    pub(crate) fn injected_error(code: u8, message: &'static str) -> DbError {
        DbError::Sqlx(sqlx::Error::Database(Box::new(InjectedDatabaseError {
            code: code.to_string(),
            message,
        })))
    }

    struct Fixture {
        root: PathBuf,
        db: Db,
    }

    impl Fixture {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-effect-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-effects', 'workspace', 'tenant-default-001', 'now', 'now')",
            )
            .execute(db.pool())
            .await
            .expect("insert workspace");
            db.create_workspace_store("ws-effects")
                .await
                .expect("create workspace store");
            let fixture = Self { root, db };
            fixture.seed_effect(EffectState::Dispatching, 4).await;
            fixture
        }

        async fn pool(&self) -> sqlx::SqlitePool {
            self.db.workspace_pool("ws-effects").await.expect("workspace pool")
        }

        async fn seed_effect(&self, state: EffectState, version: i64) {
            sqlx::query(
                "INSERT INTO effect_intents
                    (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                     request_digest, idempotency_key, external_ref, state, recovery_from, state_version, result_json,
                     created_at, updated_at)
                 VALUES ('effect-1', 'session-1', 'task-1', 'run-1', 'call-1', 'set_speed', '{}',
                         'digest-1', 'effect-1', NULL, ?, NULL, ?, NULL, '2026-11-03T00:00:00Z', '2026-11-03T00:00:00Z')",
            )
            .bind(state.as_str())
            .bind(version)
            .execute(&self.pool().await)
            .await
            .expect("seed effect");
            sqlx::query(
                "INSERT INTO device_jobs
                    (id, workspace_id, device_id, tool, args_json, state, stop_requested_at, progress_json, result_json,
                     task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
                 VALUES ('job-1', 'ws-effects', 'device-1', 'set_speed', '{}', 'running', NULL, NULL, NULL,
                         'task-1', 'run-1', 'call-1', NULL, 5000, '2026-11-03T00:00:00Z', NULL)",
            )
            .execute(&self.pool().await)
            .await
            .expect("seed job");
        }

        async fn counts(&self) -> (i64, i64, i64) {
            let pool = self.pool().await;
            let events = sqlx::query_scalar("SELECT COUNT(*) FROM session_events")
                .fetch_one(&pool)
                .await
                .expect("events");
            let wakes = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
                .fetch_one(&pool)
                .await
                .expect("wakes");
            let transitions = sqlx::query_scalar("SELECT COUNT(*) FROM harness_transitions")
                .fetch_one(&pool)
                .await
                .expect("transitions");
            (events, wakes, transitions)
        }

        async fn close(self) {
            self.db
                .close_workspace_store("ws-effects")
                .await
                .expect("close workspace store");
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn transition(id: &str, expected_version: i64) -> Transition {
        Transition {
            transition_id: id.to_string(),
            precondition: TransitionPrecondition {
                effect_id: "effect-1".to_string(),
                expected_state: EffectState::Dispatching,
                expected_version,
            },
            effect: Some(EffectMutation {
                effect_id: "effect-1".to_string(),
                state: EffectState::Completed,
                recovery_from: None,
                state_version: expected_version + 1,
                external_ref: Some("driver-call-1".to_string()),
                result: Some(json!({"confirmed": true})),
                updated_at: at(),
            }),
            event: Some(HarnessEventInput {
                event_id: format!("{id}:event"),
                workspace_id: "ws-effects".to_string(),
                session_id: "session-1".to_string(),
                execution_mode: ExecutionMode::Production,
                run_id: Some("run-1".to_string()),
                task_id: Some("task-1".to_string()),
                event_type: "effect.settled".to_string(),
                occurred_at: at(),
                cause_id: None,
                schema_v: 1,
                payload_digest: format!("{id}:digest"),
                payload_json: json!({"effectId": "effect-1", "state": "completed"}),
            }),
            additional_events: Vec::new(),
            decision: None,
            run_task: None,
            job: Some(JobProjection {
                job_id: "job-1".to_string(),
                task_id: "task-1".to_string(),
                run_id: "run-1".to_string(),
                state: "done".to_string(),
                result_json: Some(json!({"confirmed": true})),
                external_ref: Some("driver-call-1".to_string()),
                ended_at: Some(at()),
            }),
            wake: Some(WakeProjection {
                task_id: "task-1".to_string(),
                wake_id: format!("{id}:wake"),
                signal_json: json!({"effectId": "effect-1"}).to_string(),
                created_at: at(),
            }),
        }
    }

    fn noop(id: &str) -> Transition {
        Transition {
            transition_id: id.to_string(),
            precondition: TransitionPrecondition {
                effect_id: "effect-1".to_string(),
                expected_state: EffectState::Dispatching,
                expected_version: 4,
            },
            effect: None,
            event: None,
            additional_events: Vec::new(),
            decision: None,
            run_task: None,
            job: None,
            wake: None,
        }
    }

    fn expected(version: i64) -> TransitionPrecondition {
        TransitionPrecondition {
            effect_id: "effect-1".to_string(),
            expected_state: EffectState::Dispatching,
            expected_version: version,
        }
    }

    #[tokio::test]
    async fn harness_effect_transition_commits_all_writes_and_dedupes() {
        let fixture = Fixture::new().await;
        let transition = transition("transition-atomic", 4);
        let outcome = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition)
            .await
            .expect("atomic transition");
        assert_eq!(outcome.transition_id, "transition-atomic");
        assert!(!outcome.deduplicated);
        assert_eq!(outcome.state_version, 5);

        let pool = fixture.pool().await;
        let effect: (String, i64, String) = sqlx::query_as(
            "SELECT state, state_version, external_ref FROM effect_intents WHERE effect_id = 'effect-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("effect");
        assert_eq!(effect, ("completed".to_string(), 5, "driver-call-1".to_string()));
        let job: (String, String) = sqlx::query_as("SELECT state, driver_call_id FROM device_jobs WHERE id = 'job-1'")
            .fetch_one(&pool)
            .await
            .expect("job");
        assert_eq!(job, ("done".to_string(), "driver-call-1".to_string()));
        assert_eq!(fixture.counts().await, (1, 1, 1));

        let retry = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition)
            .await
            .expect("stable retry dedupes");
        assert!(retry.deduplicated);
        assert_eq!(retry.state_version, 5);
        assert_eq!(fixture.counts().await, (1, 1, 1));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_transition_revalidates_version_without_partial_writes() {
        let fixture = Fixture::new().await;
        let error = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(3), &transition("transition-stale", 3))
            .await
            .expect_err("stale expected version must conflict");
        assert!(matches!(error, DbError::CasConflict { .. }));
        assert_eq!(fixture.counts().await, (0, 0, 0));
        let state: (String, i64) =
            sqlx::query_as("SELECT state, state_version FROM effect_intents WHERE effect_id = 'effect-1'")
                .fetch_one(&fixture.pool().await)
                .await
                .expect("effect");
        assert_eq!(state, ("dispatching".to_string(), 4));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_noop_ignores_stale_late_terminal_fact() {
        let fixture = Fixture::new().await;
        fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition("transition-terminal", 4))
            .await
            .expect("terminal transition");
        let outcome = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &noop("transition-late"))
            .await
            .expect("late terminal fact is a no-op");
        assert!(outcome.deduplicated);
        assert_eq!(outcome.state_version, 5);
        assert_eq!(fixture.counts().await, (1, 1, 1));
        let state: String = sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = 'effect-1'")
            .fetch_one(&fixture.pool().await)
            .await
            .expect("state");
        assert_eq!(state, "completed");
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_retries_codes_5_and_6_across_whole_transaction() {
        for code in [5, 6] {
            let mut fixture = Fixture::new().await;
            let hooks = Arc::new(HarnessTransitionTestHooks::default());
            hooks.fail_after_effect_with_code.store(code, Ordering::SeqCst);
            fixture.db.harness_transition_test_hooks = Some(hooks.clone());

            let outcome = fixture
                .db
                .apply_workspace_transition(
                    "ws-effects",
                    &expected(4),
                    &transition(&format!("transition-lock-{code}"), 4),
                )
                .await
                .expect("transient lock retries the whole transaction");
            assert_eq!(outcome.state_version, 5);
            assert_eq!(hooks.attempts.load(Ordering::SeqCst), 2);
            assert_eq!(fixture.counts().await, (1, 1, 1));
            let state: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = 'job-1'")
                .fetch_one(&fixture.pool().await)
                .await
                .expect("job");
            assert_eq!(state, "done");
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn harness_effect_commit_unknown_dedupes_before_retry() {
        let mut fixture = Fixture::new().await;
        let hooks = Arc::new(HarnessTransitionTestHooks::default());
        hooks.fail_commit_after.store(true, Ordering::SeqCst);
        fixture.db.harness_transition_test_hooks = Some(hooks.clone());

        let outcome = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition("transition-commit", 4))
            .await
            .expect("committed transaction is discovered by stable IDs");
        assert!(outcome.deduplicated);
        assert_eq!(outcome.state_version, 5);
        assert_eq!(hooks.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.counts().await, (1, 1, 1));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_commit_unknown_without_fact_does_not_blindly_retry() {
        let mut fixture = Fixture::new().await;
        let hooks = Arc::new(HarnessTransitionTestHooks::default());
        hooks.fail_commit_before.store(true, Ordering::SeqCst);
        fixture.db.harness_transition_test_hooks = Some(hooks.clone());

        let error = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition("transition-unknown", 4))
            .await
            .expect_err("unknown commit must remain explicit");
        assert!(matches!(error, DbError::CommitUnknown { .. }));
        assert_eq!(hooks.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.counts().await, (0, 0, 0));
        let version: i64 = sqlx::query_scalar("SELECT state_version FROM effect_intents WHERE effect_id = 'effect-1'")
            .fetch_one(&fixture.pool().await)
            .await
            .expect("version");
        assert_eq!(version, 4);
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_noop_is_rejected_for_nonterminal_state() {
        let fixture = Fixture::new().await;
        let error = fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &noop("transition-premature-noop"))
            .await
            .expect_err("only a terminal effect can accept a late-fact no-op");
        assert!(matches!(error, DbError::CasConflict { .. }));
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_effect_invalid_wake_rolls_back_effect_event_and_job() {
        let fixture = Fixture::new().await;
        let mut transition = transition("transition-bad-wake", 4);
        transition.wake.as_mut().expect("wake").task_id = "task-2".to_string();
        fixture
            .db
            .apply_workspace_transition("ws-effects", &expected(4), &transition)
            .await
            .expect_err("wake identity must match transition");
        assert_eq!(fixture.counts().await, (0, 0, 0));
        let effect_version: i64 =
            sqlx::query_scalar("SELECT state_version FROM effect_intents WHERE effect_id = 'effect-1'")
                .fetch_one(&fixture.pool().await)
                .await
                .expect("effect version");
        let job_state: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = 'job-1'")
            .fetch_one(&fixture.pool().await)
            .await
            .expect("job state");
        assert_eq!(effect_version, 4);
        assert_eq!(job_state, "running");
        fixture.close().await;
    }
}
