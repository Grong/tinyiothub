use std::sync::Arc;
use crate::modules::device::DeviceService;
use crate::modules::driver::DriverService;
use crate::modules::gateway::GatewayService;

pub struct IntelligenceService {
    device_service: Arc<DeviceService>,
    driver_service: Arc<DriverService>,
    gateway_service: Arc<GatewayService>,
}

impl IntelligenceService {
    pub fn new(
        device_service: Arc<DeviceService>,
        driver_service: Arc<DriverService>,
        gateway_service: Arc<GatewayService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            device_service,
            driver_service,
            gateway_service,
        })
    }

    /// Evaluate alarm rules and run self-healing probes (with catch_unwind protection)
    pub async fn evaluate_and_probe(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Evaluate alarm rules from config
        // In production: read rules from ConfigService, evaluate against current telemetry

        // Run self-healing probes with catch_unwind
        // In production: iterate registered probes, catch panics individually
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Probes would run here
            "ok"
        }));

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "unknown panic".to_string()
                };
                tracing::error!(%msg, "Intelligence probe panicked (caught by catch_unwind)");
                Err(format!("probe panicked: {}", msg).into())
            }
        }
    }
}
