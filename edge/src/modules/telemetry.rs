use std::sync::Arc;
use super::gateway::GatewayService;
use super::driver::DriverService;
use super::offline::OfflineBuffer;

pub struct TelemetryService;

impl TelemetryService {
    pub fn new(
        _driver_service: Arc<DriverService>,
        _gateway_service: Arc<GatewayService>,
        _offline_buffer: Arc<OfflineBuffer>,
    ) -> Arc<Self> {
        Arc::new(Self)
    }
    pub async fn collect_and_forward(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
