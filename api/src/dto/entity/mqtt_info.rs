use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// MQTT configuration entity - MQTT配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttConfig {
    pub id: String,
    pub name: String,
    pub is_enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub client_id: Option<String>,
    pub keep_alive: u16,
    pub clean_session: bool,
    pub use_tls: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 4G MQTT configuration entity - 4G MQTT配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Mqtt4GConfig {
    pub id: String,
    pub is_enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub client_id: Option<String>,
    pub apn: String,             // Access Point name for 4G
    pub heartbeat_interval: u32, // seconds
    pub upload_interval: u32,    // seconds
    pub created_at: String,
    pub updated_at: String,
}

/// Host configuration entity - 主机配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HostConfig {
    pub id: String,
    pub connection_string: String,
    pub host_port: u16,
    pub auth_timeout: u16, // seconds
    pub message_max_limit: u16,
    pub app_log_enabled: bool,
    pub app_log_level: String, // "error", "warn", "info", "debug", "trace"
    pub created_at: String,
    pub updated_at: String,
}

/// UDP configuration entity - UDP配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UdpConfig {
    pub id: String,
    pub server_address: String,
    pub server_port: u16,
    pub is_enabled: bool,
    pub buffer_size: u32,
    pub timeout: u16, // seconds
    pub created_at: String,
    pub updated_at: String,
}

/// Update configuration entity - 更新配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateConfig {
    pub id: String,
    pub update_host: String,
    pub update_port: u16,
    pub is_enabled: bool,
    pub check_interval: u32, // seconds
    pub auto_update: bool,
    pub backup_before_update: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// NTP configuration entity - NTP配置实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NtpConfig {
    pub id: String,
    pub server_address: String,
    pub is_enabled: bool,
    pub sync_interval: u32, // seconds
    pub timezone: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Request for creating MQTT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateMqttConfigRequest {
    pub name: String,
    pub is_enabled: Option<bool>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub client_id: Option<String>,
    pub keep_alive: Option<u16>,
    pub clean_session: Option<bool>,
    pub use_tls: Option<bool>,
}

impl MqttConfig {
    /// Create a new MQTT configuration
    pub fn new(request: CreateMqttConfigRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            is_enabled: request.is_enabled.unwrap_or(true),
            host: request.host,
            port: request.port,
            username: request.username,
            password: request.password,
            client_id: request.client_id,
            keep_alive: request.keep_alive.unwrap_or(60),
            clean_session: request.clean_session.unwrap_or(true),
            use_tls: request.use_tls.unwrap_or(false),
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Find MQTT configuration by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<MqttConfig>, sqlx::Error> {
        let config = sqlx::query_as::<_, MqttConfig>(
            r#"
            SELECT id, name, is_enabled, host, port, username, password, client_id,
                   keep_alive, clean_session, use_tls, ca_cert_path, client_cert_path,
                   client_key_path, created_at, updated_at
            FROM MqttConfigs WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(config)
    }

    /// Create MQTT configuration in database
    pub async fn create(
        db: &Database,
        request: &CreateMqttConfigRequest,
    ) -> Result<MqttConfig, sqlx::Error> {
        let config = Self::new(request.clone());

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO MqttConfigs (
                id, name, is_enabled, host, port, username, password, client_id,
                keep_alive, clean_session, use_tls, ca_cert_path, client_cert_path,
                client_key_path, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(config.is_enabled)
        .bind(&config.host)
        .bind(config.port)
        .bind(&config.username)
        .bind(&config.password)
        .bind(&config.client_id)
        .bind(config.keep_alive)
        .bind(config.clean_session)
        .bind(config.use_tls)
        .bind(&config.ca_cert_path)
        .bind(&config.client_cert_path)
        .bind(&config.client_key_path)
        .bind(&config.created_at)
        .bind(&config.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(config)
    }

    /// Find all MQTT configurations
    pub async fn find_all(db: &Database) -> Result<Vec<MqttConfig>, sqlx::Error> {
        let configs = sqlx::query_as::<_, MqttConfig>(
            r#"
            SELECT id, name, is_enabled, host, port, username, password, client_id,
                   keep_alive, clean_session, use_tls, ca_cert_path, client_cert_path,
                   client_key_path, created_at, updated_at
            FROM MqttConfigs
            ORDER BY name
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(configs)
    }

    /// Find enabled MQTT configurations
    pub async fn find_enabled(db: &Database) -> Result<Vec<MqttConfig>, sqlx::Error> {
        let configs = sqlx::query_as::<_, MqttConfig>(
            r#"
            SELECT id, name, is_enabled, host, port, username, password, client_id,
                   keep_alive, clean_session, use_tls, ca_cert_path, client_cert_path,
                   client_key_path, created_at, updated_at
            FROM MqttConfigs
            WHERE is_enabled = true
            ORDER BY name
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(configs)
    }

    /// Update MQTT configuration
    pub async fn update(
        db: &Database,
        id: &str,
        request: &CreateMqttConfigRequest,
    ) -> Result<MqttConfig, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = db.pool().begin().await?;

        let result = sqlx::query(
            r#"
            UPDATE MqttConfigs SET 
                name = ?, is_enabled = ?, host = ?, port = ?, username = ?, password = ?,
                client_id = ?, keep_alive = ?, clean_session = ?, use_tls = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&request.name)
        .bind(request.is_enabled.unwrap_or(true))
        .bind(&request.host)
        .bind(request.port)
        .bind(&request.username)
        .bind(&request.password)
        .bind(&request.client_id)
        .bind(request.keep_alive.unwrap_or(60))
        .bind(request.clean_session.unwrap_or(true))
        .bind(request.use_tls.unwrap_or(false))
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;

        Self::find_by_id(db, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Delete MQTT configuration
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM MqttConfigs WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Enable/disable MQTT configuration
    pub async fn set_enabled(db: &Database, id: &str, enabled: bool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result =
            sqlx::query("UPDATE MqttConfigs SET is_enabled = ?, updated_at = ? WHERE id = ?")
                .bind(enabled)
                .bind(now)
                .bind(id)
                .execute(db.pool())
                .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    /// Get connection URL
    pub fn get_connection_url(&self) -> String {
        let protocol = if self.use_tls { "mqtts" } else { "mqtt" };
        format!("{}://{}:{}", protocol, self.host, self.port)
    }

    /// Check if configuration is valid
    pub fn is_valid(&self) -> bool {
        !self.host.is_empty() && self.port > 0 && !self.username.is_empty()
    }

    /// Get client ID or generate default
    pub fn get_client_id(&self) -> String {
        self.client_id
            .clone()
            .unwrap_or_else(|| format!("tinyiothub_{}", &uuid::Uuid::new_v4().to_string()[..8]))
    }

    /// Check if TLS is configured properly
    pub fn is_tls_configured(&self) -> bool {
        if !self.use_tls {
            return true; // TLS not required
        }

        // If TLS is enabled, check if certificates are configured
        self.ca_cert_path.is_some()
            || (self.client_cert_path.is_some() && self.client_key_path.is_some())
    }
}

impl NtpConfig {
    /// Create a new NTP configuration
    pub fn new(server_address: String, timezone: String) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            server_address,
            is_enabled: true,
            sync_interval: 3600, // 1 hour
            timezone,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Find NTP configuration by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<NtpConfig>, sqlx::Error> {
        let config = sqlx::query_as::<_, NtpConfig>(
            "SELECT id, server_address, is_enabled, sync_interval, timezone, created_at, updated_at FROM ntp_configs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(config)
    }

    /// Create NTP configuration in database
    pub async fn create(
        db: &Database,
        server_address: String,
        timezone: String,
    ) -> Result<NtpConfig, sqlx::Error> {
        let config = Self::new(server_address, timezone);

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO ntp_configs (id, server_address, is_enabled, sync_interval, timezone, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&config.id)
        .bind(&config.server_address)
        .bind(config.is_enabled)
        .bind(config.sync_interval)
        .bind(&config.timezone)
        .bind(&config.created_at)
        .bind(&config.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(config)
    }

    /// Get current NTP configuration (assuming single config)
    pub async fn get_current(db: &Database) -> Result<Option<NtpConfig>, sqlx::Error> {
        let config = sqlx::query_as::<_, NtpConfig>(
            "SELECT id, server_address, is_enabled, sync_interval, timezone, created_at, updated_at FROM ntp_configs LIMIT 1"
        )
        .fetch_optional(db.pool())
        .await?;

        Ok(config)
    }

    /// Update NTP configuration
    pub async fn update(
        db: &Database,
        id: &str,
        server_address: String,
        timezone: String,
        sync_interval: u32,
    ) -> Result<NtpConfig, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = db.pool().begin().await?;

        let result = sqlx::query(
            "UPDATE ntp_configs SET server_address = ?, timezone = ?, sync_interval = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&server_address)
        .bind(&timezone)
        .bind(sync_interval)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;

        Self::find_by_id(db, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Enable/disable NTP synchronization
    pub async fn set_enabled(db: &Database, id: &str, enabled: bool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result =
            sqlx::query("UPDATE ntp_configs SET is_enabled = ?, updated_at = ? WHERE id = ?")
                .bind(enabled)
                .bind(now)
                .bind(id)
                .execute(db.pool())
                .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    /// Check if NTP is enabled and configured
    pub fn is_configured(&self) -> bool {
        self.is_enabled && !self.server_address.is_empty()
    }

    /// Get sync interval in minutes
    pub fn get_sync_interval_minutes(&self) -> u32 {
        self.sync_interval / 60
    }

    /// Set sync interval from minutes
    pub fn set_sync_interval_minutes(&mut self, minutes: u32) {
        self.sync_interval = minutes * 60;
    }
}

/// Legacy MQTT configuration for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LegacyMqttInfo {
    pub enable: i32,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub enable_4g: i32,
    pub host_4g: String,
    pub port_4g: u16,
    pub username_4g: String,
    pub password_4g: String,
    pub adp_4g: String,
    pub heartbeat_time: u32,
    pub upload_time: u32,
}

// Backward compatibility
pub type MqttInfo = LegacyMqttInfo;
pub type HostInfo = HostConfig;
pub type UdpInfo = UdpConfig;
pub type UpdateInfo = UpdateConfig;
pub type NtpInfo = NtpConfig;
