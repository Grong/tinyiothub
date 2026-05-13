use std::sync::Arc;
use super::gateway::GatewayService;
use super::driver::DriverService;
use super::offline::OfflineBuffer;

pub struct HealthService;

impl HealthService {
    pub fn new(
        _gateway_service: Arc<GatewayService>,
        _offline_buffer: Arc<OfflineBuffer>,
        _driver_service: Arc<DriverService>,
    ) -> Arc<Self> {
        Arc::new(Self)
    }
    pub async fn beat_and_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
