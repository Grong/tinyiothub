use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::{EdgeConfig, GatewayCredentials};

pub struct PairingClient;

impl PairingClient {
    /// Run the full MQTT pairing flow:
    /// 1. Connect anonymously to the MQTT broker
    /// 2. Generate a 6-digit pairing code
    /// 3. Broadcast announce messages on `tinyiothub/pairing/announce`
    /// 4. Subscribe to `tinyiothub/pairing/+/response`
    /// 5. Wait for a PairingAck with success=true
    /// 6. Validate and return credentials
    pub async fn run_pairing(config: &EdgeConfig) -> Result<GatewayCredentials, Box<dyn std::error::Error>> {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".into());
        let fingerprint = get_fingerprint();
        let ip = local_ip();
        let pairing_code = generate_code();

        let announce = serde_json::json!({
            "type": "pairing_announce",
            "code": pairing_code,
            "fingerprint": fingerprint,
            "hostname": hostname,
            "os": std::env::consts::OS,
            "ip": ip,
            "hw_model": "edge-gateway"
        });
        let announce_payload = serde_json::to_vec(&announce)?;

        let (event_tx, mut event_rx) = mpsc::channel::<PairingEvent>(100);

        // Connect anonymously and subscribe to pairing responses
        let broker = config.mqtt_broker.clone();
        let port = config.mqtt_port;
        let client_id = format!("edge-pairing-{}", uuid::Uuid::new_v4());
        let mut options = rumqttc::MqttOptions::new(&client_id, &broker, port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 100);
        let sub_client = client.clone();
        let announce_client = client.clone();
        let announce_data = announce_payload;
        let announce_interval = config.pairing_interval_secs;

        // Spawn periodic announce
        let announce_handle = tokio::spawn(async move {
            loop {
                announce_client
                    .publish(
                        "tinyiothub/pairing/announce",
                        rumqttc::QoS::AtLeastOnce,
                        false,
                        announce_data.clone(),
                    )
                    .await
                    .ok();
                tokio::time::sleep(Duration::from_secs(announce_interval)).await;
            }
        });

        // Spawn event loop
        let event_handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                        tracing::info!("Pairing MQTT connected (anonymous)");
                        sub_client
                            .subscribe("tinyiothub/pairing/+/response", rumqttc::QoS::AtLeastOnce)
                            .await
                            .ok();
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        if publish.topic.starts_with("tinyiothub/pairing/")
                            && publish.topic.ends_with("/response")
                            && let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&publish.payload)
                        {
                            let _ = event_tx.send(PairingEvent::Ack(msg)).await;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(?e, "Pairing MQTT error, retrying...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // Display the pairing code
        let display = format_code(&pairing_code);
        println!("═══════════════════════════════════");
        println!("  Pairing Code: {}", display);
        println!("═══════════════════════════════════");

        // Wait for successful pairing ack
        let timeout = Duration::from_secs(300); // 5-minute pairing window
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    let PairingEvent::Ack(ack) = event;
                    if ack.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                        tracing::info!("Pairing successful!");
                        announce_handle.abort();
                        event_handle.abort();

                        let creds = parse_pairing_ack(&ack)?;
                        creds.validate()?;
                        return Ok(creds);
                    }
                }
                _ = &mut deadline => {
                    announce_handle.abort();
                    event_handle.abort();
                    return Err("Pairing timed out after 5 minutes".into());
                }
            }
        }
    }
}

enum PairingEvent {
    Ack(serde_json::Value),
}

/// Parse a successful pairing ack into gateway credentials.
///
/// `thing_id` is required: a missing key must fail loudly rather than
/// silently yield empty credentials (co-upgrade contract with cloud's
/// `PairingAck`).
fn parse_pairing_ack(ack: &serde_json::Value) -> Result<GatewayCredentials, String> {
    let thing_id = ack["thing_id"]
        .as_str()
        .ok_or_else(|| "pairing ack missing required field: thing_id".to_string())?;
    Ok(GatewayCredentials {
        thing_id: thing_id.to_string(),
        client_id: ack["credentials"]["client_id"].as_str().unwrap_or_default().to_string(),
        username: ack["credentials"]["username"].as_str().unwrap_or_default().to_string(),
        password: ack["credentials"]["password"].as_str().unwrap_or_default().to_string(),
        workspace_id: ack["workspace_id"].as_str().unwrap_or_default().to_string(),
    })
}

fn generate_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

fn format_code(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    if chars.len() == 6 {
        format!(
            "{} {} - {} {} - {} {}",
            chars[0], chars[1], chars[2], chars[3], chars[4], chars[5]
        )
    } else {
        code.to_string()
    }
}

fn get_fingerprint() -> String {
    mac_address::get_mac_address()
        .ok()
        .flatten()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_ack() -> serde_json::Value {
        serde_json::json!({
            "type": "pairing_ack",
            "success": true,
            "thing_id": "gw-1",
            "workspace_id": "ws-1",
            "credentials": {
                "client_id": "gw-1-client",
                "username": "u",
                "password": "p"
            },
            "topics": {
                "status": "t/status",
                "telemetry": "t/telemetry",
                "event": "t/event",
                "command": "t/command",
                "config": "t/config",
                "thing_discover": "t/discover",
                "thing_telemetry": "t/thing-telemetry"
            },
            "keepalive": 60
        })
    }

    #[test]
    fn pairing_ack_parses_thing_id() {
        let parsed = parse_pairing_ack(&full_ack()).unwrap();
        assert_eq!(parsed.thing_id, "gw-1");
        assert_eq!(parsed.workspace_id, "ws-1");
        assert_eq!(parsed.client_id, "gw-1-client");
        assert_eq!(parsed.username, "u");
        assert_eq!(parsed.password, "p");
    }

    #[test]
    fn pairing_ack_requires_thing_id() {
        let mut ack = full_ack();
        ack.as_object_mut().unwrap().remove("thing_id");
        let err = parse_pairing_ack(&ack).unwrap_err();
        assert!(err.contains("thing_id"), "error should name thing_id: {err}");
    }
}
