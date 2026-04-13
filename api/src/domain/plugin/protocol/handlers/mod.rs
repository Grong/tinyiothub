//! 协议处理器

use async_trait::async_trait;
use crate::{
    domain::device::driver::ResultValue,
    dto::entity::Device,
    shared::error::Error,
};


#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    async fn read_data(&self, device: &Device) -> Result<Vec<ResultValue>, Error>;
    
    async fn execute_command(
        &self,
        device: &Device,
        command: &str,
        args: &[String],
    ) -> Result<bool, Error> {
        let _ = (device, command, args);
        Err(Error::Unsupported("Command not supported".to_string()))
    }

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

pub mod http_poll;
pub mod mqtt_subscribe;

pub use http_poll::HttpPollHandler;
pub use mqtt_subscribe::MqttSubscribeHandler;
