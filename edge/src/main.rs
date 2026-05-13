mod device_discovery;
mod pairing;

use tinyiothub_edge::config::{EdgeConfig, GatewayCredentials};
use tinyiothub_edge::modules::gateway::{GatewayMessage, GatewayService};
use device_discovery::DeviceScanner;
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
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let fingerprint = get_fingerprint();
    let ip = local_ip();

    loop {
        let mut code_gen = PairingCodeGenerator::new();

        let code = code_gen.get_code().to_string();
        let display = PairingCodeGenerator::display_format(&code);
        println!("═══════════════════════════════════");
        println!("  Pairing Code: {}", display);
        println!("═══════════════════════════════════");

        let _announce = serde_json::json!({
            "type": "pairing_announce",
            "code": code,
            "fingerprint": fingerprint,
            "hostname": hostname,
            "os": std::env::consts::OS,
            "ip": ip,
            "hw_model": "edge-gateway"
        });

        tracing::info!(code = %code, "Pairing announce (MQTT not yet wired — Task 11)");

        // Try pairing via PairingClient (stub — returns error until Task 11)
        match tinyiothub_edge::modules::gateway::pairing::PairingClient::run_pairing(&config).await
        {
            Ok(creds) => {
                tracing::info!("Pairing successful!");
                if let Err(e) = creds.validate() {
                    tracing::error!(?e, device_id = %creds.device_id, "Invalid credentials from pairing");
                } else if let Err(e) = creds.save(&config.credentials_file) {
                    tracing::error!(?e, "Failed to save credentials");
                } else {
                    run_authenticated(config, creds).await;
                    return;
                }
            }
            Err(e) => {
                tracing::debug!(?e, "Pairing not yet available, retrying...");
            }
        }

        tokio::time::sleep(Duration::from_secs(config.pairing_interval_secs)).await;
    }
}

async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) {
    let scanner = DeviceScanner::new();

    // Initial device discovery
    let devices = scanner.scan().await;
    if !devices.is_empty() {
        let gw = GatewayService::new(&creds, &config);
        let msg = device_discovery::DeviceDiscoverMessage::new(devices);
        if let Ok(payload) = serde_json::to_string(&msg) {
            gw.publish_discovery(payload.as_bytes()).await.ok();
        }
    }

    loop {
        let (event_tx, mut event_rx) = mpsc::channel::<GatewayMessage>(100);
        let gw = GatewayService::new(&creds, &config);
        let _join_handle = gw.start_event_loop(event_tx).await;

        let mut heartbeat = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));

        loop {
            if !gw.is_alive() {
                tracing::error!("Gateway service not alive, recreating...");
                break;
            }
            tokio::select! {
                _ = heartbeat.tick() => {
                    let status = serde_json::json!({
                        "type": "status", "status": "online",
                        "uptime": 0u64,
                        "timestamp": chrono::Utc::now().timestamp(),
                    });
                    let _ = gw.publish_status(
                        serde_json::to_string(&status).unwrap().as_bytes(),
                    ).await;
                }
                Some(event) = event_rx.recv() => {
                    match event {
                        GatewayMessage::Command(cmd) => tracing::info!(
                            action = "command_received",
                            device_id = %creds.device_id,
                            command = ?cmd,
                        ),
                        GatewayMessage::Config(cfg) => tracing::info!(
                            action = "config_received",
                            device_id = %creds.device_id,
                            config = ?cfg,
                        ),
                        GatewayMessage::ConfigDevice(cfg) => tracing::info!(
                            action = "config_device_received",
                            device_id = %creds.device_id,
                            config = ?cfg,
                        ),
                        GatewayMessage::DriverInstall(di) => tracing::info!(
                            action = "driver_install_received",
                            device_id = %creds.device_id,
                            driver = %di.driver_name,
                        ),
                    }
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
