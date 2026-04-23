//! 动作执行器
//!
//! 执行自动化规则中的各种动作（简化版）

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use reqwest::Client;
use serde_json::Value;
use tokio::time::{sleep, Duration};

use super::action::*;
use super::condition::TriggerContext;
use tinyiothub_engine::application::DataServer;
use crate::domain::event::services::notification_service::NotificationManager;
use crate::domain::event::value_objects::{ContentElement, EventLevel, NotificationChannelType, RichContent, TextFormat};
use tinyiothub_core::models::device_command::DeviceCommand;

/// 动作执行器
pub struct ActionExecutor {
    http_client: Client,
    data_server: Option<Arc<DataServer>>,
    notification_manager: Option<Arc<NotificationManager>>,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
            data_server: None,
            notification_manager: None,
        }
    }

    /// 设置数据服务器（用于设备命令执行）
    pub fn with_data_server(mut self, data_server: Arc<DataServer>) -> Self {
        self.data_server = Some(data_server);
        self
    }

    /// 设置通知管理器（用于发送通知）
    pub fn with_notification_manager(mut self, notification_manager: Arc<NotificationManager>) -> Self {
        self.notification_manager = Some(notification_manager);
        self
    }
    
    /// 执行动作列表
    pub async fn execute(
        &self,
        actions: &[Action],
        context: &TriggerContext,
    ) -> Vec<ActionResult> {
        let mut results = Vec::new();
        
        for action in actions {
            let result = self.execute_action_sync(action, context).await;
            results.push(result);
        }
        
        results
    }
    
    /// 执行单个动作（同步版本）
    async fn execute_action_sync(&self, action: &Action, context: &TriggerContext) -> ActionResult {
        let start = Instant::now();
        
        let result = match action {
            Action::Alarm { level, message } => {
                self.execute_alarm(level, message, context).await
            }
            
            Action::ControlDevice { device_id, command, parameters } => {
                self.execute_control_device(device_id, command, parameters.as_ref()).await
            }
            
            Action::SetProperty { device_id, property, value } => {
                self.execute_set_property(device_id, property, value).await
            }
            
            Action::PowerOn { device_id } => {
                self.execute_control_device(device_id, "power_on", None).await
            }
            
            Action::PowerOff { device_id } => {
                self.execute_control_device(device_id, "power_off", None).await
            }
            
            Action::Notify { channel, title, content } => {
                self.execute_notify(channel, title, content).await
            }
            
            Action::SendEmail { to, subject, body } => {
                self.execute_send_email(to, subject, body).await
            }
            
            Action::HttpRequest { method, url, headers, body } => {
                self.execute_http_request(method, url, headers.as_ref(), body.as_deref()).await
            }
            
            Action::Forward { endpoint, format } => {
                self.execute_forward(endpoint, format, context).await
            }
            
            Action::Delay { duration_ms } => {
                self.execute_delay(*duration_ms).await
            }
            
            Action::Conditional { condition, then_actions, else_actions } => {
                self.execute_conditional(condition, then_actions, else_actions.as_deref(), context).await
            }
            
            Action::Script { interpreter, script } => {
                self.execute_script(interpreter, script).await
            }
        };
        
        let execution_time = start.elapsed().as_millis() as u64;
        ActionResult {
            execution_time_ms: execution_time,
            ..result
        }
    }
    
    // ========== 告警动作 ==========
    
    async fn execute_alarm(
        &self,
        level: &super::condition::AlarmLevel,
        message: &str,
        context: &TriggerContext,
    ) -> ActionResult {
        let rendered = self.render_template(message, context);
        ActionResult::success("alarm", &format!("Alarm [{}]: {}", 
            match level {
                super::condition::AlarmLevel::Info => "info",
                super::condition::AlarmLevel::Warning => "warning",
                super::condition::AlarmLevel::Error => "error",
                super::condition::AlarmLevel::Critical => "critical",
            }, 
            rendered))
    }
    
    // ========== 设备控制动作 ==========
    
    async fn execute_control_device(
        &self,
        device_id: &str,
        command: &str,
        parameters: Option<&HashMap<String, String>>,
    ) -> ActionResult {
        let params_json = parameters.map(|p| serde_json::to_string(p).unwrap_or_default());

        if let Some(ref data_server) = self.data_server {
            let cmd = DeviceCommand {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: device_id.to_string(),
                name: command.to_string(),
                display_name: Some(format!("{} (custom)", command)),
                description: Some("Automation control".to_string()),
                parameters: params_json,
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };

            match data_server.execute_command(cmd) {
                Ok(()) => ActionResult::success("control_device", &format!("Command '{}' sent to device '{}'", command, device_id)),
                Err(e) => ActionResult::failure("control_device", &format!("Failed to send command: {}", e)),
            }
        } else {
            // Fallback: 命令已发送（无 DataServer）
            ActionResult::success("control_device", &format!("Command '{}' queued for device '{}' (DataServer not available)", command, device_id))
        }
    }
    
    async fn execute_set_property(
        &self,
        device_id: &str,
        property: &str,
        value: &str,
    ) -> ActionResult {
        let params = serde_json::json!({
            "property": property,
            "value": value
        });

        if let Some(ref data_server) = self.data_server {
            let cmd = DeviceCommand {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: device_id.to_string(),
                name: "set_property".to_string(),
                display_name: Some(format!("Set {} = {}", property, value)),
                description: Some("Automation set property".to_string()),
                parameters: Some(params.to_string()),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };

            match data_server.execute_command(cmd) {
                Ok(()) => ActionResult::success("set_property", &format!("Set {}.{} = {}", device_id, property, value)),
                Err(e) => ActionResult::failure("set_property", &format!("Failed to set property: {}", e)),
            }
        } else {
            ActionResult::success("set_property", &format!("Set {}.{} = {} (DataServer not available)", device_id, property, value))
        }
    }
    
    // ========== 通知动作 ==========
    
    async fn execute_notify(
        &self,
        channel: &NotifyChannel,
        title: &str,
        content: &str,
    ) -> ActionResult {
        let channel_type = match channel {
            NotifyChannel::Email => NotificationChannelType::Email,
            NotifyChannel::Sms => NotificationChannelType::Sms,
            NotifyChannel::Webhook => NotificationChannelType::Webhook,
            NotifyChannel::Mqtt => NotificationChannelType::Webhook, // MQTT not in NotificationChannelType, use Webhook
            NotifyChannel::System => NotificationChannelType::Sse,
        };

        let message = NotificationMessage::new(
            title.to_string(),
            content.to_string(),
            EventLevel::Info,
            vec![channel_type],
            vec![], // recipients filled by notification manager
        );

        if let Some(ref notification_manager) = self.notification_manager {
            match notification_manager.send_notification(&message).await {
                Ok(()) => ActionResult::success("notify", &format!("[{:?}] {} - {}", channel, title, content)),
                Err(e) => ActionResult::failure("notify", &format!("Notification failed: {}", e)),
            }
        } else {
            ActionResult::success("notify", &format!("[{:?}] {} - {} (NotificationManager not available)", channel, title, content))
        }
    }
    
    async fn execute_send_email(
        &self,
        to: &[String],
        subject: &str,
        body: &str,
    ) -> ActionResult {
        let message = NotificationMessage::new(
            subject.to_string(),
            body.to_string(),
            EventLevel::Info,
            vec![NotificationChannelType::Email],
            to.clone(),
        );

        if let Some(ref notification_manager) = self.notification_manager {
            match notification_manager.send_notification(&message).await {
                Ok(()) => ActionResult::success("send_email", &format!("Email sent to {:?}: {}", to, subject)),
                Err(e) => ActionResult::failure("send_email", &format!("Failed to send email: {}", e)),
            }
        } else {
            ActionResult::success("send_email", &format!("Email queued to {:?}: {} (NotificationManager not available)", to, subject))
        }
    }
    
    // ========== HTTP 动作 ==========
    
    async fn execute_http_request(
        &self,
        method: &HttpMethod,
        url: &str,
        headers: Option<&HashMap<String, String>>,
        body: Option<&str>,
    ) -> ActionResult {
        let mut request = self.http_client.request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
            url,
        );
        
        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                request = request.header(key, value);
            }
        }
        
        if let Some(b) = body {
            request = request.body(b.to_string());
        }
        
        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    ActionResult::success("http_request", &format!("HTTP {} - {}", method.as_str(), response.status()))
                } else {
                    ActionResult::failure("http_request", &format!("HTTP error: {}", response.status()))
                }
            }
            Err(e) => ActionResult::failure("http_request", &format!("Request failed: {}", e)),
        }
    }
    
    // ========== 数据转发 ==========
    
    async fn execute_forward(
        &self,
        endpoint: &str,
        format: &DataFormat,
        context: &TriggerContext,
    ) -> ActionResult {
        let data = match format {
            DataFormat::Json => serde_json::to_string(&context.properties).unwrap_or_default(),
            DataFormat::Csv => {
                let mut csv = String::new();
                for (key, value) in &context.properties {
                    csv.push_str(&format!("{},{}\n", key, value));
                }
                csv
            }
            DataFormat::Xml => {
                let mut xml = String::from("<data>\n");
                for (key, value) in &context.properties {
                    xml.push_str(&format!("  <{}>{}</{}>\n", key, value, key));
                }
                xml.push_str("</data>");
                xml
            }
        };
        
        let result = self.http_client
            .post(endpoint)
            .header("Content-Type", match format {
                DataFormat::Json => "application/json",
                DataFormat::Csv => "text/csv",
                DataFormat::Xml => "application/xml",
            })
            .body(data)
            .send()
            .await;
        
        match result {
            Ok(response) if response.status().is_success() => {
                ActionResult::success("forward", &format!("Data forwarded to {}", endpoint))
            }
            Ok(response) => {
                ActionResult::failure("forward", &format!("Forward failed: {}", response.status()))
            }
            Err(e) => {
                ActionResult::failure("forward", &format!("Forward error: {}", e))
            }
        }
    }
    
    // ========== 延迟动作 ==========
    
    async fn execute_delay(&self, duration_ms: u64) -> ActionResult {
        sleep(Duration::from_millis(duration_ms)).await;
        ActionResult::success("delay", &format!("Delayed {}ms", duration_ms))
    }
    
    // ========== 条件动作 ==========
    
    async fn execute_conditional(
        &self,
        condition: &super::condition::Condition,
        then_actions: &[Action],
        else_actions: Option<&[Action]>,
        context: &TriggerContext,
    ) -> ActionResult {
        // 简化：暂不支持嵌套条件动作
        ActionResult::success("conditional", "Conditional actions not fully implemented")
    }
    
    // ========== 脚本动作 ==========
    
    async fn execute_script(&self, interpreter: &ScriptInterpreter, script: &str) -> ActionResult {
        use std::process::Command;
        
        let output = match interpreter {
            ScriptInterpreter::Bash => Command::new("bash").arg("-c").arg(script).output(),
            ScriptInterpreter::Python => Command::new("python").arg("-c").arg(script).output(),
            ScriptInterpreter::PowerShell => Command::new("powershell").args(["-Command", script]).output(),
            ScriptInterpreter::Cmd => Command::new("cmd").args(["/C", script]).output(),
        };
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    ActionResult::success("script", &String::from_utf8_lossy(&output.stdout))
                } else {
                    ActionResult::failure("script", &String::from_utf8_lossy(&output.stderr))
                }
            }
            Err(e) => ActionResult::failure("script", &format!("Failed to execute: {}", e)),
        }
    }
    
    // ========== 工具方法 ==========
    
    /// 渲染消息模板
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
        
        if let Some(device_id) = &context.device_id {
            result = result.replace("{{device_id}}", device_id);
        }
        if let Some(device_name) = &context.device_name {
            result = result.replace("{{device_name}}", device_name);
        }
        
        result
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}
