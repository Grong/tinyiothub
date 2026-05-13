use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::modules::gateway::pairing::{PairingCache, PairingEntry};
use crate::modules::gateway::types::*;
use sqlx::SqlitePool;

pub struct GatewayService {
    pool: SqlitePool,
    cache: Arc<PairingCache>,
    mqtt_tx: mpsc::Sender<MqttPublish>,
}

pub enum MqttPublish {
    PairingAck { code: String, ack: PairingAck },
}

impl GatewayService {
    pub fn new(pool: SqlitePool, cache: Arc<PairingCache>, mqtt_tx: mpsc::Sender<MqttPublish>) -> Self {
        Self { pool, cache, mqtt_tx }
    }

    pub async fn pair_device(
        &self,
        user_id: &str,
        req: PairingRequest,
    ) -> Result<PairingResponse, PairingError> {
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
            "INSERT INTO devices (id, name, device_type, protocol_type, fingerprint, linked_gateway, status, workspace_id, created_at, updated_at)
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

        self.cache.remove(&code).await;

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
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for device in &msg.devices {
            let sub_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO devices (id, name, device_type, protocol_type, address, driver_name, driver_options, linked_gateway, parent_id, status, workspace_id, created_at, updated_at)
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
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

fn generate_device_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("未发现设备，请确认配对码是否正确")]
    CodeNotFound,
    #[error("配对码格式无效")]
    InvalidCode,
    #[error("尝试次数过多，请1分钟后重试")]
    TooManyAttempts,
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
