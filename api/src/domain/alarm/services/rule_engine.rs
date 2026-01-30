use std::collections::HashMap;
use std::sync::Arc;

use super::super::entity::AlarmRule;
use super::super::errors::{AlarmError, AlarmResult};
use super::super::repository::AlarmRuleRepository;
use super::super::value_objects::*;
use crate::domain::event::entities::Event;
use crate::domain::event::value_objects::EventType;

/// 规则引擎
pub struct RuleEngine {
    rule_repository: Arc<dyn AlarmRuleRepository>,
}

impl RuleEngine {
    pub fn new(rule_repository: Arc<dyn AlarmRuleRepository>) -> Self {
        Self { rule_repository }
    }

    /// 评估事件是否触发报警
    pub async fn evaluate_event(&self, event: &Event) -> AlarmResult<Vec<AlarmTrigger>> {
        // 只处理设备相关事件
        if !matches!(event.event_type(), EventType::Device(_)) {
            return Ok(vec![]);
        }

        let device_id = event.source().source_id();
        let property_id = event
            .content()
            .metadata()
            .get("property_id")
            .and_then(|v| v.as_str());

        // 加载相关规则
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

    /// 评估单个规则
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

    /// 检查条件是否满足
    fn check_condition(
        &self,
        condition: &AlarmCondition,
        context: &EvaluationContext,
    ) -> AlarmResult<bool> {
        match condition {
            AlarmCondition::Threshold { operator, value } => {
                self.check_threshold(operator, *value, context)
            }
            AlarmCondition::Range {
                min,
                max,
                inclusive,
            } => self.check_range(*min, *max, *inclusive, context),
            AlarmCondition::Change {
                change_type,
                threshold,
                ..
            } => self.check_change(change_type, *threshold, context),
            AlarmCondition::Duration { condition, .. } => {
                // 持续时间条件需要历史数据，暂时简化处理
                self.check_condition(condition, context)
            }
            AlarmCondition::Composite {
                operator,
                conditions,
            } => self.check_composite(operator, conditions, context),
        }
    }

    /// 检查阈值条件
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

    /// 检查范围条件
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
            if inclusive {
                value < min_val
            } else {
                value <= min_val
            }
        } else {
            false
        };

        let above_max = if let Some(max_val) = max {
            if inclusive {
                value > max_val
            } else {
                value >= max_val
            }
        } else {
            false
        };

        Ok(below_min || above_max)
    }

    /// 检查变化条件
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

    /// 检查组合条件
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

    /// 加载相关规则
    async fn load_relevant_rules(
        &self,
        device_id: &str,
        property_id: Option<&str>,
    ) -> AlarmResult<Vec<AlarmRule>> {
        let mut rules = Vec::new();

        // 加载全局规则
        rules.extend(self.rule_repository.find_global_rules().await?);

        // 加载设备规则
        rules.extend(self.rule_repository.find_by_device(device_id).await?);

        // 如果有属性ID，加载属性规则
        if let Some(prop_id) = property_id {
            rules.extend(
                self.rule_repository
                    .find_by_property(device_id, prop_id)
                    .await?,
            );
        }

        Ok(rules)
    }

    /// 确定报警类型
    fn determine_alarm_type(&self, _event: &Event, _rule: &AlarmRule) -> AlarmType {
        // 可以根据事件类型和规则配置确定报警类型
        // 这里简化处理，使用属性阈值类型
        AlarmType::PropertyThreshold
    }

    /// 生成报警消息
    fn generate_message(
        &self,
        event: &Event,
        rule: &AlarmRule,
        _context: &EvaluationContext,
    ) -> String {
        format!("{}: {}", rule.name, event.content().title())
    }

    /// 提取阈值
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

    /// 获取规则
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
        let current_value = metadata
            .get("value")
            .and_then(|v| v.as_str())
            .map(String::from);
        let previous_value = metadata
            .get("old_value")
            .and_then(|v| v.as_str())
            .map(String::from);

        Self {
            current_value,
            previous_value,
            metadata,
        }
    }

    pub fn get_numeric_value(&self) -> Option<f64> {
        self.current_value.as_ref()?.parse().ok()
    }
}
