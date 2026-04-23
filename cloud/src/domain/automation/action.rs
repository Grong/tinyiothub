//! 自动化动作定义
//! 
//! 支持多种动作类型：告警、控制设备、HTTP请求、通知等

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::condition::AlarmLevel;

/// 动作枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Action {
    // ========== 告警动作 ==========
    
    /// 发送告警
    Alarm {
        level: AlarmLevel,
        message: String,
    },
    
    // ========== 设备控制动作 ==========
    
    /// 控制设备
    ControlDevice {
        device_id: String,
        command: String,
        parameters: Option<HashMap<String, String>>,
    },
    
    /// 设置设备属性
    SetProperty {
        device_id: String,
        property: String,
        value: String,
    },
    
    /// 打开设备
    PowerOn {
        device_id: String,
    },
    
    /// 关闭设备
    PowerOff {
        device_id: String,
    },
    
    // ========== 通知动作 ==========
    
    /// 发送通知
    Notify {
        channel: NotifyChannel,
        title: String,
        content: String,
    },
    
    /// 发送邮件
    SendEmail {
        to: Vec<String>,
        subject: String,
        body: String,
    },
    
    // ========== HTTP 动作 ==========
    
    /// HTTP 请求
    HttpRequest {
        method: HttpMethod,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    },
    
    // ========== 数据动作 ==========
    
    /// 转发数据到端点
    Forward {
        endpoint: String,
        format: DataFormat,
    },
    
    // ========== 控制流动作 ==========
    
    /// 延迟
    Delay {
        duration_ms: u64,
    },
    
    /// 条件执行
    Conditional {
        condition: super::condition::Condition,
        then_actions: Vec<Action>,
        else_actions: Option<Vec<Action>>,
    },
    
    // ========== 脚本动作 ==========
    
    /// 执行脚本
    Script {
        interpreter: ScriptInterpreter,
        script: String,
    },
}

impl Action {
    /// 从 JSON 解析
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
    
    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
    
    /// 获取动作类型名称
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

/// 通知渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel {
    Email,
    Sms,
    Webhook,
    Mqtt,
    System,
}

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

/// HTTP 方法
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    pub fn parse_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "PATCH" => HttpMethod::Patch,
            _ => HttpMethod::Get,
        }
    }
    
    pub fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
        }
    }
}

/// 数据格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Json,
    Csv,
    Xml,
}

impl DataFormat {
    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => DataFormat::Json,
            "csv" => DataFormat::Csv,
            "xml" => DataFormat::Xml,
            _ => DataFormat::Json,
        }
    }
}

/// 脚本解释器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptInterpreter {
    Bash,
    Python,
    PowerShell,
    Cmd,
}

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

/// 动作执行结果
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
        Self {
            action_type: action_type.to_string(),
            success: true,
            message: Some(message.to_string()),
            data: None,
            execution_time_ms: 0,
        }
    }
    
    pub fn failure(action_type: &str, message: &str) -> Self {
        Self {
            action_type: action_type.to_string(),
            success: false,
            message: Some(message.to_string()),
            data: None,
            execution_time_ms: 0,
        }
    }
}

/// 动作列表解析
pub fn parse_actions(json: &str) -> Result<Vec<Action>, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn actions_to_json(actions: &[Action]) -> Result<String, serde_json::Error> {
    serde_json::to_string(actions)
}
