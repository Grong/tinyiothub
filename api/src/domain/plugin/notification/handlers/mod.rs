//! 通知处理器

use async_trait::async_trait;

use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

#[async_trait]
pub trait NotificationHandler: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod feishu;
pub mod dingtalk;

pub use feishu::FeishuHandler;
pub use dingtalk::DingtalkHandler;
