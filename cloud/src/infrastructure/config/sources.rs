use std::fs;
use std::path::Path;

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use super::settings::*;

/// Configuration source trait
pub trait ConfigSource {
    /// Load configuration from this source
    fn load(&self) -> Result<ApplicationSettings, ConfigError>;

    /// Check if this source is available
    fn is_available(&self) -> bool;

    /// Get source name for logging
    fn name(&self) -> &str;
}

/// File-based configuration source
pub struct FileSource {
    path: String,
    format: FileFormat,
}

/// Supported file formats
#[derive(Debug, Clone)]
pub enum FileFormat {
    Toml,
    Json,
}

impl FileSource {
    /// Create a new file source
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let format = Self::detect_format(&path);

        Self { path, format }
    }

    /// Create a TOML file source
    pub fn toml(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            format: FileFormat::Toml,
        }
    }

    /// Create a JSON file source
    pub fn json(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            format: FileFormat::Json,
        }
    }

    /// Detect file format from extension
    fn detect_format(path: &str) -> FileFormat {
        let path = Path::new(path);

        match path.extension().and_then(|ext| ext.to_str()) {
            Some("toml") => FileFormat::Toml,
            Some("json") => FileFormat::Json,
            _ => FileFormat::Toml, // Default to TOML
        }
    }

    /// Load TOML configuration
    fn load_toml(&self) -> Result<ApplicationSettings, ConfigError> {
        let content = fs::read_to_string(&self.path)
            .map_err(|_| ConfigError::FileNotFound(self.path.clone()))?;

        let toml_value: TomlValue = content
            .parse()
            .map_err(|e| ConfigError::ParseError(format!("TOML parse error: {}", e)))?;

        self.convert_toml_to_settings(toml_value)
    }

    /// Load JSON configuration
    fn load_json(&self) -> Result<ApplicationSettings, ConfigError> {
        let content = fs::read_to_string(&self.path)
            .map_err(|_| ConfigError::FileNotFound(self.path.clone()))?;

        let json_value: JsonValue = serde_json::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("JSON parse error: {}", e)))?;

        self.convert_json_to_settings(json_value)
    }

    /// Convert TOML value to application settings
    fn convert_toml_to_settings(
        &self,
        toml: TomlValue,
    ) -> Result<ApplicationSettings, ConfigError> {
        // Try direct deserialization from TOML value
        if let Ok(settings) = toml.clone().try_into::<ApplicationSettings>() {
            return Ok(settings);
        }

        // Fall back to legacy format conversion
        let mut settings = ApplicationSettings::default();

        if let TomlValue::Table(table) = toml {
            self.apply_toml_overrides(&mut settings, &table)?;
        }

        Ok(settings)
    }

    /// Convert JSON value to application settings
    fn convert_json_to_settings(
        &self,
        json: JsonValue,
    ) -> Result<ApplicationSettings, ConfigError> {
        // Try to deserialize directly first
        if let Ok(settings) = serde_json::from_value::<ApplicationSettings>(json.clone()) {
            return Ok(settings);
        }

        // If direct deserialization fails, apply as overrides
        let mut settings = ApplicationSettings::default();

        if let JsonValue::Object(obj) = json {
            self.apply_json_overrides(&mut settings, &obj)?;
        }

        Ok(settings)
    }

    /// Apply TOML overrides to settings
    fn apply_toml_overrides(
        &self,
        settings: &mut ApplicationSettings,
        table: &toml::map::Map<String, TomlValue>,
    ) -> Result<(), ConfigError> {
        // Server configuration
        if let Some(host) = table.get("host_port").and_then(|v| v.as_integer()) {
            settings.server.port = host as u16;
        }

        if let Some(host) = table.get("serve_host_port").and_then(|v| v.as_integer()) {
            settings.server.port = host as u16;
        }

        // Database configuration
        if let Some(db_url) = table.get("connection_string").and_then(|v| v.as_str()) {
            settings.database.url = db_url.to_string();
        }

        // MQTT configuration
        if let Some(mqtt_host) = table.get("mqtt_host").and_then(|v| v.as_str()) {
            settings.mqtt.primary.host = mqtt_host.to_string();
        }

        if let Some(mqtt_port) = table.get("mqtt_port").and_then(|v| v.as_integer()) {
            settings.mqtt.primary.port = mqtt_port as u16;
        }

        if let Some(mqtt_user) = table.get("mqtt_usr").and_then(|v| v.as_str()) {
            settings.mqtt.primary.username = Some(mqtt_user.to_string());
        }

        if let Some(mqtt_pwd) = table.get("mqtt_pwd").and_then(|v| v.as_str()) {
            settings.mqtt.primary.password = Some(mqtt_pwd.to_string());
        }

        if let Some(_mqtt_enable) = table.get("mqtt_enable").and_then(|v| v.as_integer()) {
            // Convert to boolean (1 = true, 0 = false)
            // This affects whether MQTT is enabled, but we don't have a direct field for this
            // We could add it to the MQTT config or use it to set the secondary broker
        }

        // 4G MQTT configuration (secondary broker)
        if let Some(mqtt_enable_4g) = table.get("mqtt_enable_4g").and_then(|v| v.as_integer()) {
            if mqtt_enable_4g == 1 {
                let mut secondary = MqttBrokerConfig {
                    host: std::env::var("MQTT_SECONDARY_HOST").unwrap_or_else(|_| "localhost".to_string()),
                    port: 1883,
                    username: None,
                    password: None,
                    use_tls: false,
                    tls_cert_path: None,
                    connect_timeout_secs: 30,
                    keep_alive_secs: 60,
                };

                if let Some(host_4g) = table.get("mqtt_host_4g").and_then(|v| v.as_str()) {
                    secondary.host = host_4g.to_string();
                }

                if let Some(port_4g) = table.get("mqtt_port_4g").and_then(|v| v.as_integer()) {
                    secondary.port = port_4g as u16;
                }

                if let Some(user_4g) = table.get("mqtt_usr_4g").and_then(|v| v.as_str()) {
                    secondary.username = Some(user_4g.to_string());
                }

                if let Some(pwd_4g) = table.get("mqtt_pwd_4g").and_then(|v| v.as_str()) {
                    secondary.password = Some(pwd_4g.to_string());
                }

                settings.mqtt.secondary = Some(secondary);
            }
        }

        // Timing configuration
        if let Some(heartbeat_time) = table.get("heartbeat_time").and_then(|v| v.as_integer()) {
            settings.mqtt.primary.keep_alive_secs = heartbeat_time as u64;
        }

        if let Some(upload_time) = table.get("upload_time").and_then(|v| v.as_integer()) {
            settings.device.data_collection.interval_secs = upload_time as u64;
        }

        // Logging configuration
        if let Some(log_enable) = table.get("app_log_enable").and_then(|v| v.as_integer()) {
            settings.logging.console_enabled = log_enable == 1;
            settings.logging.file_enabled = log_enable == 1;
        }

        if let Some(log_level) = table.get("app_log_level").and_then(|v| v.as_str()) {
            settings.logging.level = log_level.to_string();
        }

        // Network configuration
        if let Some(udp_server) = table.get("udp_server").and_then(|v| v.as_str()) {
            // This could be used for network configuration
            settings.network.interface.gateway = Some(udp_server.to_string());
        }

        if let Some(udp_port) = table.get("udp_port").and_then(|v| v.as_integer()) {
            // Store UDP port in environment overrides
            settings.environment.overrides.insert(
                "udp_port".to_string(),
                serde_json::Value::Number(serde_json::Number::from(udp_port)),
            );
        }

        // Update configuration
        if let Some(update_host) = table.get("update_host").and_then(|v| v.as_str()) {
            settings.environment.overrides.insert(
                "update_host".to_string(),
                serde_json::Value::String(update_host.to_string()),
            );
        }

        if let Some(update_port) = table.get("update_port").and_then(|v| v.as_str()) {
            settings.environment.overrides.insert(
                "update_port".to_string(),
                serde_json::Value::String(update_port.to_string()),
            );
        }

        // Message configuration
        if let Some(message_limit) = table.get("message_max_limit").and_then(|v| v.as_integer()) {
            settings.mqtt.client.message_queue_size = message_limit as usize;
        }

        // Auth time configuration
        if let Some(auth_time) = table.get("auth_time").and_then(|v| v.as_integer()) {
            settings.security.session.timeout_secs = (auth_time * 3600) as u64; // Convert hours to seconds
        }

        // 4G serial configuration
        if let Some(serial_dev_4g) = table.get("mqtt_serial_dev_4g").and_then(|v| v.as_str()) {
            let serial_config = SerialPortConfig {
                name: "4g_modem".to_string(),
                device_path: serial_dev_4g.to_string(),
                baud_rate: 115200,
                data_bits: 8,
                stop_bits: 1,
                parity: "none".to_string(),
                flow_control: "none".to_string(),
                timeout_ms: 2000,
            };

            if let Some(timeout) = table
                .get("mqtt_serial_timeout_secs_4g")
                .and_then(|v| v.as_integer())
            {
                // Update timeout in the config
                let mut config = serial_config;
                config.timeout_ms = (timeout * 1000) as u64;
                settings.device.hardware.serial_ports.push(config);
            } else {
                settings.device.hardware.serial_ports.push(serial_config);
            }
        }

        // ADP 4G configuration
        if let Some(adp_4g) = table.get("mqtt_adp_4g").and_then(|v| v.as_str()) {
            settings.environment.overrides.insert(
                "mqtt_adp_4g".to_string(),
                serde_json::Value::String(adp_4g.to_string()),
            );
        }

        Ok(())
    }

    /// Apply JSON overrides to settings
    fn apply_json_overrides(
        &self,
        _settings: &mut ApplicationSettings,
        _obj: &serde_json::Map<String, JsonValue>,
    ) -> Result<(), ConfigError> {
        // Similar to TOML overrides but for JSON format
        // For now, this is a placeholder
        Ok(())
    }
}

impl ConfigSource for FileSource {
    fn load(&self) -> Result<ApplicationSettings, ConfigError> {
        match self.format {
            FileFormat::Toml => self.load_toml(),
            FileFormat::Json => self.load_json(),
        }
    }

    fn is_available(&self) -> bool {
        Path::new(&self.path).exists()
    }

    fn name(&self) -> &str {
        &self.path
    }
}

/// Environment variable configuration source
pub struct EnvironmentSource {
    prefix: String,
}

impl EnvironmentSource {
    /// Create a new environment source with default prefix
    pub fn new() -> Self {
        Self {
            prefix: "TINYIOTHUB".to_string(),
        }
    }

    /// Create a new environment source with custom prefix
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl ConfigSource for EnvironmentSource {
    fn load(&self) -> Result<ApplicationSettings, ConfigError> {
        Ok(super::environment::load_from_environment())
    }

    fn is_available(&self) -> bool {
        // Environment variables are always available
        true
    }

    fn name(&self) -> &str {
        "environment"
    }
}

/// Default configuration source
pub struct DefaultSource;

impl ConfigSource for DefaultSource {
    fn load(&self) -> Result<ApplicationSettings, ConfigError> {
        Ok(ApplicationSettings::default())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "defaults"
    }
}

/// Memory-based configuration source for testing
pub struct MemorySource {
    settings: ApplicationSettings,
}

impl MemorySource {
    /// Create a new memory source with given settings
    pub fn new(settings: ApplicationSettings) -> Self {
        Self { settings }
    }
}

impl ConfigSource for MemorySource {
    fn load(&self) -> Result<ApplicationSettings, ConfigError> {
        Ok(self.settings.clone())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "memory"
    }
}

/// Configuration source chain for loading from multiple sources
pub struct SourceChain {
    sources: Vec<Box<dyn ConfigSource>>,
}

impl SourceChain {
    /// Create a new source chain
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Add a source to the chain
    pub fn add_source(mut self, source: Box<dyn ConfigSource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Load configuration from all sources in order
    pub fn load(&self) -> Result<ApplicationSettings, ConfigError> {
        let mut settings = ApplicationSettings::default();

        for source in &self.sources {
            if source.is_available() {
                match source.load() {
                    Ok(source_settings) => {
                        // Merge settings (later sources override earlier ones)
                        settings = self.merge_settings(settings, source_settings);
                        tracing::debug!("Loaded configuration from source: {}", source.name());
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load configuration from source {}: {}",
                            source.name(),
                            e
                        );
                        // Continue with other sources
                    }
                }
            } else {
                tracing::debug!("Configuration source not available: {}", source.name());
            }
        }

        Ok(settings)
    }

    /// Merge two configuration settings (second overrides first)
    fn merge_settings(
        &self,
        mut base: ApplicationSettings,
        override_settings: ApplicationSettings,
    ) -> ApplicationSettings {
        // This is a simplified merge - in a real implementation, you'd want to merge each field carefully
        // For now, we'll just replace non-default values

        // Server configuration
        if override_settings.server.host != "0.0.0.0" {
            base.server.host = override_settings.server.host;
        }
        if override_settings.server.port != 3002 {
            base.server.port = override_settings.server.port;
        }

        // Database configuration
        if override_settings.database.url != "tinyiothub.db" {
            base.database.url = override_settings.database.url;
        }

        // MQTT configuration
        let default_host = std::env::var("MQTT_DEFAULT_HOST").unwrap_or_else(|_| "localhost".to_string());
        if override_settings.mqtt.primary.host != default_host {
            base.mqtt.primary.host = override_settings.mqtt.primary.host;
        }
        if override_settings.mqtt.primary.port != 1883 {
            base.mqtt.primary.port = override_settings.mqtt.primary.port;
        }
        if override_settings.mqtt.primary.username.is_some() {
            base.mqtt.primary.username = override_settings.mqtt.primary.username;
        }
        if override_settings.mqtt.primary.password.is_some() {
            base.mqtt.primary.password = override_settings.mqtt.primary.password;
        }
        if override_settings.mqtt.secondary.is_some() {
            base.mqtt.secondary = override_settings.mqtt.secondary;
        }

        // Logging configuration
        if override_settings.logging.level != "info" {
            base.logging.level = override_settings.logging.level;
        }

        // Security configuration - always override if not empty and not a default value
        let default_secrets = ["your-secret-key", "change-me-in-production", "change-this-secret"];
        if !override_settings.security.jwt.secret.is_empty() 
            && !default_secrets.contains(&override_settings.security.jwt.secret.as_str()) {
            base.security.jwt.secret = override_settings.security.jwt.secret;
        }

        // Device configuration
        if !override_settings.device.serial_number.is_empty() {
            base.device.serial_number = override_settings.device.serial_number;
        }
        if override_settings.device.name != "TinyIoTHub" {
            base.device.name = override_settings.device.name;
        }

        // Environment configuration
        if override_settings.environment.name != "development" {
            base.environment.name = override_settings.environment.name;
        }

        // Merge environment overrides
        for (key, value) in override_settings.environment.overrides {
            base.environment.overrides.insert(key, value);
        }

        // Hardware configuration
        if !override_settings.device.hardware.serial_ports.is_empty() {
            base.device
                .hardware
                .serial_ports
                .extend(override_settings.device.hardware.serial_ports);
        }

        base
    }
}

impl Default for SourceChain {
    fn default() -> Self {
        Self::new()
    }
}
