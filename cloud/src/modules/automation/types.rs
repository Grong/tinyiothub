// Automation types: conditions + actions
// Migrated from domain/automation/condition.rs + action.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ════════════════════════════════════════════════
// Conditions (from condition.rs)
// ════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Condition {
    Threshold { property: String, operator: Operator, value: f64 },
    Comparison { property: String, operator: Operator, value: serde_json::Value },
    DeviceState { device_id: String, state: DeviceState },
    DeviceOnline { device_id: String, online: bool },
    AlarmCondition { level: AlarmLevel, device_id: Option<String> },
    And { left: Box<Condition>, right: Box<Condition> },
    Or { left: Box<Condition>, right: Box<Condition> },
    Not { condition: Box<Condition> },
}

impl Condition {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> { serde_json::from_str(json) }
    pub fn to_json(&self) -> Result<String, serde_json::Error> { serde_json::to_string(self) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq, Neq, Gt, Gte, Lt, Lte, Contains, StartsWith, EndsWith, In,
}

impl Operator {
    pub fn compare(&self, left: &serde_json::Value, right: &serde_json::Value) -> bool {
        match self {
            Operator::Eq => left == right,
            Operator::Neq => left != right,
            Operator::Gt => matches!((left.as_f64(), right.as_f64()), (Some(l), Some(r)) if l > r),
            Operator::Gte => matches!((left.as_f64(), right.as_f64()), (Some(l), Some(r)) if l >= r),
            Operator::Lt => matches!((left.as_f64(), right.as_f64()), (Some(l), Some(r)) if l < r),
            Operator::Lte => matches!((left.as_f64(), right.as_f64()), (Some(l), Some(r)) if l <= r),
            Operator::Contains => matches!((left.as_str(), right.as_str()), (Some(l), Some(r)) if l.contains(r)),
            Operator::StartsWith => matches!((left.as_str(), right.as_str()), (Some(l), Some(r)) if l.starts_with(r)),
            Operator::EndsWith => matches!((left.as_str(), right.as_str()), (Some(l), Some(r)) if l.ends_with(r)),
            Operator::In => right.as_array().map_or(false, |arr| arr.contains(left)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState { Online, Offline, Warning, Error, Unknown }

impl DeviceState {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "online" => DeviceState::Online,
            "offline" => DeviceState::Offline,
            "warning" => DeviceState::Warning,
            "error" => DeviceState::Error,
            _ => DeviceState::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmLevel { Info, Warning, Error, Critical }

impl AlarmLevel {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "info" => AlarmLevel::Info,
            "warning" => AlarmLevel::Warning,
            "error" => AlarmLevel::Error,
            "critical" => AlarmLevel::Critical,
            _ => AlarmLevel::Warning,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TriggerContext {
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub device_state: Option<DeviceState>,
    pub device_online: Option<bool>,
    pub properties: HashMap<String, serde_json::Value>,
    pub trigger_type: TriggerType,
    pub event_data: Option<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl TriggerContext {
    pub fn new() -> Self {
        Self {
            device_id: None, device_name: None, device_state: None, device_online: None,
            properties: HashMap::new(), trigger_type: TriggerType::Event,
            event_data: None, timestamp: chrono::Utc::now(),
        }
    }

    pub fn from_device(device: &tinyiothub_core::models::device::Device) -> Self {
        let mut ctx = Self::new();
        ctx.device_id = Some(device.id.clone());
        ctx.device_name = Some(device.name.clone());
        ctx.device_online = Some(device.is_online());
        if let Some(props) = &device.properties {
            for prop in props {
                if let Some(value) = &prop.current_value {
                    ctx.properties.insert(prop.name.clone(), serde_json::Value::String(value.clone()));
                }
            }
        }
        ctx
    }

    pub fn get_property(&self, name: &str) -> &serde_json::Value {
        self.properties.get(name).unwrap_or(&serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType { Event, Cron, Manual }

// ════════════════════════════════════════════════
// Actions (from action.rs)
// ════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Action {
    Alarm { level: AlarmLevel, message: String },
    ControlDevice { device_id: String, command: String, parameters: Option<HashMap<String, String>> },
    SetProperty { device_id: String, property: String, value: String },
    PowerOn { device_id: String },
    PowerOff { device_id: String },
    Notify { channel: NotifyChannel, title: String, content: String },
    SendEmail { to: Vec<String>, subject: String, body: String },
    HttpRequest { method: HttpMethod, url: String, headers: Option<HashMap<String, String>>, body: Option<String> },
    Forward { endpoint: String, format: DataFormat },
    Delay { duration_ms: u64 },
    Conditional { condition: Condition, then_actions: Vec<Action>, else_actions: Option<Vec<Action>> },
    Script { interpreter: ScriptInterpreter, script: String },
}

impl Action {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> { serde_json::from_str(json) }
    pub fn to_json(&self) -> Result<String, serde_json::Error> { serde_json::to_string(self) }
    pub fn action_type(&self) -> &'static str {
        match self {
            Action::Alarm { .. } => "alarm",
            Action::ControlDevice { .. } => "control_device",
            Action::SetProperty { .. } => "set_property",
            Action::PowerOn { .. } => "power_on",
            Action::PowerOff { .. } => "power_off",
            Action::Notify { .. } => "notify",
            Action::SendEmail { .. } => "send_email",
            Action::HttpRequest { .. } => "http_request",
            Action::Forward { .. } => "forward",
            Action::Delay { .. } => "delay",
            Action::Conditional { .. } => "conditional",
            Action::Script { .. } => "script",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel { Email, Sms, Webhook, Mqtt, System }

impl NotifyChannel {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "email" => NotifyChannel::Email,
            "sms" => NotifyChannel::Sms,
            "webhook" => NotifyChannel::Webhook,
            "mqtt" => NotifyChannel::Mqtt,
            _ => NotifyChannel::System,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod { Get, Post, Put, Delete, Patch }

impl HttpMethod {
    pub fn parse_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => HttpMethod::Get, "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put, "DELETE" => HttpMethod::Delete,
            "PATCH" => HttpMethod::Patch, _ => HttpMethod::Get,
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET", HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT", HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat { Json, Csv, Xml }

impl DataFormat {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => DataFormat::Json, "csv" => DataFormat::Csv,
            "xml" => DataFormat::Xml, _ => DataFormat::Json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptInterpreter { Bash, Python, PowerShell, Cmd }

impl ScriptInterpreter {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bash" | "sh" => ScriptInterpreter::Bash,
            "python" | "python3" => ScriptInterpreter::Python,
            "powershell" | "ps" => ScriptInterpreter::PowerShell,
            "cmd" | "batch" => ScriptInterpreter::Cmd,
            _ => ScriptInterpreter::Bash,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ActionResult {
    pub action_type: String,
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
    pub execution_time_ms: u64,
}

impl ActionResult {
    pub fn success(action_type: &str, message: &str) -> Self {
        Self { action_type: action_type.to_string(), success: true, message: Some(message.to_string()), data: None, execution_time_ms: 0 }
    }
    pub fn failure(action_type: &str, message: &str) -> Self {
        Self { action_type: action_type.to_string(), success: false, message: Some(message.to_string()), data: None, execution_time_ms: 0 }
    }
}

pub fn parse_actions(json: &str) -> Result<Vec<Action>, serde_json::Error> { serde_json::from_str(json) }
pub fn actions_to_json(actions: &[Action]) -> Result<String, serde_json::Error> { serde_json::to_string(actions) }
