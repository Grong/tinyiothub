use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlSafeStr;
use tinyiothub_core::harness::ExecutionMode;

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
}
