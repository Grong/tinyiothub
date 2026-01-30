use chrono::{DateTime, Duration, Utc};

use super::entity::{Alarm, AlarmRule};
use super::value_objects::AlarmStatus;
use crate::domain::event::aggregates::NotificationChannelType;

/// 报警规约
pub struct AlarmSpecifications;

impl AlarmSpecifications {
    /// 检查报警是否可以确认
    pub fn can_acknowledge(alarm: &Alarm) -> bool {
        alarm.status == AlarmStatus::Active
    }

    /// 检查报警是否可以解决
    pub fn can_resolve(alarm: &Alarm) -> bool {
        matches!(
            alarm.status,
            AlarmStatus::Active | AlarmStatus::Acknowledged
        )
    }

    /// 检查报警是否需要通知
    pub fn should_notify(alarm: &Alarm, rule: &AlarmRule) -> bool {
        rule.notification_config.enabled && !matches!(alarm.status, AlarmStatus::Suppressed)
    }

    /// 检查报警是否应该被抑制
    pub fn should_suppress(
        _alarm: &Alarm,
        last_alarm_time: Option<DateTime<Utc>>,
        suppress_duration: std::time::Duration,
    ) -> bool {
        if let Some(last_time) = last_alarm_time {
            let duration_since_last = Utc::now().signed_duration_since(last_time);
            let suppress_chrono_duration = Duration::seconds(suppress_duration.as_secs() as i64);
            duration_since_last < suppress_chrono_duration
        } else {
            false
        }
    }

    /// 检查规则是否有效
    pub fn is_valid_rule(rule: &AlarmRule) -> Result<(), String> {
        if rule.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }

        if rule.notification_config.enabled {
            if rule.notification_config.channels.is_empty() {
                return Err("启用通知时至少需要配置一个通知渠道".to_string());
            }

            // 只有在使用需要接收人的渠道时才要求配置接收人
            let needs_recipients = rule.notification_config.channels.iter().any(|ch| {
                matches!(
                    ch,
                    NotificationChannelType::Email | NotificationChannelType::Sms
                )
            });

            if needs_recipients && rule.notification_config.recipients.is_empty() {
                return Err("使用邮件或短信通知时需要配置接收人".to_string());
            }
        }

        Ok(())
    }

    /// 检查报警是否过期（用于自动清理）
    pub fn is_expired(alarm: &Alarm, retention_days: i64) -> bool {
        if !alarm.status.is_resolved() {
            return false;
        }

        let now = Utc::now();
        let age = now.signed_duration_since(alarm.created_at);
        age > Duration::days(retention_days)
    }
}
