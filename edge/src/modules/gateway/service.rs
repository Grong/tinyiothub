use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::{EdgeConfig, GatewayCredentials};
use super::types::GatewayMessage;

pub struct GatewayService {
    client: rumqttc::AsyncClient,
    config: EdgeConfig,
    credentials: GatewayCredentials,
    event_loop_abort: tokio::sync::Mutex<Option<tokio::task::AbortHandle>>,
}

impl GatewayService {
    pub fn new(credentials: &GatewayCredentials, config: &EdgeConfig) -> Arc<Self> {
        let mut options = rumqttc::MqttOptions::new(
            &credentials.client_id,
            &config.mqtt_broker,
            config.mqtt_port,
        );
        options.set_credentials(&credentials.username, &credentials.password);
        options.set_keep_alive(Duration::from_secs(60));
        let (client, _eventloop) = rumqttc::AsyncClient::new(options, 100);
        Arc::new(Self {
            client,
            config: config.clone(),
            credentials: credentials.clone(),
            event_loop_abort: tokio::sync::Mutex::new(None),
        })
    }

    pub fn credentials(&self) -> &GatewayCredentials {
        &self.credentials
    }

    /// Start the MQTT event loop, returns JoinHandle.
    /// Subscribes to topics with longest-prefix-first ordering
    /// (e.g. /config/device before /config).
    pub async fn start_event_loop(
        self: &Arc<Self>,
        tx: mpsc::Sender<GatewayMessage>,
    ) -> tokio::task::JoinHandle<()> {
        let mut options = rumqttc::MqttOptions::new(
            &self.credentials.client_id,
            &self.config.mqtt_broker,
            self.config.mqtt_port,
        );
        options.set_credentials(&self.credentials.username, &self.credentials.password);
        options.set_keep_alive(Duration::from_secs(60));
        let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 100);

        let ws_id = self.credentials.workspace_id.clone();
        let dev_id = self.credentials.device_id.clone();

        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                        tracing::info!("MQTT connected (authenticated)");
                        let prefix = format!("tinyiothub/{}/gateway/{}", ws_id, dev_id);
                        // Subscribe longer prefixes before shorter ones
                        client
                            .subscribe(
                                &format!("{}/config/device", prefix),
                                rumqttc::QoS::AtLeastOnce,
                            )
                            .await
                            .ok();
                        client
                            .subscribe(
                                &format!("{}/driver/install", prefix),
                                rumqttc::QoS::AtLeastOnce,
                            )
                            .await
                            .ok();
                        client
                            .subscribe(&format!("{}/config", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                        client
                            .subscribe(&format!("{}/command", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        if let Ok(msg) =
                            GatewayMessage::from_topic_payload(&publish.topic, &publish.payload)
                        {
                            let _ = tx.send(msg).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(?e, "MQTT event loop error, reconnecting...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    _ => {}
                }
            }
        });

        let mut guard = self.event_loop_abort.lock().await;
        *guard = Some(handle.abort_handle());
        handle
    }

    pub fn topic_prefix(&self) -> String {
        format!(
            "tinyiothub/{}/gateway/{}",
            self.credentials.workspace_id, self.credentials.device_id
        )
    }

    pub async fn publish_status(
        &self,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .publish(
                &format!("{}/status", self.topic_prefix()),
                rumqttc::QoS::AtMostOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_telemetry(
        &self,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .publish(
                &format!("{}/telemetry", self.topic_prefix()),
                rumqttc::QoS::AtMostOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_event(
        &self,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .publish(
                &format!("{}/event", self.topic_prefix()),
                rumqttc::QoS::AtLeastOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_discovery(
        &self,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .publish(
                &format!("{}/discovery", self.topic_prefix()),
                rumqttc::QoS::AtLeastOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub fn is_alive(&self) -> bool {
        // The rumqttc event loop handles reconnection internally;
        // stub for now — Task 11 will wire real health checks.
        true
    }

    pub async fn reconnect(&self) {
        // The rumqttc event loop handles reconnection internally
    }

    pub async fn disconnect(&self) {
        self.client.disconnect().await.ok();
    }
}
