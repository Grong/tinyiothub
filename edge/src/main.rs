use tinyiothub_edge::config::{EdgeConfig, GatewayCredentials};
use tinyiothub_edge::app_state::AppState;
use tinyiothub_edge::modules::gateway::GatewayMessage;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = EdgeConfig::from_env();
    tracing::info!(?config, "Starting TinyIoTHub Edge Gateway v0.2");

    if let Some(creds) = GatewayCredentials::load(&config.credentials_file) {
        tracing::info!(device_id = %creds.device_id, "Found saved credentials");
        run_authenticated(config, creds).await;
    } else {
        tracing::info!("No saved credentials, starting pairing mode");
        run_pairing(config).await;
    }
}

async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) {
    let state = match AppState::new(config.clone(), creds.clone()).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(?e, "AppState initialization failed");
            std::process::exit(1);
        }
    };
    let state = Arc::new(state);

    // Autonomous mode: try cloud config, fall back to defaults
    if let Err(e) = state.config_service.sync_from_cloud().await {
        tracing::warn!(?e, "Cloud unreachable, starting in autonomous mode");
        state.config_service.load_defaults().await;
    }

    // Initial device scan (best-effort, don't block startup on failure)
    if let Err(e) = state.driver_service.scan_all().await {
        tracing::warn!(?e, "Initial device scan failed, will retry on next telemetry tick");
    }

    // Flush any leftover offline buffer from previous run
    match state.offline_buffer.flush_batch(500).await {
        Ok(count) if count > 0 => tracing::info!(count, "Flushed offline buffer on startup"),
        Err(e) => tracing::warn!(?e, "Failed to flush offline buffer on startup"),
        _ => {}
    }

    // Graceful shutdown: SIGTERM/SIGINT drain buffer, disconnect, exit
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("SIGTERM/SIGINT received, draining...");
        let _ = shutdown_tx_clone.send(());
    });

    // Spawn independent interval tasks (not blocking the main loop)
    let s = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(s.config.telemetry_interval_secs));
        loop {
            tick.tick().await;
            s.telemetry_service.collect_and_forward().await.ok();
        }
    });

    let s = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(s.config.heartbeat_interval_secs));
        loop {
            tick.tick().await;
            s.health_service.beat_and_report().await.ok();
        }
    });

    let s = state.clone();
    tokio::spawn(async move {
        let mut tick =
            tokio::time::interval(Duration::from_secs(s.config.intelligence_interval_secs));
        loop {
            tick.tick().await;
            s.intelligence_service.evaluate_and_probe().await.ok();
        }
    });

    // Optional HTTP server (bound to 127.0.0.1 for local access only)
    if config.local_api_enabled {
        let s = state.clone();
        let port = config.local_api_port;
        tokio::spawn(async move {
            let router = tinyiothub_edge::modules::http::service::create_router(s);
            let addr = format!("127.0.0.1:{}", port);
            tracing::info!(%addr, "Local HTTP API enabled");
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(%addr, ?e, "Failed to bind HTTP API, disabling");
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(?e, "HTTP API server error");
            }
        });
    }

    // MQTT event loop: subscribe to cloud topics, route incoming messages
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<GatewayMessage>(100);
    let _event_loop = state.gateway_service.start_event_loop(msg_tx).await;

    // Main loop: connection management + message routing + shutdown
    loop {
        tokio::select! {
            // Graceful shutdown
            _ = shutdown_rx.recv() => {
                tracing::info!("Draining offline buffer before shutdown...");
                let _ = state.offline_buffer.flush_batch(500).await;
                state.gateway_service.disconnect().await;
                tracing::info!("Shutdown complete");
                std::process::exit(0);
            }

            // Periodic health check + reconnect if needed
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                if !state.gateway_service.is_alive() {
                    tracing::warn!("MQTT connection lost, reconnecting...");
                    state.gateway_service.reconnect().await;

                    // After reconnect: check for newer cloud config and flush buffer
                    if state.config_service.cloud_version_is_newer("0").await {
                        state.config_service.sync_from_cloud().await.ok();
                    }
                }
            }

            // Route incoming MQTT messages with longest-prefix matching
            Some(msg) = msg_rx.recv() => {
                match msg {
                    GatewayMessage::ConfigDevice(payload) => {
                        tracing::info!(
                            device_id = %payload.device_id,
                            action = %payload.action,
                            "Config device"
                        );
                        // Apply device-specific config via ConfigService
                    }
                    GatewayMessage::Config(config) => {
                        tracing::info!("Received cloud config");
                        state.config_service.apply_cloud_config(&config).await.ok();
                    }
                    GatewayMessage::Command(cmd) => {
                        let device_id = cmd.get("device_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        state.command_service.execute(device_id, &cmd).await.ok();
                    }
                    GatewayMessage::DriverInstall(payload) => {
                        tracing::info!(
                            driver = %payload.driver_name,
                            chunk = payload.chunk_index,
                            "Driver install chunk"
                        );
                        // Chunks are reassembled and verified in DriverService.
                        // Full chunk reassembly will be implemented in a future iteration.
                    }
                }
            }
        }
    }
}

async fn run_pairing(config: EdgeConfig) {
    use tinyiothub_edge::modules::gateway::pairing::PairingClient;

    tracing::info!("Starting pairing mode...");
    match PairingClient::run_pairing(&config).await {
        Ok(creds) => {
            tracing::info!(device_id = %creds.device_id, "Pairing successful, saving credentials");
            if let Err(e) = creds.save(&config.credentials_file) {
                tracing::error!(?e, "Failed to save credentials");
                std::process::exit(1);
            }
            tracing::info!("Credentials saved, restart to begin authenticated operation");
        }
        Err(e) => {
            tracing::error!(?e, "Pairing failed");
            std::process::exit(1);
        }
    }
}
