//! Session projection reads.
//!
//! The event log is the durable source. This module folds the control-plane
//! events into the same TurnProjection shape used by the runner, exposing a
//! cursor (last projected seq) and the subject high-watermark. The snapshot is
//! only a cache: a missing, stale, or invalid snapshot falls back to a full fold.

use serde::Serialize;
use tinyiothub_agent::harness::TurnProjection;
use tinyiothub_storage::{Db, DbError};

const PROJECTION_NAME: &str = "turn.v1";
const PROJECTION_VERSION: i64 = 1;

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
    let snapshot = db
        .session_projection_snapshot(workspace_id, PROJECTION_NAME, session_key)
        .await?;
    let cached = snapshot.as_ref().and_then(|snapshot| {
        (snapshot.projection_version == PROJECTION_VERSION)
            .then(|| serde_json::from_value::<TurnProjection>(snapshot.state_json.clone()).ok())
            .flatten()
            .map(|turn| (turn, snapshot.last_seq))
    });

    let (mut turn, after) = match cached {
        Some((turn, last_seq)) => (turn, last_seq),
        None => (TurnProjection::default(), 0),
    };

    let (events, high_watermark) = if after > 0 {
        let events = db
            .session_projection_events_after(workspace_id, session_key, after)
            .await?;
        let high = db.projection_high_watermark(workspace_id, session_key).await?;
        (events, high)
    } else {
        let boundary = db
            .projection_boundary_events(workspace_id, session_key, i64::MAX)
            .await?;
        (boundary.events, boundary.high_watermark)
    };

    for event in &events {
        turn.apply(event.seq, &event.event_type, &event.payload_json);
    }
    if turn.last_seq < after {
        turn.last_seq = after;
    }

    let state_json = serde_json::to_value(&turn)?;
    let _ = db
        .replace_session_projection_snapshot(
            workspace_id,
            PROJECTION_NAME,
            session_key,
            high_watermark.max(turn.last_seq),
            PROJECTION_VERSION,
            &state_json,
        )
        .await;

    Ok(SessionProjectionView { turn, high_watermark })
}
