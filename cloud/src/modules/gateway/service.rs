use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use crate::modules::gateway::pairing::{PairingCache, PairingEntry};
use crate::modules::gateway::types::*;
use sqlx::SqlitePool;

const MAX_PAIRING_REQUESTS_PER_IP_PER_MINUTE: usize = 3;
const IP_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

pub struct GatewayService {
    pool: SqlitePool,
    cache: Arc<PairingCache>,
    mqtt_tx: mpsc::Sender<MqttPublish>,
    ip_attempts: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

pub enum MqttPublish {
    PairingAck { code: String, ack: PairingAck },
}

impl GatewayService {
    pub fn new(pool: SqlitePool, cache: Arc<PairingCache>, mqtt_tx: mpsc::Sender<MqttPublish>) -> Self {
        let service = Self {
            pool,
            cache,
            mqtt_tx,
            ip_attempts: Arc::new(RwLock::new(HashMap::new())),
        };
        service.spawn_ip_cleanup();
        service
    }

    pub async fn pair_device(
        &self,
        user_id: &str,
        client_ip: Option<&str>,
        req: PairingRequest,
    ) -> Result<PairingResponse, PairingError> {
        if let Some(ip) = client_ip {
            if !self.check_ip_rate_limit(ip).await {
                return Err(PairingError::TooManyAttemptsIp);
            }
        }

        let code = req.code.trim().to_string();
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(PairingError::InvalidCode);
        }

        if self.cache.is_full().await {
            return Err(PairingError::ServiceBusy);
        }

        let announce = self.cache.get(&code).await.ok_or(PairingError::CodeNotFound)?;

        if !self.cache.check_and_increment_attempts(&code, user_id).await {
            return Err(PairingError::TooManyAttempts);
        }

        let device_id = uuid::Uuid::new_v4().to_string();
        let device_name = announce.hostname.clone();
        let workspace_id = req.workspace_id.clone().unwrap_or_default();
        let password = generate_device_password();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "INSERT INTO devices (id, name, device_type, protocol_type, fingerprint, linked_gateway, state, workspace_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&device_id)
        .bind(&device_name)
        .bind("gateway")
        .bind("mqtt")
        .bind(&announce.fingerprint)
        .bind::<Option<String>>(None)
        .bind(1i32)
        .bind(&workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(?e, "Failed to create device during pairing");
            PairingError::Internal
        })?;

        // Remove code BEFORE MQTT publish to prevent simultaneous pairing of same code
        self.cache.remove(&code).await;

        let ack = PairingAck {
            msg_type: "pairing_ack".into(),
            success: true,
            device_id: device_id.clone(),
            workspace_id: workspace_id.clone(),
            credentials: MqttCredentials {
                client_id: device_id.clone(),
                username: device_id.clone(),
                password: password.clone(),
            },
            topics: GatewayTopics {
                status: format!("tinyiothub/{}/gateway/{}/status", workspace_id, device_id),
                telemetry: format!("tinyiothub/{}/gateway/{}/telemetry", workspace_id, device_id),
                event: format!("tinyiothub/{}/gateway/{}/event", workspace_id, device_id),
                command: format!("tinyiothub/{}/gateway/{}/command", workspace_id, device_id),
                config: format!("tinyiothub/{}/gateway/{}/config", workspace_id, device_id),
                device_discover: format!(
                    "tinyiothub/{}/gateway/{}/device/discover",
                    workspace_id, device_id
                ),
                device_telemetry: format!(
                    "tinyiothub/{}/gateway/{}/device/+/telemetry",
                    workspace_id, device_id
                ),
            },
            keepalive: 60,
        };

        if self
            .mqtt_tx
            .send(MqttPublish::PairingAck {
                code: code.clone(),
                ack,
            })
            .await
            .is_err()
        {
            let _ = sqlx::query("DELETE FROM devices WHERE id = ?1")
                .bind(&device_id)
                .execute(&self.pool)
                .await;
            tracing::error!("MQTT channel closed, rolled back device creation");
            return Err(PairingError::MqttPublishFailed);
        }

        tracing::info!(
            code = %code,
            device_id = %device_id,
            fingerprint = %announce.fingerprint,
            "Pairing successful"
        );

        Ok(PairingResponse {
            device_id,
            device_name,
            hostname: announce.hostname,
            ip: announce.ip,
        })
    }

    async fn check_ip_rate_limit(&self, ip: &str) -> bool {
        let mut map = self.ip_attempts.write().await;
        let now = Instant::now();
        let entries = map.entry(ip.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) < IP_RATE_LIMIT_WINDOW);
        if entries.len() >= MAX_PAIRING_REQUESTS_PER_IP_PER_MINUTE {
            return false;
        }
        entries.push(now);
        true
    }

    fn spawn_ip_cleanup(&self) {
        let ip_attempts = Arc::clone(&self.ip_attempts);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(120));
            loop {
                interval.tick().await;
                let mut map = ip_attempts.write().await;
                let now = Instant::now();
                map.retain(|_, entries| {
                    entries.retain(|t| now.duration_since(*t) < IP_RATE_LIMIT_WINDOW);
                    !entries.is_empty()
                });
            }
        });
    }

    pub async fn handle_announce(&self, announce: PairingAnnounce) -> Result<(), AnnounceError> {
        let entry = PairingEntry {
            fingerprint: announce.fingerprint.clone(),
            hostname: announce.hostname.clone(),
            os: announce.os.clone(),
            ip: announce.ip.clone(),
            hw_model: announce.hw_model.clone(),
            created_at: Instant::now(),
            attempts: std::collections::HashMap::new(),
        };

        if !self.cache.try_insert(announce.code.clone(), entry).await {
            tracing::warn!(
                code = %announce.code,
                fingerprint = %announce.fingerprint,
                "Pairing cache full, rejecting announce"
            );
            return Err(AnnounceError::CacheFull);
        }

        tracing::debug!(
            code = %announce.code,
            fingerprint = %announce.fingerprint,
            hostname = %announce.hostname,
            "Received pairing announce"
        );
        Ok(())
    }

    pub async fn handle_device_discover(
        &self,
        gateway_id: &str,
        workspace_id: &str,
        msg: DeviceDiscoverMessage,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for device in &msg.devices {
            let sub_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO devices (id, name, device_type, protocol_type, address, driver_name, driver_options, linked_gateway, parent_id, state, workspace_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .bind(&sub_id)
            .bind(&device.name)
            .bind(device.device_type.as_deref().unwrap_or("sensor"))
            .bind(device.protocol_type.as_deref())
            .bind(device.address.as_deref())
            .bind(device.driver_name.as_deref())
            .bind(device.driver_options.as_deref())
            .bind(gateway_id)
            .bind(gateway_id)
            .bind(1i32)
            .bind(workspace_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }
}

fn generate_device_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::gateway::pairing::PairingEntry;
    use sqlx::Row;

    async fn make_pool() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::shared::persistence::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        // Create a tenant and workspace for FK references
        sqlx::query("INSERT INTO tenants (id, name, slug, created_at, updated_at) VALUES ('tenant1', 'test', 'tenant1', '2025-01-01', '2025-01-01')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1', 'ws1', 'tenant1', '2025-01-01', '2025-01-01')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn make_service(pool: SqlitePool) -> (GatewayService, mpsc::Receiver<MqttPublish>) {
        let (tx, rx) = mpsc::channel(100);
        let cache = Arc::new(PairingCache::new(1000));
        let service = GatewayService::new(pool, cache, tx);
        (service, rx)
    }

    fn make_announce(code: &str) -> PairingEntry {
        PairingEntry {
            fingerprint: "aa:bb:cc:dd:ee:ff".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.100".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: std::collections::HashMap::new(),
        }
    }

    // ── pair_device ──

    #[tokio::test]
    async fn pair_device_invalid_code_short() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let req = PairingRequest { code: "12345".into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::InvalidCode)));
    }

    #[tokio::test]
    async fn pair_device_invalid_code_alpha() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let req = PairingRequest { code: "12a456".into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::InvalidCode)));
    }

    #[tokio::test]
    async fn pair_device_code_not_found() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let req = PairingRequest { code: "123456".into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::CodeNotFound)));
    }

    #[tokio::test]
    async fn pair_device_too_many_attempts() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let code = "123456";
        svc.cache.insert(code.into(), make_announce(code)).await;

        for _ in 0..5 {
            let req = PairingRequest { code: code.into(), workspace_id: None };
            let result = svc.pair_device("user1", None, req).await;
            // First 5 attempts may succeed or fail based on attempt counting
            // but the 6th should fail with TooManyAttempts
        }
        let req = PairingRequest { code: code.into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::TooManyAttempts)));
    }

    #[tokio::test]
    async fn pair_device_ip_rate_limit() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let code = "123456";
        svc.cache.insert(code.into(), make_announce(code)).await;

        // First 3 requests from same IP should be allowed
        for _ in 0..3 {
            // These will succeed or fail depending on attempt counting for different codes
            // but they should pass IP rate limiting
            let req = PairingRequest { code: code.into(), workspace_id: None };
            // Don't check result, just burn attempts from this IP
            let _ = svc.pair_device("user1", Some("10.0.0.1"), req).await;
        }
        // 4th request from same IP should be rate-limited
        let req = PairingRequest { code: code.into(), workspace_id: None };
        let result = svc.pair_device("user1", Some("10.0.0.1"), req).await;
        assert!(matches!(result, Err(PairingError::TooManyAttemptsIp)));
    }

    #[tokio::test]
    async fn pair_device_success() {
        let pool = make_pool().await;
        let (svc, mut rx) = make_service(pool.clone());
        let code = "123456";
        svc.cache.insert(code.into(), make_announce(code)).await;

        let req = PairingRequest { code: code.into(), workspace_id: Some("ws1".into()) };
        let result = svc.pair_device("user1", None, req).await;
        assert!(result.is_ok(), "pair_device failed: {:?}", result.err());
        let response = result.unwrap();

        assert!(!response.device_id.is_empty());
        assert_eq!(response.device_name, "gw-01");
        assert_eq!(response.hostname, "gw-01");
        assert_eq!(response.ip, "192.168.1.100");

        // Verify device was inserted into DB
        let row = sqlx::query("SELECT id, name, fingerprint, workspace_id FROM devices WHERE id = ?1")
            .bind(&response.device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("name"), "gw-01");
        assert_eq!(row.get::<String, _>("workspace_id"), "ws1");

        // Verify pairing ack was published to MQTT
        let publish = rx.try_recv().expect("Expected MQTT publish");
        match publish {
            MqttPublish::PairingAck { code: c, ack } => {
                assert_eq!(c, "123456");
                assert!(ack.success);
                assert_eq!(ack.device_id, response.device_id);
            }
        }

        // Verify code is removed from cache after pairing
        assert!(svc.cache.get(code).await.is_none());
    }

    // ── handle_device_discover ──

    #[tokio::test]
    async fn device_discover_inserts_sub_devices() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool.clone());

        // First create a gateway device
        let gw_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO devices (id, name, device_type, protocol_type, state, workspace_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&gw_id)
        .bind("gw-01")
        .bind("gateway")
        .bind("mqtt")
        .bind(1i32)
        .bind("ws1")
        .bind("2025-01-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .execute(&pool)
        .await
        .unwrap();

        let msg = DeviceDiscoverMessage {
            msg_type: "device_discover".into(),
            devices: vec![
                DiscoveredDevice {
                    name: "temp-sensor".into(),
                    device_type: Some("sensor".into()),
                    protocol_type: Some("modbus".into()),
                    address: Some("/dev/ttyUSB0:1".into()),
                    driver_name: Some("modbus-rtu".into()),
                    driver_options: Some(r#"{"baud":9600}"#.into()),
                },
                DiscoveredDevice {
                    name: "relay".into(),
                    device_type: Some("actuator".into()),
                    protocol_type: None,
                    address: None,
                    driver_name: None,
                    driver_options: None,
                },
            ],
        };

        svc.handle_device_discover(&gw_id, "ws1", msg).await.unwrap();

        // Verify sub-devices created
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT name, linked_gateway, parent_id FROM devices WHERE linked_gateway = ?1")
                .bind(&gw_id)
                .fetch_all(&pool)
                .await
                .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, gw_id); // linked_gateway
        assert_eq!(rows[0].2, gw_id); // parent_id
    }

    #[tokio::test]
    async fn device_discover_empty_ok() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);

        let msg = DeviceDiscoverMessage {
            msg_type: "device_discover".into(),
            devices: vec![],
        };

        svc.handle_device_discover("gw-xyz", "ws1", msg).await.unwrap();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("未发现设备，请确认配对码是否正确")]
    CodeNotFound,
    #[error("配对码格式无效")]
    InvalidCode,
    #[error("尝试次数过多，请1分钟后重试")]
    TooManyAttempts,
    #[error("请求过于频繁，请稍后重试")]
    TooManyAttemptsIp,
    #[error("服务繁忙，请稍后重试")]
    ServiceBusy,
    #[error("配对暂时失败，请稍后重试")]
    Internal,
    #[error("MQTT发布失败，配对已回滚")]
    MqttPublishFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum AnnounceError {
    #[error("Pairing cache full")]
    CacheFull,
}
