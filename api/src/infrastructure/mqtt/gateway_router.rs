//! Gateway MQTT Message Router
//! 根据主题路由网关 MQTT 消息

use crate::dto::entity::gateway::{
    GatewayRegisterPayload, DeviceListReport, GatewayStatusPayload, DeviceDataPayload,
};
use crate::infrastructure::mqtt::handlers::gateway as gateway_handler;
use crate::infrastructure::persistence::database::Database;
use serde_json::Value;

/// 解析 MQTT 主题并路由消息
pub async fn route_gateway_message(
    db: &Database,
    topic: &str,
    payload: &str,
) -> Result<(), String> {
    // 主题格式: tinyiothub/gateway/{gateway_id}/...
    let parts: Vec<&str> = topic.split('/').collect();
    
    if parts.len() < 3 || parts[0] != "tinyiothub" || parts[1] != "gateway" {
        return Err("Not a gateway topic".to_string());
    }
    
    let gateway_id = parts[2];
    let action = parts.get(3).unwrap_or(&"");
    
    // 解析 payload
    let json: Value = serde_json::from_str(payload)
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    
    let payload_obj = json.get("payload")
        .ok_or("Missing payload")?;
    
    match *action {
        "auth" => {
            let sub_action = parts.get(4).unwrap_or(&"");
            match *sub_action {
                "register" => {
                    let req: GatewayRegisterPayload = serde_json::from_value(payload_obj.clone())
                        .map_err(|e| format!("Invalid register payload: {}", e))?;
                    let _ = gateway_handler::handle_register(db, &req).await;
                    Ok(())
                }
                _ => Err("Unknown auth action".to_string())
            }
        }
        "devices" => {
            let sub_action = parts.get(4).unwrap_or(&"");
            match *sub_action {
                "list" => {
                    let req: DeviceListReport = serde_json::from_value(payload_obj.clone())
                        .map_err(|e| format!("Invalid device list payload: {}", e))?;
                    gateway_handler::handle_device_list(db, gateway_id, &req).await
                }
                _ => Err("Unknown devices action".to_string())
            }
        }
        "data" => {
            let req: DeviceDataPayload = serde_json::from_value(payload_obj.clone())
                .map_err(|e| format!("Invalid data payload: {}", e))?;
            gateway_handler::handle_device_data(db, gateway_id, &req).await
        }
        "status" => {
            let req: GatewayStatusPayload = serde_json::from_value(payload_obj.clone())
                .map_err(|e| format!("Invalid status payload: {}", e))?;
            gateway_handler::handle_gateway_status(db, gateway_id, &req).await
        }
        "online" => {
            // 处理上线消息
            gateway_handler::handle_gateway_status(db, gateway_id, &GatewayStatusPayload {
                status: "online".to_string(),
                uptime: None,
                memory_usage: None,
                cpu_usage: None,
                wifi_signal: None,
                connected_devices: None,
            }).await
        }
        "offline" => {
            // 处理离线消息
            gateway_handler::handle_gateway_status(db, gateway_id, &GatewayStatusPayload {
                status: "offline".to_string(),
                uptime: None,
                memory_usage: None,
                cpu_usage: None,
                wifi_signal: None,
                connected_devices: None,
            }).await
        }
        _ => Err(format!("Unknown action: {}", action))
    }
}

/// 获取网关需要订阅的主题列表
pub fn get_gateway_topics() -> Vec<String> {
    vec![
        "tinyiothub/gateway/+/auth/register".to_string(),
        "tinyiothub/gateway/+/devices/list".to_string(),
        "tinyiothub/gateway/+/devices/add".to_string(),
        "tinyiothub/gateway/+/devices/remove".to_string(),
        "tinyiothub/gateway/+/data".to_string(),
        "tinyiothub/gateway/+/status".to_string(),
        "tinyiothub/gateway/+/online".to_string(),
        "tinyiothub/gateway/+/offline".to_string(),
    ]
}
