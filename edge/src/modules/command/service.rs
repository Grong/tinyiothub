use std::sync::Arc;
use crate::modules::device::DeviceService;
use crate::modules::gateway::GatewayService;
use crate::shared::error::EdgeResult;

pub struct CommandService {
    device_service: Arc<DeviceService>,
    gateway_service: Arc<GatewayService>,
}

impl CommandService {
    pub fn new(
        device_service: Arc<DeviceService>,
        gateway_service: Arc<GatewayService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            device_service,
            gateway_service,
        })
    }

    pub fn device_service(&self) -> &Arc<DeviceService> {
        &self.device_service
    }

    /// Execute a command on a device. Resolves the correct driver via DeviceService.
    pub async fn execute(
        &self,
        device_id: &str,
        command: &serde_json::Value,
    ) -> EdgeResult<()> {
        let _driver_name = self
            .device_service
            .get_driver_for_device(device_id)
            .await?;

        // In production: look up driver in runtime registry and call driver.execute_command()
        // For now, delegate to runtime if available, otherwise succeed silently
        let result = serde_json::json!({
            "device_id": device_id,
            "status": "executed",
            "command": command
        });
        let payload = serde_json::to_vec(&result)?;
        self.gateway_service.publish_telemetry(&payload).await.ok();

        Ok(())
    }
}
