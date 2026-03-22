//! 条件评估器
//! 
//! 评估触发条件是否满足

use std::time::Instant;

use super::condition::{Condition, Operator, TriggerContext};

/// 条件评估器
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    pub fn new() -> Self {
        Self
    }
    
    /// 评估条件
    /// 
    /// # Arguments
    /// * `condition` - 条件定义
    /// * `context` - 触发上下文
    /// 
    /// # Returns
    /// * `bool` - 条件是否满足
    pub fn evaluate(&self, condition: &Condition, context: &TriggerContext) -> bool {
        match condition {
            Condition::Threshold { property, operator, value } => {
                self.evaluate_threshold(property, operator, *value, context)
            }
            
            Condition::Comparison { property, operator, value } => {
                self.evaluate_comparison(property, operator, value, context)
            }
            
            Condition::DeviceState { device_id, state } => {
                self.evaluate_device_state(device_id, state, context)
            }
            
            Condition::DeviceOnline { device_id, online } => {
                self.evaluate_device_online(device_id, *online, context)
            }
            
            Condition::AlarmCondition { level, device_id } => {
                self.evaluate_alarm_condition(level, device_id.as_deref(), context)
            }
            
            Condition::And { left, right } => {
                self.evaluate(left, context) && self.evaluate(right, context)
            }
            
            Condition::Or { left, right } => {
                self.evaluate(left, context) || self.evaluate(right, context)
            }
            
            Condition::Not { condition } => {
                !self.evaluate(condition, context)
            }
        }
    }
    
    /// 评估阈值条件
    fn evaluate_threshold(
        &self,
        property: &str,
        operator: &Operator,
        value: f64,
        context: &TriggerContext,
    ) -> bool {
        let current = context.get_property(property);
        
        // 尝试转换为数值
        let current_value = match current {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            _ => return false,
        };
        
        operator.compare(&serde_json::Value::Number(serde_json::Number::from_f64(current_value).unwrap_or(serde_json::Number::from(0))), &serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap_or(serde_json::Number::from(0))))
    }
    
    /// 评估比较条件
    fn evaluate_comparison(
        &self,
        property: &str,
        operator: &Operator,
        value: &serde_json::Value,
        context: &TriggerContext,
    ) -> bool {
        let current = context.get_property(property);
        operator.compare(current, value)
    }
    
    /// 评估设备状态条件
    fn evaluate_device_state(
        &self,
        device_id: &str,
        state: &super::condition::DeviceState,
        context: &TriggerContext,
    ) -> bool {
        // 检查设备 ID 是否匹配
        if let Some(ctx_device_id) = &context.device_id {
            if ctx_device_id != device_id {
                return false;
            }
        }
        
        // 检查状态
        if let Some(ctx_state) = &context.device_state {
            match state {
                super::condition::DeviceState::Online => {
                    context.device_online.unwrap_or(false)
                }
                super::condition::DeviceState::Offline => {
                    !context.device_online.unwrap_or(true)
                }
                _ => ctx_state == state,
            }
        } else {
            false
        }
    }
    
    /// 评估设备在线条件
    fn evaluate_device_online(
        &self,
        device_id: &str,
        online: bool,
        context: &TriggerContext,
    ) -> bool {
        // 检查设备 ID
        if let Some(ctx_device_id) = &context.device_id {
            if ctx_device_id != device_id {
                return false;
            }
        }
        
        // 检查在线状态
        context.device_online == Some(online)
    }
    
    /// 评估告警条件
    fn evaluate_alarm_condition(
        &self,
        level: &super::condition::AlarmLevel,
        _device_id: Option<&str>,
        context: &TriggerContext,
    ) -> bool {
        // 这里需要根据告警上下文来判断
        // 简化实现：检查事件数据中是否有告警信息
        if let Some(data) = &context.event_data {
            if let Some(alarm_level) = data.get("level").and_then(|v| v.as_str()) {
                let ctx_level = super::condition::AlarmLevel::from_str(alarm_level);
                
                // 检查级别
                match (level, ctx_level) {
                    (super::condition::AlarmLevel::Info, _) => true,
                    (super::condition::AlarmLevel::Warning, super::condition::AlarmLevel::Warning) => true,
                    (super::condition::AlarmLevel::Warning, super::condition::AlarmLevel::Error) => true,
                    (super::condition::AlarmLevel::Warning, super::condition::AlarmLevel::Critical) => true,
                    (super::condition::AlarmLevel::Error, super::condition::AlarmLevel::Error) => true,
                    (super::condition::AlarmLevel::Error, super::condition::AlarmLevel::Critical) => true,
                    (super::condition::AlarmLevel::Critical, super::condition::AlarmLevel::Critical) => true,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    }
    
    /// 评估并返回详细信息（用于调试）
    pub fn evaluate_with_details(
        &self,
        condition: &Condition,
        context: &TriggerContext,
    ) -> (bool, serde_json::Value) {
        let result = self.evaluate(condition, context);
        
        let details = serde_json::json!({
            "condition": condition,
            "context": {
                "device_id": context.device_id,
                "properties": context.properties,
            },
            "result": result,
        });
        
        (result, details)
    }
}

impl Default for ConditionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[test]
    fn test_threshold_greater() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), serde_json::json!(30.0));
        
        let condition = Condition::Threshold {
            property: "temperature".to_string(),
            operator: Operator::Gt,
            value: 25.0,
        };
        
        assert!(evaluator.evaluate(&condition, &context));
    }
    
    #[test]
    fn test_threshold_less() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("humidity".to_string(), serde_json::json!(50.0));
        
        let condition = Condition::Threshold {
            property: "humidity".to_string(),
            operator: Operator::Lt,
            value: 60.0,
        };
        
        assert!(evaluator.evaluate(&condition, &context));
    }
    
    #[test]
    fn test_and_condition() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), serde_json::json!(30.0));
        context.properties.insert("humidity".to_string(), serde_json::json!(50.0));
        
        let condition = Condition::And {
            left: Box::new(Condition::Threshold {
                property: "temperature".to_string(),
                operator: Operator::Gt,
                value: 25.0,
            }),
            right: Box::new(Condition::Threshold {
                property: "humidity".to_string(),
                operator: Operator::Lt,
                value: 60.0,
            }),
        };
        
        assert!(evaluator.evaluate(&condition, &context));
    }
}
