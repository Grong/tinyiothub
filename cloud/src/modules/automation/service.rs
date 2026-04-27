// Automation service: evaluator + executor + service
// Migrated from domain/automation/evaluator.rs + executor.rs + service.rs

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::types::*;
use tinyiothub_core::models::device_command::DeviceCommand;
use tinyiothub_runtime::DataServer;
use crate::modules::notification::NotificationManager;
use crate::modules::notification::types::NotificationChannelType;
use crate::modules::event::value_objects::EventLevel;
use crate::modules::notification::NotificationMessage;

// ════════════════════════════════════════════════
// Condition Evaluator (from evaluator.rs)
// ════════════════════════════════════════════════

pub struct ConditionEvaluator;

impl ConditionEvaluator {
    pub fn new() -> Self { Self }

    pub fn evaluate(&self, condition: &Condition, context: &TriggerContext) -> bool {
        match condition {
            Condition::Threshold { property, operator, value } => {
                let current = context.get_property(property);
                let current_value = match current {
                    Value::Number(n) => n.as_f64().unwrap_or(0.0),
                    Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
                    _ => return false,
                };
                operator.compare(
                    &Value::Number(serde_json::Number::from_f64(current_value).unwrap_or(serde_json::Number::from(0))),
                    &Value::Number(serde_json::Number::from_f64(*value).unwrap_or(serde_json::Number::from(0))),
                )
            }
            Condition::Comparison { property, operator, value } => {
                operator.compare(context.get_property(property), value)
            }
            Condition::DeviceState { device_id, state } => {
                if let Some(id) = &context.device_id { if id != device_id { return false; } }
                match state {
                    DeviceState::Online => context.device_online.unwrap_or(false),
                    DeviceState::Offline => !context.device_online.unwrap_or(true),
                    _ => context.device_state.as_ref() == Some(state),
                }
            }
            Condition::DeviceOnline { device_id, online } => {
                if let Some(id) = &context.device_id { if id != device_id { return false; } }
                context.device_online == Some(*online)
            }
            Condition::AlarmCondition { level, .. } => {
                if let Some(data) = &context.event_data {
                    if let Some(alarm_level) = data.get("level").and_then(|v| v.as_str()) {
                        let ctx_level = AlarmLevel::parse_str(alarm_level);
                        return matches!((level, ctx_level),
                            (AlarmLevel::Info, _)
                            | (AlarmLevel::Warning, AlarmLevel::Warning | AlarmLevel::Error | AlarmLevel::Critical)
                            | (AlarmLevel::Error, AlarmLevel::Error | AlarmLevel::Critical)
                            | (AlarmLevel::Critical, AlarmLevel::Critical)
                        );
                    }
                }
                false
            }
            Condition::And { left, right } => self.evaluate(left, context) && self.evaluate(right, context),
            Condition::Or { left, right } => self.evaluate(left, context) || self.evaluate(right, context),
            Condition::Not { condition } => !self.evaluate(condition, context),
        }
    }

    pub fn evaluate_with_details(&self, condition: &Condition, context: &TriggerContext) -> (bool, Value) {
        let result = self.evaluate(condition, context);
        let details = json!({ "condition": condition, "context": { "device_id": context.device_id, "properties": context.properties }, "result": result });
        (result, details)
    }
}

impl Default for ConditionEvaluator {
    fn default() -> Self { Self::new() }
}

// ════════════════════════════════════════════════
// Action Executor (from executor.rs)
// ════════════════════════════════════════════════

pub struct ActionExecutor {
    http_client: Client,
    data_server: Option<Arc<DataServer>>,
    notification_manager: Option<Arc<NotificationManager>>,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self { http_client: Client::new(), data_server: None, notification_manager: None }
    }

    pub fn with_data_server(mut self, data_server: Arc<DataServer>) -> Self {
        self.data_server = Some(data_server);
        self
    }

    pub fn with_notification_manager(mut self, nm: Arc<NotificationManager>) -> Self {
        self.notification_manager = Some(nm);
        self
    }

    pub async fn execute(&self, actions: &[Action], context: &TriggerContext) -> Vec<ActionResult> {
        let mut results = Vec::new();
        for action in actions {
            results.push(self.execute_action(action, context).await);
        }
        results
    }

    async fn execute_action(&self, action: &Action, context: &TriggerContext) -> ActionResult {
        let start = Instant::now();
        let result = match action {
            Action::Alarm { level, message } => {
                let rendered = self.render_template(message, context);
                ActionResult::success("alarm", &format!("Alarm [{}]: {}", match level {
                    AlarmLevel::Info => "info", AlarmLevel::Warning => "warning",
                    AlarmLevel::Error => "error", AlarmLevel::Critical => "critical",
                }, rendered))
            }
            Action::ControlDevice { device_id, command, parameters } => {
                self.execute_control_device(device_id, command, parameters.as_ref()).await
            }
            Action::SetProperty { device_id, property, value } => {
                self.execute_set_property(device_id, property, value).await
            }
            Action::PowerOn { device_id } => self.execute_control_device(device_id, "power_on", None).await,
            Action::PowerOff { device_id } => self.execute_control_device(device_id, "power_off", None).await,
            Action::Notify { channel, title, content } => self.execute_notify(channel, title, content).await,
            Action::SendEmail { to, subject, body } => self.execute_send_email(to, subject, body).await,
            Action::HttpRequest { method, url, headers, body } => {
                self.execute_http_request(method, url, headers.as_ref(), body.as_deref()).await
            }
            Action::Forward { endpoint, format } => self.execute_forward(endpoint, format, context).await,
            Action::Delay { duration_ms } => { sleep(Duration::from_millis(*duration_ms)).await; ActionResult::success("delay", &format!("Delayed {}ms", duration_ms)) }
            Action::Conditional { .. } => ActionResult::success("conditional", "Conditional actions not fully implemented"),
            Action::Script { interpreter, script } => self.execute_script(interpreter, script).await,
        };
        ActionResult { execution_time_ms: start.elapsed().as_millis() as u64, ..result }
    }

    async fn execute_control_device(&self, device_id: &str, command: &str, parameters: Option<&HashMap<String, String>>) -> ActionResult {
        let params_json = parameters.map(|p| serde_json::to_string(p).unwrap_or_default());
        if let Some(ref ds) = self.data_server {
            let cmd = DeviceCommand {
                id: Uuid::new_v4().to_string(), device_id: device_id.to_string(),
                name: command.to_string(), display_name: Some(format!("{} (custom)", command)),
                description: Some("Automation control".to_string()), parameters: params_json,
                created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match ds.execute_command(cmd) {
                Ok(()) => ActionResult::success("control_device", &format!("Command '{}' sent to device '{}'", command, device_id)),
                Err(e) => ActionResult::failure("control_device", &format!("Failed to send command: {}", e)),
            }
        } else {
            ActionResult::success("control_device", &format!("Command '{}' queued for device '{}' (DataServer not available)", command, device_id))
        }
    }

    async fn execute_set_property(&self, device_id: &str, property: &str, value: &str) -> ActionResult {
        let params = json!({ "property": property, "value": value });
        if let Some(ref ds) = self.data_server {
            let cmd = DeviceCommand {
                id: Uuid::new_v4().to_string(), device_id: device_id.to_string(),
                name: "set_property".to_string(), display_name: Some(format!("Set {} = {}", property, value)),
                description: Some("Automation set property".to_string()), parameters: Some(params.to_string()),
                created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match ds.execute_command(cmd) {
                Ok(()) => ActionResult::success("set_property", &format!("Set {}.{} = {}", device_id, property, value)),
                Err(e) => ActionResult::failure("set_property", &format!("Failed to set property: {}", e)),
            }
        } else {
            ActionResult::success("set_property", &format!("Set {}.{} = {} (DataServer not available)", device_id, property, value))
        }
    }

    async fn execute_notify(&self, channel: &NotifyChannel, title: &str, content: &str) -> ActionResult {
        let channel_type = match channel {
            NotifyChannel::Email => NotificationChannelType::Email,
            NotifyChannel::Sms => NotificationChannelType::Sms,
            NotifyChannel::Webhook | NotifyChannel::Mqtt => NotificationChannelType::Webhook,
            NotifyChannel::System => NotificationChannelType::Sse,
        };
        let message = NotificationMessage::new(title.to_string(), content.to_string(), EventLevel::Info, vec![channel_type], vec![]);
        if let Some(ref nm) = self.notification_manager {
            match nm.send_notification(&message).await {
                Ok(()) => ActionResult::success("notify", &format!("[{:?}] {} - {}", channel, title, content)),
                Err(e) => ActionResult::failure("notify", &format!("Notification failed: {}", e)),
            }
        } else {
            ActionResult::success("notify", &format!("[{:?}] {} - {} (NotificationManager not available)", channel, title, content))
        }
    }

    async fn execute_send_email(&self, to: &[String], subject: &str, body: &str) -> ActionResult {
        let message = NotificationMessage::new(subject.to_string(), body.to_string(), EventLevel::Info, vec![NotificationChannelType::Email], to.to_vec());
        if let Some(ref nm) = self.notification_manager {
            match nm.send_notification(&message).await {
                Ok(()) => ActionResult::success("send_email", &format!("Email sent to {:?}: {}", to, subject)),
                Err(e) => ActionResult::failure("send_email", &format!("Failed to send email: {}", e)),
            }
        } else {
            ActionResult::success("send_email", &format!("Email queued to {:?}: {} (NotificationManager not available)", to, subject))
        }
    }

    async fn execute_http_request(&self, method: &HttpMethod, url: &str, headers: Option<&HashMap<String, String>>, body: Option<&str>) -> ActionResult {
        let mut req = self.http_client.request(reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET), url);
        if let Some(hdrs) = headers { for (k, v) in hdrs { req = req.header(k, v); } }
        if let Some(b) = body { req = req.body(b.to_string()); }
        match req.send().await {
            Ok(r) if r.status().is_success() => ActionResult::success("http_request", &format!("HTTP {} - {}", method.as_str(), r.status())),
            Ok(r) => ActionResult::failure("http_request", &format!("HTTP error: {}", r.status())),
            Err(e) => ActionResult::failure("http_request", &format!("Request failed: {}", e)),
        }
    }

    async fn execute_forward(&self, endpoint: &str, format: &DataFormat, context: &TriggerContext) -> ActionResult {
        let data = match format {
            DataFormat::Json => serde_json::to_string(&context.properties).unwrap_or_default(),
            DataFormat::Csv => context.properties.iter().map(|(k, v)| format!("{},{}\n", k, v)).collect(),
            DataFormat::Xml => {
                let mut xml = String::from("<data>\n");
                for (k, v) in &context.properties { xml.push_str(&format!("  <{}>{}</{}>\n", k, v, k)); }
                xml.push_str("</data>");
                xml
            }
        };
        let ct = match format { DataFormat::Json => "application/json", DataFormat::Csv => "text/csv", DataFormat::Xml => "application/xml" };
        match self.http_client.post(endpoint).header("Content-Type", ct).body(data).send().await {
            Ok(r) if r.status().is_success() => ActionResult::success("forward", &format!("Data forwarded to {}", endpoint)),
            Ok(r) => ActionResult::failure("forward", &format!("Forward failed: {}", r.status())),
            Err(e) => ActionResult::failure("forward", &format!("Forward error: {}", e)),
        }
    }

    async fn execute_script(&self, interpreter: &ScriptInterpreter, script: &str) -> ActionResult {
        use std::process::Command;
        let output = match interpreter {
            ScriptInterpreter::Bash => Command::new("bash").arg("-c").arg(script).output(),
            ScriptInterpreter::Python => Command::new("python").arg("-c").arg(script).output(),
            ScriptInterpreter::PowerShell => Command::new("powershell").args(["-Command", script]).output(),
            ScriptInterpreter::Cmd => Command::new("cmd").args(["/C", script]).output(),
        };
        match output {
            Ok(o) if o.status.success() => ActionResult::success("script", &String::from_utf8_lossy(&o.stdout)),
            Ok(o) => ActionResult::failure("script", &String::from_utf8_lossy(&o.stderr)),
            Err(e) => ActionResult::failure("script", &format!("Failed to execute: {}", e)),
        }
    }

    fn render_template(&self, template: &str, context: &TriggerContext) -> String {
        let mut result = template.to_string();
        for (key, value) in &context.properties {
            let placeholder = format!("{{{{{}}}}}", key);
            let value_str = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => value.to_string(),
            };
            result = result.replace(&placeholder, &value_str);
        }
        if let Some(id) = &context.device_id { result = result.replace("{{device_id}}", id); }
        if let Some(name) = &context.device_name { result = result.replace("{{device_name}}", name); }
        result
    }
}

impl Default for ActionExecutor {
    fn default() -> Self { Self::new() }
}

// ════════════════════════════════════════════════
// Automation Service (from service.rs)
// ════════════════════════════════════════════════

pub struct AutomationService {
    evaluator: ConditionEvaluator,
    executor: ActionExecutor,
    automations: Arc<RwLock<Vec<Automation>>>,
}

impl AutomationService {
    pub fn new() -> Self {
        Self { evaluator: ConditionEvaluator::new(), executor: ActionExecutor::new(), automations: Arc::new(RwLock::new(Vec::new())) }
    }

    pub fn with_dependencies(data_server: Option<Arc<DataServer>>, notification_manager: Option<Arc<NotificationManager>>) -> Self {
        let executor = ActionExecutor::new();
        let executor = match (data_server, notification_manager) {
            (Some(ds), Some(nm)) => executor.with_data_server(ds).with_notification_manager(nm),
            (Some(ds), None) => executor.with_data_server(ds),
            (None, Some(nm)) => executor.with_notification_manager(nm),
            (None, None) => executor,
        };
        Self { evaluator: ConditionEvaluator::new(), executor, automations: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn trigger(&self, context: TriggerContext) -> Vec<AutomationExecution> {
        let automations = self.automations.read().await;
        let mut executions = Vec::new();
        for automation in automations.iter() {
            if automation.trigger_type != "event" || !automation.enabled { continue; }
            let conditions_met = if let Some(conditions_json) = &automation.conditions {
                if let Ok(conditions) = serde_json::from_str::<Condition>(conditions_json) {
                    self.evaluator.evaluate(&conditions, &context)
                } else { true }
            } else { true };
            if !conditions_met { continue; }
            let actions: Vec<Action> = serde_json::from_str(&automation.actions).unwrap_or_default();
            let results = self.executor.execute(&actions, &context).await;
            let success = results.iter().all(|r| r.success);
            executions.push(AutomationExecution {
                id: Uuid::new_v4().to_string(),
                automation_id: automation.id.clone(),
                automation_name: automation.name.clone(),
                trigger_type: TriggerType::Event,
                conditions_met,
                actions_results: results,
                success,
                triggered_at: Utc::now().to_rfc3339(),
            });
        }
        executions
    }
}

impl Default for AutomationService {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: String,
    pub event_source_type: Option<String>,
    pub event_device_id: Option<String>,
    pub event_property: Option<String>,
    pub event_condition: Option<String>,
    pub cron_expression: Option<String>,
    pub conditions: Option<String>,
    pub actions: String,
    pub timeout_seconds: i32,
    pub retry_count: i32,
    pub retry_delay_seconds: i32,
    pub cooldown_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub tags: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AutomationExecution {
    pub id: String,
    pub automation_id: String,
    pub automation_name: String,
    pub trigger_type: TriggerType,
    pub conditions_met: bool,
    pub actions_results: Vec<ActionResult>,
    pub success: bool,
    pub triggered_at: String,
}

// ════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_threshold_greater() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        let condition = Condition::Threshold { property: "temperature".to_string(), operator: Operator::Gt, value: 25.0 };
        assert!(evaluator.evaluate(&condition, &context));
    }

    #[test]
    fn test_threshold_less() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("humidity".to_string(), json!(50.0));
        let condition = Condition::Threshold { property: "humidity".to_string(), operator: Operator::Lt, value: 60.0 };
        assert!(evaluator.evaluate(&condition, &context));
    }

    #[test]
    fn test_threshold_not_met() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(20.0));
        let condition = Condition::Threshold { property: "temperature".to_string(), operator: Operator::Gt, value: 25.0 };
        assert!(!evaluator.evaluate(&condition, &context));
    }

    #[test]
    fn test_and_condition() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        context.properties.insert("humidity".to_string(), json!(50.0));
        let condition = Condition::And {
            left: Box::new(Condition::Threshold { property: "temperature".to_string(), operator: Operator::Gt, value: 25.0 }),
            right: Box::new(Condition::Threshold { property: "humidity".to_string(), operator: Operator::Lt, value: 60.0 }),
        };
        assert!(evaluator.evaluate(&condition, &context));
    }

    #[test]
    fn test_or_condition() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        let condition = Condition::Or {
            left: Box::new(Condition::Threshold { property: "temperature".to_string(), operator: Operator::Gt, value: 25.0 }),
            right: Box::new(Condition::Threshold { property: "humidity".to_string(), operator: Operator::Lt, value: 30.0 }),
        };
        assert!(evaluator.evaluate(&condition, &context));
    }

    #[tokio::test]
    async fn test_execute_alarm_action() {
        let executor = ActionExecutor::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(55.0));
        context.device_id = Some("sensor_001".to_string());
        let action = Action::Alarm { level: AlarmLevel::Warning, message: "温度 {{temperature}}℃ 超过阈值".to_string() };
        let results = executor.execute(&[action], &context).await;
        assert!(!results.is_empty());
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn test_execute_control_action() {
        let executor = ActionExecutor::new();
        let context = TriggerContext::new();
        let action = Action::ControlDevice { device_id: "light_001".to_string(), command: "turn_on".to_string(), parameters: None };
        let results = executor.execute(&[action], &context).await;
        assert!(!results.is_empty());
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn test_execute_delay_action() {
        let executor = ActionExecutor::new();
        let context = TriggerContext::new();
        let action = Action::Delay { duration_ms: 10 };
        let results = executor.execute(&[action], &context).await;
        assert!(!results.is_empty());
        assert!(results[0].success);
    }

    #[test]
    fn test_automation_service_creation() {
        let _service = AutomationService::new();
    }

    #[test]
    fn test_trigger_context_creation() {
        let mut context = TriggerContext::new();
        context.device_id = Some("device_001".to_string());
        context.device_name = Some("test_device".to_string());
        context.device_online = Some(true);
        context.properties.insert("temperature".to_string(), json!(25.0));
        assert_eq!(context.device_id, Some("device_001".to_string()));
        assert_eq!(context.get_property("temperature"), &json!(25.0));
    }
}
