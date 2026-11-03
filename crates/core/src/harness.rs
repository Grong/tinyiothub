use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Production,
    Simulation,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Simulation => "simulation",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "production" => Some(Self::Production),
            "simulation" => Some(Self::Simulation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEvent {
    pub seq: i64,
    pub event_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub execution_mode: ExecutionMode,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub cause_id: Option<i64>,
    pub schema_v: i64,
    pub payload_digest: String,
    pub payload_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEventInput {
    pub event_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub execution_mode: ExecutionMode,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub cause_id: Option<i64>,
    pub schema_v: i64,
    pub payload_digest: String,
    pub payload_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFactsSnapshot {
    pub observed_at: DateTime<Utc>,
    pub declaration_hash: String,
    pub driver_version: String,
    pub facts: Value,
}
