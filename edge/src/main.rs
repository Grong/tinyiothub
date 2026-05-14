use std::sync::Arc;
use std::time::Duration;
use tinyiothub_edge::app_state::AppState;
use tinyiothub_edge::config::{EdgeConfig, GatewayCredentials};
use tinyiothub_edge::modules::gateway::GatewayMessage;
use tokio::task::JoinHandle;

/// Holds JoinHandles for the 5 long-running background tasks.
/// The main loop watchdog checks for panics and restarts dead tasks.
struct TaskHandles {
    telemetry: Option<JoinHandle<()>>,
    heartbeat: Option<JoinHandle<()>>,
    intelligence: Option<JoinHandle<()>>,
    http: Option<JoinHandle<()>>,
    event_loop: Option<JoinHandle<()>>,
}

impl TaskHandles {
    async fn check_and_restart(
        &mut self,
        state: &Arc<AppState>,
        config: &EdgeConfig,
        msg_tx: &tokio::sync::mpsc::Sender<GatewayMessage>,
    ) {
        if self.telemetry.as_ref().is_some_and(|h| h.is_finished()) {
            tracing::warn!("Telemetry task died, restarting");
            self.telemetry = Some(spawn_telemetry_loop(state.clone()));
        }
        if self.heartbeat.as_ref().is_some_and(|h| h.is_finished()) {
            tracing::warn!("Heartbeat task died, restarting");
            self.heartbeat = Some(spawn_heartbeat_loop(state.clone()));
        }
        if self.intelligence.as_ref().is_some_and(|h| h.is_finished()) {
            tracing::warn!("Intelligence task died, restarting");
            self.intelligence = Some(spawn_intelligence_loop(state.clone()));
        }
        if self.http.as_ref().is_some_and(|h| h.is_finished()) {
            tracing::warn!("HTTP server task died, restarting");
            self.http = Some(spawn_http_loop(state.clone(), config.local_api_port));
        }
        if self.event_loop.as_ref().is_some_and(|h| h.is_finished()) {
            tracing::warn!("MQTT event loop died, restarting");
            self.event_loop = Some(state.gateway_service.start_event_loop(msg_tx.clone()).await);
        }
    }
}

fn spawn_telemetry_loop(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(state.config.telemetry_interval_secs));
        loop {
            tick.tick().await;
            state.telemetry_service.collect_and_forward().await.ok();
        }
    })
}

fn spawn_heartbeat_loop(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(state.config.heartbeat_interval_secs));
        loop {
            tick.tick().await;
            state.health_service.beat_and_report().await.ok();
        }
    })
}

fn spawn_intelligence_loop(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(state.config.intelligence_interval_secs));
        loop {
            tick.tick().await;
            state.intelligence_service.evaluate_and_probe().await.ok();
        }
    })
}

fn spawn_http_loop(state: Arc<AppState>, port: u16) -> JoinHandle<()> {
    tokio::spawn(async move {
        let router = tinyiothub_edge::modules::http::service::create_router(state);
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
    })
}

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
    {
        let gw = state.gateway_service.clone();
        match state
            .offline_buffer
            .flush_batch_with(500, move |topic, payload| {
                let gw = gw.clone();
                async move { gw.publish_raw(&topic, payload).await }
            })
            .await
        {
            Ok(count) if count > 0 => tracing::info!(count, "Flushed offline buffer on startup"),
            Err(e) => tracing::warn!(?e, "Failed to flush offline buffer on startup"),
            _ => {}
        }
    }

    // Graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("SIGTERM/SIGINT received, draining...");
        let _ = shutdown_tx_clone.send(());
    });

    // MQTT event loop — start before other tasks so the channel is ready
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<GatewayMessage>(100);
    let msg_tx_restart = msg_tx.clone();
    let event_loop_handle = state.gateway_service.start_event_loop(msg_tx).await;

    // Spawn background tasks with watchdog handles
    let mut handles = TaskHandles {
        telemetry: Some(spawn_telemetry_loop(state.clone())),
        heartbeat: Some(spawn_heartbeat_loop(state.clone())),
        intelligence: Some(spawn_intelligence_loop(state.clone())),
        http: if config.local_api_enabled {
            Some(spawn_http_loop(state.clone(), config.local_api_port))
        } else {
            None
        },
        event_loop: Some(event_loop_handle),
    };

    // Main loop: connection management + message routing + task watchdog + shutdown
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                tracing::info!("Draining offline buffer before shutdown...");
                let gw = state.gateway_service.clone();
                let flushed = state.offline_buffer.flush_batch_with(500, move |topic, payload| {
                    let gw = gw.clone();
                    async move { gw.publish_raw(&topic, payload).await }
                }).await.unwrap_or(0);
                tracing::info!(flushed, "Offline buffer drained");
                state.gateway_service.disconnect().await;
                tracing::info!("Shutdown complete");
                std::process::exit(0);
            }

            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                // Task watchdog: detect and restart any dead background tasks
                handles.check_and_restart(&state, &config, &msg_tx_restart).await;

                // Periodic health check + reconnect if needed
                if !state.gateway_service.is_alive() {
                    tracing::warn!("MQTT connection lost, reconnecting...");
                    handles.event_loop =
                        Some(state.gateway_service.reconnect(msg_tx_restart.clone()).await);

                    if state.config_service.cloud_version_is_newer("0").await {
                        state.config_service.sync_from_cloud().await.ok();
                    }
                }
            }

            Some(msg) = msg_rx.recv() => {
                match msg {
                    GatewayMessage::ConfigDevice(payload) => {
                        tracing::info!(
                            device_id = %payload.device_id,
                            action = %payload.action,
                            "Config device"
                        );
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
