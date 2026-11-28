//! Process-local live frames for the current session turn.
//!
//! Live frames are ephemeral: they never enter `session_events`. A reconnect
//! recovers the final content from the durable `assistant/message` settlement.

use dashmap::DashMap;
use serde::Serialize;
use tinyiothub_core::session_protocol::StreamKind;
use tokio::sync::broadcast;

const LIVE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "frame")]
pub enum SessionLiveFrame {
    #[serde(rename = "stream/start")]
    StreamStart {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        #[serde(rename = "stepId")]
        step_id: String,
        #[serde(rename = "attemptId")]
        attempt_id: String,
    },
    #[serde(rename = "stream/delta")]
    StreamDelta {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        #[serde(rename = "stepId")]
        step_id: String,
        kind: StreamKind,
        text: String,
    },
    #[serde(rename = "stream/end")]
    StreamEnd {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        #[serde(rename = "stepId")]
        step_id: String,
        status: String,
    },
}

#[derive(Default)]
pub struct SessionLiveHub {
    channels: DashMap<String, broadcast::Sender<SessionLiveFrame>>,
}

impl SessionLiveHub {
    fn sender(&self, session_id: &str) -> broadcast::Sender<SessionLiveFrame> {
        self.channels
            .entry(session_id.to_string())
            .or_insert_with(|| broadcast::channel(LIVE_CAPACITY).0)
            .clone()
    }

    pub fn publish(&self, session_id: &str, frame: SessionLiveFrame) {
        let _ = self.sender(session_id).send(frame);
    }

    pub fn subscribe(&self, session_id: &str) -> broadcast::Receiver<SessionLiveFrame> {
        self.sender(session_id).subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publishes_and_receives_live_frames() {
        let hub = SessionLiveHub::default();
        let mut rx = hub.subscribe("session-1");
        hub.publish(
            "session-1",
            SessionLiveFrame::StreamDelta {
                stream_id: "t1:s1".into(),
                turn_id: "t1".into(),
                step_id: "t1:s1".into(),
                kind: StreamKind::Text,
                text: "hi".into(),
            },
        );
        let frame = rx.recv().await.expect("frame");
        assert!(matches!(frame, SessionLiveFrame::StreamDelta { text, .. } if text == "hi"));
    }
}
