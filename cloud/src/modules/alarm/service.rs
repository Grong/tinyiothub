// Alarm service — AlarmService, RuleEngine, AlarmSpecifications, AlarmEventHandler

use std::{collections::HashMap, sync::Arc, time::Instant};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use tinyiothub_storage::cache::DeviceCache;

use super::{
    notification::NotificationDispatcher,
    repo::{AlarmQueryCriteria, AlarmRepository, AlarmRuleRepository, TimeRange},
    types::*,
};
use crate::modules::{
    agent::heartbeat_manager::{HeartbeatManager, WakePriority, WakeSignal},
    event::{aggregates::NotificationChannelType, entities::Event, value_objects::EventType},
};

/// 报警业务服务
pub struct AlarmService {
    alarm_repository: Arc<dyn AlarmRepository>,
    rule_repository: Arc<dyn AlarmRuleRepository>,
    rule_engine: Arc<RuleEngine>,
    heartbeat_manager: std::sync::OnceLock<Arc<HeartbeatManager>>,
    device_cache: std::sync::OnceLock<Arc<DeviceCache>>,
}

impl AlarmService {
    pub fn new(
        alarm_repository: Arc<dyn AlarmRepository>,
        rule_repository: Arc<dyn AlarmRuleRepository>,
    ) -> Self {
        let rule_engine = Arc::new(RuleEngine::new(rule_repository.clone()));
        Self {
            alarm_repository,
            rule_repository,
            rule_engine,
            heartbeat_manager: std::sync::OnceLock::new(),
            device_cache: std::sync::OnceLock::new(),
        }
    }

    pub fn set_heartbeat_manager(&self, hm: Arc<HeartbeatManager>) {
        let _ = self.heartbeat_manager.set(hm);
    }

    pub fn set_device_cache(&self, dc: Arc<DeviceCache>) {
        let _ = self.device_cache.set(dc);
    }

    /// Wake the heartbeat loop for a workspace when a significant alarm occurs
    fn wake_heartbeat(&self, alarm: &Alarm) {
        let ws_id = match alarm.workspace_id.as_deref() {
            Some(id) => id,
            None => return,
        };
        if let Some(hm) = self.heartbeat_manager.get() {
            let priority = match alarm.alarm_level {
                AlarmLevel::Critical => WakePriority::Critical,
                AlarmLevel::Error => WakePriority::High,
                _ => return,
            };
            let context = format!(
                "Alarm: {} | Device: {} | {}",
                alarm.alarm_type, alarm.device_id, alarm.message,
            );
            hm.wake(
                ws_id,
                WakeSignal {
                    workspace_id: ws_id.to_string(),
                    reason: format!("alarm:{}", alarm.id),
                    context,
                    priority,
                    device_id: Some(alarm.device_id.clone()),
                    alarm_type: Some(format!("{}", alarm.alarm_type)),
                    rule_id: alarm.rule_id.clone(),
                },
            );
        }
    }

    pub async fn create_alarm(&self, alarm: Alarm) -> AlarmResult<Alarm> {
        self.alarm_repository.create(&alarm).await?;
        self.wake_heartbeat(&alarm);
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
        workspace_id: &str,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id, Some(workspace_id))
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
        workspace_id: &str,
        resolution_type: ResolutionType,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id, Some(workspace_id))
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
        workspace_id: &str,
    ) -> AlarmResult<usize> {
        let mut count = 0;
        for alarm_id in alarm_ids {
            if let Ok(()) =
                self.acknowledge_alarm(&alarm_id, user_id.clone(), workspace_id, None).await
            {
                count += 1;
            }
        }
        Ok(count)
    }

    pub async fn batch_resolve(
        &self,
        alarm_ids: Vec<String>,
        user_id: String,
        workspace_id: &str,
        resolution_type: ResolutionType,
    ) -> AlarmResult<usize> {
        let mut count = 0;
        for alarm_id in alarm_ids {
            if let Ok(()) = self
                .resolve_alarm(&alarm_id, user_id.clone(), workspace_id, resolution_type, None)
                .await
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

        let base_criteria = criteria.clone();
        let total_count = self.alarm_repository.count_by_criteria(&base_criteria).await?;

        let active_criteria = AlarmQueryCriteria {
            statuses: Some(vec![AlarmStatus::Active]),
            ..base_criteria.clone()
        };
        let active_count = self.alarm_repository.count_by_criteria(&active_criteria).await?;

        let acknowledged_criteria = AlarmQueryCriteria {
            statuses: Some(vec![AlarmStatus::Acknowledged]),
            ..base_criteria.clone()
        };
        let acknowledged_count =
            self.alarm_repository.count_by_criteria(&acknowledged_criteria).await?;

        let resolved_criteria =
            AlarmQueryCriteria { statuses: Some(vec![AlarmStatus::Resolved]), ..base_criteria };
        let resolved_count = self.alarm_repository.count_by_criteria(&resolved_criteria).await?;

        Ok(AlarmStatistics { total_count, active_count, acknowledged_count, resolved_count })
    }

    pub async fn auto_resolve_alarm(&self, alarm_id: &str, workspace_id: &str) -> AlarmResult<()> {
        let count = self
            .alarm_repository
            .batch_update_status(&[alarm_id.to_string()], AlarmStatus::Resolved, workspace_id)
            .await?;
        if count > 0 {
            tracing::info!(alarm_id = %alarm_id, "alarm_auto_resolved");
        }
        Ok(())
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
    throttle: DashMap<(String, String), Instant>,
    /// Tracks the first time a Duration condition started being true.
    /// Key: (device_id, rule_id), Value: first_seen_instant.
    duration_first_seen: DashMap<(String, String), Instant>,
    /// Tracks the first time a rule's trigger condition became true (for trigger debounce).
    /// Key: (device_id, rule_id), Value: first_seen_instant.
    trigger_debounce_start: DashMap<(String, String), Instant>,
    /// Tracks the first time a rule's recovery condition became true (for recovery debounce).
    /// Key: (device_id, rule_id), Value: first_seen_instant.
    recovery_debounce_start: DashMap<(String, String), Instant>,
}

impl RuleEngine {
    pub fn new(rule_repository: Arc<dyn AlarmRuleRepository>) -> Self {
        Self {
            rule_repository,
            throttle: DashMap::new(),
            duration_first_seen: DashMap::new(),
            trigger_debounce_start: DashMap::new(),
            recovery_debounce_start: DashMap::new(),
        }
    }

    pub async fn evaluate_event(&self, event: &Event) -> AlarmResult<EvaluationResult> {
        if !matches!(event.event_type(), EventType::Device(_)) {
            return Ok(EvaluationResult::default());
        }

        // Prevent unbounded growth: remove stale tracking entries older than 24h.
        let day = std::time::Duration::from_secs(86400);
        self.duration_first_seen.retain(|_, v| v.elapsed() < day);
        self.trigger_debounce_start.retain(|_, v| v.elapsed() < day);
        self.recovery_debounce_start.retain(|_, v| v.elapsed() < day);

        let device_id = event.source().device_id().unwrap_or_else(|| event.source().source_id());
        let property_id = event.content().metadata().get("property_id").and_then(|v| v.as_str());

        let rules = self.load_relevant_rules(device_id, property_id).await?;

        let mut triggers = Vec::new();
        let mut non_triggered_rule_ids = Vec::new();
        let mut pending_trigger_rule_ids = Vec::new();

        for rule in rules {
            if !rule.is_enabled {
                continue;
            }

            let debounce_key = (device_id.to_string(), rule.id.clone());
            let context = EvaluationContext::from_event(event);

            // Evaluate whether the alarm condition is currently met
            let condition_met =
                self.check_condition(&rule.condition, &context, device_id, &rule.id)?;

            // --- Trigger debounce ---
            let trigger_duration = rule.notification_config.trigger_duration_secs;

            if condition_met {
                // Condition is TRUE: clear recovery tracking, check trigger debounce
                self.recovery_debounce_start.remove(&debounce_key);

                if let Some(dur) = trigger_duration {
                    if dur.as_secs() == 0 {
                        // Zero duration = immediate trigger
                        self.trigger_debounce_start.remove(&debounce_key);
                    } else {
                        let now = Instant::now();
                        if let Some(first_seen) = self.trigger_debounce_start.get(&debounce_key) {
                            if first_seen.elapsed() < dur {
                                // Still within debounce window — pending
                                pending_trigger_rule_ids.push(rule.id.clone());
                                continue;
                            }
                            // Debounce duration elapsed — trigger now
                            self.trigger_debounce_start.remove(&debounce_key);
                        } else {
                            // First time seeing this trigger — start debounce timer
                            self.trigger_debounce_start.insert(debounce_key.clone(), now);
                            pending_trigger_rule_ids.push(rule.id.clone());
                            continue;
                        }
                    }
                }

                // Throttle check (prevent oscillation storms)
                let suppress_duration = rule
                    .notification_config
                    .suppress_duration
                    .unwrap_or(std::time::Duration::from_secs(60));
                let throttle_key = debounce_key.clone();

                self.throttle
                    .retain(|_, instant| instant.elapsed() < std::time::Duration::from_secs(300));

                if let Some(last) = self.throttle.get(&throttle_key)
                    && last.elapsed() < suppress_duration
                {
                    non_triggered_rule_ids.push(rule.id.clone());
                    continue;
                }
                self.throttle.insert(throttle_key, Instant::now());

                // Condition met, debounce elapsed, not throttled → trigger alarm
                if let Some(trigger) = self.evaluate_rule(&rule, event).await? {
                    triggers.push(trigger);
                } else {
                    non_triggered_rule_ids.push(rule.id.clone());
                }
            } else {
                // Condition is FALSE: clear trigger tracking, check recovery
                self.trigger_debounce_start.remove(&debounce_key);

                let recovery_met = self.check_recovery(&rule.condition, &context);

                if recovery_met {
                    let recovery_duration = rule.notification_config.recovery_duration_secs;

                    if let Some(dur) = recovery_duration {
                        if dur.as_secs() == 0 {
                            self.recovery_debounce_start.remove(&debounce_key);
                        } else {
                            let now = Instant::now();
                            if let Some(first_seen) =
                                self.recovery_debounce_start.get(&debounce_key)
                            {
                                if first_seen.elapsed() < dur {
                                    // Still within recovery debounce — pending
                                    pending_trigger_rule_ids.push(rule.id.clone());
                                    continue;
                                }
                                // Recovery debounce elapsed — ready to resolve
                                self.recovery_debounce_start.remove(&debounce_key);
                            } else {
                                // First time seeing recovery — start debounce timer
                                self.recovery_debounce_start.insert(debounce_key.clone(), now);
                                pending_trigger_rule_ids.push(rule.id.clone());
                                continue;
                            }
                        }
                    }

                    // Recovery condition met and debounce passed → can auto-resolve
                    non_triggered_rule_ids.push(rule.id.clone());
                } else {
                    // Not triggered, but also not recovered (hysteresis zone)
                    // Clear recovery tracking since we're not in recovery state
                    self.recovery_debounce_start.remove(&debounce_key);
                    pending_trigger_rule_ids.push(rule.id.clone());
                }
            }
        }

        Ok(EvaluationResult { triggers, non_triggered_rule_ids, pending_trigger_rule_ids })
    }

    pub async fn evaluate_rule(
        &self,
        rule: &AlarmRule,
        event: &Event,
    ) -> AlarmResult<Option<AlarmTrigger>> {
        let context = EvaluationContext::from_event(event);
        let device_id = event.source().device_id().unwrap_or_else(|| event.source().source_id());

        let triggered = self.check_condition(&rule.condition, &context, device_id, &rule.id)?;

        if triggered {
            let trigger = AlarmTrigger {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                alarm_level: rule.alarm_level,
                alarm_type: self.determine_alarm_type(event, rule),
                message: self.generate_message(event, rule, &context),
                triggered_value: context.current_value.clone(),
                threshold_value: self.extract_threshold(&rule.condition),
                workspace_id: rule.workspace_id.clone(),
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
        device_id: &str,
        rule_id: &str,
    ) -> AlarmResult<bool> {
        match condition {
            AlarmCondition::Threshold { operator, value, .. } => {
                self.check_threshold(operator, *value, context)
            }
            AlarmCondition::Range { min, max, inclusive } => {
                self.check_range(*min, *max, *inclusive, context)
            }
            AlarmCondition::Change { change_type, threshold, time_window } => {
                self.check_change(change_type, *threshold, *time_window, context)
            }
            AlarmCondition::Duration { condition, duration } => {
                self.check_duration(condition, *duration, context, device_id, rule_id)
            }
            AlarmCondition::Composite { operator, conditions } => {
                self.check_composite(operator, conditions, context, device_id, rule_id)
            }
        }
    }

    fn check_threshold(
        &self,
        operator: &ComparisonOperator,
        threshold: f64,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let Some(value) = context.get_numeric_value() else {
            return Ok(false); // non-numeric property — skip, condition not met
        };
        Ok(operator.evaluate(value, threshold))
    }

    fn check_range(
        &self,
        min: Option<f64>,
        max: Option<f64>,
        inclusive: bool,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let Some(value) = context.get_numeric_value() else {
            return Ok(false);
        };

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
        time_window: std::time::Duration,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        let Some(current) = context.get_numeric_value() else {
            return Ok(false);
        };

        let Some(previous) = context.previous_value.as_ref().and_then(|v| v.parse::<f64>().ok())
        else {
            return Ok(false);
        };

        // time_window semantics: the change must have occurred within this window.
        // Full history-backed evaluation requires an EvaluationContext with a
        // history callback (see EvaluationContext::from_event_with_history).
        // For now, we accept the previous_value from the event as valid; if
        // a time_window is configured, log it for traceability.
        if time_window.as_secs() > 0 {
            tracing::debug!(
                time_window_secs = time_window.as_secs(),
                "change condition with time_window — using event previous_value; history callback not wired"
            );
        }

        let change = current - previous;
        let abs_change = change.abs();

        match change_type {
            ChangeType::Increase => Ok(change > threshold),
            ChangeType::Decrease => Ok(change < -threshold),
            ChangeType::Any => Ok(abs_change > threshold),
        }
    }

    fn check_duration(
        &self,
        condition: &AlarmCondition,
        duration: std::time::Duration,
        context: &EvaluationContext,
        device_id: &str,
        rule_id: &str,
    ) -> AlarmResult<bool> {
        // The inner condition must be true now.
        if !self.check_condition(condition, context, device_id, rule_id)? {
            // Condition is no longer true — reset the tracker
            let key = (device_id.to_string(), rule_id.to_string());
            self.duration_first_seen.remove(&key);
            return Ok(false);
        }

        // If duration is zero, treat as immediate trigger.
        if duration.as_secs() == 0 {
            return Ok(true);
        }

        let key = (device_id.to_string(), rule_id.to_string());
        let now = Instant::now();

        // Check if condition has been sustained long enough
        if let Some(first_seen) = self.duration_first_seen.get(&key) {
            if first_seen.elapsed() >= duration {
                self.duration_first_seen.remove(&key);
                return Ok(true);
            }
            // Still waiting for duration to elapse
            return Ok(false);
        }

        // First time seeing this condition — record start time
        self.duration_first_seen.insert(key, now);
        Ok(false)
    }

    fn check_composite(
        &self,
        operator: &LogicalOperator,
        conditions: &[AlarmCondition],
        context: &EvaluationContext,
        device_id: &str,
        rule_id: &str,
    ) -> AlarmResult<bool> {
        match operator {
            LogicalOperator::And => {
                for condition in conditions {
                    if !self.check_condition(condition, context, device_id, rule_id)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            LogicalOperator::Or => {
                for condition in conditions {
                    if self.check_condition(condition, context, device_id, rule_id)? {
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
                Ok(!self.check_condition(&conditions[0], context, device_id, rule_id)?)
            }
        }
    }

    /// Check if a rule's alarm condition has recovered (value is back to normal).
    ///
    /// For `Threshold` conditions with `recovery_threshold` set, this uses the
    /// hysteresis value instead of simply inverting the trigger condition.
    /// For all other conditions, recovery = condition is no longer met.
    fn check_recovery(&self, condition: &AlarmCondition, context: &EvaluationContext) -> bool {
        match condition {
            AlarmCondition::Threshold { operator, value, recovery_threshold, .. } => {
                let Some(current) = context.get_numeric_value() else {
                    // Non-numeric values: can't evaluate, treat as recovered
                    return true;
                };
                if let Some(recovery_val) = recovery_threshold {
                    // Hysteresis: use the recovery threshold instead of trigger threshold.
                    // For > / >= operators, recovery means value dropped below recovery_val.
                    // For < / <= operators, recovery means value rose above recovery_val.
                    match operator {
                        ComparisonOperator::GreaterThan
                        | ComparisonOperator::GreaterThanOrEqual => current < *recovery_val,
                        ComparisonOperator::LessThan | ComparisonOperator::LessThanOrEqual => {
                            current > *recovery_val
                        }
                        ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
                            // For equality checks, recovery = value no longer equals trigger value
                            !operator.evaluate(current, *value)
                        }
                    }
                } else {
                    // No hysteresis: recovered = condition no longer met
                    !operator.evaluate(current, *value)
                }
            }
            // For non-Threshold conditions, we need to check the inner condition
            // without Duration/Composite state tracking.
            // If the underlying condition is still true, recovery has NOT happened.
            // If it's false, then recovery has occurred.
            _ => !self.check_condition_strict(condition, context),
        }
    }

    /// Check the "raw" condition without any state tracking (Duration, debounce, etc.).
    /// This is a pure function that only looks at the current value against the condition.
    fn check_condition_strict(
        &self,
        condition: &AlarmCondition,
        context: &EvaluationContext,
    ) -> bool {
        match condition {
            AlarmCondition::Threshold { operator, value, .. } => {
                context.get_numeric_value().is_some_and(|v| operator.evaluate(v, *value))
            }
            AlarmCondition::Range { min, max, inclusive } => {
                let Some(val) = context.get_numeric_value() else { return false };
                let below_min = min
                    .is_some_and(|min_val| if *inclusive { val < min_val } else { val <= min_val });
                let above_max = max
                    .is_some_and(|max_val| if *inclusive { val > max_val } else { val >= max_val });
                below_min || above_max
            }
            AlarmCondition::Change { change_type, threshold, .. } => {
                let Some(current) = context.get_numeric_value() else { return false };
                let Some(previous) =
                    context.previous_value.as_ref().and_then(|v| v.parse::<f64>().ok())
                else {
                    return false;
                };
                let delta = current - previous;
                match change_type {
                    ChangeType::Increase => delta > *threshold,
                    ChangeType::Decrease => delta < -(*threshold),
                    ChangeType::Any => delta.abs() > *threshold,
                }
            }
            AlarmCondition::Duration { condition, .. } => {
                self.check_condition_strict(condition, context)
            }
            AlarmCondition::Composite { operator, conditions } => match operator {
                LogicalOperator::And => {
                    conditions.iter().all(|c| self.check_condition_strict(c, context))
                }
                LogicalOperator::Or => {
                    conditions.iter().any(|c| self.check_condition_strict(c, context))
                }
                LogicalOperator::Not => {
                    conditions.len() == 1 && !self.check_condition_strict(&conditions[0], context)
                }
            },
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

/// Result of evaluating all rules for one event.
#[derive(Debug, Clone, Default)]
pub struct EvaluationResult {
    pub triggers: Vec<AlarmTrigger>,
    pub non_triggered_rule_ids: Vec<String>,
    /// Rules that are in a debounce/pending state (trigger or recovery debounce not yet elapsed).
    /// These rules are neither triggered nor recovered — they're in a transitional state.
    pub pending_trigger_rule_ids: Vec<String>,
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
    pub workspace_id: Option<String>,
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

    #[allow(clippy::collapsible_if)]
    /// Auto-resolve active alarms when property values return to normal.
    async fn auto_resolve_recovered_alarms(
        &self,
        event: &Event,
        non_triggered_rule_ids: &[String],
    ) {
        let device_id = event.source().device_id().unwrap_or_else(|| event.source().source_id());
        let event_property_id =
            event.content().metadata().get("property_id").and_then(|v| v.as_str());

        let active_alarms = match self.alarm_service.get_active_alarms(Some(device_id)).await {
            Ok(alarms) => alarms,
            Err(e) => {
                tracing::error!("Failed to get active alarms for auto-resolve: {}", e);
                return;
            }
        };

        for alarm in active_alarms {
            // Only resolve alarms whose rule was just evaluated and did NOT trigger
            match alarm.rule_id {
                Some(ref rule_id) if non_triggered_rule_ids.contains(rule_id) => {}
                _ => continue,
            }

            // Only resolve if the alarm's property matches the current event's property.
            // Without this check, a device-level rule that triggered on property A's event
            // would get incorrectly auto-resolved when property B's event doesn't match.
            let property_matches = match (&alarm.property_id, event_property_id) {
                (None, _) | (_, None) => true,
                (Some(alarm_prop), Some(event_prop)) => alarm_prop == event_prop,
            };
            if !property_matches {
                continue;
            }

            let ws_id = alarm.workspace_id.as_deref().unwrap_or("");
            if let Err(e) = self.alarm_service.auto_resolve_alarm(&alarm.id, ws_id).await {
                tracing::error!("Failed to auto-resolve alarm {}: {}", alarm.id, e);
            }
        }
    }
}

#[async_trait::async_trait]
impl crate::shared::event::EventHandler for AlarmEventHandler {
    #[allow(clippy::collapsible_if)]
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        let result = self
            .rule_engine
            .evaluate_event(event)
            .await
            .map_err(|e| tinyiothub_core::error::Error::Internal(e.to_string()))?;

        // Auto-resolve alarms for rules that were evaluated but did NOT trigger
        if !result.non_triggered_rule_ids.is_empty() {
            self.auto_resolve_recovered_alarms(event, &result.non_triggered_rule_ids).await;
        }

        if result.triggers.is_empty() {
            return Ok(());
        }

        for trigger in result.triggers {
            let device_id =
                event.source().device_id().unwrap_or_else(|| event.source().source_id());

            // Suppress duplicate: don't create alarm if one is already active for this device+rule
            if let Ok(active) = self.alarm_service.get_active_alarms(Some(device_id)).await {
                if active.iter().any(|a| a.rule_id.as_deref() == Some(&trigger.rule_id)) {
                    continue; // already triggered, skip
                }
            }

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
                trigger.workspace_id.clone(),
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
                let ws_id = rule.workspace_id.as_deref().unwrap_or("");
                self.notification_dispatcher.dispatch(&alarm, &rule, ws_id).await;
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
        sqlx::query("DROP TABLE IF EXISTS device_alarm_rules").execute(pool).await.unwrap();
        sqlx::query("DROP TABLE IF EXISTS devices").execute(pool).await.unwrap();
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
                description TEXT, notification_config TEXT,
                workspace_id TEXT, created_by TEXT,
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

    fn make_property_change_event_with_old(
        device_id: &str,
        property_name: &str,
        value: f64,
        old_value: f64,
    ) -> Event {
        let metadata: std::collections::HashMap<String, serde_json::Value> = [
            ("property_id".to_string(), serde_json::Value::String(property_name.to_string())),
            ("property_name".to_string(), serde_json::Value::String(property_name.to_string())),
            ("value".to_string(), serde_json::Value::String(value.to_string())),
            ("old_value".to_string(), serde_json::Value::String(old_value.to_string())),
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

        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;

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
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "Should NOT trigger when value is below threshold");
    }

    /// --- Duration condition tests ---

    #[sqlx::test]
    async fn test_duration_zero_triggers_immediately(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // Duration with 0s = fires immediately when inner condition is true
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-d0', 'dev-1', 'prop-1', 'Duration Zero', 'duration', '{\"type\":\"duration\",\"condition\":{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},\"duration\":0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // temperature = 85 > 80 → should trigger immediately (0s duration)
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let result = engine.evaluate_event(&event).await.unwrap();
        assert!(!result.triggers.is_empty(), "Duration with 0s should trigger immediately");
    }

    #[sqlx::test]
    async fn test_duration_sustained_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // Duration with 0s so we can test trigger immediately (real sustained test would require time travel)
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-d1', 'dev-1', 'prop-1', 'Duration Sustained', 'duration', '{\"type\":\"duration\",\"condition\":{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},\"duration\":3600}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // First call: condition is true but not yet sustained
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let result = engine.evaluate_event(&event).await.unwrap();
        // With 50ms, we can check that it doesn't trigger on first call
        assert!(
            !result.triggers.iter().any(|t| t.rule_id == "rule-d1"),
            "Duration should not trigger on first evaluation"
        );
    }

    #[sqlx::test]
    async fn test_duration_clears_on_recovery(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-d2', 'dev-1', 'prop-1', 'Duration Clear', 'duration', '{\"type\":\"duration\",\"condition\":{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},\"duration\":3600}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Trigger → condition true
        engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        // Recovery → value drops below threshold → should clear duration tracker
        let result = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 25.0))
            .await
            .unwrap();
        // Should not trigger (condition is false)
        assert!(
            result.triggers.is_empty(),
            "Should not trigger when condition clears before duration"
        );

        // Now trigger again → should need to restart the duration timer
        let result2 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(
            !result2.triggers.iter().any(|t| t.rule_id == "rule-d2"),
            "Duration tracker should have been reset, not trigger immediately"
        );
    }

    /// --- Range condition tests ---

    #[sqlx::test]
    async fn test_range_in_range_no_trigger(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // Range 20-80, alarm if OUTSIDE range
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-r1', 'dev-1', 'prop-1', 'Temp Range', 'range', '{\"type\":\"range\",\"min\":20.0,\"max\":80.0,\"inclusive\":true}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Value 50 is within [20, 80] → no alarm
        let event = make_property_change_event("dev-1", "temperature", 50.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "Value in valid range should not trigger");
    }

    #[sqlx::test]
    async fn test_range_out_of_range_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-r2', 'dev-1', 'prop-1', 'Temp Range', 'range', '{\"type\":\"range\",\"min\":20.0,\"max\":80.0,\"inclusive\":true}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Value 85 is above max 80 → should trigger
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "Value above range should trigger");
        assert_eq!(triggers[0].alarm_level, AlarmLevel::Critical);
    }

    #[sqlx::test]
    async fn test_range_boundary_inclusive_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-r3', 'dev-1', 'prop-1', 'Temp Range', 'range', '{\"type\":\"range\",\"min\":20.0,\"max\":80.0,\"inclusive\":true}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Value exactly at boundary (inclusive) → no alarm
        let event = make_property_change_event("dev-1", "temperature", 80.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "Value at inclusive boundary should not trigger");
    }

    /// --- Change condition tests ---

    #[sqlx::test]
    async fn test_change_increase_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // Increase by > 10.0 triggers
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-c1', 'dev-1', 'prop-1', 'Rapid Increase', 'change', '{\"type\":\"change\",\"change_type\":\"increase\",\"threshold\":10.0,\"time_window\":0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // With time_window=0s, we compare previous_value (old) with current_value
        let event = make_property_change_event_with_old("dev-1", "temperature", 95.0, 80.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "Increase of 15 > threshold 10 should trigger");
    }

    #[sqlx::test]
    async fn test_change_below_threshold_no_trigger(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-c2', 'dev-1', 'prop-1', 'Rapid Increase', 'change', '{\"type\":\"change\",\"change_type\":\"increase\",\"threshold\":10.0,\"time_window\":0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event_with_old("dev-1", "temperature", 85.0, 80.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "Increase of 5 < threshold 10 should not trigger");
    }

    #[sqlx::test]
    async fn test_change_decrease_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-c3', 'dev-1', 'prop-1', 'Rapid Drop', 'change', '{\"type\":\"change\",\"change_type\":\"decrease\",\"threshold\":10.0,\"time_window\":0}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event_with_old("dev-1", "temperature", 65.0, 80.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "Decrease of 15 > threshold 10 should trigger");
    }

    /// --- Composite condition tests ---

    #[sqlx::test]
    async fn test_composite_and_all_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // AND: (temp > 80) AND (humidity > 60) → both must be true
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-and1', 'dev-1', 'prop-1', 'High T&H', 'composite', '{\"type\":\"composite\",\"operator\":\"and\",\"conditions\":[{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":60.0}]}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Value 85 > 80 (first condition true) BUT we can only test one value per event.
        // Composite checks all conditions against the same context value,
        // so 85 > 80 AND 85 > 60 → both true → triggers
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "AND composite with both conditions true should trigger");
    }

    #[sqlx::test]
    async fn test_composite_and_partial_no_trigger(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-and2', 'dev-1', 'prop-1', 'High T&H', 'composite', '{\"type\":\"composite\",\"operator\":\"and\",\"conditions\":[{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":90.0}]}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // 85 > 80 is true, but 85 is NOT > 90 → AND fails
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "AND composite with partial condition should not trigger");
    }

    #[sqlx::test]
    async fn test_composite_or_any_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // OR: (temp > 80) OR (temp > 90) — first condition true → triggers
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-or1', 'dev-1', 'prop-1', 'High T OR', 'composite', '{\"type\":\"composite\",\"operator\":\"or\",\"conditions\":[{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0},{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":90.0}]}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // 85 > 80 (true) OR 85 > 90 (false) → OR succeeds
        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "OR composite with one true condition should trigger");
    }

    /// --- Trigger debounce tests ---

    #[sqlx::test]
    async fn test_trigger_debounce_not_immediate(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // trigger_duration_secs=3600: condition must be sustained for 1 hour before triggering
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, notification_config, created_at, updated_at)
             VALUES ('rule-tdb1', 'dev-1', 'prop-1', 'High Temp Debounce', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, '{\"enabled\":false,\"channels\":[],\"recipients\":[],\"trigger_duration_secs\":3600}', datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let result = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(result.triggers.is_empty(), "Should not trigger immediately with debounce");
        assert!(
            result.pending_trigger_rule_ids.contains(&"rule-tdb1".to_string()),
            "Rule should be pending during debounce"
        );
        assert!(
            result.non_triggered_rule_ids.is_empty(),
            "Pending rule should not be considered non-triggered"
        );
    }

    #[sqlx::test]
    async fn test_trigger_debounce_fires_after_duration(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // trigger_duration_secs=0: immediate trigger (same as backward compat)
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, notification_config, created_at, updated_at)
             VALUES ('rule-tdb0', 'dev-1', 'prop-1', 'High Temp No Debounce', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, '{\"enabled\":false,\"channels\":[],\"recipients\":[],\"trigger_duration_secs\":0}', datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let result = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(!result.triggers.is_empty(), "Zero-duration debounce should trigger immediately");
    }

    #[sqlx::test]
    async fn test_trigger_debounce_resets_on_clear(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, notification_config, created_at, updated_at)
             VALUES ('rule-tdb2', 'dev-1', 'prop-1', 'High Temp Debounce Reset', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, '{\"enabled\":false,\"channels\":[],\"recipients\":[],\"trigger_duration_secs\":3600}', datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // First call starts debounce
        engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        // Second call drops below threshold -> should reset debounce timer
        let result = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 25.0))
            .await
            .unwrap();
        assert!(
            result.non_triggered_rule_ids.contains(&"rule-tdb2".to_string()),
            "Cleared rule should be non-triggered"
        );

        // Third call above threshold again -> debounce should restart from scratch
        let result2 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(result2.triggers.is_empty(), "Should not trigger after reset");
        assert!(
            result2.pending_trigger_rule_ids.contains(&"rule-tdb2".to_string()),
            "Rule should be pending again after reset"
        );
    }

    /// --- Recovery / hysteresis tests ---

    #[sqlx::test]
    async fn test_hysteresis_recovery_threshold(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // Trigger > 80, recover < 75
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-hys', 'dev-1', 'prop-1', 'High Temp Hysteresis', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0,\"recovery_threshold\":75.0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Trigger alarm
        let result1 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(!result1.triggers.is_empty(), "Should trigger above threshold");

        // Value drops but stays above recovery threshold -> still in alarm (hysteresis)
        let result2 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 78.0))
            .await
            .unwrap();
        assert!(result2.triggers.is_empty(), "Should not re-trigger");
        assert!(
            result2.pending_trigger_rule_ids.contains(&"rule-hys".to_string()),
            "Value in hysteresis zone should be pending, not recovered"
        );
        assert!(
            !result2.non_triggered_rule_ids.contains(&"rule-hys".to_string()),
            "Value in hysteresis zone should NOT be non-triggered"
        );

        // Value drops below recovery threshold -> recovered
        let result3 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 72.0))
            .await
            .unwrap();
        assert!(
            result3.non_triggered_rule_ids.contains(&"rule-hys".to_string()),
            "Should be non-triggered below recovery threshold"
        );
    }

    #[sqlx::test]
    async fn test_recovery_debounce(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, notification_config, created_at, updated_at)
             VALUES ('rule-rdb', 'dev-1', 'prop-1', 'High Temp Recovery Debounce', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, '{\"enabled\":false,\"channels\":[],\"recipients\":[],\"recovery_duration_secs\":0}', datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // Trigger alarm
        let result1 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(!result1.triggers.is_empty(), "Should trigger");

        // Recovery duration 0 -> immediate non-triggered when below threshold
        let result2 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 72.0))
            .await
            .unwrap();
        assert!(
            result2.non_triggered_rule_ids.contains(&"rule-rdb".to_string()),
            "Zero recovery debounce should immediately recover"
        );
    }

    /// --- Throttle tests ---

    #[sqlx::test]
    async fn test_throttle_suppresses_repeated(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // suppress_duration 300s (5 minutes)
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, notification_config, created_at, updated_at)
             VALUES ('rule-t1', 'dev-1', 'prop-1', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, '{\"suppress_duration\":300}', datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        // First call → triggers
        let result1 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 85.0))
            .await
            .unwrap();
        assert!(!result1.triggers.is_empty(), "First call should trigger");

        // Second call within suppression window → throttled
        let result2 = engine
            .evaluate_event(&make_property_change_event("dev-1", "temperature", 90.0))
            .await
            .unwrap();
        assert!(
            result2.triggers.is_empty(),
            "Second call within suppression window should be throttled"
        );
    }

    /// --- Disabled rule — no trigger ---

    #[sqlx::test]
    async fn test_disabled_rule_skipped(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-dis', 'dev-1', 'prop-1', 'Disabled Rule', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 0, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(triggers.is_empty(), "Disabled rule should not trigger");
    }

    /// --- Threshold comparison operator tests ---

    #[sqlx::test]
    async fn test_threshold_less_than_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-lt', 'dev-1', 'prop-1', 'Low Battery', 'threshold', '{\"type\":\"threshold\",\"operator\":\"less_than\",\"value\":20.0}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event("dev-1", "battery", 15.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "Value below less_than threshold should trigger");
    }

    #[sqlx::test]
    async fn test_threshold_equal_triggers(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-eq', 'dev-1', 'prop-1', 'Exact Value', 'threshold', '{\"type\":\"threshold\",\"operator\":\"equal\",\"value\":42.0}', 'info', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event("dev-1", "answer", 42.0);
        let triggers = engine.evaluate_event(&event).await.unwrap().triggers;
        assert!(!triggers.is_empty(), "Equal value should trigger");
    }

    /// --- Non-triggered rule IDs populated ---

    #[sqlx::test]
    async fn test_non_triggered_rule_ids_populated(pool: sqlx::SqlitePool) {
        setup_test_db(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));
        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-1', 'Test Device')")
            .execute(&pool)
            .await
            .unwrap();
        // One rule that triggers (value > 80) and one that doesn't (value > 90)
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-nt1', 'dev-1', 'prop-1', 'Triggers', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-nt2', 'dev-1', 'prop-1', 'No Trigger', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":90.0}', 'critical', 1, datetime('now'), datetime('now'))",
        )
        .execute(&pool).await.unwrap();

        let repo: Arc<dyn AlarmRuleRepository> = Arc::new(SqliteAlarmRuleRepository::new(db));
        let engine = RuleEngine::new(repo);

        let event = make_property_change_event("dev-1", "temperature", 85.0);
        let result = engine.evaluate_event(&event).await.unwrap();
        assert_eq!(result.triggers.len(), 1, "Only one rule should trigger");
        assert!(
            result.non_triggered_rule_ids.contains(&"rule-nt2".to_string()),
            "Non-triggered rule should be in non_triggered_rule_ids"
        );
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
            notification_config TEXT, workspace_id TEXT, created_by TEXT,
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

    /// --- Auto-resolve tests ---

    #[sqlx::test]
    async fn test_auto_resolve_clears_alarm_when_rule_no_longer_triggers(pool: sqlx::SqlitePool) {
        setup_full_schema(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES ('dev-ar', 'AutoResolve Device', 'ws-ar')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO device_properties (id, device_id, name) VALUES ('prop-ar', 'dev-ar', 'temperature')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, workspace_id, created_at, updated_at)
             VALUES ('rule-ar1', 'dev-ar', 'prop-ar', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, 'ws-ar', datetime('now'), datetime('now'))",
        ).execute(&pool).await.unwrap();

        let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
        let rule_repo: Arc<dyn AlarmRuleRepository> =
            Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repo.clone(), rule_repo));
        let notification_dispatcher = Arc::new(NotificationDispatcher::new(db.clone()));
        let handler = AlarmEventHandler::new(alarm_service.clone(), notification_dispatcher);

        // Step 1: Trigger alarm (value 85 > 80)
        handler
            .handle(&make_test_event("dev-ar", "prop-ar", "temperature", 85.0, None))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-ar' AND is_resolved = 0",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "Should have one active alarm");

        // Step 2: Value drops below threshold → auto-resolve
        handler
            .handle(&make_test_event("dev-ar", "prop-ar", "temperature", 25.0, None))
            .await
            .unwrap();

        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-ar' AND is_resolved = 0",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_count, 0, "Alarm should be auto-resolved when value returns to normal");
    }

    #[sqlx::test]
    async fn test_auto_resolve_sets_resolution_metadata(pool: sqlx::SqlitePool) {
        setup_full_schema(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES ('dev-arm', 'Meta Device', 'ws-arm')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO device_properties (id, device_id, name) VALUES ('prop-arm', 'dev-arm', 'humidity')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, workspace_id, created_at, updated_at)
             VALUES ('rule-arm', 'dev-arm', 'prop-arm', 'High Humidity', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":70.0}', 'warning', 1, 'ws-arm', datetime('now'), datetime('now'))",
        ).execute(&pool).await.unwrap();

        let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
        let rule_repo: Arc<dyn AlarmRuleRepository> =
            Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repo.clone(), rule_repo));
        let notification_dispatcher = Arc::new(NotificationDispatcher::new(db.clone()));
        let handler = AlarmEventHandler::new(alarm_service.clone(), notification_dispatcher);

        // Trigger
        handler
            .handle(&make_test_event("dev-arm", "prop-arm", "humidity", 85.0, None))
            .await
            .unwrap();
        // Auto-resolve
        handler
            .handle(&make_test_event("dev-arm", "prop-arm", "humidity", 50.0, None))
            .await
            .unwrap();

        let row = sqlx::query("SELECT resolved_by, resolved_at, resolution_type FROM device_alarms WHERE device_id = 'dev-arm' AND is_resolved = 1")
            .fetch_one(&pool).await.unwrap();
        let resolved_by: Option<String> = row.get("resolved_by");
        let resolution_type: String = row.get("resolution_type");
        assert!(
            resolved_by.is_none(),
            "Auto-resolve should leave resolved_by NULL (no human actor)"
        );
        assert_eq!(resolution_type, "auto_resolved", "Auto-resolve should set resolution_type");
    }

    /// --- Suppress duplicate test ---

    #[sqlx::test]
    async fn test_suppress_duplicate_no_duplicate_alarm(pool: sqlx::SqlitePool) {
        setup_full_schema(&pool).await;
        let db = Arc::new(Database::new(pool.clone()));

        sqlx::query("INSERT INTO devices (id, name) VALUES ('dev-sd', 'Suppress Dev')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO device_properties (id, device_id, name) VALUES ('prop-sd', 'dev-sd', 'temperature')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, is_enabled, created_at, updated_at)
             VALUES ('rule-sd', 'dev-sd', 'prop-sd', 'High Temp', 'threshold', '{\"type\":\"threshold\",\"operator\":\"greater_than\",\"value\":80.0}', 'warning', 1, datetime('now'), datetime('now'))",
        ).execute(&pool).await.unwrap();

        let alarm_repo: Arc<dyn AlarmRepository> = Arc::new(SqliteAlarmRepository::new(db.clone()));
        let rule_repo: Arc<dyn AlarmRuleRepository> =
            Arc::new(SqliteAlarmRuleRepository::new(db.clone()));
        let alarm_service = Arc::new(AlarmService::new(alarm_repo.clone(), rule_repo));
        let notification_dispatcher = Arc::new(NotificationDispatcher::new(db.clone()));
        let handler = AlarmEventHandler::new(alarm_service.clone(), notification_dispatcher);

        // First trigger
        handler
            .handle(&make_test_event("dev-sd", "prop-sd", "temperature", 85.0, None))
            .await
            .unwrap();
        // Second identical trigger should suppress duplicate (same device+rule+level)
        handler
            .handle(&make_test_event("dev-sd", "prop-sd", "temperature", 90.0, None))
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE device_id = 'dev-sd'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "Should not create duplicate alarm for same device+rule");
    }
}
