use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use tracing::{debug, error, trace};

use crate::{
    domain::event::{
        entities::Event,
        repositories::EventRepository,
        value_objects::{DeviceEventType, EventLevel, EventType},
    },
    infrastructure::event::EventHandler,
};

/// 持久化事件处理器
///
/// 职责：
/// - 根据业务规则决定是否持久化事件
/// - 实现批量写入优化
/// - 管理事件缓冲区
pub struct PersistenceEventHandler {
    repository: Arc<dyn EventRepository>,
    buffer: Arc<RwLock<EventBuffer>>,
    config: PersistenceConfig,
}

/// 持久化配置
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// 批量大小
    pub batch_size: usize,
    /// 刷新间隔
    pub flush_interval: Duration,
    /// 是否启用批量写入
    pub enable_batching: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self { batch_size: 100, flush_interval: Duration::from_secs(5), enable_batching: true }
    }
}

impl PersistenceEventHandler {
    /// 创建新的持久化处理器
    pub fn new(repository: Arc<dyn EventRepository>, config: PersistenceConfig) -> Self {
        // HarmonyOS: 强制禁用批量写入，避免后台任务
        #[cfg(feature = "harmonyos")]
        let config = PersistenceConfig { enable_batching: false, ..config };

        let buffer = Arc::new(RwLock::new(EventBuffer::new(config.batch_size)));

        // 启动定时刷新任务
        if config.enable_batching {
            Self::start_flush_task(buffer.clone(), repository.clone(), config.flush_interval);
        }

        Self { repository, buffer, config }
    }

    /// 判断是否应该持久化
    ///
    /// 业务规则：
    /// - Debug 级别的属性变化不持久化
    /// - Info 级别的属性变化只持久化有报警的
    /// - Warning 及以上级别总是持久化
    fn should_persist(&self, event: &Event) -> bool {
        match event.level() {
            EventLevel::Debug => {
                // Debug 级别的属性变化不持久化
                !event.event_type().is_property_event()
            }
            EventLevel::Info => {
                // Info 级别根据事件类型决定
                if event.event_type().is_property_event() {
                    // PropertyChange 只在有报警时持久化
                    // PropertyAlarm 和 PropertyNormal 总是持久化
                    matches!(
                        event.event_type(),
                        EventType::Device(
                            DeviceEventType::PropertyAlarm | DeviceEventType::PropertyNormal
                        )
                    ) || event.content().metadata().contains_key("alarm_triggered")
                } else {
                    true
                }
            }
            EventLevel::Warning | EventLevel::Error | EventLevel::Critical => {
                // 高级别事件总是持久化
                true
            }
        }
    }

    /// 启动定时刷新任务
    fn start_flush_task(
        buffer: Arc<RwLock<EventBuffer>>,
        repository: Arc<dyn EventRepository>,
        interval: Duration,
    ) {
        // HarmonyOS: 禁用后台刷新任务（current_thread runtime不支持spawn）
        #[cfg(feature = "harmonyos")]
        {
            tracing::warn!(
                "Event buffer auto-flush disabled on HarmonyOS (current_thread runtime)"
            );
            drop((buffer, repository, interval));
        }

        // 其他平台: 使用panic保护的spawn
        #[cfg(not(feature = "harmonyos"))]
        {
            use std::panic;
            tokio::spawn(async move {
                let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut ticker = tokio::time::interval(interval);
                        loop {
                            ticker.tick().await;
                            if let Err(e) = Self::flush_buffer(&buffer, &repository).await {
                                error!("Failed to flush event buffer: {}", e);
                            }
                        }
                    })
                }));

                let Err(e) = result;
                error!("Event flush task panicked: {:?}", e);
            });
            });
        }
    }

    /// 刷新缓冲区
    async fn flush_buffer(
        buffer: &Arc<RwLock<EventBuffer>>,
        repository: &Arc<dyn EventRepository>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let events = {
            let mut buf = buffer.write().await;
            buf.drain()
        };

        if events.is_empty() {
            return Ok(());
        }

        debug!("Flushing {} events to database", events.len());

        // 批量保存
        repository
            .save_batch(&events)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl EventHandler for PersistenceEventHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 判断是否应该持久化
        if !self.should_persist(event) {
            trace!(
                "Skipping persistence for event {} (level: {:?}, type: {:?})",
                event.id(),
                event.level(),
                event.event_type()
            );
            return Ok(());
        }

        if self.config.enable_batching {
            // 批量模式：添加到缓冲区
            let mut buffer = self.buffer.write().await;
            buffer.add(event.clone());

            // 如果缓冲区满了，立即刷新
            if buffer.is_full() {
                drop(buffer); // 释放锁
                Self::flush_buffer(&self.buffer, &self.repository).await?;
            }
        } else {
            // 立即模式：直接保存
            self.repository
                .save(event)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "PersistenceEventHandler"
    }

    fn should_handle(&self, _event: &Event) -> bool {
        // 所有事件都经过这个 handler
        true
    }

    fn priority(&self) -> u8 {
        // 持久化优先级较低，让其他 handler 先处理
        90
    }
}

/// 事件缓冲区
struct EventBuffer {
    events: Vec<Event>,
    capacity: usize,
}

impl EventBuffer {
    fn new(capacity: usize) -> Self {
        Self { events: Vec::with_capacity(capacity), capacity }
    }

    fn add(&mut self, event: Event) {
        self.events.push(event);
    }

    fn is_full(&self) -> bool {
        self.events.len() >= self.capacity
    }

    fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
