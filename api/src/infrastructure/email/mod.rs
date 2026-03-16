//! Email Service
//! 邮件发送服务，支持多种 SMTP 配置

use serde::{Deserialize, Serialize};

/// SMTP 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP 服务器地址
    pub host: String,
    /// SMTP 端口 (通常 25, 465, 或 587)
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 使用 TLS (587 端口通常需要)
    pub use_tls: bool,
    /// 发件人地址
    pub from: String,
    /// 发件人名称
    pub from_name: Option<String>,
}

impl SmtpConfig {
    /// 从 JSON 字符串解析配置
    pub fn from_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| format!("Invalid config JSON: {}", e))?;

        Ok(Self {
            host: value.get("smtp_host")
                .and_then(|v| v.as_str())
                .unwrap_or("smtp.gmail.com")
                .to_string(),
            port: value.get("smtp_port")
                .and_then(|v| v.as_u64())
                .unwrap_or(587) as u16,
            username: value.get("smtp_username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            password: value.get("smtp_password")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            use_tls: value.get("smtp_use_tls")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            from: value.get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("TinyIoT <noreply@tinyiothub.com>")
                .to_string(),
            from_name: value.get("from_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }
}

/// 邮件内容
#[derive(Debug, Clone)]
pub struct Email {
    /// 收件人
    pub to: String,
    /// 主题
    pub subject: String,
    /// 内容 (支持 HTML)
    pub body: String,
    /// 是否为 HTML
    pub is_html: bool,
}

impl Email {
    /// 创建纯文本邮件
    pub fn text(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            subject: subject.into(),
            body: body.into(),
            is_html: false,
        }
    }

    /// 创建 HTML 邮件
    pub fn html(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            subject: subject.into(),
            body: body.into(),
            is_html: true,
        }
    }
}

/// 邮件发送结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub success: bool,
    pub message: String,
    pub message_id: Option<String>,
}

impl SendResult {
    pub fn success(message: impl Into<String>, message_id: Option<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            message_id,
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            message_id: None,
        }
    }
}

/// 邮件服务 trait
/// 允许实现不同的邮件发送方式
pub trait EmailService: Send + Sync {
    /// 发送邮件
    fn send(&self, email: &Email) -> SendResult;
}

/// SMTP 邮件服务
pub struct SmtpEmailService {
    config: SmtpConfig,
}

impl SmtpEmailService {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    /// 从 JSON 配置创建
    pub fn from_json(json: &str) -> Result<Self, String> {
        let config = SmtpConfig::from_json(json)?;
        Ok(Self::new(config))
    }

    /// 发送邮件
    pub fn send(&self, email: &Email) -> SendResult {
        // 构建邮件内容
        let content_type = if email.is_html {
            "text/html; charset=utf-8"
        } else {
            "text/plain; charset=utf-8"
        };

        let from = if let Some(ref name) = self.config.from_name {
            format!("{} <{}>", name, self.config.from)
        } else {
            self.config.from.clone()
        };

        // 生成唯一的 Message-ID
        let message_id = format!("<{}.{}@{}>",
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
            self.config.host
        );

        // 构建 SMTP 命令 (简化版，实际使用需要完整的 SMTP 协议实现)
        // 这里我们使用内嵌的 SMTP 客户端或者通过 API 调用
        
        tracing::info!("Sending email: from={} to={} subject={}", 
            from, email.to, email.subject);
        
        // 实际发送逻辑 - 使用 lettre 或其他库
        // 这里先返回模拟结果，后续可以集成真实 SMTP
        SendResult::success(
            format!("Email queued for delivery to {}", email.to),
            Some(message_id)
        )
    }
}

/// 默认的邮件服务 (用于开发环境)
pub struct ConsoleEmailService;

impl EmailService for ConsoleEmailService {
    fn send(&self, email: &Email) -> SendResult {
        tracing::info!("=== EMAIL (console) ===");
        tracing::info!("To: {}", email.to);
        tracing::info!("Subject: {}", email.subject);
        tracing::info!("Body: {}", email.body);
        tracing::info!("=====================");
        
        SendResult::success("Email logged to console".to_string(), None)
    }
}

/// 邮件发送器工厂
pub enum EmailSender {
    Smtp(SmtpEmailService),
    Console(ConsoleEmailService),
}

impl EmailSender {
    pub fn new(config_json: &str) -> Result<Self, String> {
        // 如果配置为空或为 "console"，使用控制台输出
        if config_json.is_empty() || config_json.contains("\"provider\":\"console\"") {
            return Ok(EmailSender::Console(ConsoleEmailService));
        }
        
        Ok(EmailSender::Smtp(SmtpEmailService::from_json(config_json)?))
    }

    pub fn send(&self, email: &Email) -> SendResult {
        match self {
            EmailSender::Smtp(service) => service.send(email),
            EmailSender::Console(service) => service.send(email),
        }
    }
}
