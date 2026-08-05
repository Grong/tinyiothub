//! 集成处理器

use async_trait::async_trait;

use crate::plugin::integration::IntegrationRequest;
use tinyiothub_core::error::Error;

#[async_trait]
pub trait IntegrationHandler: Send + Sync {
    async fn send(&self, request: &IntegrationRequest) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod wechat;
pub mod wecom;

pub use wechat::WechatHandler;
pub use wecom::WeComHandler;
