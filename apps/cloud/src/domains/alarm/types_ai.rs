//! AI 子系统出站端口 + 跨域告警事件 payload。
//!
//! payload（AlarmEvent）归位 core::models::event；AlarmService 经
//! [`AlarmAiPublisher`] 端口发布，组合层适配器见 shared/ai_adapter.rs。

pub use tinyiothub_core::models::event::AlarmEvent;

/// Outbound port: notify the AI subsystem that a significant alarm occurred.
pub trait AlarmAiPublisher: Send + Sync {
    fn publish_alarm_created(&self, event: AlarmEvent);
}
