use std::sync::Arc;

use crate::{
    domain::event::entities::Event,
    infrastructure::event::{EventHandler, SseConnectionManager},
};

/// SSE 推送处理器
///
/// 职责：
/// - 将事件实时推送到前端
/// - 转换事件格式为 SSE 消息
pub struct SseEventHandler {
    sse_manager: Arc<SseConnectionManager>,
}

impl SseEventHandler {
    pub fn new(sse_manager: Arc<SseConnectionManager>) -> Self {
        Self { sse_manager }
    }
}

#[async_trait::async_trait]
impl EventHandler for SseEventHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 直接广播事件到所有连接的客户端
        // SseConnectionManager 会负责序列化
        self.sse_manager.broadcast_event(event).await;

        Ok(())
    }

    fn name(&self) -> &str {
        "SseEventHandler"
    }

    fn should_handle(&self, _event: &Event) -> bool {
        // 所有事件都推送到前端
        true
    }

    fn priority(&self) -> u8 {
        // SSE 推送优先级最高（实时性要求）
        1
    }
}
