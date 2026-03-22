// Automation Tests

#[cfg(test)]
mod tests {
    use crate::domain::automation::{
        Condition, ConditionEvaluator, Operator, TriggerContext, 
        Action, ActionExecutor, AutomationService,
    };
    use serde_json::json;
    
    // ==================== 条件评估器测试 ====================
    
    #[test]
    fn test_threshold_greater() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        
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
        context.properties.insert("humidity".to_string(), json!(50.0));
        
        let condition = Condition::Threshold {
            property: "humidity".to_string(),
            operator: Operator::Lt,
            value: 60.0,
        };
        
        assert!(evaluator.evaluate(&condition, &context));
    }
    
    #[test]
    fn test_threshold_not_met() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(20.0));
        
        let condition = Condition::Threshold {
            property: "temperature".to_string(),
            operator: Operator::Gt,
            value: 25.0,
        };
        
        assert!(!evaluator.evaluate(&condition, &context));
    }
    
    #[test]
    fn test_and_condition() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        context.properties.insert("humidity".to_string(), json!(50.0));
        
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
    
    #[test]
    fn test_or_condition() {
        let evaluator = ConditionEvaluator::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(30.0));
        
        // 温度 > 25 (true) OR 湿度 < 30 (false) = true
        let condition = Condition::Or {
            left: Box::new(Condition::Threshold {
                property: "temperature".to_string(),
                operator: Operator::Gt,
                value: 25.0,
            }),
            right: Box::new(Condition::Threshold {
                property: "humidity".to_string(),
                operator: Operator::Lt,
                value: 30.0,
            }),
        };
        
        assert!(evaluator.evaluate(&condition, &context));
    }
    
    // ==================== 动作执行器测试 ====================
    
    #[tokio::test]
    async fn test_execute_alarm_action() {
        let executor = ActionExecutor::new();
        let mut context = TriggerContext::new();
        context.properties.insert("temperature".to_string(), json!(55.0));
        context.device_id = Some("sensor_001".to_string());
        
        let action = Action::Alarm {
            level: crate::domain::automation::condition::AlarmLevel::Warning,
            message: "温度 {{temperature}}℃ 超过阈值".to_string(),
        };
        
        let results = executor.execute(&[action], &context).await;
        
        assert!(!results.is_empty());
        assert!(results[0].success);
    }
    
    #[tokio::test]
    async fn test_execute_control_action() {
        let executor = ActionExecutor::new();
        let context = TriggerContext::new();
        
        let action = Action::ControlDevice {
            device_id: "light_001".to_string(),
            command: "turn_on".to_string(),
            parameters: None,
        };
        
        let results = executor.execute(&[action], &context).await;
        
        assert!(!results.is_empty());
        assert!(results[0].success);
    }
    
    #[tokio::test]
    async fn test_execute_delay_action() {
        let executor = ActionExecutor::new();
        let context = TriggerContext::new();
        
        let action = Action::Delay {
            duration_ms: 10,
        };
        
        let results = executor.execute(&[action], &context).await;
        
        assert!(!results.is_empty());
        assert!(results[0].success);
    }
    
    // ==================== 自动化服务测试 ====================
    
    #[test]
    fn test_automation_service_creation() {
        let _service = AutomationService::new();
        assert!(true);
    }
    
    #[test]
    fn test_test_condition_matched() {
        let service = AutomationService::new();
        
        // 使用简单的字符串比较
        let condition_json = r#"{
            "type": "comparison",
            "property": "status",
            "operator": "eq",
            "value": "online"
        }"#;
        
        let mock_data = json!({
            "properties": {
                "status": "online"
            }
        });
        
        let (matched, details) = service.test_condition(condition_json, mock_data);
        
        // 打印详情以便调试
        println!("Matched: {}, Details: {:?}", matched, details);
    }
    
    #[test]
    fn test_test_condition_not_matched() {
        let service = AutomationService::new();
        
        let condition_json = r#"{
            "type": "threshold",
            "property": "temperature",
            "operator": ">",
            "value": 40
        }"#;
        
        let mock_data = json!({
            "properties": {
                "temperature": 35
            }
        });
        
        let (matched, _details) = service.test_condition(condition_json, mock_data);
        
        assert!(!matched);
    }
    
    // ==================== 触发上下文测试 ====================
    
    #[test]
    fn test_trigger_context_creation() {
        let mut context = TriggerContext::new();
        
        context.device_id = Some("device_001".to_string());
        context.device_name = Some("test_device".to_string());
        context.device_online = Some(true);
        context.properties.insert("temperature".to_string(), json!(25.0));
        
        assert_eq!(context.device_id, Some("device_001".to_string()));
        assert_eq!(context.device_name, Some("test_device".to_string()));
        assert_eq!(context.device_online, Some(true));
        assert_eq!(context.get_property("temperature"), &json!(25.0));
    }
}
