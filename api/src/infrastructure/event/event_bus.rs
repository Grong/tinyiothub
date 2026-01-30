use crate::domain::event::{entities::Event, Result as EventResult};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info};

/// 事件处理器接口
///
/// 所有事件处理器必须实现此接口
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// 处理事件
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// 获取处理器名称（用于日志）
    fn name(&self) -> &str;

    /// 判断是否应该处理该事件
    fn should_handle(&self, event: &Event) -> bool;

    /// 获取处理优先级（数字越小优先级越高）
    fn priority(&self) -> u8 {
        100 // 默认优先级
    }
}

/// 事件总线 - 基础设施层的消息分发机制
///
/// 职责：
/// - 接收领域事件
/// - 分发给所有注册的 handlers
/// - 提供实时订阅能力
///
/// 不包含业务逻辑，只负责技术层面的消息传递
pub struct EventBus {
    /// 实时事件广播通道
    event_sender: broadcast::Sender<Event>,
    /// 保持一个接收者防止通道关闭
    _event_receiver: broadcast::Receiver<Event>,

    /// 注册的事件处理器
    handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        let (event_sender, event_receiver) = broadcast::channel(1000);

        Self {
            event_sender,
            _event_receiver: event_receiver,
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 发布事件
    ///
    /// 事件会被：
    /// 1. 广播给所有实时订阅者
    /// 2. 分发给所有注册的 handlers
    pub async fn publish(&self, event: Event) -> EventResult<()> {
        debug!(
            "Publishing event: {} (type: {:?}, level: {:?})",
            event.id(),
            event.event_type(),
            event.level()
        );

        // 1. 广播给实时订阅者
        match self.event_sender.send(event.clone()) {
            Ok(subscriber_count) => {
                debug!(
                    "Event {} broadcasted to {} subscribers",
                    event.id(),
                    subscriber_count
                );
            }
            Err(_) => {
                debug!("No subscribers for event {}", event.id());
            }
        }

        // 2. 分发给所有 handlers
        self.dispatch_to_handlers(&event).await?;

        Ok(())
    }

    /// 订阅事件（用于实时推送）
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_sender.subscribe()
    }

    /// 注册事件处理器
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.write().await;
        info!("Registering event handler: {}", handler.name());
        handlers.push(handler);
    }

    /// 获取订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.event_sender.receiver_count()
    }

    /// 获取处理器数量
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.len()
    }

    /// 分发事件到所有 handlers
    async fn dispatch_to_handlers(&self, event: &Event) -> EventResult<()> {
        let handlers = self.handlers.read().await;

        // 按优先级排序
        let mut sorted_handlers: Vec<_> = handlers.iter().collect();
        sorted_handlers.sort_by_key(|h| h.priority());

        for handler in sorted_handlers {
            if handler.should_handle(event) {
                if let Err(e) = handler.handle(event).await {
                    error!(
                        "Handler {} failed to process event {}: {}",
                        handler.name(),
                        event.id(),
                        e
                    );
                    // 继续处理其他 handlers，不中断
                }
            }
        }

        Ok(())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscriber_count", &self.event_sender.receiver_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::{
        EventLevel, EventSource, EventType, RichContent, SystemEventType,
    };

    fn create_test_event() -> Event {
        Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .expect("Failed to create test event")
    }

    #[tokio::test]
    async fn test_event_bus_creation() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(bus.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_event_publishing() {
        let bus = EventBus::new();
        let event = create_test_event();

        let result = bus.publish(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        assert_eq!(bus.subscriber_count(), 1);

        let event = create_test_event();
        let event_id = event.id().clone();

        // Publish event
        bus.publish(event).await.unwrap();

        // Receive event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.id(), &event_id);
    }
}
