//! 通知处理器

use async_trait::async_trait;

use crate::{modules::plugin::notification::Notification, shared::error::Error};

#[async_trait]
pub trait NotificationHandler: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod dingtalk;
pub mod feishu;

pub use dingtalk::DingtalkHandler;
pub use feishu::FeishuHandler;
