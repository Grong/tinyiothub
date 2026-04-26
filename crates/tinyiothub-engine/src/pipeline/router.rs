//! Data router — routes processed telemetry to storage, rules, or external systems.
//!
//! TODO: Migrate routing logic from `cloud/src/application/`.

/// Routes telemetry data to downstream consumers.
#[derive(Debug, Default)]
pub struct DataRouter;

impl DataRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Route a telemetry event to the appropriate handlers.
    ///
    /// Currently a passthrough — no routing logic is applied.
    pub fn route(
        &self,
        _device_id: &str,
        _payload: &serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }
}
