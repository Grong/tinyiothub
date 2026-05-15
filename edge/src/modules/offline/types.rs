use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferPriority {
    Normal = 0,
    Permanent = 1,
}

pub struct BufferMessage {
    pub msg_type: String,
    pub topic: String,
    pub payload: Vec<u8>,
    pub priority: BufferPriority,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferStatus {
    pub total_telemetry: u64,
    pub total_alarms: u64,
    pub oldest_timestamp: Option<i64>,
    pub newest_timestamp: Option<i64>,
}
