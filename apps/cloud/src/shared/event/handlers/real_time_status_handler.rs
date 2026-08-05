use std::sync::Arc;

use tinyiothub_event::{entities::Event, repositories::RealTimeEventRepository};

use crate::shared::event::EventHandler;

/// 实时状态处理器
///
/// 职责：
/// - 将需要追踪实时状态的事件写（upsert）入 events 表。
///   events 表已吸收原 real_time_events 表的功能（Thing Ontology 迁移）。
/// - 根据 Event 实体的业务规则判断是否需要更新实时状态
///   (event.should_update_real_time_status())。
pub struct RealTimeStatusHandler {
    repository: Arc<dyn RealTimeEventRepository>,
}

impl RealTimeStatusHandler {
    pub fn new(repository: Arc<dyn RealTimeEventRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl EventHandler for RealTimeStatusHandler {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        // 根据实体的业务规则判断（类型 + 级别）
        if event.should_update_real_time_status() {
            self.repository
                .upsert_status(event)
                .await
                .map_err(|e| tinyiothub_core::error::Error::Internal(e.to_string()))?;
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "RealTimeStatusHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        // 委托给 Event 实体的业务规则：仅 Device (Warning+) 和 System (Critical/Error)
        event.should_update_real_time_status()
    }

    fn priority(&self) -> u8 {
        // 实时状态更新优先级高
        10
    }
}
