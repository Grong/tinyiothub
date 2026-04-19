use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    errors::{AlarmError, AlarmResult},
    value_objects::*,
};
use crate::domain::event::aggregates::NotificationChannelType;

/// 报警实例实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub device_id: String,
    pub property_id: Option<String>,
    pub rule_id: Option<String>,
    pub alarm_type: AlarmType,
    pub alarm_level: AlarmLevel,
    pub message: String,
    pub alarm_value: Option<String>,
    pub threshold_value: Option<String>,
    pub alarm_time: DateTime<Utc>,
    pub status: AlarmStatus,
    pub acknowledgement: Option<Acknowledgement>,
    pub resolution: Option<Resolution>,
    pub created_at: DateTime<Utc>,
}

impl Alarm {
    /// 创建新报警
    pub fn new(
        device_id: String,
        property_id: Option<String>,
        rule_id: Option<String>,
        alarm_type: AlarmType,
        alarm_level: AlarmLevel,
        message: String,
        alarm_value: Option<String>,
        threshold_value: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id,
            property_id,
            rule_id,
            alarm_type,
            alarm_level,
            message,
            alarm_value,
            threshold_value,
            alarm_time: now,
            status: AlarmStatus::Active,
            acknowledgement: None,
            resolution: None,
            created_at: now,
        }
    }

    /// 确认报警
    pub fn acknowledge(&mut self, user_id: String, note: Option<String>) -> AlarmResult<()> {
        if self.status != AlarmStatus::Active {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "acknowledged".to_string(),
            });
        }

        self.acknowledgement = Some(Acknowledgement::new(user_id, note));
        self.status = AlarmStatus::Acknowledged;

        Ok(())
    }

    /// 解决报警
    pub fn resolve(
        &mut self,
        user_id: String,
        resolution_type: ResolutionType,
        note: Option<String>,
    ) -> AlarmResult<()> {
        if !matches!(self.status, AlarmStatus::Active | AlarmStatus::Acknowledged) {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "resolved".to_string(),
            });
        }

        self.resolution = Some(Resolution::new(user_id, resolution_type, note));
        self.status = AlarmStatus::Resolved;

        Ok(())
    }

    /// 抑制报警
    pub fn suppress(&mut self) -> AlarmResult<()> {
        if self.status != AlarmStatus::Active {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "suppressed".to_string(),
            });
        }

        self.status = AlarmStatus::Suppressed;
        Ok(())
    }

    /// 检查是否可以确认
    pub fn can_acknowledge(&self) -> bool {
        self.status == AlarmStatus::Active
    }

    /// 检查是否可以解决
    pub fn can_resolve(&self) -> bool {
        matches!(self.status, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }

    /// 检查是否是活跃状态
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 报警规则实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_type: RuleType,
    pub condition: AlarmCondition,
    pub alarm_level: AlarmLevel,
    pub is_enabled: bool,
    pub notification_config: NotificationConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AlarmRule {
    /// 创建新规则
    pub fn new(
        name: String,
        description: Option<String>,
        device_id: Option<String>,
        property_id: Option<String>,
        rule_type: RuleType,
        condition: AlarmCondition,
        alarm_level: AlarmLevel,
        notification_config: NotificationConfig,
    ) -> AlarmResult<Self> {
        // 验证规则配置
        Self::validate_config(&name, &condition, &notification_config)?;

        let now = Utc::now();
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description,
            device_id,
            property_id,
            rule_type,
            condition,
            alarm_level,
            is_enabled: true,
            notification_config,
            created_at: now,
            updated_at: now,
        })
    }

    /// 更新规则
    pub fn update(
        &mut self,
        name: Option<String>,
        description: Option<String>,
        condition: Option<AlarmCondition>,
        alarm_level: Option<AlarmLevel>,
        notification_config: Option<NotificationConfig>,
    ) -> AlarmResult<()> {
        if let Some(n) = name {
            if n.is_empty() {
                return Err(AlarmError::InvalidRuleConfig("规则名称不能为空".to_string()));
            }
            self.name = n;
        }

        if let Some(d) = description {
            self.description = Some(d);
        }

        if let Some(c) = condition {
            self.condition = c;
        }

        if let Some(l) = alarm_level {
            self.alarm_level = l;
        }

        if let Some(nc) = notification_config {
            self.notification_config = nc;
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// 启用规则
    pub fn enable(&mut self) {
        self.is_enabled = true;
        self.updated_at = Utc::now();
    }

    /// 禁用规则
    pub fn disable(&mut self) {
        self.is_enabled = false;
        self.updated_at = Utc::now();
    }

    /// 验证规则配置
    fn validate_config(
        name: &str,
        _condition: &AlarmCondition,
        notification_config: &NotificationConfig,
    ) -> AlarmResult<()> {
        if name.is_empty() {
            return Err(AlarmError::InvalidRuleConfig("规则名称不能为空".to_string()));
        }

        if notification_config.enabled && notification_config.channels.is_empty() {
            return Err(AlarmError::InvalidRuleConfig(
                "启用通知时至少需要配置一个通知渠道".to_string(),
            ));
        }

        // 只有在使用需要接收人的渠道时才要求配置接收人
        if notification_config.enabled {
            let needs_recipients = notification_config.channels.iter().any(|ch| {
                matches!(ch, NotificationChannelType::Email | NotificationChannelType::Sms)
            });

            if needs_recipients && notification_config.recipients.is_empty() {
                return Err(AlarmError::InvalidRuleConfig(
                    "使用邮件或短信通知时需要配置接收人".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// 规则类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Threshold,
    Range,
    Change,
    Duration,
    Composite,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::Threshold => "threshold",
            RuleType::Range => "range",
            RuleType::Change => "change",
            RuleType::Duration => "duration",
            RuleType::Composite => "composite",
        }
    }
}
