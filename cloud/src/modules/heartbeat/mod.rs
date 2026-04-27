pub mod handler;
pub mod types;

use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use types::{HeartbeatConfig, HeartbeatStatus};

/// Global heartbeat state
static HEARTBEAT_STATUS: OnceLock<Arc<RwLock<HeartbeatStatus>>> = OnceLock::new();
static HEARTBEAT_CONFIG: OnceLock<Arc<RwLock<HeartbeatConfig>>> = OnceLock::new();

/// Initialize global heartbeat state
pub fn init_heartbeat_state() -> (Arc<RwLock<HeartbeatStatus>>, Arc<RwLock<HeartbeatConfig>>) {
    let status = HEARTBEAT_STATUS
        .get_or_init(|| Arc::new(RwLock::new(HeartbeatStatus::default())))
        .clone();
    let config = HEARTBEAT_CONFIG
        .get_or_init(|| Arc::new(RwLock::new(HeartbeatConfig::default())))
        .clone();
    (status, config)
}

/// Get heartbeat status
pub fn get_heartbeat_status() -> Option<Arc<RwLock<HeartbeatStatus>>> {
    HEARTBEAT_STATUS.get().cloned()
}

/// Get heartbeat config
pub fn get_heartbeat_config() -> Option<Arc<RwLock<HeartbeatConfig>>> {
    HEARTBEAT_CONFIG.get().cloned()
}
