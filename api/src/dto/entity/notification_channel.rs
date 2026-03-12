use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::infrastructure::persistence::database::Database;

/// 通知渠道类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Sms,
    Email,
    Webhook,
}

impl ChannelType {
    pub fn as_str(&self) -> &str {
        match self {
            ChannelType::Sms => "sms",
            ChannelType::Email => "email",
            ChannelType::Webhook => "webhook",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sms" => Some(ChannelType::Sms),
            "email" => Some(ChannelType::Email),
            "webhook" => Some(ChannelType::Webhook),
            _ => None,
        }
    }
}

/// 通知渠道实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub config: String,
    pub is_enabled: bool,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct NotificationChannelQueryParams {
    pub channel_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: String,
    pub description: Option<String>,
}

/// 更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateNotificationChannelRequest {
    pub name: Option<String>,
    pub channel_type: Option<String>,
    pub config: Option<String>,
    pub description: Option<String>,
}

/// 发送消息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendMessageRequest {
    pub recipient: String,      // 手机号/邮箱/ webhook 地址
    pub title: Option<String>,  // 标题（邮件/短信）
    pub content: String,         // 消息内容
}

/// 渠道统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelStatistics {
    pub total_channels: i64,
    pub enabled_channels: i64,
    pub sms_channels: i64,
    pub email_channels: i64,
    pub webhook_channels: i64,
}

impl NotificationChannel {
    /// 根据 ID 查询
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<NotificationChannel>, sqlx::Error> {
        let sql = format!("SELECT * FROM notification_channels WHERE id = '{}' LIMIT 1", id);
        
        let mut rows = db.query(&sql, |row| {
            Ok(NotificationChannel {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                channel_type: row.try_get("channel_type")?,
                config: row.try_get("config")?,
                is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
                description: row.try_get("description")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        }).await?;
        
        Ok(rows.pop())
    }

    /// 查询所有
    pub async fn find_all(db: &Database, params: &NotificationChannelQueryParams) -> Result<Vec<NotificationChannel>, sqlx::Error> {
        let mut sql = String::from("SELECT * FROM notification_channels WHERE 1=1");
        
        if let Some(ref channel_type) = params.channel_type {
            sql.push_str(&format!(" AND channel_type = '{}'", channel_type));
        }
        if let Some(is_enabled) = params.is_enabled {
            sql.push_str(&format!(" AND is_enabled = {}", if is_enabled { 1 } else { 0 }));
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let offset = (page - 1) * page_size;
        sql.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));
        
        db.query(&sql, |row| {
            Ok(NotificationChannel {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                channel_type: row.try_get("channel_type")?,
                config: row.try_get("config")?,
                is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
                description: row.try_get("description")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        }).await
    }

    /// 创建
    pub async fn create(db: &Database, req: &CreateNotificationChannelRequest) -> Result<NotificationChannel, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let sql = format!(r#"
            INSERT INTO notification_channels (id, name, channel_type, config, is_enabled, description, created_at, updated_at)
            VALUES ('{}', '{}', '{}', '{}', 1, '{}', '{}', '{}')
        "#,
            id,
            req.name,
            req.channel_type,
            req.config,
            req.description.as_deref().unwrap_or(""),
            now,
            now
        );
        
        db.execute(&sql).await?;
        
        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新
    pub async fn update(db: &Database, id: &str, req: &UpdateNotificationChannelRequest) -> Result<NotificationChannel, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let mut updates = vec![format!("updated_at = '{}'", now)];
        
        if let Some(ref name) = req.name {
            updates.push(format!("name = '{}'", name));
        }
        if let Some(ref channel_type) = req.channel_type {
            updates.push(format!("channel_type = '{}'", channel_type));
        }
        if let Some(ref config) = req.config {
            updates.push(format!("config = '{}'", config));
        }
        if let Some(ref description) = req.description {
            updates.push(format!("description = '{}'", description));
        }
        
        let sql = format!("UPDATE notification_channels SET {} WHERE id = '{}'", updates.join(", "), id);
        let _ = db.execute(&sql).await;
        
        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let sql = format!("DELETE FROM notification_channels WHERE id = '{}'", id);
        db.execute(&sql).await
    }

    /// 设置启用/禁用
    pub async fn set_enabled(db: &Database, id: &str, is_enabled: bool) -> Result<NotificationChannel, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let sql = format!(
            "UPDATE notification_channels SET is_enabled = {}, updated_at = '{}' WHERE id = '{}'",
            if is_enabled { 1 } else { 0 },
            now,
            id
        );
        let _ = db.execute(&sql).await;
        
        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 获取统计
    pub async fn get_statistics(db: &Database) -> Result<ChannelStatistics, sqlx::Error> {
        let total: i64 = db.query_first(
            "SELECT COUNT(*) FROM notification_channels", 
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);
        
        let enabled: i64 = db.query_first(
            "SELECT COUNT(*) FROM notification_channels WHERE is_enabled = 1", 
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);
        
        let sms: i64 = db.query_first(
            "SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'sms'", 
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);
        
        let email: i64 = db.query_first(
            "SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'email'", 
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);
        
        let webhook: i64 = db.query_first(
            "SELECT COUNT(*) FROM notification_channels WHERE channel_type = 'webhook'", 
            |row| row.try_get::<i64, _>(0)
        ).await?.unwrap_or(0);

        Ok(ChannelStatistics {
            total_channels: total,
            enabled_channels: enabled,
            sms_channels: sms,
            email_channels: email,
            webhook_channels: webhook,
        })
    }

    /// 发送消息
    pub async fn send_message(&self, req: &SendMessageRequest) -> Result<String, String> {
        match self.channel_type.as_str() {
            "sms" => self.send_sms(req).await,
            "email" => self.send_email(req).await,
            "webhook" => self.send_webhook(req).await,
            _ => Err(format!("Unknown channel type: {}", self.channel_type)),
        }
    }

    /// 发送短信
    async fn send_sms(&self, req: &SendMessageRequest) -> Result<String, String> {
        let config: serde_json::Value = serde_json::from_str(&self.config)
            .map_err(|e| format!("Invalid config JSON: {}", e))?;
        
        let provider = config.get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("aliyun");
        
        let sign_name = config.get("sign_name")
            .and_then(|v| v.as_str())
            .unwrap_or("TinyIoT");
        
        let template_id = config.get("template_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        tracing::info!("Sending SMS via {} to {}: {}", provider, req.recipient, req.content);
        
        // TODO: 实现实际的短信发送
        // 这里模拟发送成功
        Ok(format!("SMS sent to {} via {} (sign: {}, template: {})", 
            req.recipient, provider, sign_name, template_id))
    }

    /// 发送邮件
    async fn send_email(&self, req: &SendMessageRequest) -> Result<String, String> {
        let config: serde_json::Value = serde_json::from_str(&self.config)
            .map_err(|e| format!("Invalid config JSON: {}", e))?;
        
        let smtp_host = config.get("smtp_host")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let from = config.get("from")
            .and_then(|v| v.as_str())
            .unwrap_or("TinyIoT <noreply@tinyiot.com>");
        
        tracing::info!("Sending email via {} from {} to {}", smtp_host, from, req.recipient);
        
        // TODO: 实现实际的邮件发送
        Ok(format!("Email sent to {} (from: {}, subject: {})", 
            req.recipient, from, req.title.as_deref().unwrap_or("")))
    }

    /// 发送 Webhook
    async fn send_webhook(&self, req: &SendMessageRequest) -> Result<String, String> {
        let config: serde_json::Value = serde_json::from_str(&self.config)
            .map_err(|e| format!("Invalid config JSON: {}", e))?;
        
        let url = config.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing URL in config")?;
        
        let method = config.get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST");
        
        tracing::info!("Sending webhook {} {} to {}", method, url, req.recipient);
        
        // 构建请求体
        let body = serde_json::json!({
            "msgtype": "text",
            "text": {
                "content": format!("{}\n{}", req.title.as_deref().unwrap_or(""), req.content)
            }
        });
        
        // TODO: 实现实际的 HTTP 请求
        Ok(format!("Webhook sent to {} via {} {}", url, method, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_as_str() {
        assert_eq!(ChannelType::Sms.as_str(), "sms");
        assert_eq!(ChannelType::Email.as_str(), "email");
        assert_eq!(ChannelType::Webhook.as_str(), "webhook");
    }

    #[test]
    fn test_channel_type_from_str() {
        assert_eq!(ChannelType::from_str("sms"), Some(ChannelType::Sms));
        assert_eq!(ChannelType::from_str("email"), Some(ChannelType::Email));
        assert_eq!(ChannelType::from_str("webhook"), Some(ChannelType::Webhook));
        assert_eq!(ChannelType::from_str("unknown"), None);
    }

    #[test]
    fn test_create_channel_request() {
        let req = CreateNotificationChannelRequest {
            name: "Test SMS".to_string(),
            channel_type: "sms".to_string(),
            config: r#"{"provider": "aliyun", "sign_name": "Test"}"#.to_string(),
            description: Some("Test channel".to_string()),
        };

        assert_eq!(req.name, "Test SMS");
        assert_eq!(req.channel_type, "sms");
        assert!(req.description.is_some());
    }

    #[test]
    fn test_update_channel_request() {
        let req = UpdateNotificationChannelRequest {
            name: Some("Updated SMS".to_string()),
            channel_type: None,
            config: None,
            description: None,
        };

        assert_eq!(req.name, Some("Updated SMS".to_string()));
    }

    #[test]
    fn test_send_message_request() {
        let req = SendMessageRequest {
            recipient: "13800138000".to_string(),
            title: Some("Test Title".to_string()),
            content: "Test content".to_string(),
        };

        assert_eq!(req.recipient, "13800138000");
        assert_eq!(req.title, Some("Test Title".to_string()));
    }

    #[test]
    fn test_channel_statistics() {
        let stats = ChannelStatistics {
            total_channels: 10,
            enabled_channels: 8,
            sms_channels: 3,
            email_channels: 3,
            webhook_channels: 4,
        };

        assert_eq!(stats.total_channels, 10);
        assert_eq!(stats.enabled_channels, 8);
    }

    #[test]
    fn test_sms_config_parsing() {
        let config = r#"{
            "provider": "aliyun",
            "access_key": "test_key",
            "access_secret": "test_secret",
            "sign_name": "TinyIoT",
            "template_id": "SMS_123456"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(parsed["provider"], "aliyun");
        assert_eq!(parsed["sign_name"], "TinyIoT");
    }

    #[test]
    fn test_email_config_parsing() {
        let config = r#"{
            "provider": "smtp",
            "smtp_host": "smtp.qq.com",
            "smtp_port": 465,
            "username": "test@qq.com",
            "password": "test_password",
            "from": "TinyIoT <test@qq.com>"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(parsed["smtp_host"], "smtp.qq.com");
        assert_eq!(parsed["smtp_port"], 465);
    }

    #[test]
    fn test_webhook_config_parsing() {
        let config = r#"{
            "url": "https://oapi.dingtalk.com/robot/send?access_token=xxx",
            "method": "POST",
            "headers": {
                "Content-Type": "application/json"
            },
            "secret": "SECxxx"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(parsed["url"], "https://oapi.dingtalk.com/robot/send?access_token=xxx");
        assert_eq!(parsed["method"], "POST");
    }
}
