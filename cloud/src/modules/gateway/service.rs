use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tinyiothub_core::models::device::CreateDeviceRequest;
use tokio::sync::{RwLock, mpsc};

use crate::{
    modules::{
        event::{
            EventError,
            entities::Event,
            repositories::EventRepository,
            value_objects::{
                ContentElement, DeviceEventType, EventLevel, EventSource, RichContent,
            },
        },
        gateway::{
            pairing::{PairingCache, PairingEntry},
            types::*,
        },
    },
    shared::persistence::factory::DeviceRepositoryFactory,
};

const MAX_PAIRING_REQUESTS_PER_IP_PER_MINUTE: usize = 3;
const IP_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

pub struct GatewayService {
    device_repo_factory: Arc<DeviceRepositoryFactory>,
    event_repository: Arc<dyn EventRepository>,
    cache: Arc<PairingCache>,
    mqtt_tx: mpsc::Sender<MqttPublish>,
    ip_attempts: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

pub enum MqttPublish {
    PairingAck { code: String, ack: PairingAck },
}

impl GatewayService {
    pub fn new(
        device_repo_factory: Arc<DeviceRepositoryFactory>,
        event_repository: Arc<dyn EventRepository>,
        cache: Arc<PairingCache>,
        mqtt_tx: mpsc::Sender<MqttPublish>,
    ) -> Self {
        let service = Self {
            device_repo_factory,
            event_repository,
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
        if let Some(ip) = client_ip
            && !self.check_ip_rate_limit(ip).await
        {
            return Err(PairingError::TooManyAttemptsIp);
        }

        let code = req.code.trim().to_string();
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(PairingError::InvalidCode);
        }

        if self.cache.is_full().await {
            return Err(PairingError::ServiceBusy);
        }

        // Check for expired code before not-found to return correct status
        if self.cache.is_code_expired(&code).await {
            return Err(PairingError::CodeExpired);
        }

        let announce = self.cache.get(&code).await.ok_or(PairingError::CodeNotFound)?;

        if !self.cache.check_and_increment_attempts(&code, user_id).await {
            return Err(PairingError::TooManyAttempts);
        }

        let device_name = announce.hostname.clone();
        let workspace_id = req.workspace_id.clone().unwrap_or_default();
        let password = generate_device_password();

        let repo = self.device_repo_factory.create_for_workspace(workspace_id.clone());
        let create_req = CreateDeviceRequest {
            name: device_name.clone(),
            device_type: Some("gateway".into()),
            protocol_type: Some("mqtt".into()),
            fingerprint: Some(announce.fingerprint.clone()),
            workspace_id: Some(workspace_id.clone()),
            ..Default::default()
        };
        let device = repo.create(&create_req).await.map_err(|e| {
            tracing::error!(?e, "Failed to create device during pairing");
            PairingError::Internal
        })?;
        let device_id = device.id.clone();

        // Set gateway as online
        let _ = repo.update_state(&device_id, 1i32).await;

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

        if self.mqtt_tx.send(MqttPublish::PairingAck { code: code.clone(), ack }).await.is_err() {
            let _ = repo.delete(&device_id).await;
            // Restore the code to cache so the gateway can still be paired
            self.cache
                .insert(
                    code.clone(),
                    PairingEntry {
                        fingerprint: announce.fingerprint.clone(),
                        hostname: announce.hostname.clone(),
                        os: announce.os.clone(),
                        ip: announce.ip.clone(),
                        hw_model: announce.hw_model.clone(),
                        created_at: std::time::Instant::now(),
                        attempts: std::collections::HashMap::new(),
                    },
                )
                .await;
            tracing::error!(code = %code, "MQTT channel closed, rolled back device and restored cache entry");
            return Err(PairingError::MqttPublishFailed);
        }

        tracing::info!(
            code = %code,
            device_id = %device_id,
            fingerprint = %announce.fingerprint,
            "Pairing successful"
        );

        Ok(PairingResponse { device_id, device_name, hostname: announce.hostname, ip: announce.ip })
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
    ) -> Result<(), tinyiothub_core::error::Error> {
        if msg.devices.is_empty() {
            return Ok(());
        }

        let repo = self.device_repo_factory.create_for_workspace(workspace_id.to_string());
        let requests: Vec<CreateDeviceRequest> = msg
            .devices
            .iter()
            .map(|d| CreateDeviceRequest {
                name: d.name.clone(),
                device_type: Some(d.device_type.clone().unwrap_or_else(|| "sensor".into())),
                protocol_type: d.protocol_type.clone(),
                address: d.address.clone(),
                driver_name: d.driver_name.clone(),
                driver_options: d.driver_options.clone(),
                linked_gateway: Some(gateway_id.to_string()),
                parent_id: Some(gateway_id.to_string()),
                workspace_id: Some(workspace_id.to_string()),
                ..Default::default()
            })
            .collect();

        repo.create_batch(&requests).await.map(|_| ())
    }

    pub async fn handle_gateway_data(&self, data: GatewayDataMessage) {
        match &data {
            GatewayDataMessage::Status { gateway_id, workspace_id, .. } => {
                let repo = self.device_repo_factory.create_for_workspace(workspace_id.clone());
                if let Err(e) = repo.update_state(gateway_id, 1i32).await {
                    tracing::warn!(?e, gateway_id = %gateway_id, "Failed to update gateway last_seen");
                }
                tracing::debug!(gateway_id = %gateway_id, "Gateway status received, last_seen updated");
            }
            GatewayDataMessage::DeviceDiscover { gateway_id, workspace_id, msg } => {
                if let Err(e) =
                    self.handle_device_discover(gateway_id, workspace_id, msg.clone()).await
                {
                    tracing::error!(?e, gateway_id = %gateway_id, "Failed to handle device discover");
                }
            }
            GatewayDataMessage::Telemetry { gateway_id, workspace_id, msg } => {
                if let Err(e) = self.store_telemetry_event(gateway_id, workspace_id, msg).await {
                    tracing::warn!(?e, gateway_id = %gateway_id, "Failed to store gateway telemetry");
                }
                tracing::debug!(gateway_id = %gateway_id, "Gateway telemetry stored");
            }
            GatewayDataMessage::DeviceTelemetry { gateway_id, workspace_id, msg } => {
                if let Err(e) =
                    self.store_device_telemetry_event(gateway_id, workspace_id, msg).await
                {
                    tracing::warn!(?e, gateway_id = %gateway_id, device_id = %msg.device_id, "Failed to store device telemetry");
                }
                tracing::debug!(gateway_id = %gateway_id, device_id = %msg.device_id, "Device telemetry stored");
            }
        }
    }

    async fn store_telemetry_event(
        &self,
        gateway_id: &str,
        workspace_id: &str,
        msg: &TelemetryMessage,
    ) -> Result<(), EventError> {
        let content = vec![
            ContentElement::plain_text(format!("timestamp: {}", msg.timestamp)),
            ContentElement::code(msg.data.to_string(), Some("json".to_string())),
        ];
        let event = Event::new_device_event(
            DeviceEventType::PropertyChange,
            EventLevel::Debug,
            EventSource::device(gateway_id.to_string(), Some(workspace_id.to_string())),
            RichContent::new(format!("Gateway {} telemetry", gateway_id), content),
        )
        .map_err(|e| EventError::Validation { message: e.to_string() })?;
        self.event_repository.save(&event).await?;
        Ok(())
    }

    async fn store_device_telemetry_event(
        &self,
        _gateway_id: &str,
        workspace_id: &str,
        msg: &DeviceTelemetryMessage,
    ) -> Result<(), EventError> {
        let content = vec![
            ContentElement::plain_text(format!("timestamp: {}", msg.timestamp)),
            ContentElement::code(msg.data.to_string(), Some("json".to_string())),
        ];
        let event = Event::new_device_event(
            DeviceEventType::PropertyChange,
            EventLevel::Debug,
            EventSource::device(msg.device_id.clone(), Some(workspace_id.to_string())),
            RichContent::new(format!("Device {} telemetry", msg.device_id), content),
        )
        .map_err(|e| EventError::Validation { message: e.to_string() })?;
        self.event_repository.save(&event).await?;
        Ok(())
    }
}

fn generate_device_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect()
}

#[cfg(test)]
mod tests {
    use sqlx::{Row, SqlitePool};

    use super::*;
    use crate::modules::gateway::pairing::PairingEntry;

    async fn make_pool() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::shared::persistence::test_helpers::run_all_migrations(&pool).await.unwrap();
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
        let database = Arc::new(crate::shared::persistence::Database::new(pool));
        let factory = Arc::new(DeviceRepositoryFactory::new(database.clone()));
        let event_repo: Arc<dyn EventRepository> =
            Arc::new(crate::shared::persistence::repositories::SqliteEventRepository::new(
                database.as_ref().clone(),
            ));
        let service = GatewayService::new(factory, event_repo, cache, tx);
        (service, rx)
    }

    fn make_announce(_code: &str) -> PairingEntry {
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
            let _result = svc.pair_device("user1", None, req).await;
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
        let row =
            sqlx::query("SELECT id, name, fingerprint, workspace_id FROM devices WHERE id = ?1")
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
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT name, linked_gateway, parent_id FROM devices WHERE linked_gateway = ?1",
        )
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

        let msg = DeviceDiscoverMessage { msg_type: "device_discover".into(), devices: vec![] };

        svc.handle_device_discover("gw-xyz", "ws1", msg).await.unwrap();
    }

    // ── handle_announce ──

    #[tokio::test]
    async fn handle_announce_success() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);

        let announce = PairingAnnounce {
            msg_type: "pairing_announce".into(),
            code: "123456".into(),
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
        };
        svc.handle_announce(announce).await.unwrap();

        let entry = svc.cache.get("123456").await.unwrap();
        assert_eq!(entry.fingerprint, "aa:bb:cc");
        assert_eq!(entry.hostname, "gw-01");
    }

    #[tokio::test]
    async fn handle_announce_cache_full() {
        let pool = make_pool().await;
        let (_svc, _rx) = make_service(pool.clone());

        // Create a tiny cache and fill it
        let tiny_cache = Arc::new(PairingCache::new(1));
        let (tx, _rx2) = mpsc::channel(1);
        let database = Arc::new(crate::shared::persistence::Database::new(pool));
        let factory = Arc::new(DeviceRepositoryFactory::new(database.clone()));
        let event_repo: Arc<dyn EventRepository> =
            Arc::new(crate::shared::persistence::repositories::SqliteEventRepository::new(
                database.as_ref().clone(),
            ));
        let svc2 = GatewayService::new(factory, event_repo, tiny_cache.clone(), tx);

        // Fill the cache
        tiny_cache.insert("111111".into(), make_announce("111111")).await;

        let announce = PairingAnnounce {
            msg_type: "pairing_announce".into(),
            code: "222222".into(),
            fingerprint: "dd:ee:ff".into(),
            hostname: "gw-02".into(),
            os: "Linux".into(),
            ip: "192.168.1.2".into(),
            hw_model: "RPi5".into(),
        };
        let result = svc2.handle_announce(announce).await;
        assert!(matches!(result, Err(AnnounceError::CacheFull)));
    }

    // ── ServiceBusy on full cache ──

    #[tokio::test]
    async fn pair_device_service_busy() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);

        // Fill the cache
        for i in 0..1000 {
            svc.cache.insert(format!("{:06}", i), make_announce("")).await;
        }
        assert!(svc.cache.is_full().await);

        let req = PairingRequest { code: "123456".into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::ServiceBusy)));
    }

    // ── MQTT channel closed = rollback ──

    #[tokio::test]
    async fn pair_device_mqtt_rollback() {
        let pool = make_pool().await;
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // Close the receiver to simulate MQTT channel failure

        let cache = Arc::new(PairingCache::new(1000));
        let database = Arc::new(crate::shared::persistence::Database::new(pool.clone()));
        let factory = Arc::new(DeviceRepositoryFactory::new(database.clone()));
        let event_repo: Arc<dyn EventRepository> =
            Arc::new(crate::shared::persistence::repositories::SqliteEventRepository::new(
                database.as_ref().clone(),
            ));
        let svc = GatewayService::new(factory, event_repo, cache.clone(), tx);

        let code = "123456";
        cache.insert(code.into(), make_announce(code)).await;

        let req = PairingRequest { code: code.into(), workspace_id: Some("ws1".into()) };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::MqttPublishFailed)));

        // Verify the code was restored to cache (race condition fix)
        assert!(cache.get(code).await.is_some());

        // Verify no device was created (rollback)
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE name = 'gw-01'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    // ── Different IPs not blocked together ──

    #[tokio::test]
    async fn different_ips_not_blocked() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let code = "123456";

        // Burn attempts from IP 10.0.0.1
        for _ in 0..3 {
            svc.cache.insert(code.into(), make_announce(code)).await;
            let req = PairingRequest { code: code.into(), workspace_id: None };
            let _ = svc.pair_device("user1", Some("10.0.0.1"), req).await;
        }
        // 4th attempt from 10.0.0.1 should be rate-limited
        svc.cache.insert(code.into(), make_announce(code)).await;
        let req = PairingRequest { code: code.into(), workspace_id: None };
        let result = svc.pair_device("user1", Some("10.0.0.1"), req).await;
        assert!(matches!(result, Err(PairingError::TooManyAttemptsIp)));

        // Same code from DIFFERENT IP should NOT be rate-limited
        svc.cache.insert(code.into(), make_announce(code)).await;
        let req = PairingRequest { code: code.into(), workspace_id: None };
        let result = svc.pair_device("user1", Some("10.0.0.2"), req).await;
        // Should NOT be TooManyAttemptsIp (may succeed or fail for other reasons)
        assert!(!matches!(result, Err(PairingError::TooManyAttemptsIp)));
    }

    // ── Code expired aka 410 Gone ──

    #[tokio::test]
    async fn code_expired_returns_410() {
        let pool = make_pool().await;
        let (svc, _rx) = make_service(pool);
        let code = "123456";

        let mut entry = make_announce(code);
        entry.created_at = Instant::now() - Duration::from_secs(301);
        svc.cache.insert(code.into(), entry).await;

        let req = PairingRequest { code: code.into(), workspace_id: None };
        let result = svc.pair_device("user1", None, req).await;
        assert!(matches!(result, Err(PairingError::CodeExpired)));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("未发现设备，请确认配对码是否正确")]
    CodeNotFound,
    #[error("配对码已过期，请查看网关屏幕上的新配对码")]
    CodeExpired,
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
