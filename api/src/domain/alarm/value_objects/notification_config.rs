use crate::domain::event::aggregates::NotificationChannelType;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 通知配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub channels: Vec<NotificationChannelType>,
    pub recipients: Vec<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "optional_duration_serde",
        default
    )]
    pub suppress_duration: Option<Duration>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "optional_duration_serde",
        default
    )]
    pub repeat_interval: Option<Duration>,
}

// Optional Duration 序列化辅助模块
mod optional_duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => serializer.serialize_some(&d.as_secs()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<u64> = Option::deserialize(deserializer)?;
        Ok(opt.map(Duration::from_secs))
    }
}
