use serde::{Deserialize, Serialize};

/// 报警状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmStatus {
    /// 活跃（未确认）
    Active,
    /// 已确认（未解决）
    Acknowledged,
    /// 已解决
    Resolved,
    /// 已抑制
    Suppressed,
}

impl AlarmStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlarmStatus::Active => "active",
            AlarmStatus::Acknowledged => "acknowledged",
            AlarmStatus::Resolved => "resolved",
            AlarmStatus::Suppressed => "suppressed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "active" => Some(AlarmStatus::Active),
            "acknowledged" => Some(AlarmStatus::Acknowledged),
            "resolved" => Some(AlarmStatus::Resolved),
            "suppressed" => Some(AlarmStatus::Suppressed),
            _ => None,
        }
    }

    /// 是否是活跃状态
    pub fn is_active(&self) -> bool {
        matches!(self, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }

    /// 是否已解决
    pub fn is_resolved(&self) -> bool {
        matches!(self, AlarmStatus::Resolved)
    }
}

impl std::fmt::Display for AlarmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
