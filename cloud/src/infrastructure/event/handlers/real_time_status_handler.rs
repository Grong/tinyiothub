use std::sync::Arc;

use crate::{
    domain::event::{
        entities::Event, repositories::RealTimeEventRepository, value_objects::EventLevel,
    },
    infrastructure::event::EventHandler,
};

/// 实时状态处理器
///
/// 职责：
/// - 更新实时事件状态表
/// - 只处理 Warning 及以上级别的事件
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
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 只处理需要更新实时状态的事件
        if event.level().should_update_real_time_status() {
            self.repository
                .upsert_status(event)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "RealTimeStatusHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        // 只处理 Warning 及以上级别
        matches!(event.level(), EventLevel::Warning | EventLevel::Error | EventLevel::Critical)
    }

    fn priority(&self) -> u8 {
        // 实时状态更新优先级高
        10
    }
}
