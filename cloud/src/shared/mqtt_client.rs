use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

use tinyiothub_event::{
    bus::ThingEventBus,
    router::{ThingEventInput, ThingEventPayload, ThrottleState, route_thing_event},
};

use tinyiothub_driver::gateway::{
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker_url: &str,
        broker_port: u16,
        username: &str,
        password: &str,
        announce_tx: mpsc::Sender<PairingAnnounce>,
        mut mqtt_rx: mpsc::Receiver<MqttPublish>,
        data_tx: mpsc::Sender<GatewayDataMessage>,
        throttle: Arc<ThrottleState>,
        event_bus: Arc<ThingEventBus>,
        db_pool: sqlx::SqlitePool,
        alarm_service: Option<Arc<tinyiothub_alarm::AlarmService>>,
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
                                // Thing event topic
                                subscribe_client
                                    .subscribe("thing/+/event/+", QoS::AtLeastOnce)
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
                                } else if topic.starts_with("thing/") && topic.contains("/event/") {
                                    // Handle thing/+/event/+ topic
                                    Self::route_thing_event_message(
                                        &topic,
                                        &publish.payload,
                                        &throttle,
                                        &event_bus,
                                        &db_pool,
                                        alarm_service.clone(),
                                    )
                                    .await;
                                } else {
                                    // Route gateway data messages by topic pattern
                                    Self::route_data_message(&topic, &publish.payload, &data_tx).await;
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::debug!(?e, "Platform MQTT event loop error, reconnecting...");
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

    /// Parse a `thing/{thing_id}/event/{event_name}` topic, deserialize the
    /// payload, and route it through the thing event pipeline.
    async fn route_thing_event_message(
        topic: &str,
        payload: &[u8],
        throttle: &ThrottleState,
        event_bus: &ThingEventBus,
        db_pool: &sqlx::SqlitePool,
        alarm_service: Option<Arc<tinyiothub_alarm::AlarmService>>,
    ) {
        // Parse topic: thing/{thing_id}/event/{event_name}
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() < 4 {
            tracing::warn!(topic = %topic, "Malformed thing event topic");
            return;
        }
        let thing_id = parts[1].to_string();
        let event_name = parts[3].to_string();

        // Parse payload JSON
        let payload_data: ThingEventPayload = match serde_json::from_slice(payload) {
            Ok(p) => p,
            Err(e) => {
                let preview: String = String::from_utf8_lossy(payload).chars().take(200).collect();
                tracing::warn!(
                    topic = %topic,
                    thing_id = %thing_id,
                    event_name = %event_name,
                    error = %e,
                    payload_preview = %preview,
                    metric = "events_malformed",
                    "Malformed thing event payload"
                );
                return;
            }
        };

        // Map level string to EventLevel
        let level = match payload_data.level.to_lowercase().as_str() {
            "critical" => tinyiothub_core::models::event::EventLevel::Critical,
            "error" => tinyiothub_core::models::event::EventLevel::Error,
            "warning" => tinyiothub_core::models::event::EventLevel::Warning,
            "info" => tinyiothub_core::models::event::EventLevel::Info,
            _ => {
                tracing::warn!(
                    thing_id = %thing_id,
                    level = %payload_data.level,
                    "Unknown event level, defaulting to Info"
                );
                tinyiothub_core::models::event::EventLevel::Info
            }
        };

        // Resolve tenant scope AND the template event definitions in ONE
        // round trip (perf review: was two separate queries per event on the
        // ingest hot path).
        let thing_row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT d.workspace_id, t.events FROM devices d \
             LEFT JOIN thing_templates t ON t.id = d.template_id WHERE d.id = ?",
        )
        .bind(&thing_id)
        .fetch_optional(db_pool)
        .await
        .ok()
        .flatten();
        let workspace_id: String =
            thing_row.as_ref().and_then(|(ws, _)| ws.clone()).unwrap_or_default();
        let template_events = thing_row.and_then(|(_, ev)| ev);
        if workspace_id.is_empty() {
            tracing::warn!(
                thing_id = %thing_id,
                event_name = %event_name,
                metric = "events_malformed",
                "Thing event dropped: unknown thing or no workspace"
            );
            return;
        }

        let input = ThingEventInput {
            thing_id: thing_id.clone(),
            workspace_id,
            event_name: event_name.clone(),
            level,
            data: payload_data.data,
            ts: payload_data.ts,
            template_events,
        };

        // MQTT-ingested events are device-reported: actor "device" (T6).
        let alarm_hook: Option<Arc<dyn tinyiothub_event::router::EventAlarmHook>> =
            alarm_service.map(|svc| svc as Arc<dyn tinyiothub_event::router::EventAlarmHook>);
        let result =
            route_thing_event(db_pool, throttle, alarm_hook, event_bus, "device", input).await;

        if result.throttled {
            tracing::info!(
                thing_id = %thing_id,
                event_name = %event_name,
                "Thing event throttled"
            );
        } else if result.malformed {
            tracing::warn!(
                thing_id = %thing_id,
                event_name = %event_name,
                "Thing event rejected (malformed or persist failure)"
            );
        } else {
            tracing::info!(
                event_id = %result.event_id,
                thing_id = %thing_id,
                event_name = %event_name,
                "Thing event routed successfully"
            );
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
