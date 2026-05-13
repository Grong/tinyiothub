use std::sync::Arc;
use super::device::DeviceService;
use super::gateway::GatewayService;

pub struct CommandService;

impl CommandService {
    pub fn new(
        _device_service: Arc<DeviceService>,
        _gateway_service: Arc<GatewayService>,
    ) -> Arc<Self> {
        Arc::new(Self)
    }
    pub async fn execute(
        &self,
        _device_id: &str,
        _command: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
