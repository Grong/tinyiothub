//! 自动化规则事件处理器
//! 
//! 监听设备事件，触发自动化规则

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::automation::{AutomationService, TriggerContext};
use crate::domain::event::entities::Event;
use crate::infrastructure::event::event_bus::{EventHandler, EventHandler};
use crate::infrastructure::event::EventBus;

/// 自动化事件处理器
/// 
/// 监听设备相关事件，触发自动化规则
pub struct AutomationEventHandler {
    automation_service: Arc<AutomationService>,
    event_bus: Arc<EventBus>,
}

impl AutomationEventHandler {
    pub fn new(automation_service: Arc<AutomationService>, event_bus: Arc<EventBus>) -> Self {
        Self {
            automation_service,
            event_bus,
        }
    }
    
    /// 启动事件监听
    pub async fn start_listening(&self) {
        // 订阅设备事件
        let bus = self.event_bus.clone();
        let service = self.automation_service.clone();
        
        tokio::spawn(async move {
            let mut receiver = bus.subscribe();
            
            while let Ok(event) = receiver.recv().await {
                // 检查是否是设备相关事件
                if Self::should_handle_event(&event) {
                    // 构建触发上下文
                    if let Some(context) = Self::build_context(&event) {
                        // 触发自动化规则
                        let _ = service.trigger(context).await;
                    }
                }
            }
        });
    }
    
    /// 判断是否应该处理该事件
    fn should_handle_event(event: &Event) -> bool {
        let event_type = event.event_type();
        
        // 监听设备数据变化、告警等事件
        matches!(
            event_type.as_str(),
            "device.data_changed" | 
            "device.status_changed" | 
            "device.property_updated" |
            "alarm.created" |
            "alarm.acknowledged"
        )
    }
    
    /// 从事件构建触发上下文
    fn build_context(event: &Event) -> Option<TriggerContext> {
        let content = event.content();
        let mut context = TriggerContext::new();
        
        // 从事件源获取设备信息
        if let Some(source) = content.get("device_id").or(content.get("source")) {
            if let Some(device_id) = source.as_str() {
                context.device_id = Some(device_id.to_string());
            }
        }
        
        // 获取设备名称
        if let Some(name) = content.get("device_name").or(content.get("name")) {
            if let Some(device_name) = name.as_str() {
                context.device_name = Some(device_name.to_string());
            }
        }
        
        // 获取属性数据
        if let Some(data) = content.get("data").or(content.get("properties")) {
            if let Some(obj) = data.as_object() {
                for (key, value) in obj {
                    context.properties.insert(key.clone(), value.clone());
                }
            }
        }
        
        // 特殊处理设备状态变化
        if let Some(state) = content.get("state").or(content.get("status")) {
            context.device_online = Some(state.as_str() == Some("online"));
        }
        
        // 设置事件数据
        context.event_data = Some(content.clone());
        
        Some(context)
    }
}

#[async_trait]
impl EventHandler for AutomationEventHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if Self::should_handle_event(event) {
            if let Some(context) = Self::build_context(event) {
                self.automation_service.trigger(context).await;
            }
        }
        Ok(())
    }
    
    fn name(&self) -> &str {
        "automation_event_handler"
    }
    
    fn should_handle(&self, event: &Event) -> bool {
        Self::should_handle_event(event)
    }
    
    fn priority(&self) -> u8 {
        50 // 高优先级
    }
}
