//! Session projection reads.
//!
//! The event log is the durable source. This module folds the control-plane
//! events into the same TurnProjection shape used by the runner, exposing a
//! cursor (last projected seq) and the subject high-watermark.

use serde::Serialize;
use tinyiothub_agent::harness::TurnProjection;
use tinyiothub_storage::{Db, DbError};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjectionView {
    pub turn: TurnProjection,
    pub high_watermark: i64,
}

pub async fn load_session_projection(
    db: &Db,
    workspace_id: &str,
    session_key: &str,
) -> Result<SessionProjectionView, DbError> {
    let boundary = db
        .projection_boundary_events(workspace_id, session_key, i64::MAX)
        .await?;
    let mut turn = TurnProjection::default();
    for event in &boundary.events {
        turn.apply(event.seq, &event.event_type, &event.payload_json);
    }
    Ok(SessionProjectionView {
        turn,
        high_watermark: boundary.high_watermark,
    })
}
