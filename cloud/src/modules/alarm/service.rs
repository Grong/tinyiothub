// Alarm service — AlarmService, RuleEngine, AlarmSpecifications, AlarmEventHandler

use std::{collections::HashMap, sync::Arc};
use chrono::{DateTime, Duration, Utc};

use super::types::*;
use super::repo::{AlarmQueryCriteria, AlarmRepository, AlarmRuleRepository, TimeRange};
use crate::modules::event::aggregates::NotificationChannelType;
use crate::modules::event::entities::Event;
use crate::modules::event::value_objects::EventType;

/// 报警业务服务
pub struct AlarmService {
    alarm_repository: Arc<dyn AlarmRepository>,
    rule_repository: Arc<dyn AlarmRuleRepository>,
    rule_engine: Arc<RuleEngine>,
}

impl AlarmService {
    pub fn new(
        alarm_repository: Arc<dyn AlarmRepository>,
        rule_repository: Arc<dyn AlarmRuleRepository>,
    ) -> Self {
        let rule_engine = Arc::new(RuleEngine::new(rule_repository.clone()));
        Self { alarm_repository, rule_repository, rule_engine }
    }

    pub async fn create_alarm(&self, alarm: Alarm) -> AlarmResult<Alarm> {
        self.alarm_repository.create(&alarm).await?;
        Ok(alarm)
    }

    pub async fn get_alarm_by_id(&self, id: &str) -> AlarmResult<Option<Alarm>> {
        self.alarm_repository.find_by_id(id).await
    }

    pub async fn acknowledge_alarm(
        &self,
        alarm_id: &str,
        user_id: String,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id)
            .await?
            .ok_or_else(|| AlarmError::NotFound(alarm_id.to_string()))?;

        if !AlarmSpecifications::can_acknowledge(&alarm) {
            return Err(AlarmError::InvalidStatusTransition {
                from: alarm.status.as_str().to_string(),
                to: AlarmStatus::Acknowledged.as_str().to_string(),
            });
        }

        alarm.acknowledge(user_id, note)?;
        self.alarm_repository.update(&alarm).await?;
        Ok(())
    }

    pub async fn resolve_alarm(
        &self,
        alarm_id: &str,
        user_id: String,
        resolution_type: ResolutionType,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id)
            .await?
            .ok_or_else(|| AlarmError::NotFound(alarm_id.to_string()))?;

        if !AlarmSpecifications::can_resolve(&alarm) {
            return Err(AlarmError::InvalidStatusTransition {
                from: alarm.status.as_str().to_string(),
                to: AlarmStatus::Resolved.as_str().to_string(),
            });
        }

        alarm.resolve(user_id, resolution_type, note)?;
        self.alarm_repository.update(&alarm).await?;
        Ok(())
    }

    pub async fn batch_acknowledge(
        &self,
        alarm_ids: Vec<String>,
        user_id: String,
    ) -> AlarmResult<usize> {
        let mut count = 0;
        for alarm_id in alarm_ids {
            if let Ok(()) = self.acknowledge_alarm(&alarm_id, user_id.clone(), None).await {
                count += 1;
            }
        }
        Ok(count)
    }

    pub async fn batch_resolve(
        &self,
        alarm_ids: Vec<String>,
        user_id: String,
        resolution_type: ResolutionType,
    ) -> AlarmResult<usize> {
        let mut count = 0;
        for alarm_id in alarm_ids {
            if let Ok(()) =
                self.resolve_alarm(&alarm_id, user_id.clone(), resolution_type, None).await
            {
                count += 1;
            }
        }
        Ok(count)
    }

    pub async fn get_active_alarms(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
        self.alarm_repository.find_active(device_id).await
    }

    pub async fn get_alarm_history(&self, criteria: AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>> {
        self.alarm_repository.find_by_criteria(&criteria).await
    }

    pub async fn count_alarms(&self, criteria: AlarmQueryCriteria) -> AlarmResult<u64> {
        self.alarm_repository.count_by_criteria(&criteria).await
    }

    pub async fn get_alarm_statistics(
        &self,
        time_range: TimeRange,
    ) -> AlarmResult<AlarmStatistics> {
        let criteria = AlarmQueryCriteria { time_range: Some(time_range), ..Default::default() };

        let alarms = self.alarm_repository.find_by_criteria(&criteria).await?;

        let total_count = alarms.len() as u64;
        let active_count = alarms.iter().filter(|a| a.status == AlarmStatus::Active).count() as u64;
        let acknowledged_count =
            alarms.iter().filter(|a| a.status == AlarmStatus::Acknowledged).count() as u64;
        let resolved_count =
            alarms.iter().filter(|a| a.status == AlarmStatus::Resolved).count() as u64;

        Ok(AlarmStatistics { total_count, active_count, acknowledged_count, resolved_count })
    }

    pub async fn check_auto_resolution(&self) -> AlarmResult<usize> {
        let active_alarms = self.alarm_repository.find_active(None).await?;
        let resolved_count = 0;

        for _alarm in active_alarms {
            // TODO: 实现自动解决逻辑
        }

        Ok(resolved_count)
    }

    // 规则管理方法

    pub async fn create_rule(&self, rule: AlarmRule) -> AlarmResult<AlarmRule> {
        AlarmSpecifications::is_valid_rule(&rule).map_err(AlarmError::InvalidRuleConfig)?;
        self.rule_repository.create(&rule).await?;
        Ok(rule)
    }

    pub async fn get_rule_by_id(&self, id: &str) -> AlarmResult<Option<AlarmRule>> {
        self.rule_repository.find_by_id(id).await
    }

    pub async fn get_all_rules(&self) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_enabled().await
    }

    pub async fn get_rules_by_device(&self, device_id: &str) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_by_device(device_id).await
    }

    pub async fn update_rule(&self, rule: AlarmRule) -> AlarmResult<()> {
        AlarmSpecifications::is_valid_rule(&rule).map_err(AlarmError::InvalidRuleConfig)?;
        self.rule_repository.update(&rule).await
    }

    pub async fn delete_rule(&self, id: &str) -> AlarmResult<()> {
        self.rule_repository.delete(id).await
    }

    pub async fn set_rule_enabled(&self, id: &str, enabled: bool) -> AlarmResult<()> {
        self.rule_repository.set_enabled(id, enabled).await
    }

    pub fn rule_engine(&self) -> Arc<RuleEngine> {
        self.rule_engine.clone()
    }
}

/// 报警统计
#[derive(Debug, Clone)]
pub struct AlarmStatistics {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}

// ============================================================================
// Rule Engine
// ============================================================================

/// 规则引擎
pub struct RuleEngine {
    rule_repository: Arc<dyn AlarmRuleRepository>,
}

impl RuleEngine {
    pub fn new(rule_repository: Arc<dyn AlarmRuleRepository>) -> Self {
        Self { rule_repository }
    }

    pub async fn evaluate_event(&self, event: &Event) -> AlarmResult<Vec<AlarmTrigger>> {
        if !matches!(event.event_type(), EventType::Device(_)) {
            return Ok(vec![]);
        }

        let device_id = event.source().source_id();
        let property_id = event.content().metadata().get("property_id").and_then(|v| v.as_str());

        let rules = self.load_relevant_rules(device_id, property_id).await?;

        let mut triggers = Vec::new();

        for rule in rules {
            if !rule.is_enabled {
                continue;
            }

            if let Some(trigger) = self.evaluate_rule(&rule, event).await? {
                triggers.push(trigger);
            }
        }

        Ok(triggers)
    }

    pub async fn evaluate_rule(
        &self,
        rule: &AlarmRule,
        event: &Event,
    ) -> AlarmResult<Option<AlarmTrigger>> {
        let context = EvaluationContext::from_event(event);

        let triggered = self.check_condition(&rule.condition, &context)?;

        if triggered {
            let trigger = AlarmTrigger {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                alarm_level: rule.alarm_level,
                alarm_type: self.determine_alarm_type(event, rule),
                message: self.generate_message(event, rule, &context),
                triggered_value: context.current_value.clone(),
                threshold_value: self.extract_threshold(&rule.condition),
            };

            Ok(Some(trigger))
        } else {
            Ok(None)
        }
    }

    fn check_condition(
        &self,
        condition: &AlarmCondition,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        match condition {
            AlarmCondition::Threshold { operator, value } => {
                self.check_threshold(operator, *value, context)
            }
            AlarmCondition::Range { min, max, inclusive } => {
                self.check_range(*min, *max, *inclusive, context)
            }
            AlarmCondition::Change { change_type, threshold, .. } => {
                self.check_change(change_type, *threshold, context)
            }
            AlarmCondition::Duration { condition, .. } => {
                self.check_condition(condition, context)
            }
            AlarmCondition::Composite { operator, conditions } => {
                self.check_composite(operator, conditions, context)
            }
        }
    }

    fn check_threshold(
        &self,
        operator: &ComparisonOperator,
        threshold: f64,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let value = context
            .get_numeric_value()
            .ok_or_else(|| AlarmError::EvaluationError("无法获取数值".to_string()))?;
        Ok(operator.evaluate(value, threshold))
    }

    fn check_range(
        &self,
        min: Option<f64>,
        max: Option<f64>,
        inclusive: bool,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let value = context
            .get_numeric_value()
            .ok_or_else(|| AlarmError::EvaluationError("无法获取数值".to_string()))?;

        let below_min = if let Some(min_val) = min {
            if inclusive { value < min_val } else { value <= min_val }
        } else {
            false
        };

        let above_max = if let Some(max_val) = max {
            if inclusive { value > max_val } else { value >= max_val }
        } else {
            false
        };

        Ok(below_min || above_max)
    }

    fn check_change(
        &self,
        change_type: &ChangeType,
        threshold: f64,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let current = context
            .get_numeric_value()
            .ok_or_else(|| AlarmError::EvaluationError("无法获取当前值".to_string()))?;

        let previous = context
            .previous_value
            .as_ref()
            .and_then(|v| v.parse::<f64>().ok())
            .ok_or_else(|| AlarmError::EvaluationError("无法获取历史值".to_string()))?;

        let change = current - previous;
        let abs_change = change.abs();

        match change_type {
            ChangeType::Increase => Ok(change > threshold),
            ChangeType::Decrease => Ok(change < -threshold),
            ChangeType::Any => Ok(abs_change > threshold),
        }
    }

    fn check_composite(
        &self,
        operator: &LogicalOperator,
        conditions: &[AlarmCondition],
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        match operator {
            LogicalOperator::And => {
                for condition in conditions {
                    if !self.check_condition(condition, context)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            LogicalOperator::Or => {
                for condition in conditions {
                    if self.check_condition(condition, context)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            LogicalOperator::Not => {
                if conditions.len() != 1 {
                    return Err(AlarmError::InvalidCondition(
                        "NOT 运算符只能有一个条件".to_string(),
                    ));
                }
                Ok(!self.check_condition(&conditions[0], context)?)
            }
        }
    }

    async fn load_relevant_rules(
        &self,
        device_id: &str,
        property_id: Option<&str>,
    ) -> AlarmResult<Vec<AlarmRule>> {
        let mut rules = Vec::new();
        rules.extend(self.rule_repository.find_global_rules().await?);
        rules.extend(self.rule_repository.find_by_device(device_id).await?);
        if let Some(prop_id) = property_id {
            rules.extend(self.rule_repository.find_by_property(device_id, prop_id).await?);
        }
        Ok(rules)
    }

    fn determine_alarm_type(&self, _event: &Event, _rule: &AlarmRule) -> AlarmType {
        AlarmType::PropertyThreshold
    }

    fn generate_message(
        &self,
        event: &Event,
        rule: &AlarmRule,
        _context: &EvaluationContext,
    ) -> String {
        format!("{}: {}", rule.name, event.content().title())
    }

    fn extract_threshold(&self, condition: &AlarmCondition) -> Option<String> {
        match condition {
            AlarmCondition::Threshold { value, .. } => Some(value.to_string()),
            AlarmCondition::Range { min, max, .. } => {
                if let (Some(min_val), Some(max_val)) = (min, max) {
                    Some(format!("{}-{}", min_val, max_val))
                } else if let Some(min_val) = min {
                    Some(format!(">={}", min_val))
                } else {
                    max.as_ref().map(|max_val| format!("<={}", max_val))
                }
            }
            _ => None,
        }
    }

    pub async fn get_rule(&self, rule_id: &str) -> AlarmResult<Option<AlarmRule>> {
        self.rule_repository.find_by_id(rule_id).await
    }
}

/// 报警触发信息
#[derive(Debug, Clone)]
pub struct AlarmTrigger {
    pub rule_id: String,
    pub rule_name: String,
    pub alarm_level: AlarmLevel,
    pub alarm_type: AlarmType,
    pub message: String,
    pub triggered_value: Option<String>,
    pub threshold_value: Option<String>,
}

/// 评估上下文
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub current_value: Option<String>,
    pub previous_value: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl EvaluationContext {
    pub fn from_event(event: &Event) -> Self {
        let metadata = event.content().metadata().clone();
        let current_value = metadata.get("value").and_then(|v| v.as_str()).map(String::from);
        let previous_value = metadata.get("old_value").and_then(|v| v.as_str()).map(String::from);
        Self { current_value, previous_value, metadata }
    }

    pub fn get_numeric_value(&self) -> Option<f64> {
        self.current_value.as_ref()?.parse().ok()
    }
}

// ============================================================================
// Alarm Specifications
// ============================================================================

/// 报警规约
pub struct AlarmSpecifications;

impl AlarmSpecifications {
    pub fn can_acknowledge(alarm: &Alarm) -> bool {
        alarm.status == AlarmStatus::Active
    }

    pub fn can_resolve(alarm: &Alarm) -> bool {
        matches!(alarm.status, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }

    pub fn should_notify(alarm: &Alarm, rule: &AlarmRule) -> bool {
        rule.notification_config.enabled && !matches!(alarm.status, AlarmStatus::Suppressed)
    }

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

    pub fn is_valid_rule(rule: &AlarmRule) -> Result<(), String> {
        if rule.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }
        if rule.notification_config.enabled {
            if rule.notification_config.channels.is_empty() {
                return Err("启用通知时至少需要配置一个通知渠道".to_string());
            }
            let needs_recipients = rule.notification_config.channels.iter().any(|ch| {
                matches!(ch, NotificationChannelType::Email | NotificationChannelType::Sms)
            });
            if needs_recipients && rule.notification_config.recipients.is_empty() {
                return Err("使用邮件或短信通知时需要配置接收人".to_string());
            }
        }
        Ok(())
    }

    pub fn is_expired(alarm: &Alarm, retention_days: i64) -> bool {
        if !alarm.status.is_resolved() {
            return false;
        }
        let now = Utc::now();
        let age = now.signed_duration_since(alarm.created_at);
        age > Duration::days(retention_days)
    }
}

// ============================================================================
// Alarm Event Handler
// ============================================================================

/// 报警事件处理器
pub struct AlarmEventHandler {
    alarm_service: Arc<AlarmService>,
    rule_engine: Arc<RuleEngine>,
}

impl AlarmEventHandler {
    pub fn new(alarm_service: Arc<AlarmService>) -> Self {
        let rule_engine = alarm_service.rule_engine();
        Self { alarm_service, rule_engine }
    }
}

#[async_trait::async_trait]
impl crate::shared::event::EventHandler for AlarmEventHandler {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        let triggers = self
            .rule_engine
            .evaluate_event(event)
            .await
            .map_err(|e| tinyiothub_core::error::Error::Internal(e.to_string()))?;

        if triggers.is_empty() {
            return Ok(());
        }

        for trigger in triggers {
            let alarm = Alarm::new(
                event.source().source_id().to_string(),
                event
                    .content()
                    .metadata()
                    .get("property_id")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                Some(trigger.rule_id.clone()),
                trigger.alarm_type,
                trigger.alarm_level,
                trigger.message,
                trigger.triggered_value,
                trigger.threshold_value,
            );

            if let Err(e) = self.alarm_service.create_alarm(alarm.clone()).await {
                tracing::error!("Failed to create alarm: {}", e);
                continue;
            }

            tracing::info!(
                "Alarm created: device={}, level={:?}, message={}",
                alarm.device_id,
                alarm.alarm_level,
                alarm.message
            );
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "AlarmEventHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Device(_))
    }

    fn priority(&self) -> u8 {
        50
    }
}
