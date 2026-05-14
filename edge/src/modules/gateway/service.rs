use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};

use super::types::GatewayMessage;
use crate::config::{EdgeConfig, GatewayCredentials};
use crate::shared::error::EdgeResult;

fn build_mqtt_options(credentials: &GatewayCredentials, config: &EdgeConfig) -> rumqttc::MqttOptions {
    let mut options = rumqttc::MqttOptions::new(&credentials.client_id, &config.mqtt_broker, config.mqtt_port);
    options.set_credentials(&credentials.username, &credentials.password);
    options.set_keep_alive(Duration::from_secs(60));
    options
}

pub struct GatewayService {
    client: RwLock<rumqttc::AsyncClient>,
    eventloop: Mutex<Option<rumqttc::EventLoop>>,
    config: EdgeConfig,
    credentials: GatewayCredentials,
    event_loop_abort: Mutex<Option<tokio::task::AbortHandle>>,
    last_heard: Arc<AtomicI64>,
}

impl GatewayService {
    /// Create a new GatewayService with a single (AsyncClient, EventLoop) pair.
    pub fn new(credentials: &GatewayCredentials, config: &EdgeConfig) -> Arc<Self> {
        let options = build_mqtt_options(credentials, config);
        let (client, eventloop) = rumqttc::AsyncClient::new(options, 100);
        Arc::new(Self {
            client: RwLock::new(client),
            eventloop: Mutex::new(Some(eventloop)),
            config: config.clone(),
            credentials: credentials.clone(),
            event_loop_abort: Mutex::new(None),
            last_heard: Arc::new(AtomicI64::new(chrono::Utc::now().timestamp())),
        })
    }

    pub fn credentials(&self) -> &GatewayCredentials {
        &self.credentials
    }

    /// Take the stored EventLoop and spawn a task to poll it.
    /// Subscribes on the same AsyncClient (cloned) used for publishing.
    pub async fn start_event_loop(self: &Arc<Self>, tx: mpsc::Sender<GatewayMessage>) -> tokio::task::JoinHandle<()> {
        let mut eventloop = self
            .eventloop
            .lock()
            .await
            .take()
            .expect("start_event_loop called but no eventloop available");

        let sub_client = self.client.read().await.clone();
        let ws_id = self.credentials.workspace_id.clone();
        let dev_id = self.credentials.device_id.clone();
        let last_heard = self.last_heard.clone();

        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                        last_heard.store(chrono::Utc::now().timestamp(), Ordering::Release);
                        tracing::info!("MQTT connected (authenticated)");
                        let prefix = format!("tinyiothub/{}/gateway/{}", ws_id, dev_id);
                        // Subscribe longer prefixes before shorter ones
                        sub_client
                            .subscribe(&format!("{}/config/device", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                        sub_client
                            .subscribe(&format!("{}/driver/install", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                        sub_client
                            .subscribe(&format!("{}/config", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                        sub_client
                            .subscribe(&format!("{}/command", prefix), rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        last_heard.store(chrono::Utc::now().timestamp(), Ordering::Release);
                        if let Ok(msg) = GatewayMessage::from_topic_payload(&publish.topic, &publish.payload) {
                            let _ = tx.send(msg).await;
                        }
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::PingResp)) => {
                        last_heard.store(chrono::Utc::now().timestamp(), Ordering::Release);
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

    pub async fn publish_status(&self, payload: &[u8]) -> EdgeResult<()> {
        self.client
            .read()
            .await
            .publish(
                &format!("{}/status", self.topic_prefix()),
                rumqttc::QoS::AtMostOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_telemetry(&self, payload: &[u8]) -> EdgeResult<()> {
        self.client
            .read()
            .await
            .publish(
                &format!("{}/telemetry", self.topic_prefix()),
                rumqttc::QoS::AtMostOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_event(&self, payload: &[u8]) -> EdgeResult<()> {
        self.client
            .read()
            .await
            .publish(
                &format!("{}/event", self.topic_prefix()),
                rumqttc::QoS::AtLeastOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn publish_discovery(&self, payload: &[u8]) -> EdgeResult<()> {
        self.client
            .read()
            .await
            .publish(
                &format!("{}/discovery", self.topic_prefix()),
                rumqttc::QoS::AtLeastOnce,
                false,
                payload,
            )
            .await
            .map_err(|e| e.to_string().into())
    }

    /// Check if the MQTT connection is alive.
    /// Returns true if we've heard from the broker within 90 seconds
    /// (1.5x the 60s keep-alive).
    pub fn is_alive(&self) -> bool {
        let last = self.last_heard.load(Ordering::Acquire);
        let now = chrono::Utc::now().timestamp();
        (now - last) < 90
    }

    /// Abort the old event loop, create a fresh MQTT connection, and restart.
    /// Returns the new JoinHandle so the caller can update the watchdog.
    pub async fn reconnect(self: &Arc<Self>, tx: mpsc::Sender<GatewayMessage>) -> tokio::task::JoinHandle<()> {
        if let Some(handle) = self.event_loop_abort.lock().await.take() {
            handle.abort();
        }

        // Create a fresh MQTT connection pair
        let options = build_mqtt_options(&self.credentials, &self.config);
        let (new_client, new_eventloop) = rumqttc::AsyncClient::new(options, 100);

        *self.client.write().await = new_client;
        *self.eventloop.lock().await = Some(new_eventloop);

        tracing::info!("MQTT connection recreated, restarting event loop");
        self.start_event_loop(tx).await
    }

    /// Publish to an arbitrary topic. Used by offline buffer flush.
    pub async fn publish_raw(&self, topic: &str, payload: Vec<u8>) -> EdgeResult<()> {
        self.client
            .read()
            .await
            .publish(topic, rumqttc::QoS::AtLeastOnce, false, payload)
            .await
            .map_err(|e| e.to_string().into())
    }

    pub async fn disconnect(&self) {
        self.client.read().await.disconnect().await.ok();
    }
}
