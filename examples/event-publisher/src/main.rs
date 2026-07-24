//! Minimal MQTT event publisher for TinyIoTHub.
//!
//! Reads config from environment variables:
//!   MQTT_HOST (default: localhost)
//!   MQTT_PORT (default: 1883)
//!   THING_ID  (required)
//!   EVENT_NAME (default: status_update)
//!   EVENT_LEVEL (default: info)
//!   EVENT_DATA (optional JSON object, default: {"status":"ok"})

use rumqttc::{Client, MqttOptions, QoS};
use serde::Serialize;

#[derive(Serialize)]
struct EventPayload {
    level: String,
    data: serde_json::Value,
    ts: String,
}

fn main() {
    let host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1883);
    let thing_id = std::env::var("THING_ID").expect("THING_ID env var is required");
    let event_name = std::env::var("EVENT_NAME").unwrap_or_else(|_| "status_update".to_string());
    let level = std::env::var("EVENT_LEVEL").unwrap_or_else(|_| "info".to_string());
    let data: serde_json::Value = std::env::var("EVENT_DATA")
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_else(|| serde_json::json!({"status": "ok"}));

    let ts = chrono::Utc::now().to_rfc3339();

    let payload = EventPayload { level, data, ts };

    let topic = format!("thing/{}/event/{}", thing_id, event_name);
    let body = serde_json::to_string(&payload).expect("failed to serialize payload");

    let mut mqttoptions = MqttOptions::new("event-publisher", &host, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

    let (client, mut connection) = Client::new(mqttoptions, 10);

    // Spawn network loop
    std::thread::spawn(move || {
        for msg in connection.iter() {
            match msg {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                    eprintln!("[connected]");
                }
                Err(e) => {
                    eprintln!("[mqtt error] {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    client
        .publish(&topic, QoS::AtLeastOnce, false, body.as_bytes())
        .expect("failed to publish");

    eprintln!("[published] {} -> {}", topic, body);

    // Let the publish complete
    std::thread::sleep(std::time::Duration::from_millis(500));
}
