use serde::{Deserialize, Serialize};

/// 报警级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlarmLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlarmLevel::Info => "info",
            AlarmLevel::Warning => "warning",
            AlarmLevel::Error => "error",
            AlarmLevel::Critical => "critical",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(AlarmLevel::Info),
            "warning" => Some(AlarmLevel::Warning),
            "error" => Some(AlarmLevel::Error),
            "critical" => Some(AlarmLevel::Critical),
            _ => None,
        }
    }

    /// 转换为事件级别
    pub fn to_event_level(&self) -> crate::domain::event::value_objects::EventLevel {
        match self {
            AlarmLevel::Info => crate::domain::event::value_objects::EventLevel::Info,
            AlarmLevel::Warning => crate::domain::event::value_objects::EventLevel::Warning,
            AlarmLevel::Error => crate::domain::event::value_objects::EventLevel::Error,
            AlarmLevel::Critical => crate::domain::event::value_objects::EventLevel::Critical,
        }
    }

    /// 从事件级别转换
    pub fn from_event_level(level: &crate::domain::event::value_objects::EventLevel) -> Self {
        match level {
            crate::domain::event::value_objects::EventLevel::Debug => AlarmLevel::Info,
            crate::domain::event::value_objects::EventLevel::Info => AlarmLevel::Info,
            crate::domain::event::value_objects::EventLevel::Warning => AlarmLevel::Warning,
            crate::domain::event::value_objects::EventLevel::Error => AlarmLevel::Error,
            crate::domain::event::value_objects::EventLevel::Critical => AlarmLevel::Critical,
        }
    }

    /// 获取优先级（数值越大优先级越高）
    pub fn priority(&self) -> u8 {
        match self {
            AlarmLevel::Info => 1,
            AlarmLevel::Warning => 2,
            AlarmLevel::Error => 3,
            AlarmLevel::Critical => 4,
        }
    }
}

impl std::fmt::Display for AlarmLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
