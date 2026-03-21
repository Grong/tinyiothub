//! WebSocket Handler for Real-time Communication
//! 
//! 支持设备状态实时推送、告警通知等

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::shared::app_state::AppState;

/// WebSocket消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
    /// 设备状态更新
    DeviceStatusUpdate {
        device_id: String,
        status: String,
        properties: Option<serde_json::Value>,
    },
    /// 设备数据上报
    DeviceData {
        device_id: String,
        data: serde_json::Value,
    },
    /// 告警通知
    Alarm {
        alarm_id: String,
        device_id: String,
        level: String,
        message: String,
    },
    /// 网关状态
    GatewayStatus {
        gateway_id: String,
        status: String,
    },
    /// 心跳
    Ping,
    /// 心跳响应
    Pong,
    /// 订阅确认
    Subscribed { channel: String },
    /// 错误
    Error { message: String },
}

/// 在线连接管理
pub struct WsConnectionManager {
    connections: Arc<RwLock<std::collections::HashMap<String, Vec<tokio::sync::mpsc::Sender<WsMessage>>>>>,
}

impl WsConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 订阅频道
    pub async fn subscribe(&self, channel: String, sender: tokio::sync::mpsc::Sender<WsMessage>) {
        let mut connections = self.connections.write().await;
        connections.entry(channel).or_default().push(sender);
    }

    /// 向频道发送消息
    pub async fn broadcast(&self, channel: &str, message: WsMessage) {
        let connections = self.connections.read().await;
        if let Some(senders) = connections.get(channel) {
            for sender in senders {
                if let Err(e) = sender.send(message.clone()).await {
                    warn!("Failed to send WS message: {}", e);
                }
            }
        }
    }

    /// 向指定用户发送消息
    pub async fn send_to_user(&self, user_id: &str, message: WsMessage) {
        self.broadcast(&format!("user:{}", user_id), message).await;
    }

    /// 向所有订阅者广播设备更新
    pub async fn broadcast_device_update(&self, device_id: &str, status: &str, properties: Option<serde_json::Value>) {
        let message = WsMessage::DeviceStatusUpdate {
            device_id: device_id.to_string(),
            status: status.to_string(),
            properties,
        };
        self.broadcast("devices", message).await;
    }

    /// 广播告警
    pub async fn broadcast_alarm(&self, alarm_id: &str, device_id: &str, level: &str, message: &str) {
        let message = WsMessage::Alarm {
            alarm_id: alarm_id.to_string(),
            device_id: device_id.to_string(),
            level: level.to_string(),
            message: message.to_string(),
        };
        self.broadcast("alarms", message).await;
    }
}

impl Default for WsConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket路由
pub fn ws_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route("/ws", get(ws_handler))
        .route("/ws/devices", get(ws_devices_handler))
        .route("/ws/alarms", get(ws_alarms_handler))
}

/// WebSocket处理 - 通用
pub async fn ws_handler(
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(handle_socket)
}

/// WebSocket处理 - 设备实时更新
pub async fn ws_devices_handler(
    ws: WebSocketUpgrade,
) -> Response {
    info!("Device WebSocket connection request");
    ws.on_upgrade(handle_device_socket)
}

/// WebSocket处理 - 告警实时推送
pub async fn ws_alarms_handler(
    ws: WebSocketUpgrade,
) -> Response {
    info!("Alarm WebSocket connection request");
    ws.on_upgrade(handle_alarm_socket)
}

/// 通用WebSocket处理
async fn handle_socket(ws: WebSocket) {
    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(100);

    // 启动接收任务
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<WsMessage>(&text) {
                        match parsed {
                            WsMessage::Ping => {
                                let _ = tx_clone.send(WsMessage::Pong).await;
                            }
                            _ => {
                                info!("Received WS message: {:?}", parsed);
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // 发送消息
    while let Some(msg) = rx.recv().await {
        if let Ok(text) = serde_json::to_string(&msg) {
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }
}

/// 设备WebSocket处理
async fn handle_device_socket(ws: WebSocket) {
    let connection_id = Uuid::new_v4().to_string();
    info!("New device WebSocket connection: {}", connection_id);

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(100);

    // 接收并处理消息
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    info!("Device WS received: {}", text);
                    if let Ok(parsed) = serde_json::from_str::<WsMessage>(&text) {
                        match parsed {
                            WsMessage::Ping => {
                                let _ = tx_clone.send(WsMessage::Pong).await;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Device WS closed: {}", connection_id);
                    break;
                }
                Err(e) => {
                    error!("Device WS error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // 发送消息
    while let Some(msg) = rx.recv().await {
        if let Ok(text) = serde_json::to_string(&msg) {
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }

    info!("Device WebSocket connection closed: {}", connection_id);
}

/// 告警WebSocket处理
async fn handle_alarm_socket(ws: WebSocket) {
    let connection_id = Uuid::new_v4().to_string();
    info!("New alarm WebSocket connection: {}", connection_id);

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(100);

    // 接收并处理消息
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    info!("Alarm WS received: {}", text);
                    if let Ok(parsed) = serde_json::from_str::<WsMessage>(&text) {
                        match parsed {
                            WsMessage::Ping => {
                                let _ = tx_clone.send(WsMessage::Pong).await;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Alarm WS closed: {}", connection_id);
                    break;
                }
                Err(e) => {
                    error!("Alarm WS error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // 发送消息
    while let Some(msg) = rx.recv().await {
        if let Ok(text) = serde_json::to_string(&msg) {
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }

    info!("Alarm WebSocket connection closed: {}", connection_id);
}
