use serde::{Deserialize, Serialize};

/// 配对请求（前端提交）
#[derive(Debug, Deserialize)]
pub struct PairingRequest {
    pub code: String,
    pub workspace_id: Option<String>,
}

/// 配对响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResponse {
    pub device_id: String,
    pub device_name: String,
    pub hostname: String,
    pub ip: String,
}

/// 网关宣告（MQTT 消息，网关→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingAnnounce {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub code: String,
    pub fingerprint: String,
    pub hostname: String,
    pub os: String,
    pub ip: String,
    pub hw_model: String,
}

/// 配对响应（MQTT 消息，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingAck {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub success: bool,
    pub device_id: String,
    pub workspace_id: String,
    pub credentials: MqttCredentials,
    pub topics: GatewayTopics,
    pub keepalive: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttCredentials {
    pub client_id: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayTopics {
    pub status: String,
    pub telemetry: String,
    pub event: String,
    pub command: String,
    pub config: String,
    pub device_discover: String,
    pub device_telemetry: String,
}

/// 子设备发现消息（MQTT，网关→平台）
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDiscoverMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub devices: Vec<DiscoveredDevice>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredDevice {
    pub name: String,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
}

/// 遥测消息（MQTT，网关/子设备→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// 子设备遥测消息
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceTelemetryMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub device_id: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// 状态消息（MQTT，网关→平台）
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct StatusMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub status: String,
    pub uptime: Option<u64>,
    pub timestamp: i64,
}

/// 指令下发请求（前端→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub device_id: String,
    pub action: String,
    pub params: serde_json::Value,
}

/// 指令下发消息（MQTT，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub command_id: String,
    pub device_id: String,
    pub action: String,
    pub params: serde_json::Value,
    pub timestamp: i64,
}

/// 网关配置下发消息（MQTT，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConfigMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub config: serde_json::Value,
    pub timestamp: i64,
}

/// 网关上行数据消息（MQTT，网关→平台）
#[derive(Debug)]
pub enum GatewayDataMessage {
    Status { gateway_id: String, workspace_id: String, msg: StatusMessage },
    Telemetry { gateway_id: String, workspace_id: String, msg: TelemetryMessage },
    DeviceDiscover { gateway_id: String, workspace_id: String, msg: DeviceDiscoverMessage },
    DeviceTelemetry { gateway_id: String, workspace_id: String, msg: DeviceTelemetryMessage },
}
