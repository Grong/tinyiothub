use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc;

use tinyiothub_edge::config::{EdgeConfig, GatewayCredentials};

pub struct EdgeMqttClient {
    client: AsyncClient,
    event_loop_handle: tokio::task::JoinHandle<()>,
}

impl EdgeMqttClient {
    pub fn is_event_loop_alive(&self) -> bool {
        !self.event_loop_handle.is_finished()
    }
}

pub enum MqttEvent {
    PairingAck(serde_json::Value),
    Command(serde_json::Value),
    Config(serde_json::Value),
}

impl EdgeMqttClient {
    pub fn new_anonymous(config: &EdgeConfig, event_tx: mpsc::Sender<MqttEvent>) -> Self {
        let broker = config.mqtt_broker.clone();
        let port = config.mqtt_port;
        let client_id = format!("edge-pairing-{}", uuid::Uuid::new_v4());
        let mut options = MqttOptions::new(&client_id, &broker, port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let sub_client = client.clone();

        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!("Edge MQTT connected (anonymous)");
                        sub_client
                            .subscribe("tinyiothub/pairing/+/response", QoS::AtLeastOnce)
                            .await
                            .ok();
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if publish.topic.starts_with("tinyiothub/pairing/")
                            && publish.topic.ends_with("/response")
                        {
                            if let Ok(msg) =
                                serde_json::from_slice::<serde_json::Value>(&publish.payload)
                            {
                                let _ = event_tx.send(MqttEvent::PairingAck(msg)).await;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(?e, "Edge MQTT error (anonymous), retrying...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Self {
            client,
            event_loop_handle: handle,
        }
    }

    pub fn new_authenticated(
        credentials: &GatewayCredentials,
        config: &EdgeConfig,
        event_tx: mpsc::Sender<MqttEvent>,
    ) -> Self {
        let broker = config.mqtt_broker.clone();
        let port = config.mqtt_port;
        let mut options = MqttOptions::new(&credentials.client_id, &broker, port);
        options.set_credentials(&credentials.username, &credentials.password);
        options.set_keep_alive(Duration::from_secs(60));

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let sub_client = client.clone();
        let ws_id = credentials.workspace_id.clone();
        let dev_id = credentials.device_id.clone();

        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!("Edge MQTT connected (authenticated)");
                        sub_client
                            .subscribe(
                                &format!("tinyiothub/{}/gateway/{}/command", ws_id, dev_id),
                                QoS::AtLeastOnce,
                            )
                            .await
                            .ok();
                        sub_client
                            .subscribe(
                                &format!("tinyiothub/{}/gateway/{}/config", ws_id, dev_id),
                                QoS::AtLeastOnce,
                            )
                            .await
                            .ok();
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if publish.topic.contains("/command") {
                            if let Ok(msg) =
                                serde_json::from_slice::<serde_json::Value>(&publish.payload)
                            {
                                let _ = event_tx.send(MqttEvent::Command(msg)).await;
                            }
                        } else if publish.topic.contains("/config") {
                            if let Ok(msg) = serde_json::from_slice(&publish.payload) {
                                let _ = event_tx.send(MqttEvent::Config(msg)).await;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(?e, "Edge MQTT error (authenticated), retrying...");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Self {
            client,
            event_loop_handle: handle,
        }
    }

    pub async fn publish_announce(&self, payload: &[u8]) {
        self.client
            .publish(
                "tinyiothub/pairing/announce",
                QoS::AtLeastOnce,
                false,
                payload,
            )
            .await
            .ok();
    }

    pub async fn publish_status(&self, topic: &str, payload: &[u8]) {
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload)
            .await
            .ok();
    }

    pub async fn publish_telemetry(&self, topic: &str, payload: &[u8]) {
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload)
            .await
            .ok();
    }

    pub async fn publish_event(&self, topic: &str, payload: &[u8]) {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .ok();
    }

    pub async fn publish_discovery(&self, topic: &str, payload: &[u8]) {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .ok();
    }

    pub async fn subscribe_topics(&self, command_topic: &str, config_topic: &str) {
        self.client
            .subscribe(command_topic, QoS::AtLeastOnce)
            .await
            .ok();
        self.client
            .subscribe(config_topic, QoS::AtLeastOnce)
            .await
            .ok();
    }
}
