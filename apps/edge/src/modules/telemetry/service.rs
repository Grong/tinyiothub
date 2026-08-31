use super::types::TransformRule;
use crate::modules::driver::DriverService;
use crate::modules::gateway::GatewayService;
use crate::modules::offline::{BufferMessage, BufferPriority, OfflineBuffer};
use crate::shared::error::EdgeResult;
use std::sync::Arc;

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
    pub async fn collect_and_forward(&self) -> EdgeResult<()> {
        let things = self.driver_service.scan_all().await?;
        let payload = build_telemetry_payload(things.into_iter().map(serde_json::Value::from).collect());

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
    pub fn apply_transform(input: &serde_json::Value, rules: &[TransformRule]) -> serde_json::Value {
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

/// Build the telemetry payload in the cloud TelemetryMessage contract shape:
/// `{"type":"telemetry","data":[...],"timestamp":<unix seconds>}`
/// (apps/cloud/src/domains/driver/gateway/types.rs:91).
fn build_telemetry_payload(things: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": "telemetry",
        "data": things,
        "timestamp": chrono::Utc::now().timestamp(),
    }))
    .expect("telemetry payload serialization is infallible for Value")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_payload_matches_cloud_contract() {
        // Cloud parses TelemetryMessage { msg_type("type"), data, timestamp }
        // (apps/cloud/src/domains/driver/gateway/types.rs:91, snake_case).
        let things = vec![serde_json::json!({"thing_id": "t1", "value": 42})];
        let payload = build_telemetry_payload(things);
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(parsed["type"], "telemetry");
        assert!(parsed["data"].is_array());
        assert_eq!(parsed["data"][0]["thing_id"], "t1");
        assert!(parsed["timestamp"].is_i64());
    }
}
