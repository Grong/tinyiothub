use std::sync::Arc;

use super::super::{
    entity::Alarm,
    services::{AlarmService, RuleEngine},
};
use crate::{
    domain::event::{entities::Event, value_objects::EventType},
    infrastructure::event::EventHandler,
};

/// 报警事件处理器（领域层）
///
/// 职责：
/// - 根据业务规则判断是否触发报警
/// - 执行报警业务逻辑（创建报警记录、更新设备状态等）
/// - 触发报警通知流程
///
/// 这是一个领域服务，包含报警相关的业务逻辑
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
impl EventHandler for AlarmEventHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 1. 评估规则，获取触发的报警
        let triggers = self
            .rule_engine
            .evaluate_event(event)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        if triggers.is_empty() {
            return Ok(());
        }

        // 2. 处理每个触发的报警
        for trigger in triggers {
            // 创建报警实例
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

            // 保存报警
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

            // TODO: 触发通知
            // 需要集成 NotificationService
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "AlarmEventHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        // 处理设备相关事件
        matches!(event.event_type(), EventType::Device(_))
    }

    fn priority(&self) -> u8 {
        // 报警处理优先级：在实时状态更新之后，持久化之前
        // 这样可以确保报警基于最新的状态，但不会阻塞持久化
        50
    }
}
