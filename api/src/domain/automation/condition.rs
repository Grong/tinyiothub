//! 自动化条件定义
//! 
//! 支持多种条件类型：阈值、条件组合、设备状态等

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 条件枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Condition {
    /// 阈值条件
    Threshold {
        property: String,
        operator: Operator,
        value: f64,
    },
    
    /// 比较条件
    Comparison {
        property: String,
        operator: Operator,
        value: serde_json::Value,
    },
    
    /// 设备状态条件
    DeviceState {
        device_id: String,
        state: DeviceState,
    },
    
    /// 设备在线条件
    DeviceOnline {
        device_id: String,
        online: bool,
    },
    
    /// 告警条件
    AlarmCondition {
        level: AlarmLevel,
        device_id: Option<String>,
    },
    
    /// AND 条件
    And {
        left: Box<Condition>,
        right: Box<Condition>,
    },
    
    /// OR 条件
    Or {
        left: Box<Condition>,
        right: Box<Condition>,
    },
    
    /// NOT 条件
    Not {
        condition: Box<Condition>,
    },
}

impl Condition {
    /// 从 JSON 字符串解析
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
    
    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// 操作符
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,       // ==
    Neq,      // !=
    Gt,       // >
    Gte,      // >=
    Lt,       // <
    Lte,      // <=
    Contains, // 包含
    StartsWith,
    EndsWith,
    In,
}

impl Operator {
    /// 执行比较
    pub fn compare(&self, left: &serde_json::Value, right: &serde_json::Value) -> bool {
        match self {
            Operator::Eq => left == right,
            Operator::Neq => left != right,
            Operator::Gt => {
                if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
                    l > r
                } else {
                    false
                }
            }
            Operator::Gte => {
                if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
                    l >= r
                } else {
                    false
                }
            }
            Operator::Lt => {
                if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
                    l < r
                } else {
                    false
                }
            }
            Operator::Lte => {
                if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
                    l <= r
                } else {
                    false
                }
            }
            Operator::Contains => {
                if let (Some(l), Some(r)) = (left.as_str(), right.as_str()) {
                    l.contains(r)
                } else {
                    false
                }
            }
            Operator::StartsWith => {
                if let (Some(l), Some(r)) = (left.as_str(), right.as_str()) {
                    l.starts_with(r)
                } else {
                    false
                }
            }
            Operator::EndsWith => {
                if let (Some(l), Some(r)) = (left.as_str(), right.as_str()) {
                    l.ends_with(r)
                } else {
                    false
                }
            }
            Operator::In => {
                if let Some(arr) = right.as_array() {
                    arr.contains(left)
                } else {
                    false
                }
            }
        }
    }
}

/// 设备状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Online,
    Offline,
    Warning,
    Error,
    Unknown,
}

impl DeviceState {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "online" => DeviceState::Online,
            "offline" => DeviceState::Offline,
            "warning" => DeviceState::Warning,
            "error" => DeviceState::Error,
            _ => DeviceState::Unknown,
        }
    }
}

/// 告警级别
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlarmLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "info" => AlarmLevel::Info,
            "warning" => AlarmLevel::Warning,
            "error" => AlarmLevel::Error,
            "critical" => AlarmLevel::Critical,
            _ => AlarmLevel::Warning,
        }
    }
}

/// 触发上下文
/// 条件评估时传入的上下文数据
#[derive(Debug, Clone)]
pub struct TriggerContext {
    /// 设备 ID
    pub device_id: Option<String>,
    /// 设备名称
    pub device_name: Option<String>,
    /// 设备状态
    pub device_state: Option<DeviceState>,
    /// 设备是否在线
    pub device_online: Option<bool>,
    /// 属性值映射
    pub properties: HashMap<String, serde_json::Value>,
    /// 触发类型
    pub trigger_type: TriggerType,
    /// 原始事件数据
    pub event_data: Option<serde_json::Value>,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl TriggerContext {
    pub fn new() -> Self {
        Self {
            device_id: None,
            device_name: None,
            device_state: None,
            device_online: None,
            properties: HashMap::new(),
            trigger_type: TriggerType::Event,
            event_data: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// 从设备数据创建上下文
    pub fn from_device(device: &crate::dto::entity::Device) -> Self {
        let mut ctx = Self::new();
        ctx.device_id = Some(device.id.clone());
        ctx.device_name = Some(device.name.clone());
        ctx.device_online = Some(device.is_online);
        
        // 填充属性
        if let Some(props) = &device.properties {
            for prop in props {
                if let Some(value) = &prop.current_value {
                    ctx.properties.insert(prop.name.clone(), serde_json::Value::String(value.clone()));
                }
            }
        }
        
        ctx
    }
    
    /// 获取属性值
    pub fn get_property(&self, name: &str) -> &serde_json::Value {
        self.properties.get(name).unwrap_or(&serde_json::Value::Null)
    }
}

/// 触发类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Event,
    Cron,
    Manual,
}
