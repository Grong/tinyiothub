mod config;
mod device_discovery;
mod mqtt_client;
mod pairing;

use config::{EdgeConfig, GatewayCredentials};
use device_discovery::DeviceScanner;
use mqtt_client::{EdgeMqttClient, MqttEvent};
use pairing::PairingCodeGenerator;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = EdgeConfig::from_env();
    tracing::info!(?config, "Starting TinyIoTHub Edge Gateway");

    if let Some(creds) = GatewayCredentials::load(&config.credentials_file) {
        tracing::info!(device_id = %creds.device_id, "Found saved credentials");
        run_authenticated(config, creds).await;
    } else {
        tracing::info!("No saved credentials, starting pairing mode");
        run_pairing(config).await;
    }
}

async fn run_pairing(config: EdgeConfig) {
    let (event_tx, mut event_rx) = mpsc::channel::<MqttEvent>(100);
    let mqtt = EdgeMqttClient::new_anonymous(&config, event_tx);
    let mut code_gen = PairingCodeGenerator::new();

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let fingerprint = get_fingerprint();
    let ip = local_ip();

    loop {
        let code = code_gen.get_code().to_string();
        let display = PairingCodeGenerator::display_format(&code);
        println!("═══════════════════════════════════");
        println!("  Pairing Code: {}", display);
        println!("═══════════════════════════════════");

        let announce = serde_json::json!({
            "type": "pairing_announce",
            "code": code,
            "fingerprint": fingerprint,
            "hostname": hostname,
            "os": std::env::consts::OS,
            "ip": ip,
            "hw_model": "edge-gateway"
        });

        mqtt.publish_announce(serde_json::to_string(&announce).unwrap().as_bytes())
            .await;

        let deadline = tokio::time::sleep(Duration::from_secs(config.pairing_interval_secs));
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    if let MqttEvent::PairingAck(ack) = event {
                        if ack.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                            tracing::info!("Pairing successful!");
                            let creds = GatewayCredentials {
                                device_id: ack["device_id"].as_str().unwrap_or_default().to_string(),
                                client_id: ack["credentials"]["client_id"].as_str().unwrap_or_default().to_string(),
                                username: ack["credentials"]["username"].as_str().unwrap_or_default().to_string(),
                                password: ack["credentials"]["password"].as_str().unwrap_or_default().to_string(),
                                workspace_id: ack["workspace_id"].as_str().unwrap_or_default().to_string(),
                            };
                            if let Err(e) = creds.save(&config.credentials_file) {
                                tracing::error!(?e, "Failed to save credentials");
                            }
                            run_authenticated(config, creds).await;
                            return;
                        }
                    }
                }
                _ = &mut deadline => { break; }
            }
        }
    }
}

async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) {
    let (event_tx, mut event_rx) = mpsc::channel::<MqttEvent>(100);
    let mqtt = EdgeMqttClient::new_authenticated(&creds, &config, event_tx);
    let scanner = DeviceScanner::new();

    let command_topic = format!(
        "tinyiothub/{}/gateway/{}/command",
        creds.workspace_id, creds.device_id
    );
    let config_topic = format!(
        "tinyiothub/{}/gateway/{}/config",
        creds.workspace_id, creds.device_id
    );
    mqtt.subscribe_topics(&command_topic, &config_topic).await;

    let discover_topic = format!(
        "tinyiothub/{}/gateway/{}/device/discover",
        creds.workspace_id, creds.device_id
    );
    let devices = scanner.scan().await;
    if !devices.is_empty() {
        let msg = device_discovery::DeviceDiscoverMessage::new(devices);
        if let Ok(payload) = serde_json::to_string(&msg) {
            mqtt.publish_discovery(&discover_topic, payload.as_bytes())
                .await;
        }
    }

    let mut heartbeat = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));
    let status_topic = format!(
        "tinyiothub/{}/gateway/{}/status",
        creds.workspace_id, creds.device_id
    );

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let status = serde_json::json!({
                    "type": "status", "status": "online",
                    "uptime": 0u64,
                    "timestamp": chrono::Utc::now().timestamp(),
                });
                mqtt.publish_status(&status_topic, serde_json::to_string(&status).unwrap().as_bytes()).await;
            }
            Some(event) = event_rx.recv() => {
                match event {
                    MqttEvent::Command(cmd) => tracing::info!(?cmd, "Received command"),
                    MqttEvent::Config(cfg) => tracing::info!(?cfg, "Received config update"),
                    _ => {}
                }
            }
        }
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
