use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::modules::gateway::service::MqttPublish;
use crate::modules::gateway::types::PairingAnnounce;

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
    ) -> Self {
        let broker_url = broker_url.to_string();
        let username = username.to_string();
        let password = password.to_string();
        let client_id = format!("tinyiothub-platform-{}", uuid::Uuid::new_v4());
        let mut options = MqttOptions::new(&client_id, &broker_url, broker_port);
        options.set_credentials(&username, &password);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_max_packet_size(256 * 1024, 256 * 1024);

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let subscribe_client = client.clone();

        tokio::spawn(async move {
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
                            }
                            Ok(Event::Incoming(Packet::Publish(publish))) => {
                                if publish.topic == "tinyiothub/pairing/announce" {
                                    match serde_json::from_slice::<PairingAnnounce>(&publish.payload) {
                                        Ok(announce) => {
                                            let _ = announce_tx.send(announce).await;
                                        }
                                        Err(e) => {
                                            tracing::warn!(?e, "Failed to parse pairing announce");
                                        }
                                    }
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

    pub async fn subscribe_gateway(&self, workspace_id: &str, device_id: &str) {
        let status = format!("tinyiothub/{}/gateway/{}/status", workspace_id, device_id);
        let telemetry = format!("tinyiothub/{}/gateway/{}/telemetry", workspace_id, device_id);
        let event = format!("tinyiothub/{}/gateway/{}/event", workspace_id, device_id);
        let discover = format!(
            "tinyiothub/{}/gateway/{}/device/discover",
            workspace_id, device_id
        );
        let device_telemetry = format!(
            "tinyiothub/{}/gateway/{}/device/+/telemetry",
            workspace_id, device_id
        );

        self.client.subscribe(&status, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&telemetry, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&event, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&discover, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&device_telemetry, QoS::AtMostOnce).await.ok();
    }
}
