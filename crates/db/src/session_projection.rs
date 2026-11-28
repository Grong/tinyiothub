//! Durable session-projection snapshots.
//!
//! The event log remains the source of truth. A snapshot only caches the fold
//! up to `last_seq`; readers replay the suffix after that boundary.

use chrono::Utc;
use serde_json::Value;
use tinyiothub_core::harness::HarnessEvent;

use crate::{Db, error::Result};

#[derive(Debug, Clone)]
pub struct SessionProjectionSnapshot {
    pub last_seq: i64,
    pub projection_version: i64,
    pub state_json: Value,
}

impl Db {
    pub async fn session_projection_snapshot(
        &self,
        workspace_id: &str,
        projection_name: &str,
        subject_id: &str,
    ) -> Result<Option<SessionProjectionSnapshot>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let row = sqlx::query_as::<_, (i64, i64, String)>(
            "SELECT last_seq, projection_version, state_json FROM session_projection_snapshots
             WHERE projection_name = ? AND subject_id = ?",
        )
        .bind(projection_name)
        .bind(subject_id)
        .fetch_optional(&pool)
        .await?;
        row.map(|(last_seq, projection_version, state_json)| {
            Ok(SessionProjectionSnapshot {
                last_seq,
                projection_version,
                state_json: serde_json::from_str(&state_json)?,
            })
        })
        .transpose()
    }

    pub async fn replace_session_projection_snapshot(
        &self,
        workspace_id: &str,
        projection_name: &str,
        subject_id: &str,
        last_seq: i64,
        projection_version: i64,
        state_json: &Value,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query(
            "INSERT INTO session_projection_snapshots
                 (projection_name, subject_id, last_seq, projection_version, state_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(projection_name, subject_id) DO UPDATE SET
                 last_seq = excluded.last_seq,
                 projection_version = excluded.projection_version,
                 state_json = excluded.state_json,
                 updated_at = excluded.updated_at
             WHERE excluded.last_seq > session_projection_snapshots.last_seq",
        )
        .bind(projection_name)
        .bind(subject_id)
        .bind(last_seq)
        .bind(projection_version)
        .bind(serde_json::to_string(state_json)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await?;
        Ok(())
    }

    /// Production events after a snapshot boundary, in seq order.
    pub async fn session_projection_events_after(
        &self,
        workspace_id: &str,
        session_id: &str,
        after: i64,
    ) -> Result<Vec<HarnessEvent>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, crate::harness_event::SessionEventRow>(
            "SELECT seq, event_id, session_id, execution_mode, run_id, task_id, type, occurred_at,
                    cause_id, schema_v, payload_digest, payload_json
             FROM session_events
             WHERE execution_mode = 'production' AND session_id = ? AND seq > ?
             ORDER BY seq ASC",
        )
        .bind(session_id)
        .bind(after)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(|row| row.into_event(workspace_id)).collect()
    }
}
