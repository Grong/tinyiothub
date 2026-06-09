// Alarm service — AlarmService, RuleEngine, AlarmSpecifications, AlarmEventHandler

use std::{collections::HashMap, sync::Arc, time::Instant};

use chrono::{DateTime, Duration, Utc};

use super::{
    notification::NotificationDispatcher,
    repo::{AlarmQueryCriteria, AlarmRepository, AlarmRuleRepository, TimeRange},
    types::*,
};
use crate::modules::event::{
    aggregates::NotificationChannelType, entities::Event, value_objects::EventType,
};

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

    pub async fn get_alarm_by_id(
        &self,
        id: &str,
        workspace_id: Option<&str>,
    ) -> AlarmResult<Option<Alarm>> {
        self.alarm_repository.find_by_id(id, workspace_id).await
    }

    pub async fn acknowledge_alarm(
        &self,
        alarm_id: &str,
        user_id: String,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id, None)
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
            .find_by_id(alarm_id, None)
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
        workspace_id: &str,
    ) -> AlarmResult<AlarmStatistics> {
        let criteria = AlarmQueryCriteria {
            time_range: Some(time_range),
            workspace_id: Some(workspace_id.to_string()),
            ..Default::default()
        };

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
        let mut auto_resolve_ids: Vec<String> = Vec::new();

        let now = Utc::now();
        for alarm in active_alarms {
            // Auto-resolve alarms older than 24 hours
            let age = now.signed_duration_since(alarm.created_at);
            if age.num_hours() >= 24 {
                auto_resolve_ids.push(alarm.id);
            }
        }

        if auto_resolve_ids.is_empty() {
            return Ok(0);
        }

        let count = self
            .alarm_repository
            .batch_update_status(&auto_resolve_ids, AlarmStatus::Resolved)
            .await?;

        tracing::info!("Auto-resolved {} alarms", count);
        Ok(count)
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

    pub async fn get_all_rules(&self, workspace_id: &str) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_enabled(Some(workspace_id)).await
    }

    pub async fn get_rules_by_device(
        &self,
        device_id: &str,
        workspace_id: &str,
    ) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_by_device(device_id, Some(workspace_id)).await
    }

    pub async fn update_rule(
        &self,
        rule: AlarmRule,
        workspace_id: Option<&str>,
    ) -> AlarmResult<()> {
        AlarmSpecifications::is_valid_rule(&rule).map_err(AlarmError::InvalidRuleConfig)?;
        self.rule_repository.update(&rule, workspace_id).await
    }

    pub async fn delete_rule(&self, id: &str, workspace_id: Option<&str>) -> AlarmResult<()> {
        self.rule_repository.delete(id, workspace_id).await
    }

    pub async fn set_rule_enabled(
        &self,
        id: &str,
        enabled: bool,
        workspace_id: Option<&str>,
    ) -> AlarmResult<()> {
        self.rule_repository.set_enabled(id, enabled, workspace_id).await
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
    throttle: std::sync::Mutex<HashMap<(String, String), Instant>>,
}

impl RuleEngine {
    pub fn new(rule_repository: Arc<dyn AlarmRuleRepository>) -> Self {
        Self { rule_repository, throttle: std::sync::Mutex::new(HashMap::new()) }
    }

    pub async fn evaluate_event(&self, event: &Event) -> AlarmResult<Vec<AlarmTrigger>> {
        if !matches!(event.event_type(), EventType::Device(_)) {
            return Ok(vec![]);
        }

        let device_id = event.source().device_id().unwrap_or_else(|| event.source().source_id());
        let property_id = event.content().metadata().get("property_id").and_then(|v| v.as_str());

        let rules = self.load_relevant_rules(device_id, property_id).await?;

        let mut triggers = Vec::new();

        for rule in rules {
            if !rule.is_enabled {
                continue;
            }

            // Throttle: prevent oscillation storms (min 60s between same device+rule evaluation)
            let throttle_key = (device_id.to_string(), rule.id.clone());
            {
                let mut throttle = self.throttle.lock().unwrap();
                // Clean stale entries (older than 5 minutes)
                throttle
                    .retain(|_, instant| instant.elapsed() < std::time::Duration::from_secs(300));
                if let Some(last) = throttle.get(&throttle_key)
                    && last.elapsed() < std::time::Duration::from_secs(60)
                {
                    continue;
                }
                throttle.insert(throttle_key, Instant::now());
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
            AlarmCondition::Duration { condition, .. } => self.check_condition(condition, context),
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
        rules.extend(self.rule_repository.find_by_device(device_id, None).await?);
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
    notification_dispatcher: Arc<NotificationDispatcher>,
}

impl AlarmEventHandler {
    pub fn new(
        alarm_service: Arc<AlarmService>,
        notification_dispatcher: Arc<NotificationDispatcher>,
    ) -> Self {
        let rule_engine = alarm_service.rule_engine();
        Self { alarm_service, rule_engine, notification_dispatcher }
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
            let device_id =
                event.source().device_id().unwrap_or_else(|| event.source().source_id());
            let alarm = Alarm::new(
                device_id.to_string(),
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
                alarm_id = %alarm.id,
                device_id = %alarm.device_id,
                level = %alarm.alarm_level,
                message = %alarm.message,
                "alarm_created"
            );

            // Dispatch notifications for this alarm
            if let Ok(Some(rule)) = self.rule_engine.get_rule(&trigger.rule_id).await {
                self.notification_dispatcher
                    .dispatch(&alarm, &rule, rule.workspace_id.as_deref())
                    .await;
            }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tinyiothub_core::models::event::{
        ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
    };
    use tinyiothub_storage::sqlite::Database;

    use super::*;
    use crate::modules::alarm::repo::SqliteAlarmRuleRepository;

    async fn setup_test_db(pool: &sqlx::SqlitePool) {
        sqlx::query("PRAGMA foreign_keys = OFF").execute(pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY, name TEXT, workspace_id TEXT
            )",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS device_alarm_rules (
                id TEXT PRIMARY KEY, device_id TEXT, property_id TEXT,
                rule_name TEXT NOT NULL, rule_type TEXT NOT NULL,
                condition_config TEXT NOT NULL,
                alarm_level TEXT NOT NULL,
                is_enabled BOOLEAN NOT NULL DEFAULT true,
                description TEXT, workspace_id TEXT, created_by TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_property_change_event(device_id: &str, property_name: &str, value: f64) -> Event {
        let metadata: std::collections::HashMap<String, serde_json::Value> = [
            ("property_id".to_string(), serde_json::Value::String(property_name.to_string())),
            ("property_name".to_string(), serde_json::Value::String(property_name.to_string())),
            ("value".to_string(), serde_json::Value::String(value.to_string())),
        ]
        .into();

        let mut content = RichContent::new(
            format!("Property Changed: {}", property_name),
            vec![ContentElement::Text {
                content: format!("Current value: {}", value),
                format: TextFormat::Plain,
            }],
        );

        for (k, v) in metadata {
            content = content.with_metadata(k, v);
        }

        Event::new_device_event(
            DeviceEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(
                device_id.to_string(),
                property_name.to_string(),
                "test".to_string(),
            ),
            content,
        )
        .expect("failed to create test event")
    }

    #[sqlx::test]
    async fn test_evaluate_event_triggers_alarm(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        // Insert a device
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();

        // Insert an alarm rule: temperature > 80 → Warning
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-1', 'dev-1', 'prop-1', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Simulate property change event: temperature = 85 (> 80)
        let event = make_property_change_event("dev-1", "temperature", 85.0);

        let triggers = engine.evaluate_event(&event).await.unwrap();

        assert!(!triggers.is_empty(), "Should trigger alarm when value exceeds threshold");
        assert_eq!(triggers[0].rule_id, "rule-1");
        assert_eq!(triggers[0].alarm_level, AlarmLevel::Warning);
        assert_eq!(triggers[0].triggered_value.as_deref(), Some("85"));
    }

    #[sqlx::test]
    async fn test_evaluate_event_no_trigger_below_threshold(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-1', 'dev-1', 'prop-1', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // temperature = 25 (below threshold)
        let event = make_property_change_event("dev-1", "temperature", 25.0);
        let triggers = engine.evaluate_event(&event).await.unwrap();
        assert!(triggers.is_empty(), "Should NOT trigger when value is below threshold");
    }
}

#[cfg(test)]
mod integration_tests {
    use std::sync::Arc;

    use sqlx::Row;
    use tinyiothub_core::{
        event::EventHandler,
        models::event::{
            ContentElement, DeviceEventType, EventLevel, EventSource, RichContent, TextFormat,
        },
    };
    use tinyiothub_storage::sqlite::Database;

    use super::*;
    use crate::modules::alarm::repo::{SqliteAlarmRepository, SqliteAlarmRuleRepository};

    async fn setup_full_schema(pool: &sqlx::SqlitePool) {
        sqlx::query("PRAGMA foreign_keys = OFF").execute(pool).await.unwrap();
        sqlx::query("DROP TABLE IF EXISTS device_alarms").execute(pool).await.unwrap();
        sqlx::query("DROP TABLE IF EXISTS device_alarm_rules").execute(pool).await.unwrap();
        sqlx::query("DROP TABLE IF EXISTS device_properties").execute(pool).await.unwrap();
        sqlx::query("DROP TABLE IF EXISTS devices").execute(pool).await.unwrap();

        // Match production schema with FK constraints
        sqlx::query(
            "CREATE TABLE devices (
            id TEXT PRIMARY KEY, name TEXT, display_name TEXT, workspace_id TEXT,
            driver_name TEXT, device_type TEXT, protocol_type TEXT, status TEXT DEFAULT 'online',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE device_properties (
            id TEXT PRIMARY KEY, device_id TEXT NOT NULL, name TEXT NOT NULL,
            data_type TEXT NOT NULL DEFAULT 'float',
            FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
        )",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE device_alarm_rules (
            id TEXT PRIMARY KEY, device_id TEXT, property_id TEXT,
            rule_name TEXT NOT NULL, rule_type TEXT NOT NULL,
            condition_config TEXT NOT NULL, alarm_level TEXT NOT NULL,
            is_enabled BOOLEAN NOT NULL DEFAULT true, description TEXT,
            workspace_id TEXT, created_by TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
            FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE CASCADE
        )",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE device_alarms (
            id TEXT PRIMARY KEY, device_id TEXT NOT NULL, property_id TEXT, rule_id TEXT,
            alarm_level TEXT NOT NULL, alarm_message TEXT NOT NULL,
            alarm_value TEXT, threshold_value TEXT, alarm_time TEXT NOT NULL,
            is_acknowledged BOOLEAN NOT NULL DEFAULT false,
            acknowledged_by TEXT, acknowledged_at TEXT, acknowledged_note TEXT,
            is_resolved BOOLEAN NOT NULL DEFAULT false,
            resolved_by TEXT, resolved_at TEXT, resolved_note TEXT,
            resolution_type TEXT, workspace_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
            FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE SET NULL,
            FOREIGN KEY (rule_id) REFERENCES device_alarm_rules(id) ON DELETE SET NULL
        )",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("PRAGMA foreign_keys = ON").execute(pool).await.unwrap();
    }

    fn make_test_event(
        device_id: &str,
        property_id: &str,
        property_name: &str,
        value: f64,
        old_value: Option<f64>,
    ) -> Event {
        let mut content = RichContent::new(
            format!("Property Changed: {}", property_name),
            vec![ContentElement::Text {
                content: format!("Current value: {}", value),
                format: TextFormat::Plain,
            }],
        )
        .with_metadata(
            "property_id".to_string(),
            serde_json::Value::String(property_id.to_string()),
        )
        .with_metadata(
            "property_name".to_string(),
            serde_json::Value::String(property_name.to_string()),
        )
        .with_metadata("value".to_string(), serde_json::Value::String(value.to_string()));

        if let Some(old) = old_value {
            content = content
                .with_metadata("old_value".to_string(), serde_json::Value::String(old.to_string()));
        }

        Event::new_device_event(
            DeviceEventType::PropertyChange,
            EventLevel::Info,
            EventSource::device_property(
                device_id.to_string(),
                property_id.to_string(),
                "test".to_string(),
            ),
            content,
        )
        .expect("failed to create test event")
    }

    #[sqlx::test]
    async fn test_full_alarm_pipeline_event_to_alarm(pool: sqlx::SqlitePool) {
        setup_full_schema(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO device_properties (id, device_id, name) VALUES ('prop-1', 'dev-1', 'temperature')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-1', 'dev-1', 'prop-1', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        ).execute(&pool).await.unwrap();

        let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
        let rule_repo: Arc<dyn AlarmRuleRepository> =
            Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repo.clone(), rule_repo));
        let notification_dispatcher = Arc::new(NotificationDispatcher::new(db.clone()));
        let handler = AlarmEventHandler::new(alarm_service, notification_dispatcher);

        // Value 85 > threshold 80 → should trigger
        let event = make_test_event("dev-1", "prop-1", "temperature", 85.0, Some(70.0));
        handler.handle(&event).await.unwrap();

        let row = sqlx::query("SELECT * FROM device_alarms WHERE device_id = 'dev-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("device_id"), "dev-1");
        assert_eq!(row.get::<Option<String>, _>("property_id").as_deref(), Some("prop-1"));
        assert_eq!(row.get::<Option<String>, _>("rule_id").as_deref(), Some("rule-1"));
        assert_eq!(row.get::<String, _>("alarm_level"), "warning");
        assert_eq!(row.get::<Option<String>, _>("alarm_value").as_deref(), Some("85"));
        let is_ack: bool = row.get("is_acknowledged");
        let is_res: bool = row.get("is_resolved");
        assert!(!is_ack, "new alarm should not be acknowledged");
        assert!(!is_res, "new alarm should not be resolved");

        // Round-trip test: read back via repo (exercises row_to_alarm datetime parsing)
        let alarm_opt =
            alarm_repo.find_by_id(row.get::<String, _>("id").as_str(), None).await.unwrap();
        assert!(alarm_opt.is_some(), "Should be able to read alarm back via repo");
        let alarm = alarm_opt.unwrap();
        assert_eq!(alarm.device_id, "dev-1");
        assert_eq!(alarm.alarm_level, AlarmLevel::Warning);
        assert_eq!(alarm.alarm_value.as_deref(), Some("85"));
    }

    #[sqlx::test]
    async fn test_alarm_pipeline_no_trigger_below_threshold(pool: sqlx::SqlitePool) {
        setup_full_schema(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO device_properties (id, device_id, name) VALUES ('prop-1', 'dev-1', 'temperature')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-1', 'dev-1', 'prop-1', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        ).execute(&pool).await.unwrap();

        let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
        let rule_repo: Arc<dyn AlarmRuleRepository> =
            Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repo.clone(), rule_repo));
        let notification_dispatcher = Arc::new(NotificationDispatcher::new(db.clone()));
        let handler = AlarmEventHandler::new(alarm_service, notification_dispatcher);

        // Temperature 25 < threshold 80 → should NOT trigger
        let event = make_test_event("dev-1", "prop-1", "temperature", 25.0, None);
        handler.handle(&event).await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "Should not create alarm when value is below threshold");
    }
}
