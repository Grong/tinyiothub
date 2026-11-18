use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackProtocol {
    Mjpeg,
    Hls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackDescriptor {
    pub protocol: PlaybackProtocol,
    pub url: String,
    pub expires_at: DateTime<Utc>,
}
