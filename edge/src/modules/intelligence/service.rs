use crate::modules::device::DeviceService;
use crate::modules::driver::DriverService;
use crate::modules::gateway::GatewayService;
use crate::shared::error::{EdgeError, EdgeResult};
use std::sync::Arc;

pub struct IntelligenceService {
    #[allow(dead_code)]
    device_service: Arc<DeviceService>,
    #[allow(dead_code)]
    driver_service: Arc<DriverService>,
    #[allow(dead_code)]
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
    pub async fn evaluate_and_probe(&self) -> EdgeResult<()> {
        // Evaluate alarm rules from config
        // In production: read rules from ConfigService, evaluate against current telemetry

        // Run self-healing probes with catch_unwind
        // In production: iterate registered probes, catch panics individually
        Self::run_probe(|| "ok").map(|_| ())
    }

    /// Run a probe closure with catch_unwind protection (AssertUnwindSafe wrapper).
    /// Returns Ok(result) or Err(ProbePanic) if the probe panicked.
    pub(crate) fn run_probe<F, R>(f: F) -> EdgeResult<R>
    where
        F: FnOnce() -> R,
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(result) => Ok(result),
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "unknown panic".to_string()
                };
                tracing::error!(%msg, "Intelligence probe panicked (caught by catch_unwind)");
                Err(EdgeError::ProbePanic(msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_probe_success() {
        let result = IntelligenceService::run_probe(|| 42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_run_probe_catches_panic_string() {
        let result = IntelligenceService::run_probe(|| {
            panic!("test panic message");
        });
        assert!(matches!(result.unwrap_err(), EdgeError::ProbePanic(ref msg) if msg.contains("test panic message")));
    }

    #[test]
    fn test_run_probe_catches_panic_str() {
        let result = IntelligenceService::run_probe(|| {
            panic!("static str panic");
        });
        assert!(matches!(result.unwrap_err(), EdgeError::ProbePanic(_)));
    }
}
