use std::sync::Arc;
use super::types::TransformRule;
use crate::modules::gateway::GatewayService;
use crate::modules::driver::DriverService;
use crate::modules::offline::{BufferMessage, BufferPriority, OfflineBuffer};

pub struct TelemetryService {
    driver_service: Arc<DriverService>,
    gateway_service: Arc<GatewayService>,
    offline_buffer: Arc<OfflineBuffer>,
}

impl TelemetryService {
    pub fn new(
        driver_service: Arc<DriverService>,
        gateway_service: Arc<GatewayService>,
        offline_buffer: Arc<OfflineBuffer>,
    ) -> Arc<Self> {
        Arc::new(Self {
            driver_service,
            gateway_service,
            offline_buffer,
        })
    }

    /// Collect telemetry from all drivers and forward to cloud.
    /// On publish failure, buffer locally for later flush.
    pub async fn collect_and_forward(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let devices = self.driver_service.scan_all().await?;
        let payload = serde_json::to_vec(&devices)?;

        let topic = format!("{}/telemetry", self.gateway_service.topic_prefix());

        // Inline buffering: if publish fails, write to offline buffer
        if let Err(e) = self.gateway_service.publish_telemetry(&payload).await {
            tracing::warn!(?e, "Telemetry publish failed, buffering locally");
            self.offline_buffer
                .write(BufferMessage {
                    msg_type: "telemetry".into(),
                    topic,
                    payload: payload.to_vec(),
                    priority: BufferPriority::Normal,
                })
                .await
                .ok();
            return Err(e);
        }

        Ok(())
    }

    /// Apply value mapping transforms to telemetry data (zero-dependency, pure function)
    pub fn apply_transform(
        input: &serde_json::Value,
        rules: &[TransformRule],
    ) -> serde_json::Value {
        let mut output = input.clone();
        for rule in rules {
            if let Some(source_val) = input.get(&rule.source).and_then(|v| v.as_f64()) {
                let result = match rule.op.as_str() {
                    "multiply" => source_val * rule.factor,
                    "add" => source_val + rule.factor,
                    "divide" => source_val / rule.factor,
                    "subtract" => source_val - rule.factor,
                    _ => source_val,
                };
                if let Some(obj) = output.as_object_mut() {
                    obj.insert(rule.target.clone(), serde_json::Value::from(result));
                }
            }
        }
        output
    }
}
