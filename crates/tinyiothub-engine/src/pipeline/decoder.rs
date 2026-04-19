//! Protocol decoder — converts raw bytes to structured telemetry.
//!
//! TODO: Migrate protocol-specific decoders from `cloud/src/domain/device/driver/`.

/// Decodes raw device payload into structured data.
pub trait ProtocolDecoder: Send + Sync {
    /// Decode raw bytes into a JSON value.
    fn decode(&self, payload: &[u8]) -> Result<serde_json::Value, String>;

    /// Returns the protocol name (e.g. "modbus", "opcua", "mqtt").
    fn protocol_name(&self) -> &str;
}
