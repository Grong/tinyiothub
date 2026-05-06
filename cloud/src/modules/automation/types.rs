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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_state_parse_str() {
        assert_eq!(DeviceState::parse_str("online"), DeviceState::Online);
        assert_eq!(DeviceState::parse_str("offline"), DeviceState::Offline);
        assert_eq!(DeviceState::parse_str("warning"), DeviceState::Warning);
        assert_eq!(DeviceState::parse_str("error"), DeviceState::Error);
        assert_eq!(DeviceState::parse_str("unknown"), DeviceState::Unknown);
        assert_eq!(DeviceState::parse_str("UNKNOWN"), DeviceState::Unknown);
    }

    #[test]
    fn test_alarm_level_parse_str() {
        assert!(matches!(AlarmLevel::parse_str("info"), AlarmLevel::Info));
        assert!(matches!(AlarmLevel::parse_str("warning"), AlarmLevel::Warning));
        assert!(matches!(AlarmLevel::parse_str("error"), AlarmLevel::Error));
        assert!(matches!(AlarmLevel::parse_str("critical"), AlarmLevel::Critical));
        assert!(matches!(AlarmLevel::parse_str("unknown"), AlarmLevel::Warning));
    }

    #[test]
    fn test_operator_compare_eq() {
        let op = Operator::Eq;
        assert!(op.compare(&serde_json::json!(42), &serde_json::json!(42)));
        assert!(!op.compare(&serde_json::json!(42), &serde_json::json!(43)));
    }

    #[test]
    fn test_operator_compare_neq() {
        let op = Operator::Neq;
        assert!(op.compare(&serde_json::json!(1), &serde_json::json!(2)));
        assert!(!op.compare(&serde_json::json!(1), &serde_json::json!(1)));
    }

    #[test]
    fn test_operator_compare_gt() {
        let op = Operator::Gt;
        assert!(op.compare(&serde_json::json!(5.0), &serde_json::json!(3.0)));
        assert!(!op.compare(&serde_json::json!(3.0), &serde_json::json!(5.0)));
        assert!(!op.compare(&serde_json::json!("not a number"), &serde_json::json!(3.0)));
    }

    #[test]
    fn test_operator_compare_gte() {
        let op = Operator::Gte;
        assert!(op.compare(&serde_json::json!(5.0), &serde_json::json!(3.0)));
        assert!(op.compare(&serde_json::json!(3.0), &serde_json::json!(3.0)));
        assert!(!op.compare(&serde_json::json!(2.0), &serde_json::json!(3.0)));
    }

    #[test]
    fn test_operator_compare_lt() {
        let op = Operator::Lt;
        assert!(op.compare(&serde_json::json!(2.0), &serde_json::json!(5.0)));
        assert!(!op.compare(&serde_json::json!(5.0), &serde_json::json!(2.0)));
    }

    #[test]
    fn test_operator_compare_lte() {
        let op = Operator::Lte;
        assert!(op.compare(&serde_json::json!(2.0), &serde_json::json!(5.0)));
        assert!(op.compare(&serde_json::json!(5.0), &serde_json::json!(5.0)));
        assert!(!op.compare(&serde_json::json!(6.0), &serde_json::json!(5.0)));
    }

    #[test]
    fn test_operator_compare_contains() {
        let op = Operator::Contains;
        assert!(op.compare(&serde_json::json!("hello world"), &serde_json::json!("world")));
        assert!(!op.compare(&serde_json::json!("hello"), &serde_json::json!("xyz")));
        assert!(!op.compare(&serde_json::json!(123), &serde_json::json!("xyz")));
    }

    #[test]
    fn test_operator_compare_starts_with() {
        let op = Operator::StartsWith;
        assert!(op.compare(&serde_json::json!("hello world"), &serde_json::json!("hello")));
        assert!(!op.compare(&serde_json::json!("hello"), &serde_json::json!("world")));
    }

    #[test]
    fn test_operator_compare_ends_with() {
        let op = Operator::EndsWith;
        assert!(op.compare(&serde_json::json!("hello world"), &serde_json::json!("world")));
        assert!(!op.compare(&serde_json::json!("hello"), &serde_json::json!("world")));
    }

    #[test]
    fn test_operator_compare_in() {
        let op = Operator::In;
        let arr = serde_json::json!([1, 2, 3]);
        assert!(op.compare(&serde_json::json!(2), &arr));
        assert!(!op.compare(&serde_json::json!(5), &arr));
        assert!(!op.compare(&serde_json::json!(2), &serde_json::json!("not array")));
    }

    #[test]
    fn test_trigger_context_new() {
        let ctx = TriggerContext::new();
        assert!(ctx.device_id.is_none());
        assert!(ctx.properties.is_empty());
        assert!(matches!(ctx.trigger_type, TriggerType::Event));
    }

    #[test]
    fn test_trigger_context_get_property_existing() {
        let mut ctx = TriggerContext::new();
        ctx.properties.insert("temp".to_string(), serde_json::json!(25.5));
        assert_eq!(ctx.get_property("temp"), &serde_json::json!(25.5));
    }

    #[test]
    fn test_trigger_context_get_property_missing() {
        let ctx = TriggerContext::new();
        assert_eq!(ctx.get_property("missing"), &serde_json::Value::Null);
    }

    #[test]
    fn test_action_action_type() {
        assert_eq!(Action::Alarm { level: AlarmLevel::Info, message: "test".to_string() }.action_type(), "alarm");
        assert_eq!(Action::ControlDevice { device_id: "d1".to_string(), command: "on".to_string(), parameters: None }.action_type(), "control_device");
        assert_eq!(Action::SetProperty { device_id: "d1".to_string(), property: "temp".to_string(), value: "25".to_string() }.action_type(), "set_property");
        assert_eq!(Action::PowerOn { device_id: "d1".to_string() }.action_type(), "power_on");
        assert_eq!(Action::PowerOff { device_id: "d1".to_string() }.action_type(), "power_off");
        assert_eq!(Action::Notify { channel: NotifyChannel::Email, title: "t".to_string(), content: "c".to_string() }.action_type(), "notify");
        assert_eq!(Action::SendEmail { to: vec![], subject: "s".to_string(), body: "b".to_string() }.action_type(), "send_email");
        assert_eq!(Action::HttpRequest { method: HttpMethod::Get, url: "u".to_string(), headers: None, body: None }.action_type(), "http_request");
        assert_eq!(Action::Forward { endpoint: "e".to_string(), format: DataFormat::Json }.action_type(), "forward");
        assert_eq!(Action::Delay { duration_ms: 100 }.action_type(), "delay");
        assert_eq!(Action::Conditional { condition: Condition::Threshold { property: "p".to_string(), operator: Operator::Eq, value: 1.0 }, then_actions: vec![], else_actions: None }.action_type(), "conditional");
        assert_eq!(Action::Script { interpreter: ScriptInterpreter::Bash, script: "echo hi".to_string() }.action_type(), "script");
    }

    #[test]
    fn test_notify_channel_parse_str() {
        assert!(matches!(NotifyChannel::parse_str("email"), NotifyChannel::Email));
        assert!(matches!(NotifyChannel::parse_str("sms"), NotifyChannel::Sms));
        assert!(matches!(NotifyChannel::parse_str("webhook"), NotifyChannel::Webhook));
        assert!(matches!(NotifyChannel::parse_str("mqtt"), NotifyChannel::Mqtt));
        assert!(matches!(NotifyChannel::parse_str("unknown"), NotifyChannel::System));
        assert!(matches!(NotifyChannel::parse_str("SYSTEM"), NotifyChannel::System));
    }

    #[test]
    fn test_http_method_parse_str() {
        assert!(matches!(HttpMethod::parse_str("GET"), HttpMethod::Get));
        assert!(matches!(HttpMethod::parse_str("post"), HttpMethod::Post));
        assert!(matches!(HttpMethod::parse_str("put"), HttpMethod::Put));
        assert!(matches!(HttpMethod::parse_str("delete"), HttpMethod::Delete));
        assert!(matches!(HttpMethod::parse_str("patch"), HttpMethod::Patch));
        assert!(matches!(HttpMethod::parse_str("unknown"), HttpMethod::Get));
    }

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
        assert_eq!(HttpMethod::Patch.as_str(), "PATCH");
    }

    #[test]
    fn test_data_format_parse_str() {
        assert!(matches!(DataFormat::parse_str("json"), DataFormat::Json));
        assert!(matches!(DataFormat::parse_str("csv"), DataFormat::Csv));
        assert!(matches!(DataFormat::parse_str("xml"), DataFormat::Xml));
        assert!(matches!(DataFormat::parse_str("unknown"), DataFormat::Json));
    }

    #[test]
    fn test_script_interpreter_parse_str() {
        assert!(matches!(ScriptInterpreter::parse_str("bash"), ScriptInterpreter::Bash));
        assert!(matches!(ScriptInterpreter::parse_str("sh"), ScriptInterpreter::Bash));
        assert!(matches!(ScriptInterpreter::parse_str("python"), ScriptInterpreter::Python));
        assert!(matches!(ScriptInterpreter::parse_str("python3"), ScriptInterpreter::Python));
        assert!(matches!(ScriptInterpreter::parse_str("powershell"), ScriptInterpreter::PowerShell));
        assert!(matches!(ScriptInterpreter::parse_str("ps"), ScriptInterpreter::PowerShell));
        assert!(matches!(ScriptInterpreter::parse_str("cmd"), ScriptInterpreter::Cmd));
        assert!(matches!(ScriptInterpreter::parse_str("batch"), ScriptInterpreter::Cmd));
        assert!(matches!(ScriptInterpreter::parse_str("unknown"), ScriptInterpreter::Bash));
    }

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success("alarm", "triggered");
        assert_eq!(result.action_type, "alarm");
        assert!(result.success);
        assert_eq!(result.message, Some("triggered".to_string()));
        assert!(result.data.is_none());
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure("control_device", "timeout");
        assert_eq!(result.action_type, "control_device");
        assert!(!result.success);
        assert_eq!(result.message, Some("timeout".to_string()));
    }

    #[test]
    fn test_condition_json_roundtrip() {
        let cond = Condition::Threshold { property: "temp".to_string(), operator: Operator::Gt, value: 30.0 };
        let json = cond.to_json().unwrap();
        let parsed = Condition::from_json(&json).unwrap();
        match parsed {
            Condition::Threshold { property, operator, value } => {
                assert_eq!(property, "temp");
                assert!(matches!(operator, Operator::Gt));
                assert_eq!(value, 30.0);
            }
            _ => panic!("Expected Threshold condition"),
        }
    }

    #[test]
    fn test_action_json_roundtrip() {
        let action = Action::Delay { duration_ms: 500 };
        let json = action.to_json().unwrap();
        let parsed = Action::from_json(&json).unwrap();
        match parsed {
            Action::Delay { duration_ms } => assert_eq!(duration_ms, 500),
            _ => panic!("Expected Delay action"),
        }
    }

    #[test]
    fn test_parse_actions() {
        let json = r#"[{"type":"delay","duration_ms":100}]"#;
        let actions = parse_actions(json).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type(), "delay");
    }

    #[test]
    fn test_actions_to_json() {
        let actions = vec![Action::PowerOn { device_id: "d1".to_string() }];
        let json = actions_to_json(&actions).unwrap();
        assert!(json.contains("power_on"));
    }
}
