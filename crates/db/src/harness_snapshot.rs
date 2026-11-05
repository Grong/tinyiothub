//! Projection snapshot persistence (Spec T18).
//!
//! **Canonical state contract.** `projection_snapshots.state_json` MUST be the
//! canonical document produced by
//! `tinyiothub_cloud::domains::agent::workflows::projection_contract::canonical_state`
//! (shape: `projectionName`, `subjectId`, `lastSeq`, `events` with
//! `seq/eventId/type/occurredAt/causeId/payloadDigest/payload`) and verified by
//! `canonical_hash`. The comparison inspector uses the same functions, so a
//! future writer cannot drift silently. Any shape change requires a new
//! `projection_version`; otherwise an existing snapshot becomes a visible
//! `hash_mismatch` instead of being silently accepted.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlSafeStr;
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent};

use crate::{Db, error::Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSubject {
    /// Workspace database containing the subject's session events.
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub execution_mode: ExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionSnapshot {
    pub projection_name: String,
    pub subject_id: String,
    pub last_seq: i64,
    pub projection_version: i64,
    pub state_version: i64,
    /// Canonical projection document; see the module-level contract note.
    pub state_json: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ProjectionSnapshotRow {
    projection_name: String,
    subject_id: String,
    last_seq: i64,
    projection_version: i64,
    state_version: i64,
    state_json: String,
    updated_at: String,
}

impl ProjectionSnapshotRow {
    fn into_snapshot(self) -> Result<ProjectionSnapshot> {
        Ok(ProjectionSnapshot {
            projection_name: self.projection_name,
            subject_id: self.subject_id,
            last_seq: self.last_seq,
            projection_version: self.projection_version,
            state_version: self.state_version,
            state_json: serde_json::from_str(&self.state_json)?,
            updated_at: DateTime::parse_from_rfc3339(&self.updated_at)
                .map_err(|error| crate::DbError::Internal(format!("invalid snapshot updated_at: {error}")))?
                .with_timezone(&Utc),
        })
    }
}

/// The immutable production event slice at one projection boundary plus the
/// subject high-watermark observed in the same read transaction. Concurrent
/// appends change the high-watermark and invalidate a comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionBoundary {
    pub events: Vec<HarnessEvent>,
    pub high_watermark: i64,
}

#[derive(sqlx::FromRow)]
struct ProjectionEventRow {
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

impl ProjectionEventRow {
    fn into_event(self, workspace_id: &str) -> Result<HarnessEvent> {
        crate::harness_event::validate_schema(self.schema_v)?;
        let execution_mode =
            ExecutionMode::from_db(&self.execution_mode).ok_or_else(|| crate::DbError::Validation {
                message: format!("unknown execution mode: {}", self.execution_mode),
            })?;
        let occurred_at = DateTime::parse_from_rfc3339(&self.occurred_at)
            .map_err(|error| crate::DbError::Internal(format!("invalid event occurred_at: {error}")))?
            .with_timezone(&Utc);
        let payload_json: Value = serde_json::from_str(&self.payload_json)?;
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

const PROJECTION_BOUNDARY_SQL: &str = "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at, cause_id, schema_v, payload_digest, payload_json
    FROM session_events
    WHERE execution_mode = ? AND session_id = ? AND seq <= ?
    ORDER BY seq ASC";

const PROJECTION_HIGH_WATERMARK_SQL: &str =
    "SELECT COALESCE(MAX(seq), 0) FROM session_events WHERE execution_mode = ? AND session_id = ?";

/// Workspace migration for `projection_snapshots`. Writers of `state_json`
/// must follow the module-level canonical contract note; the migration itself
/// stores an opaque string and cannot enforce that shape.
pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000006,
        "harness_snapshots".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000006_harness_snapshots.sql").into_sql_str(),
        false,
    )
}

impl Db {
    /// Load only a snapshot with the projection format the caller can consume.
    /// A missing or version-mismatched snapshot requires an explicit rebuild.
    pub async fn projection_snapshot(
        &self,
        workspace_id: &str,
        projection_name: &str,
        subject_id: &str,
        projection_version: i64,
    ) -> Result<Option<ProjectionSnapshot>> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query_as::<_, ProjectionSnapshotRow>(
            "SELECT projection_name, subject_id, last_seq, projection_version, state_version, state_json, updated_at
             FROM projection_snapshots
             WHERE projection_name = ? AND subject_id = ? AND projection_version = ?",
        )
        .bind(projection_name)
        .bind(subject_id)
        .bind(projection_version)
        .fetch_optional(&pool)
        .await?
        .map(ProjectionSnapshotRow::into_snapshot)
        .transpose()
    }

    /// Read the immutable production event slice at one fixed boundary and
    /// the subject high-watermark from the same transaction. The comparison
    /// inspector re-reads the high-watermark after rebuilding so an append
    /// that arrives mid-rebuild is visible instead of silently ignored.
    pub async fn projection_boundary_events(
        &self,
        workspace_id: &str,
        subject_id: &str,
        last_seq: i64,
    ) -> Result<ProjectionBoundary> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let rows = sqlx::query_as::<_, ProjectionEventRow>(PROJECTION_BOUNDARY_SQL)
            .bind(ExecutionMode::Production.as_str())
            .bind(subject_id)
            .bind(last_seq)
            .fetch_all(&mut *tx)
            .await?;
        let high_watermark = sqlx::query_scalar::<_, i64>(PROJECTION_HIGH_WATERMARK_SQL)
            .bind(ExecutionMode::Production.as_str())
            .bind(subject_id)
            .fetch_one(&mut *tx)
            .await?;
        let events = rows
            .into_iter()
            .map(|row| row.into_event(workspace_id))
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(ProjectionBoundary { events, high_watermark })
    }

    /// Subject high-watermark for the post-rebuild concurrency check.
    pub async fn projection_high_watermark(&self, workspace_id: &str, subject_id: &str) -> Result<i64> {
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(sqlx::query_scalar::<_, i64>(PROJECTION_HIGH_WATERMARK_SQL)
            .bind(ExecutionMode::Production.as_str())
            .bind(subject_id)
            .fetch_one(&pool)
            .await?)
    }

    /// CAS-replace exactly one projection snapshot. The WHERE clause carries
    /// the fixed boundary and the observed state version, so a newer snapshot
    /// is never overwritten by an older comparison.
    pub async fn replace_projection_snapshot(
        &self,
        workspace_id: &str,
        snapshot: &ProjectionSnapshot,
        expected_last_seq: i64,
        expected_state_version: i64,
    ) -> Result<ProjectionSnapshot> {
        let pool = self.workspace_pool(workspace_id).await?;
        let updated = sqlx::query(
            "UPDATE projection_snapshots
             SET state_json = ?, state_version = ?, updated_at = ?
             WHERE projection_name = ? AND subject_id = ? AND projection_version = ?
               AND last_seq = ? AND state_version = ?",
        )
        .bind(serde_json::to_string(&snapshot.state_json)?)
        .bind(snapshot.state_version)
        .bind(snapshot.updated_at.to_rfc3339())
        .bind(&snapshot.projection_name)
        .bind(&snapshot.subject_id)
        .bind(snapshot.projection_version)
        .bind(expected_last_seq)
        .bind(expected_state_version)
        .execute(&pool)
        .await?
        .rows_affected();
        if updated == 0 {
            let current = self
                .projection_snapshot(
                    workspace_id,
                    &snapshot.projection_name,
                    &snapshot.subject_id,
                    snapshot.projection_version,
                )
                .await?;
            return Err(match current {
                Some(current) => crate::DbError::CasConflict {
                    message: format!(
                        "projection snapshot advanced: expected last_seq={expected_last_seq} state_version={expected_state_version}, actual last_seq={} state_version={}",
                        current.last_seq, current.state_version
                    ),
                },
                None => crate::DbError::CasConflict {
                    message: "projection snapshot no longer exists".to_string(),
                },
            });
        }
        Ok(snapshot.clone())
    }
}
