use std::time::{Duration, Instant};

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

use crate::modules::gateway::{
    service::MqttPublish,
    types::{
        DeviceDiscoverMessage, DeviceTelemetryMessage, GatewayDataMessage, PairingAnnounce,
        StatusMessage, TelemetryMessage,
    },
};

const ANNOUNCE_MAX_BURST: usize = 50;
const ANNOUNCE_RATE_WINDOW: Duration = Duration::from_secs(1);
const ANNOUNCE_MAX_PER_WINDOW: usize = 20;

pub struct PlatformMqttClient {
    client: AsyncClient,
}

impl PlatformMqttClient {
    pub fn new(
        broker_url: &str,
        broker_port: u16,
        username: &str,
        password: &str,
        announce_tx: mpsc::Sender<PairingAnnounce>,
        mut mqtt_rx: mpsc::Receiver<MqttPublish>,
        data_tx: mpsc::Sender<GatewayDataMessage>,
    ) -> Self {
        let broker_url = broker_url.to_string();
        let username = username.to_string();
        let password = password.to_string();
        let client_id = format!("tinyiothub-platform-{}", uuid::Uuid::new_v4());
        let mut options = MqttOptions::new(&client_id, &broker_url, broker_port);
        if !username.is_empty() || !password.is_empty() {
            options.set_credentials(&username, &password);
        }
        options.set_keep_alive(Duration::from_secs(30));
        options.set_max_packet_size(256 * 1024, 256 * 1024);

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let subscribe_client = client.clone();

        tokio::spawn(async move {
            let mut announce_timestamps: Vec<Instant> = Vec::with_capacity(ANNOUNCE_MAX_BURST);

            loop {
                tokio::select! {
                    event = eventloop.poll() => {
                        match event {
                            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                                tracing::info!("Platform MQTT client connected to {}:{}", broker_url, broker_port);
                                subscribe_client
                                    .subscribe("tinyiothub/pairing/announce", QoS::AtLeastOnce)
                                    .await
                                    .ok();
                                subscribe_client
                                    .subscribe("tinyiothub/+/gateway/+/status", QoS::AtMostOnce)
                                    .await
                                    .ok();
                                subscribe_client
                                    .subscribe("tinyiothub/+/gateway/+/telemetry", QoS::AtMostOnce)
                                    .await
                                    .ok();
                                subscribe_client
                                    .subscribe("tinyiothub/+/gateway/+/event", QoS::AtLeastOnce)
                                    .await
                                    .ok();
                                subscribe_client
                                    .subscribe("tinyiothub/+/gateway/+/device/discover", QoS::AtLeastOnce)
                                    .await
                                    .ok();
                                subscribe_client
                                    .subscribe("tinyiothub/+/gateway/+/device/+/telemetry", QoS::AtMostOnce)
                                    .await
                                    .ok();
                            }
                            Ok(Event::Incoming(Packet::Publish(publish))) => {
                                let topic = publish.topic.clone();
                                if topic == "tinyiothub/pairing/announce" {
                                    // Token bucket rate limiting
                                    let now = Instant::now();
                                    announce_timestamps.retain(|t| now.duration_since(*t) < ANNOUNCE_RATE_WINDOW);
                                    if announce_timestamps.len() >= ANNOUNCE_MAX_PER_WINDOW {
                                        tracing::warn!(
                                            count = announce_timestamps.len(),
                                            "Announce rate limit exceeded, dropping announce"
                                        );
                                        continue;
                                    }
                                    announce_timestamps.push(now);

                                    match serde_json::from_slice::<PairingAnnounce>(&publish.payload) {
                                        Ok(announce) => {
                                            let _ = announce_tx.send(announce).await;
                                        }
                                        Err(e) => {
                                            tracing::warn!(?e, "Failed to parse pairing announce");
                                        }
                                    }
                                } else {
                                    // Route gateway data messages by topic pattern
                                    Self::route_data_message(&topic, &publish.payload, &data_tx).await;
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::error!(?e, "Platform MQTT event loop error, reconnecting...");
                                tokio::time::sleep(Duration::from_secs(3)).await;
                            }
                        }
                    }
                    Some(publish) = mqtt_rx.recv() => {
                        match publish {
                            MqttPublish::PairingAck { code, ack } => {
                                let topic = format!("tinyiothub/pairing/{}/response", code);
                                if let Ok(payload) = serde_json::to_vec(&ack) {
                                    subscribe_client
                                        .publish(&topic, QoS::AtLeastOnce, false, payload)
                                        .await
                                        .ok();
                                    tracing::info!(code = %code, "Published pairing ack");
                                }
                            }
                        }
                    }
                }
            }
        });

        Self { client }
    }

    /// Parse topic and route to appropriate GatewayDataMessage variant.
    /// Topic format: tinyiothub/{ws_id}/gateway/{gw_id}/{category}
    ///           or: tinyiothub/{ws_id}/gateway/{gw_id}/device/{sub_id}/telemetry
    async fn route_data_message(
        topic: &str,
        payload: &[u8],
        data_tx: &mpsc::Sender<GatewayDataMessage>,
    ) {
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() < 5 {
            return;
        }
        // parts: ["tinyiothub", ws_id, "gateway", gw_id, ...]
        let workspace_id = parts[1].to_string();
        let gateway_id = parts[3].to_string();

        let msg = match parts.get(4).copied() {
            Some("status") => serde_json::from_slice::<StatusMessage>(payload)
                .ok()
                .map(|msg| GatewayDataMessage::Status { gateway_id, workspace_id, msg }),
            Some("telemetry") => serde_json::from_slice::<TelemetryMessage>(payload)
                .ok()
                .map(|msg| GatewayDataMessage::Telemetry { gateway_id, workspace_id, msg }),
            Some("event") => {
                // Events are logged but not yet handled by GatewayService
                tracing::debug!(gateway_id = %gateway_id, "Gateway event received (not yet handled)");
                None
            }
            Some("device") if parts.len() >= 7 && parts[5] == "discover" => {
                serde_json::from_slice::<DeviceDiscoverMessage>(payload)
                    .ok()
                    .map(|msg| GatewayDataMessage::DeviceDiscover { gateway_id, workspace_id, msg })
            }
            Some("device") if parts.len() >= 7 && parts[5] != "discover" => {
                let sub_id = parts[5].to_string();
                serde_json::from_slice::<DeviceTelemetryMessage>(payload).ok().map(|msg| {
                    GatewayDataMessage::DeviceTelemetry { gateway_id: sub_id, workspace_id, msg }
                })
            }
            _ => None,
        };

        if let Some(data_msg) = msg {
            let _ = data_tx.send(data_msg).await;
        }
    }

    pub async fn subscribe_gateway(&self, workspace_id: &str, device_id: &str) {
        let status = format!("tinyiothub/{}/gateway/{}/status", workspace_id, device_id);
        let telemetry = format!("tinyiothub/{}/gateway/{}/telemetry", workspace_id, device_id);
        let event = format!("tinyiothub/{}/gateway/{}/event", workspace_id, device_id);
        let discover = format!("tinyiothub/{}/gateway/{}/device/discover", workspace_id, device_id);
        let device_telemetry =
            format!("tinyiothub/{}/gateway/{}/device/+/telemetry", workspace_id, device_id);

        self.client.subscribe(&status, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&telemetry, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&event, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&discover, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&device_telemetry, QoS::AtMostOnce).await.ok();
    }
}
