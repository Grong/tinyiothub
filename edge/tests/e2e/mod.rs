pub mod green_path_test;
pub mod offline_recovery_test;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::process::{Child, Command};
use std::time::Duration;
use uuid::Uuid;

/// Lightweight async MQTT client for publishing/subscribing to test topics.
pub struct MqttTestClient {
    client: AsyncClient,
}

impl MqttTestClient {
    pub async fn new() -> Self {
        let mut options = MqttOptions::new(&format!("e2e-test-{}", Uuid::new_v4()), "localhost", 1883);
        options.set_keep_alive(Duration::from_secs(10));
        let (client, mut eventloop) = AsyncClient::new(options, 100);

        // Spawn event loop handler to process ACKs and inbound messages
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(?e, "MqttTestClient event loop ended");
                        break;
                    }
                }
            }
        });

        // Wait for the connection to settle
        tokio::time::sleep(Duration::from_millis(500)).await;

        Self { client }
    }

    pub async fn subscribe(&self, topic: &str) {
        self.client.subscribe(topic, QoS::AtLeastOnce).await.ok();
    }

    #[allow(dead_code)]
    pub async fn publish(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtLeastOnce, false, payload).await.ok();
    }

    /// Create a fresh listener client, subscribe to a wildcard topic, and
    /// wait up to `timeout_secs` for a publish matching the prefix.
    pub async fn wait_for_message(&self, topic_filter: &str, timeout_secs: u64) -> Option<(String, Vec<u8>)> {
        let mut options = MqttOptions::new(&format!("e2e-listener-{}", Uuid::new_v4()), "localhost", 1883);
        options.set_keep_alive(Duration::from_secs(10));
        let (client, mut eventloop) = AsyncClient::new(options, 100);

        client.subscribe(topic_filter, QoS::AtLeastOnce).await.ok();

        let deadline = tokio::time::sleep(Duration::from_secs(timeout_secs));
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                _ = &mut deadline => return None,
                event = eventloop.poll() => {
                    if let Ok(Event::Incoming(Packet::Publish(p))) = event {
                        return Some((p.topic, p.payload.to_vec()));
                    }
                }
            }
        }
    }
}

/// Manages the edge binary as a subprocess.
pub struct EdgeProcess {
    child: Child,
    tmpdir: std::path::PathBuf,
}

impl EdgeProcess {
    /// Start the edge binary with the given credentials JSON.
    /// Sets up a temporary directory with credentials.json and env vars for MQTT.
    pub fn start(credentials_json: &str) -> Self {
        let tmpdir = std::env::temp_dir().join(format!("edge-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        // Write credentials so the edge boots in authenticated mode
        let creds_path = tmpdir.join("credentials.json");
        std::fs::write(&creds_path, credentials_json).unwrap();

        let db_path = tmpdir.join("edge.db");

        let child = Command::new(find_edge_binary())
            .env("EDGE_CREDENTIALS_FILE", &creds_path)
            .env("EDGE_DB_PATH", &db_path)
            .env("EDGE_MQTT_BROKER", "localhost")
            .env("EDGE_MQTT_PORT", "1883")
            .env("EDGE_TELEMETRY_INTERVAL", "5")
            .env("EDGE_HEARTBEAT_INTERVAL", "5")
            .env("EDGE_LOCAL_API", "0")
            .spawn()
            .expect("Failed to start edge process");

        Self { child, tmpdir }
    }
}

impl Drop for EdgeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.tmpdir);
    }
}

/// Resolve the edge binary path.
/// Tries the cargo env var first, then falls back to common relative paths.
fn find_edge_binary() -> String {
    // Cargo sets CARGO_BIN_EXE_<name> for integration tests.
    // Name normalization: hyphens → underscores, uppercase.
    let env_names = [
        "CARGO_BIN_EXE_tinyiothub-edge",
        "CARGO_BIN_EXE_TINYIOTHUB_EDGE",
        "CARGO_BIN_EXE_tinyiothub_edge",
    ];
    for name in &env_names {
        if let Ok(path) = std::env::var(name) {
            if std::path::Path::new(&path).exists() {
                return path;
            }
        }
    }

    // Fallback: look relative to CARGO_MANIFEST_DIR (edge/ directory during tests)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        for rel in &["target/debug/tinyiothub-edge", "../target/debug/tinyiothub-edge"] {
            let candidate = std::path::Path::new(&manifest_dir).join(rel);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // Last resort: look relative to current working directory
    for rel in &[
        "target/debug/tinyiothub-edge",
        "edge/target/debug/tinyiothub-edge",
        "../target/debug/tinyiothub-edge",
    ] {
        if std::path::Path::new(rel).exists() {
            return rel.to_string();
        }
    }

    panic!(
        "Cannot find edge binary. Run `cargo build -p tinyiothub-edge` first.\n\
         Tried env vars: {:?} and common relative paths.",
        env_names
    );
}
