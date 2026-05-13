use std::sync::Arc;
use super::device::DeviceService;
use super::driver::DriverService;
use super::gateway::GatewayService;

pub struct IntelligenceService;

impl IntelligenceService {
    pub fn new(
        _device_service: Arc<DeviceService>,
        _driver_service: Arc<DriverService>,
        _gateway_service: Arc<GatewayService>,
    ) -> Arc<Self> {
        Arc::new(Self)
    }
    pub async fn evaluate_and_probe(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
