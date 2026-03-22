use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 解决信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub resolved_by: String,
    pub resolved_at: DateTime<Utc>,
    pub note: Option<String>,
    pub resolution_type: ResolutionType,
}

impl Resolution {
    pub fn new(user_id: String, resolution_type: ResolutionType, note: Option<String>) -> Self {
        Self { resolved_by: user_id, resolved_at: Utc::now(), note, resolution_type }
    }
}

/// 解决方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionType {
    /// 已修复
    Fixed,
    /// 误报
    FalseAlarm,
    /// 忽略
    Ignored,
    /// 自动解决
    AutoResolved,
}

impl ResolutionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResolutionType::Fixed => "fixed",
            ResolutionType::FalseAlarm => "false_alarm",
            ResolutionType::Ignored => "ignored",
            ResolutionType::AutoResolved => "auto_resolved",
        }
    }
}
