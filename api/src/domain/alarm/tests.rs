#![cfg(test)]

mod tests {
    use crate::domain::alarm::entity::{Alarm, AlarmRule, RuleType};
    use crate::domain::alarm::value_objects::{
        AlarmCondition, AlarmLevel, AlarmStatus, AlarmType, ComparisonOperator,
        NotificationConfig,
    };
    use crate::domain::event::aggregates::NotificationChannelType;

    /// 测试创建新报警
    #[test]
    fn test_alarm_creation() {
        let alarm = Alarm::new(
            "device-001".to_string(),
            Some("property-temp".to_string()),
            Some("rule-001".to_string()),
            AlarmType::PropertyThreshold,
            AlarmLevel::Warning,
            "温度超过阈值".to_string(),
            Some("85.5".to_string()),
            Some("80.0".to_string()),
        );

        assert!(!alarm.id.is_empty());
        assert_eq!(alarm.device_id, "device-001");
        assert_eq!(alarm.property_id, Some("property-temp".to_string()));
        assert_eq!(alarm.alarm_type, AlarmType::PropertyThreshold);
        assert_eq!(alarm.alarm_level, AlarmLevel::Warning);
        assert!(alarm.is_active());
    }

    /// 测试报警确认
    #[test]
    fn test_alarm_acknowledge() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        let result = alarm.acknowledge("user-001".to_string(), Some("已确认处理".to_string()));
        assert!(result.is_ok());
        // 注意: Acknowledged 状态仍算作活跃（未解决）
        assert!(alarm.is_active());
        assert!(alarm.acknowledgement.is_some());

        let ack = alarm.acknowledgement.as_ref().unwrap();
        assert_eq!(ack.acknowledged_by, "user-001");
        assert_eq!(ack.note, Some("已确认处理".to_string()));
    }

    /// 测试重复确认失败
    #[test]
    fn test_alarm_double_acknowledge_fails() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        alarm.acknowledge("user-001".to_string(), None).unwrap();
        let result = alarm.acknowledge("user-002".to_string(), None);
        assert!(result.is_err());
    }

    /// 测试报警解决
    #[test]
    fn test_alarm_resolve() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceError,
            AlarmLevel::Critical,
            "设备故障".to_string(),
            None,
            None,
        );

        let result = alarm.resolve(
            "user-001".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            Some("已修复".to_string()),
        );
        assert!(result.is_ok());
        assert!(!alarm.is_active());
        assert!(alarm.resolution.is_some());
    }

    /// 测试抑制报警
    #[test]
    fn test_alarm_suppress() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::PropertyThreshold,
            AlarmLevel::Info,
            "温度告警".to_string(),
            None,
            None,
        );

        let result = alarm.suppress();
        assert!(result.is_ok());
        assert!(!alarm.is_active());
    }

    /// 测试 can_acknowledge 状态检查
    #[test]
    fn test_can_acknowledge() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        assert!(alarm.can_acknowledge());

        alarm.acknowledge("user-001".to_string(), None).unwrap();
        assert!(!alarm.can_acknowledge());
    }

    /// 测试 can_resolve 状态检查
    #[test]
    fn test_can_resolve() {
        let alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        // 活跃状态可以解决
        assert!(alarm.can_resolve());

        let mut acknowledged_alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );
        acknowledged_alarm.acknowledge("user-001".to_string(), None).unwrap();
        assert!(acknowledged_alarm.can_resolve());
    }

    /// 测试报警级别优先级
    #[test]
    fn test_alarm_level_priority() {
        assert!(AlarmLevel::Critical.priority() > AlarmLevel::Error.priority());
        assert!(AlarmLevel::Error.priority() > AlarmLevel::Warning.priority());
        assert!(AlarmLevel::Warning.priority() > AlarmLevel::Info.priority());
    }

    /// 测试报警类型序列化
    #[test]
    fn test_alarm_type_serialization() {
        let alarm_type = AlarmType::PropertyThreshold;
        let serialized = alarm_type.as_str();
        assert_eq!(serialized, "property_threshold");

        let deserialized = AlarmType::from_str("property_threshold");
        assert_eq!(deserialized, AlarmType::PropertyThreshold);
    }

    /// 测试报警类型自定义
    #[test]
    fn test_alarm_type_custom() {
        let custom = AlarmType::Custom {
            name: "high_cpu".to_string(),
        };
        assert_eq!(custom.as_str(), "custom_high_cpu");

        let parsed = AlarmType::from_str("custom_high_cpu");
        assert_eq!(
            parsed,
            AlarmType::Custom {
                name: "high_cpu".to_string()
            }
        );
    }

    /// 测试创建告警规则 - 成功场景
    #[test]
    fn test_alarm_rule_creation_success() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: true,
            channels: vec![NotificationChannelType::Email],
            recipients: vec!["admin@example.com".to_string()],
            suppress_duration: None,
            repeat_interval: None,
        };

        let rule = AlarmRule::new(
            "温度告警规则".to_string(),
            Some("监控温度超过80度".to_string()),
            Some("device-001".to_string()),
            Some("property-temp".to_string()),
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        );

        assert!(rule.is_ok());
        let rule = rule.unwrap();
        assert_eq!(rule.name, "温度告警规则");
        assert!(rule.is_enabled);
    }

    /// 测试创建告警规则 - 空名称失败
    #[test]
    fn test_alarm_rule_empty_name_fails() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            repeat_interval: None,
        };

        let result = AlarmRule::new(
            "".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        );

        assert!(result.is_err());
    }

    /// 测试创建告警规则 - 启用通知但无渠道失败
    #[test]
    fn test_alarm_rule_no_channel_fails() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: true,
            channels: vec![],
            recipients: vec!["admin@example.com".to_string()],
            suppress_duration: None,
            repeat_interval: None,
        };

        let result = AlarmRule::new(
            "温度告警".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        );

        assert!(result.is_err());
    }

    /// 测试创建告警规则 - 邮件通知无接收人失败
    #[test]
    fn test_alarm_rule_email_no_recipient_fails() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: true,
            channels: vec![NotificationChannelType::Email],
            recipients: vec![],
            suppress_duration: None,
            repeat_interval: None,
        };

        let result = AlarmRule::new(
            "温度告警".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        );

        assert!(result.is_err());
    }

    /// 测试规则启用/禁用
    #[test]
    fn test_alarm_rule_enable_disable() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            repeat_interval: None,
        };

        let mut rule = AlarmRule::new(
            "温度告警".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        )
        .unwrap();

        assert!(rule.is_enabled);

        rule.disable();
        assert!(!rule.is_enabled);

        rule.enable();
        assert!(rule.is_enabled);
    }

    /// 测试规则更新
    #[test]
    fn test_alarm_rule_update() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };
        
        let notification_config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            repeat_interval: None,
        };

        let mut rule = AlarmRule::new(
            "温度告警".to_string(),
            Some("原始描述".to_string()),
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        )
        .unwrap();

        let result = rule.update(
            Some("新温度告警".to_string()),
            Some("新描述".to_string()),
            None,
            Some(AlarmLevel::Critical),
            None,
        );

        assert!(result.is_ok());
        assert_eq!(rule.name, "新温度告警");
        assert_eq!(rule.description, Some("新描述".to_string()));
        assert_eq!(rule.alarm_level, AlarmLevel::Critical);
    }

    /// 测试规则类型字符串转换
    #[test]
    fn test_rule_type_string() {
        assert_eq!(RuleType::Threshold.as_str(), "threshold");
        assert_eq!(RuleType::Range.as_str(), "range");
        assert_eq!(RuleType::Change.as_str(), "change");
        assert_eq!(RuleType::Duration.as_str(), "duration");
        assert_eq!(RuleType::Composite.as_str(), "composite");
    }

    /// 测试比较运算符
    #[test]
    fn test_comparison_operators() {
        use crate::domain::alarm::value_objects::ComparisonOperator;

        assert!(ComparisonOperator::GreaterThan.evaluate(10.0, 5.0));
        assert!(!ComparisonOperator::GreaterThan.evaluate(5.0, 10.0));

        assert!(ComparisonOperator::LessThan.evaluate(5.0, 10.0));
        assert!(!ComparisonOperator::LessThan.evaluate(10.0, 5.0));

        assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(10.0, 10.0));
        assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(10.0, 5.0));

        assert!(ComparisonOperator::LessThanOrEqual.evaluate(10.0, 10.0));
        assert!(ComparisonOperator::LessThanOrEqual.evaluate(5.0, 10.0));

        assert!(ComparisonOperator::Equal.evaluate(10.0, 10.0));
        assert!(!ComparisonOperator::Equal.evaluate(10.0, 5.0));

        assert!(ComparisonOperator::NotEqual.evaluate(10.0, 5.0));
        assert!(!ComparisonOperator::NotEqual.evaluate(10.0, 10.0));
    }

    /// 测试告警级别与事件级别转换
    #[test]
    fn test_alarm_level_to_event_level() {
        assert_eq!(
            AlarmLevel::Info.to_event_level(),
            crate::domain::event::value_objects::EventLevel::Info
        );
        assert_eq!(
            AlarmLevel::Warning.to_event_level(),
            crate::domain::event::value_objects::EventLevel::Warning
        );
        assert_eq!(
            AlarmLevel::Error.to_event_level(),
            crate::domain::event::value_objects::EventLevel::Error
        );
        assert_eq!(
            AlarmLevel::Critical.to_event_level(),
            crate::domain::event::value_objects::EventLevel::Critical
        );
    }

    // ===== 额外的状态转换边界测试 =====

    /// 测试解决已解决的报警失败
    #[test]
    fn test_alarm_resolve_already_resolved_fails() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        // 先解决
        alarm.resolve(
            "user-001".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            None,
        )
        .unwrap();

        // 再次解决应该失败
        let result = alarm.resolve(
            "user-002".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            None,
        );
        assert!(result.is_err());
    }

    /// 测试抑制已解决的报警失败
    #[test]
    fn test_alarm_suppress_after_resolve_fails() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Error,
            "设备离线".to_string(),
            None,
            None,
        );

        alarm.resolve(
            "user-001".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            None,
        )
        .unwrap();

        let result = alarm.suppress();
        assert!(result.is_err());
    }

    /// 测试确认已抑制的报警失败
    #[test]
    fn test_alarm_acknowledge_suppressed_fails() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::PropertyThreshold,
            AlarmLevel::Warning,
            "温度告警".to_string(),
            None,
            None,
        );

        alarm.suppress().unwrap();

        let result = alarm.acknowledge("user-001".to_string(), None);
        assert!(result.is_err());
    }

    /// 测试解决已抑制的报警失败
    #[test]
    fn test_alarm_resolve_suppressed_fails() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::PropertyThreshold,
            AlarmLevel::Warning,
            "温度告警".to_string(),
            None,
            None,
        );

        alarm.suppress().unwrap();

        let result = alarm.resolve(
            "user-001".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            None,
        );
        assert!(result.is_err());
    }

    /// 测试从已确认状态解决报警
    #[test]
    fn test_alarm_resolve_from_acknowledged() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::DeviceError,
            AlarmLevel::Critical,
            "设备故障".to_string(),
            None,
            None,
        );

        alarm.acknowledge("user-001".to_string(), None).unwrap();
        let result = alarm.resolve(
            "user-002".to_string(),
            crate::domain::alarm::value_objects::ResolutionType::Fixed,
            Some("已修复".to_string()),
        );

        assert!(result.is_ok());
        assert!(!alarm.is_active());
        let res = alarm.resolution.as_ref().unwrap();
        assert_eq!(res.resolved_by, "user-002");
        assert_eq!(
            res.resolution_type,
            crate::domain::alarm::value_objects::ResolutionType::Fixed
        );
    }

    /// 测试抑制后的报警不再是活跃状态
    #[test]
    fn test_suppressed_alarm_is_not_active() {
        let mut alarm = Alarm::new(
            "device-001".to_string(),
            None,
            None,
            AlarmType::PropertyThreshold,
            AlarmLevel::Warning,
            "温度告警".to_string(),
            None,
            None,
        );

        assert!(alarm.is_active());

        alarm.suppress().unwrap();
        assert!(!alarm.is_active());
        // 抑制状态仍然不是已解决
        assert!(!alarm.status.is_resolved());
    }

    /// 测试规则更新时名称为空失败
    #[test]
    fn test_alarm_rule_update_empty_name_fails() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };

        let notification_config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            repeat_interval: None,
        };

        let mut rule = AlarmRule::new(
            "温度告警".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        )
        .unwrap();

        let result = rule.update(Some("".to_string()), None, None, None, None);
        assert!(result.is_err());
    }

    /// 测试 ResolutionType 字符串转换
    #[test]
    fn test_resolution_type_string() {
        use crate::domain::alarm::value_objects::ResolutionType;

        assert_eq!(ResolutionType::Fixed.as_str(), "fixed");
        assert_eq!(ResolutionType::FalseAlarm.as_str(), "false_alarm");
        assert_eq!(ResolutionType::Ignored.as_str(), "ignored");
        assert_eq!(ResolutionType::AutoResolved.as_str(), "auto_resolved");
    }

    /// 测试告警规则 - 未启用通知时不需要渠道和接收人
    #[test]
    fn test_alarm_rule_notification_disabled_no_channels_needed() {
        let condition = AlarmCondition::Threshold {
            operator: ComparisonOperator::GreaterThan,
            value: 80.0,
        };

        let notification_config = NotificationConfig {
            enabled: false, // 未启用通知
            channels: vec![], // 空渠道
            recipients: vec![], // 空接收人
            suppress_duration: None,
            repeat_interval: None,
        };

        let result = AlarmRule::new(
            "不通知规则".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            condition,
            AlarmLevel::Warning,
            notification_config,
        );

        assert!(result.is_ok());
    }

    /// 测试 AlarmStatus 字符串解析
    #[test]
    fn test_alarm_status_string() {
        use crate::domain::alarm::value_objects::AlarmStatus;

        assert_eq!(AlarmStatus::Active.as_str(), "active");
        assert_eq!(AlarmStatus::Acknowledged.as_str(), "acknowledged");
        assert_eq!(AlarmStatus::Resolved.as_str(), "resolved");
        assert_eq!(AlarmStatus::Suppressed.as_str(), "suppressed");

        assert_eq!(AlarmStatus::from_str("active"), Some(AlarmStatus::Active));
        assert_eq!(
            AlarmStatus::from_str("acknowledged"),
            Some(AlarmStatus::Acknowledged)
        );
        assert_eq!(
            AlarmStatus::from_str("resolved"),
            Some(AlarmStatus::Resolved)
        );
        assert_eq!(
            AlarmStatus::from_str("suppressed"),
            Some(AlarmStatus::Suppressed)
        );
        assert_eq!(AlarmStatus::from_str("invalid"), None);
    }

    /// 测试 AlarmLevel 字符串解析
    #[test]
    fn test_alarm_level_string() {
        use crate::domain::alarm::value_objects::AlarmLevel;

        assert_eq!(AlarmLevel::Info.as_str(), "info");
        assert_eq!(AlarmLevel::Warning.as_str(), "warning");
        assert_eq!(AlarmLevel::Error.as_str(), "error");
        assert_eq!(AlarmLevel::Critical.as_str(), "critical");

        assert_eq!(AlarmLevel::from_str("info"), Some(AlarmLevel::Info));
        assert_eq!(
            AlarmLevel::from_str("warning"),
            Some(AlarmLevel::Warning)
        );
        assert_eq!(AlarmLevel::from_str("error"), Some(AlarmLevel::Error));
        assert_eq!(
            AlarmLevel::from_str("critical"),
            Some(AlarmLevel::Critical)
        );
        assert_eq!(AlarmLevel::from_str("invalid"), None);
    }

    /// 测试告警级别事件级别反向转换
    #[test]
    fn test_alarm_level_from_event_level() {
        use crate::domain::alarm::value_objects::AlarmLevel;
        use crate::domain::event::value_objects::EventLevel;

        assert_eq!(
            AlarmLevel::from_event_level(&EventLevel::Debug),
            AlarmLevel::Info
        );
        assert_eq!(
            AlarmLevel::from_event_level(&EventLevel::Info),
            AlarmLevel::Info
        );
        assert_eq!(
            AlarmLevel::from_event_level(&EventLevel::Warning),
            AlarmLevel::Warning
        );
        assert_eq!(
            AlarmLevel::from_event_level(&EventLevel::Error),
            AlarmLevel::Error
        );
        assert_eq!(
            AlarmLevel::from_event_level(&EventLevel::Critical),
            AlarmLevel::Critical
        );
    }

    /// 测试 AlarmType 所有变体
    #[test]
    fn test_alarm_type_all_variants() {
        assert_eq!(AlarmType::DeviceOffline.as_str(), "device_offline");
        assert_eq!(AlarmType::DeviceError.as_str(), "device_error");
        assert_eq!(
            AlarmType::PropertyThreshold.as_str(),
            "property_threshold"
        );
        assert_eq!(
            AlarmType::PropertyAnomaly.as_str(),
            "property_anomaly"
        );
        assert_eq!(AlarmType::CommandFailed.as_str(), "command_failed");

        // 测试反解析
        assert_eq!(
            AlarmType::from_str("device_offline"),
            AlarmType::DeviceOffline
        );
        assert_eq!(
            AlarmType::from_str("device_error"),
            AlarmType::DeviceError
        );
        assert_eq!(
            AlarmType::from_str("property_threshold"),
            AlarmType::PropertyThreshold
        );
        assert_eq!(
            AlarmType::from_str("property_anomaly"),
            AlarmType::PropertyAnomaly
        );
        assert_eq!(
            AlarmType::from_str("command_failed"),
            AlarmType::CommandFailed
        );
    }

    /// 测试创建带所有字段的告警
    #[test]
    fn test_alarm_creation_full_fields() {
        let alarm = Alarm::new(
            "device-001".to_string(),
            Some("property-temp".to_string()),
            Some("rule-001".to_string()),
            AlarmType::PropertyThreshold,
            AlarmLevel::Critical,
            "温度严重超标".to_string(),
            Some("120.5".to_string()),
            Some("80.0".to_string()),
        );

        assert!(!alarm.id.is_empty());
        assert_eq!(alarm.device_id, "device-001");
        assert_eq!(alarm.property_id, Some("property-temp".to_string()));
        assert_eq!(alarm.rule_id, Some("rule-001".to_string()));
        assert_eq!(alarm.alarm_type, AlarmType::PropertyThreshold);
        assert_eq!(alarm.alarm_level, AlarmLevel::Critical);
        assert_eq!(alarm.message, "温度严重超标");
        assert_eq!(alarm.alarm_value, Some("120.5".to_string()));
        assert_eq!(alarm.threshold_value, Some("80.0".to_string()));
        assert_eq!(alarm.status, AlarmStatus::Active);
        assert!(alarm.acknowledgement.is_none());
        assert!(alarm.resolution.is_none());
    }
}
