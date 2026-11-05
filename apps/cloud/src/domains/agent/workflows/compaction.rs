//! Fixed-boundary projection comparison and explicit CAS replacement (Spec T18).
//!
//! A comparison rebuilds a temporary projection from the immutable production
//! event slice at `projection_version + last_seq`, canonicalizes it, and
//! compares the canonical hash with the snapshot at the same boundary. Only
//! an exact match can replace the snapshot, and the replacement is a CAS on
//! the observed `last_seq`/`state_version`. Session facts, Task/Run state,
//! Effect/Decision rows and external systems are never written and no Effect
//! is dispatched.

use std::sync::Arc;

use chrono::Utc;
use futures_util::future::BoxFuture;
use serde::Serialize;
use serde_json::Value;
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_snapshot::ProjectionSnapshot;

use super::projection_contract::{canonical_hash, canonical_state};

/// Optional test seam invoked between the fixed-boundary read and the
/// post-rebuild high-watermark recheck. Production callers never set it.
pub type CompareBarrier = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionComparisonStatus {
    /// Canonical hashes match at the same boundary and version.
    Matched,
    /// A snapshot exists at the boundary but its canonical hash differs.
    HashMismatch,
    /// No snapshot exists for this projection/version.
    SnapshotMissing,
    /// The current snapshot already covers a later event boundary.
    SnapshotAdvanced,
    /// The current snapshot is fixed at a different (older) boundary.
    BoundaryMismatch,
    /// The requested boundary has not been observed yet.
    BoundaryUnavailable,
    /// An append arrived while the temporary projection was being rebuilt.
    StaleAppend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionEvidence {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub reference: String,
}

/// One locatable canonical difference. Paths use JSON Pointer syntax.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionDifference {
    pub path: String,
    pub rebuilt: Option<Value>,
    pub snapshot: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionComparison {
    #[serde(skip_serializing)]
    pub workspace_id: String,
    pub projection_name: String,
    pub subject_id: String,
    pub projection_version: i64,
    /// The explicit boundary both rebuild and snapshot must share.
    pub boundary_last_seq: i64,
    pub snapshot_last_seq: Option<i64>,
    pub snapshot_state_version: Option<i64>,
    pub seq_first: Option<i64>,
    pub seq_last: Option<i64>,
    /// Highest boundary seq proven complete by the read.
    pub last_complete_seq: i64,
    pub observed_high_watermark: i64,
    pub appended_during_compare: bool,
    pub status: ProjectionComparisonStatus,
    pub matched: bool,
    pub canonical_hash: String,
    pub snapshot_canonical_hash: Option<String>,
    pub canonical_state: Value,
    pub differences: Vec<ProjectionDifference>,
    pub evidence_refs: Vec<ProjectionEvidence>,
}

#[derive(Clone)]
pub struct ProjectionInspector {
    db: Db,
    workspace_id: String,
    compare_barrier: Option<CompareBarrier>,
}

impl ProjectionInspector {
    pub fn new(db: Db, workspace_id: impl Into<String>) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
            compare_barrier: None,
        }
    }

    /// Test seam for the concurrent-append case; see [`CompareBarrier`].
    #[doc(hidden)]
    pub fn with_compare_barrier(mut self, barrier: CompareBarrier) -> Self {
        self.compare_barrier = Some(barrier);
        self
    }

    /// Rebuild a temporary projection at the fixed boundary and compare its
    /// canonical hash with the current snapshot at the same boundary.
    /// `subject_id` is the production session whose facts the projection
    /// covers; simulation events and all durable facts stay read-only. No
    /// event, Task/Run, Effect/Decision or external write happens here.
    pub async fn compare(
        &self,
        workspace_id: &str,
        projection_name: &str,
        subject_id: &str,
        last_seq: i64,
        projection_version: i64,
    ) -> Result<ProjectionComparison, DbError> {
        if workspace_id != self.workspace_id {
            return Err(DbError::Validation {
                message: "workspace does not match this projection inspector".to_string(),
            });
        }
        if projection_name.trim().is_empty() {
            return Err(DbError::Validation {
                message: "projection_name must not be blank".to_string(),
            });
        }
        if subject_id.trim().is_empty() {
            return Err(DbError::Validation {
                message: "subject_id must not be blank".to_string(),
            });
        }
        if last_seq < 0 {
            return Err(DbError::Validation {
                message: "last_seq must not be negative".to_string(),
            });
        }
        if projection_version <= 0 {
            return Err(DbError::Validation {
                message: "projection_version must be positive".to_string(),
            });
        }

        let boundary = self
            .db
            .projection_boundary_events(workspace_id, subject_id, last_seq)
            .await?;
        let snapshot = self
            .db
            .projection_snapshot(workspace_id, projection_name, subject_id, projection_version)
            .await?;
        if let Some(barrier) = &self.compare_barrier {
            barrier().await;
        }
        let high_watermark_after = self.db.projection_high_watermark(workspace_id, subject_id).await?;
        let appended_during_compare = high_watermark_after != boundary.high_watermark;

        let canonical_state = canonical_state(projection_name, subject_id, last_seq, &boundary.events);
        let rebuilt_hash = canonical_hash(&canonical_state);
        let snapshot_canonical_hash = snapshot.as_ref().map(|snapshot| canonical_hash(&snapshot.state_json));

        let status = if appended_during_compare {
            ProjectionComparisonStatus::StaleAppend
        } else if last_seq > boundary.high_watermark {
            ProjectionComparisonStatus::BoundaryUnavailable
        } else {
            match &snapshot {
                None => ProjectionComparisonStatus::SnapshotMissing,
                Some(snapshot) if snapshot.last_seq > last_seq => ProjectionComparisonStatus::SnapshotAdvanced,
                Some(snapshot) if snapshot.last_seq < last_seq => ProjectionComparisonStatus::BoundaryMismatch,
                Some(_) if snapshot_canonical_hash.as_deref() == Some(rebuilt_hash.as_str()) => {
                    ProjectionComparisonStatus::Matched
                }
                Some(_) => ProjectionComparisonStatus::HashMismatch,
            }
        };

        let differences = match &snapshot {
            Some(snapshot) => differences(&canonical_state, &snapshot.state_json),
            None => Vec::new(),
        };
        let evidence_refs = boundary
            .events
            .iter()
            .map(|event| ProjectionEvidence {
                seq: event.seq,
                event_id: event.event_id.clone(),
                event_type: event.event_type.clone(),
                reference: format!("event:{}@{}", event.event_id, event.seq),
            })
            .collect();

        Ok(ProjectionComparison {
            workspace_id: workspace_id.to_string(),
            projection_name: projection_name.to_string(),
            subject_id: subject_id.to_string(),
            projection_version,
            boundary_last_seq: last_seq,
            snapshot_last_seq: snapshot.as_ref().map(|snapshot| snapshot.last_seq),
            snapshot_state_version: snapshot.as_ref().map(|snapshot| snapshot.state_version),
            seq_first: boundary.events.first().map(|event| event.seq),
            seq_last: boundary.events.last().map(|event| event.seq),
            // The highest production seq actually present in the slice. The
            // requested boundary can fall inside a cross-session seq gap, so
            // echoing `last_seq` here would overstate what was rebuilt.
            last_complete_seq: boundary.events.last().map(|event| event.seq).unwrap_or(0),
            observed_high_watermark: high_watermark_after,
            appended_during_compare,
            status,
            matched: status == ProjectionComparisonStatus::Matched,
            canonical_hash: rebuilt_hash,
            snapshot_canonical_hash,
            canonical_state,
            differences,
            evidence_refs,
        })
    }

    /// Replace the snapshot only for an exact same-boundary match, using the
    /// observed `last_seq`/`state_version` as the CAS precondition. The latest
    /// snapshot always wins over an older comparison.
    pub async fn replace_snapshot(
        &self,
        comparison: &ProjectionComparison,
        expected_last_seq: i64,
        expected_state_version: i64,
    ) -> Result<ProjectionSnapshot, DbError> {
        if comparison.workspace_id != self.workspace_id {
            return Err(DbError::Validation {
                message: "workspace does not match this projection inspector".to_string(),
            });
        }
        if comparison.status != ProjectionComparisonStatus::Matched || !comparison.matched {
            return Err(DbError::Conflict {
                message: "only an exact same-boundary match may replace the projection snapshot".to_string(),
            });
        }
        if comparison.snapshot_canonical_hash.as_deref() != Some(comparison.canonical_hash.as_str()) {
            return Err(DbError::Conflict {
                message: "canonical hashes do not match; the snapshot must be compared again".to_string(),
            });
        }
        if comparison.boundary_last_seq != expected_last_seq || comparison.snapshot_last_seq != Some(expected_last_seq)
        {
            return Err(DbError::CasConflict {
                message: format!(
                    "last_seq changed: expected {expected_last_seq}, boundary {}",
                    comparison.boundary_last_seq
                ),
            });
        }
        if comparison.snapshot_state_version != Some(expected_state_version) {
            return Err(DbError::CasConflict {
                message: format!(
                    "state_version changed: expected {expected_state_version}, compared {:?}",
                    comparison.snapshot_state_version
                ),
            });
        }

        let snapshot = ProjectionSnapshot {
            projection_name: comparison.projection_name.clone(),
            subject_id: comparison.subject_id.clone(),
            last_seq: comparison.boundary_last_seq,
            projection_version: comparison.projection_version,
            state_version: expected_state_version + 1,
            state_json: comparison.canonical_state.clone(),
            updated_at: Utc::now(),
        };
        self.db
            .replace_projection_snapshot(
                &comparison.workspace_id,
                &snapshot,
                expected_last_seq,
                expected_state_version,
            )
            .await
    }
}

fn differences(rebuilt: &Value, snapshot: &Value) -> Vec<ProjectionDifference> {
    let mut found = Vec::new();
    collect_differences("", rebuilt, snapshot, &mut found);
    found
}

fn collect_differences(path: &str, rebuilt: &Value, snapshot: &Value, found: &mut Vec<ProjectionDifference>) {
    match (rebuilt, snapshot) {
        (Value::Object(rebuilt), Value::Object(snapshot)) => {
            let mut keys: Vec<&String> = rebuilt.keys().chain(snapshot.keys()).collect();
            keys.sort_unstable();
            keys.dedup();
            for key in keys {
                let next = format!("{path}/{}", escape_pointer(key));
                match (rebuilt.get(key), snapshot.get(key)) {
                    (Some(rebuilt), Some(snapshot)) => collect_differences(&next, rebuilt, snapshot, found),
                    (Some(rebuilt), None) => found.push(ProjectionDifference {
                        path: next,
                        rebuilt: Some(rebuilt.clone()),
                        snapshot: None,
                    }),
                    (None, Some(snapshot)) => found.push(ProjectionDifference {
                        path: next,
                        rebuilt: None,
                        snapshot: Some(snapshot.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(rebuilt), Value::Array(snapshot)) => {
            for index in 0..rebuilt.len().max(snapshot.len()) {
                let next = format!("{path}/{index}");
                match (rebuilt.get(index), snapshot.get(index)) {
                    (Some(rebuilt), Some(snapshot)) => collect_differences(&next, rebuilt, snapshot, found),
                    (Some(rebuilt), None) => found.push(ProjectionDifference {
                        path: next,
                        rebuilt: Some(rebuilt.clone()),
                        snapshot: None,
                    }),
                    (None, Some(snapshot)) => found.push(ProjectionDifference {
                        path: next,
                        rebuilt: None,
                        snapshot: Some(snapshot.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ if rebuilt == snapshot => {}
        _ => found.push(ProjectionDifference {
            path: path.to_string(),
            rebuilt: Some(rebuilt.clone()),
            snapshot: Some(snapshot.clone()),
        }),
    }
}

fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::differences;

    #[test]
    fn differences_are_locatable_json_pointers() {
        let rebuilt = json!({"events": [{"payload": {"text": "one"}}]});
        let snapshot = json!({"events": [{"payload": {"text": "two"}}]});
        let found = differences(&rebuilt, &snapshot);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "/events/0/payload/text");
        assert_eq!(found[0].rebuilt, Some(json!("one")));
        assert_eq!(found[0].snapshot, Some(json!("two")));
    }
}
