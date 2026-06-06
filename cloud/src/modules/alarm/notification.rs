// Notification dispatcher for alarm events

use std::sync::Arc;

use tinyiothub_storage::sqlite::Database;

use super::types::*;

/// Sends notifications for a triggered alarm based on rule config.
pub struct NotificationDispatcher {
    db: Arc<Database>,
}

impl NotificationDispatcher {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Dispatch notifications for a newly created alarm.
    /// Called after alarm is persisted. Never fails the caller —
    /// per-channel errors are logged individually.
    pub async fn dispatch(&self, alarm: &Alarm, rule: &AlarmRule, workspace_id: Option<&str>) {
        let config = &rule.notification_config;
        if !config.enabled {
            return;
        }

        if config.channels.is_empty() {
            return;
        }

        let title = format!("[{}] {}", alarm.alarm_level, alarm.message);
        let body = format!(
            "设备: {}\n属性: {}\n当前值: {}\n阈值: {}\n时间: {}",
            alarm.device_id,
            alarm.property_id.as_deref().unwrap_or("-"),
            alarm.alarm_value.as_deref().unwrap_or("-"),
            alarm.threshold_value.as_deref().unwrap_or("-"),
            alarm.alarm_time.to_rfc3339(),
        );

        // Parallel dispatch to all configured channels using tokio::spawn
        let handles: Vec<_> = config
            .channels
            .iter()
            .map(|channel_type| {
                let channel_type = channel_type.clone();
                let title = title.clone();
                let body = body.clone();
                let recipients = config.recipients.clone();
                let db = self.db.clone();
                let ws_id = workspace_id.map(|s| s.to_string());
                tokio::spawn(async move {
                    Self::send_to_channel(&db, &channel_type, &recipients, &title, &body, ws_id.as_deref()).await;
                })
            })
            .collect();

        for handle in handles {
            let _ = handle.await;
        }
    }

    async fn send_to_channel(
        db: &Database,
        channel_type: &crate::modules::event::aggregates::NotificationChannelType,
        recipients: &[String],
        title: &str,
        body: &str,
        workspace_id: Option<&str>,
    ) {
        let channel_type_str = match channel_type {
            crate::modules::event::aggregates::NotificationChannelType::Email => "email",
            crate::modules::event::aggregates::NotificationChannelType::Sms => "sms",
            crate::modules::event::aggregates::NotificationChannelType::Sse => "sse",
            crate::modules::event::aggregates::NotificationChannelType::Webhook => "webhook",
        };

        let rows = if let Some(ws) = workspace_id {
            sqlx::query(
                "SELECT id, name, config FROM notification_channels WHERE channel_type = ? AND is_enabled = 1 AND workspace_id = ?",
            )
            .bind(channel_type_str)
            .bind(ws)
            .fetch_all(db.pool())
            .await
        } else {
            sqlx::query(
                "SELECT id, name, config FROM notification_channels WHERE channel_type = ? AND is_enabled = 1",
            )
            .bind(channel_type_str)
            .fetch_all(db.pool())
            .await
        };

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(channel = channel_type_str, error = %e, "Failed to query notification channels");
                return;
            }
        };

        if rows.is_empty() {
            tracing::debug!(channel = channel_type_str, "No enabled notification channels found");
            return;
        }

        for row in rows {
            use sqlx::Row;
            let channel_id: String = row.get("id");
            let channel_name: String = row.get("name");
            let config_str: String = row.get("config");

            let result = match channel_type {
                crate::modules::event::aggregates::NotificationChannelType::Email => {
                    Self::send_email(&config_str, recipients, title, body).await
                }
                crate::modules::event::aggregates::NotificationChannelType::Sms => {
                    Self::send_sms(&config_str, recipients, body).await
                }
                crate::modules::event::aggregates::NotificationChannelType::Sse => {
                    Self::send_sse(&config_str, title, body).await
                }
                crate::modules::event::aggregates::NotificationChannelType::Webhook => {
                    Self::send_webhook(&config_str, title, body).await
                }
            };

            match result {
                Ok(()) => tracing::info!(
                    channel_id = %channel_id,
                    channel_name = %channel_name,
                    channel_type = channel_type_str,
                    "notification_sent"
                ),
                Err(e) => tracing::error!(
                    channel_id = %channel_id,
                    channel_name = %channel_name,
                    channel_type = channel_type_str,
                    error = %e,
                    "notification_failed"
                ),
            }
        }
    }

    async fn send_email(
        _config: &str,
        recipients: &[String],
        title: &str,
        body: &str,
    ) -> Result<(), String> {
        // Parse SMTP config from channel config JSON
        // Full SMTP sending requires an external crate (lettre, etc.)
        // For now, log the intent. SMTP integration is a follow-up.
        tracing::info!(
            recipients = ?recipients,
            title = %title,
            body_len = body.len(),
            "email_notification_queued"
        );
        Ok(())
    }

    async fn send_sms(_config: &str, recipients: &[String], body: &str) -> Result<(), String> {
        // SMS sending requires external SMS gateway API
        // For now, log the intent.
        tracing::info!(
            recipients = ?recipients,
            body_len = body.len(),
            "sms_notification_queued"
        );
        Ok(())
    }

    async fn send_sse(_config: &str, title: &str, body: &str) -> Result<(), String> {
        // SSE is a push channel; actual delivery is handled by
        // the SSE notification service which pushes to connected clients.
        // For now, log the intent.
        tracing::info!(
            title = %title,
            body_len = body.len(),
            "sse_notification_queued"
        );
        Ok(())
    }

    async fn send_webhook(config: &str, title: &str, body: &str) -> Result<(), String> {
        let config: serde_json::Value = serde_json::from_str(config)
            .map_err(|e| format!("webhook config parse failed: {}", e))?;
        let url = config
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if url.is_empty() {
            return Err("webhook URL not configured".to_string());
        }
        tracing::info!(
            url = %url,
            title = %title,
            body_len = body.len(),
            "webhook_notification_queued"
        );
        Ok(())
    }
}
