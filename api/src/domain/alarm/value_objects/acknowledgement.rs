use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 确认信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acknowledgement {
    pub acknowledged_by: String,
    pub acknowledged_at: DateTime<Utc>,
    pub note: Option<String>,
}

impl Acknowledgement {
    pub fn new(user_id: String, note: Option<String>) -> Self {
        Self { acknowledged_by: user_id, acknowledged_at: Utc::now(), note }
    }
}
