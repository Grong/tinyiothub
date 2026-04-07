use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::infrastructure::event::security::EventSecurityConfig;

/// Configuration error type
#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(String),
    ParseError(String),
    ValidationError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::FileNotFound(msg) => write!(f, "Config file not found: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Config parse error: {}", msg),
            ConfigError::ValidationError(msg) => write!(f, "Config validation error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplicationSettings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub mqtt: MqttConfig,
    pub logging: LoggingConfig,
    pub security: SecurityConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub device: DeviceConfig,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub features: FeaturesConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub marketplace: MarketplaceConfig,
    #[serde(default)]
    pub sms: SmsConfig,
    #[serde(default)]
    pub social: SocialConfig,
    #[serde(default)]
    pub event: EventSettings,
    #[serde(default)]
    pub harmonyos: HarmonyosConfig,
    #[serde(default)]
    pub redis: Option<RedisConfig>,
    #[serde(default)]
    pub agent: Option<GatewayConfig>,
}

/// Gateway (ZeroClaw) Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayConfig {
    pub url: String,
    #[serde(default)]
    pub ws_url: Option<String>,
    #[serde(default)]
    pub gateway_token: Option<String>,
}

/// Event system configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct EventSettings {
    #[serde(default)]
    pub security: EventSecurityConfig,
}

/// HarmonyOS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct HarmonyosConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Redis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RedisConfig {
    pub url: String,
    #[serde(default = "default_redis_max_connections")]
    pub max_connections: u32,
}

fn default_redis_max_connections() -> u32 {
    16
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_static_dir")]
    pub static_files_dir: String,
    #[serde(default = "default_upload_dir")]
    pub upload_dir: String,
    #[serde(default = "default_true")]
    pub enable_cors: bool,
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: u64,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_db_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_db_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_query_timeout")]
    pub query_timeout_secs: u64,
    #[serde(default)]
    pub log_queries: bool,
    #[serde(default = "default_true")]
    pub auto_migrate: bool,
}

/// MQTT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttConfig {
    pub primary: MqttBrokerConfig,
    #[serde(default)]
    pub secondary: Option<MqttBrokerConfig>,
    pub client: MqttClientConfig,
    pub topics: MqttTopicConfig,
}

/// MQTT broker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttBrokerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_keep_alive")]
    pub keep_alive_secs: u64,
}

/// MQTT client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttClientConfig {
    pub client_id: String,
    #[serde(default = "default_true")]
    pub clean_session: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_max_retries")]
    pub max_reconnect_attempts: u32,
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_secs: u64,
    #[serde(default = "default_queue_size")]
    pub message_queue_size: usize,
}

/// MQTT topic configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttTopicConfig {
    pub prefix: String,
    pub heartbeat: String,
    pub device_registration: String,
    pub command: String,
    pub data_upload: String,
    pub alarm: String,
    #[serde(default = "default_qos")]
    pub publish_qos: u8,
    #[serde(default = "default_qos")]
    pub subscribe_qos: u8,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoggingConfig {
    pub level: String,
    #[serde(default = "default_true")]
    pub console_enabled: bool,
    #[serde(default = "default_true")]
    pub file_enabled: bool,
    #[serde(default = "default_log_path")]
    pub file_path: PathBuf,
    #[serde(default = "default_log_size")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_log_files")]
    pub max_files: u32,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default)]
    pub structured: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityConfig {
    pub jwt: JwtConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub rate_limiting: RateLimitingConfig,
}

/// JWT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JwtConfig {
    pub secret: String,
    pub expiration_secs: u64,
    pub issuer: String,
    pub audience: String,
    #[serde(default = "default_jwt_algorithm")]
    pub algorithm: String,
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SessionConfig {
    #[serde(default = "default_session_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_sessions")]
    pub max_concurrent_sessions: u32,
    #[serde(default)]
    pub persistent: bool,
    #[serde(default = "default_storage_type")]
    pub storage_type: String,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RateLimitingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rate_limit")]
    pub requests_per_minute: u32,
    #[serde(default = "default_auth_rate_limit")]
    pub auth_requests_per_minute: u32,
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct NetworkConfig {
    #[serde(default)]
    pub interface: NetworkInterfaceConfig,
    #[serde(default)]
    pub ntp: NtpConfig,
    #[serde(default)]
    pub defaults: NetworkDefaultsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkInterfaceConfig {
    #[serde(default = "default_true")]
    pub use_dhcp: bool,
    #[serde(default = "default_interface_name")]
    pub interface_name: String,
}

impl Default for NetworkInterfaceConfig {
    fn default() -> Self {
        Self { use_dhcp: true, interface_name: "eth0".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NtpConfig {
    #[serde(default = "default_ntp_servers")]
    pub servers: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for NtpConfig {
    fn default() -> Self {
        Self { servers: vec!["pool.ntp.org".to_string()], enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkDefaultsConfig {
    #[serde(default = "default_ip_address")]
    pub ip_address: String,
    #[serde(default = "default_gateway")]
    pub gateway: String,
    #[serde(default = "default_subnet_mask")]
    pub subnet_mask: String,
    #[serde(default = "default_dns_primary")]
    pub dns_primary: String,
    #[serde(default = "default_dns_secondary")]
    pub dns_secondary: String,
}

impl Default for NetworkDefaultsConfig {
    fn default() -> Self {
        Self {
            ip_address: default_ip_address(),
            gateway: default_gateway(),
            subnet_mask: default_subnet_mask(),
            dns_primary: default_dns_primary(),
            dns_secondary: default_dns_secondary(),
        }
    }
}

fn default_ip_address() -> String {
    "0.0.0.0".to_string()
}
fn default_gateway() -> String {
    "0.0.0.0".to_string()
}
fn default_subnet_mask() -> String {
    "255.255.255.0".to_string()
}
fn default_dns_primary() -> String {
    "8.8.8.8".to_string()
}
fn default_dns_secondary() -> String {
    "8.8.4.4".to_string()
}

/// Device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceConfig {
    #[serde(default)]
    pub serial_number: String,
    #[serde(default = "default_device_name")]
    pub name: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub data_collection: DataCollectionConfig,
    #[serde(default)]
    pub drivers: DriverConfig,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            serial_number: String::new(),
            name: "TinyIoTHub".to_string(),
            location: String::new(),
            data_collection: DataCollectionConfig::default(),
            drivers: DriverConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DataCollectionConfig {
    #[serde(default = "default_collection_interval")]
    pub interval_secs: u64,
}

impl Default for DataCollectionConfig {
    fn default() -> Self {
        Self { interval_secs: 10 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DriverConfig {
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_drivers_dir")]
    pub dynamic_drivers_dir: String,
    #[serde(default = "default_true")]
    pub auto_load_on_startup: bool,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            dynamic_drivers_dir: "drivers".to_string(),
            auto_load_on_startup: true,
        }
    }
}

fn default_drivers_dir() -> String {
    "drivers".to_string()
}

fn default_retry_delay() -> u64 {
    1000
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct MonitoringConfig {
    // Empty for now, can be extended later
}

/// Features configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct FeaturesConfig {
    // Empty for now, can be extended later
}

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EnvironmentConfig {
    #[serde(default = "default_environment")]
    pub name: String,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self { name: "production".to_string() }
    }
}

/// Marketplace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketplaceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default = "default_github_branch")]
    pub github_branch: String,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_hours: u64,
    #[serde(default = "default_download_timeout")]
    pub download_timeout_secs: u64,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_url: None,
            github_repo: Some("tinyiothub/marketplace".to_string()),
            github_branch: "main".to_string(),
            cache_ttl_hours: 24,
            download_timeout_secs: 300,
        }
    }
}

fn default_github_branch() -> String {
    "main".to_string()
}

fn default_false() -> bool {
    false
}

fn default_sms_provider() -> String {
    "aliyun".to_string()
}

fn default_sms_rate_limit() -> u32 {
    5
}

fn default_sms_expire() -> u64 {
    300
}

/// SMS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SmsConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_sms_provider")]
    pub provider: String,           // aliyun, tencent, twilio
    #[serde(default)]
    pub rate_limit: Option<SmsRateLimit>,
    // 阿里云 SMS
    #[serde(default)]
    pub aliyun: Option<AliyunSmsConfig>,
    // 腾讯防水墙
    #[serde(default)]
    pub captcha: Option<CaptchaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AliyunSmsConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub sign_name: String,        // 短信签名
    pub template_code: String,    // 短信模板 code
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CaptchaConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmsRateLimit {
    pub code_expire_secs: Option<u64>,  // 验证码有效期，默认 300
    pub max_per_minute: Option<u64>,    // 每分钟最大发送次数
    pub daily_limit: Option<u64>,       // 每天最大发送次数，默认 5
    pub interval_secs: Option<u64>,     // 发送间隔，默认 90
}

/// Social login configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SocialConfig {
    #[serde(default)]
    pub wechat: Option<WechatConfig>,
    #[serde(default)]
    pub wechat_miniprogram: Option<WeChatMiniProgramConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WechatConfig {
    pub enabled: bool,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatMiniProgramConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
}

fn default_cache_ttl() -> u64 {
    24
}

fn default_download_timeout() -> u64 {
    300
}

// Default value functions
fn default_static_dir() -> String {
    "wwwroot".to_string()
}
fn default_upload_dir() -> String {
    "uploads".to_string()
}
fn default_true() -> bool {
    true
}
fn default_cors_origins() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_max_connections() -> usize {
    1000
}
fn default_timeout() -> u64 {
    30
}
fn default_db_max_connections() -> u32 {
    10
}
fn default_db_min_connections() -> u32 {
    1
}
fn default_query_timeout() -> u64 {
    60
}
fn default_keep_alive() -> u64 {
    60
}
fn default_max_retries() -> u32 {
    5
}
fn default_reconnect_delay() -> u64 {
    5
}
fn default_queue_size() -> usize {
    1000
}
fn default_qos() -> u8 {
    1
}
fn default_log_path() -> PathBuf {
    PathBuf::from("logs/app.log")
}
fn default_log_size() -> u64 {
    10
}
fn default_log_files() -> u32 {
    5
}
fn default_log_format() -> String {
    "text".to_string()
}
fn default_jwt_algorithm() -> String {
    "HS256".to_string()
}
fn default_session_timeout() -> u64 {
    3600
}
fn default_max_sessions() -> u32 {
    5
}
fn default_storage_type() -> String {
    "memory".to_string()
}
fn default_rate_limit() -> u32 {
    60
}
fn default_auth_rate_limit() -> u32 {
    10
}
fn default_burst_size() -> u32 {
    10
}
fn default_interface_name() -> String {
    "eth0".to_string()
}
fn default_ntp_servers() -> Vec<String> {
    vec!["pool.ntp.org".to_string()]
}
fn default_device_name() -> String {
    "TinyIoTHub".to_string()
}
fn default_collection_interval() -> u64 {
    10
}
fn default_environment() -> String {
    "production".to_string()
}

impl ApplicationSettings {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate JWT secret
        if self.security.jwt.secret.len() < 32 {
            return Err(ConfigError::ValidationError(
                "JWT secret must be at least 32 characters long".to_string(),
            ));
        }

        if self.security.jwt.secret.contains("your-secret-key")
            || self.security.jwt.secret.contains("change-this")
        {
            return Err(ConfigError::ValidationError(
                "JWT secret must be changed from default value".to_string(),
            ));
        }

        // Validate server port
        if self.server.port == 0 {
            return Err(ConfigError::ValidationError("Server port cannot be 0".to_string()));
        }

        // Validate database URL
        if self.database.url.is_empty() {
            return Err(ConfigError::ValidationError("Database URL cannot be empty".to_string()));
        }

        Ok(())
    }

    /// Get server bind address
    pub fn server_bind_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    /// Get MQTT broker address
    pub fn mqtt_broker_address(&self) -> String {
        format!("{}:{}", self.mqtt.primary.host, self.mqtt.primary.port)
    }

    /// Get log file path
    pub fn log_file_path(&self) -> &PathBuf {
        &self.logging.file_path
    }

    /// Get environment name
    pub fn environment(&self) -> &str {
        &self.environment.name
    }

    /// Check if running in development mode
    pub fn is_development(&self) -> bool {
        self.environment.name == "development"
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.environment.name == "production"
    }
}
