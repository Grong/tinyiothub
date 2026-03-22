use serde::{Deserialize, Serialize};

/// 报警类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmType {
    /// 设备离线
    DeviceOffline,
    /// 设备错误
    DeviceError,
    /// 属性阈值
    PropertyThreshold,
    /// 属性异常
    PropertyAnomaly,
    /// 命令失败
    CommandFailed,
    /// 自定义类型
    Custom { name: String },
}

impl AlarmType {
    pub fn as_str(&self) -> String {
        match self {
            AlarmType::DeviceOffline => "device_offline".to_string(),
            AlarmType::DeviceError => "device_error".to_string(),
            AlarmType::PropertyThreshold => "property_threshold".to_string(),
            AlarmType::PropertyAnomaly => "property_anomaly".to_string(),
            AlarmType::CommandFailed => "command_failed".to_string(),
            AlarmType::Custom { name } => format!("custom_{}", name),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "device_offline" => AlarmType::DeviceOffline,
            "device_error" => AlarmType::DeviceError,
            "property_threshold" => AlarmType::PropertyThreshold,
            "property_anomaly" => AlarmType::PropertyAnomaly,
            "command_failed" => AlarmType::CommandFailed,
            s if s.starts_with("custom_") => {
                AlarmType::Custom { name: s.strip_prefix("custom_").unwrap_or(s).to_string() }
            }
            _ => AlarmType::Custom { name: s.to_string() },
        }
    }
}

impl std::fmt::Display for AlarmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
