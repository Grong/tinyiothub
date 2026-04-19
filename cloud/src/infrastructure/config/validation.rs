use std::net::{IpAddr, SocketAddr};

use super::settings::*;

/// Configuration validator
pub struct ConfigValidator;

impl ConfigValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validate the entire configuration
    pub fn validate(&self, settings: &ApplicationSettings) -> Result<(), ConfigError> {
        self.validate_server(&settings.server)?;
        self.validate_database(&settings.database)?;
        self.validate_mqtt(&settings.mqtt)?;
        self.validate_logging(&settings.logging)?;
        self.validate_security(&settings.security)?;
        self.validate_network(&settings.network)?;
        self.validate_device(&settings.device)?;
        self.validate_monitoring(&settings.monitoring)?;

        Ok(())
    }

    /// Validate server configuration
    fn validate_server(&self, config: &ServerConfig) -> Result<(), ConfigError> {
        // Validate host
        if config.host.is_empty() {
            return Err(ConfigError::ValidationError(
                "Server host cannot be empty".to_string(),
            ));
        }

        // Try to parse as IP address or hostname
        let localhost_variants = ["0.0.0.0", "localhost", "127.0.0.1"];
        if !localhost_variants.contains(&config.host.as_str())
            && config.host.parse::<IpAddr>().is_err() {
                // If not a valid IP, assume it's a hostname (basic validation)
                if !self.is_valid_hostname(&config.host) {
                    return Err(ConfigError::ValidationError(format!(
                        "Invalid server host: {}",
                        config.host
                    )));
                }
            }

        // Validate port
        if config.port == 0 {
            return Err(ConfigError::ValidationError(
                "Server port cannot be 0".to_string(),
            ));
        }

        // Validate socket address combination
        let socket_addr = format!("{}:{}", config.host, config.port);
        if socket_addr.parse::<SocketAddr>().is_err() {
            // Only validate if host is an IP address
            if config.host.parse::<IpAddr>().is_ok() {
                return Err(ConfigError::ValidationError(format!(
                    "Invalid server address: {}",
                    socket_addr
                )));
            }
        }

        // Validate CORS configuration for production
        if config.enable_cors {
            // Check if running in production (you may need to pass environment info)
            // For now, we'll warn about wildcard CORS
            if config.cors_origins.contains(&"*".to_string()) {
                tracing::warn!(
                    "CORS wildcard (*) is enabled. This is not recommended for production environments."
                );
                // Uncomment to enforce in production:
                // if std::env::var("ENVIRONMENT").unwrap_or_default() == "production" {
                //     return Err(ConfigError::ValidationError(
                //         "CORS wildcard (*) is not allowed in production. Please specify allowed origins.".to_string(),
                //     ));
                // }
            }
        }

        // Validate max connections
        if config.max_connections == 0 {
            return Err(ConfigError::ValidationError(
                "Max connections must be greater than 0".to_string(),
            ));
        }

        // Validate timeout
        if config.request_timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Request timeout must be greater than 0".to_string(),
            ));
        }

        // Validate directories
        if !config.static_files_dir.is_absolute() && !config.static_files_dir.starts_with(".") {
            // Allow relative paths starting with "."
        }

        if !config.upload_dir.is_absolute() && !config.upload_dir.starts_with(".") {
            // Allow relative paths starting with "."
        }

        Ok(())
    }

    /// Validate database configuration
    fn validate_database(&self, config: &DatabaseConfig) -> Result<(), ConfigError> {
        // Validate URL
        if config.url.is_empty() {
            return Err(ConfigError::ValidationError(
                "Database URL cannot be empty".to_string(),
            ));
        }

        // Validate connection pool settings
        if config.max_connections == 0 {
            return Err(ConfigError::ValidationError(
                "Database max connections must be greater than 0".to_string(),
            ));
        }

        if config.min_connections > config.max_connections {
            return Err(ConfigError::ValidationError(
                "Database min connections cannot be greater than max connections".to_string(),
            ));
        }

        // Validate timeouts
        if config.connect_timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Database connect timeout must be greater than 0".to_string(),
            ));
        }

        if config.query_timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Database query timeout must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate MQTT configuration
    fn validate_mqtt(&self, config: &MqttConfig) -> Result<(), ConfigError> {
        // Validate primary broker
        self.validate_mqtt_broker(&config.primary, "primary")?;

        // Validate secondary broker if present
        if let Some(ref secondary) = config.secondary {
            self.validate_mqtt_broker(secondary, "secondary")?;
        }

        // Validate client configuration
        self.validate_mqtt_client(&config.client)?;

        // Validate topic configuration
        self.validate_mqtt_topics(&config.topics)?;

        Ok(())
    }

    /// Validate MQTT broker configuration
    fn validate_mqtt_broker(
        &self,
        config: &MqttBrokerConfig,
        name: &str,
    ) -> Result<(), ConfigError> {
        // Validate host
        if config.host.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "MQTT {} broker host cannot be empty",
                name
            )));
        }

        // Validate port
        if config.port == 0 {
            return Err(ConfigError::ValidationError(format!(
                "MQTT {} broker port cannot be 0",
                name
            )));
        }

        // Check for default password
        if let Some(ref password) = config.password {
            if password == "password" || password == "admin" || password == "123456" {
                return Err(ConfigError::ValidationError(format!(
                    "MQTT {} broker password must be changed from default value",
                    name
                )));
            }
        }

        // Validate timeouts
        if config.connect_timeout_secs == 0 {
            return Err(ConfigError::ValidationError(format!(
                "MQTT {} broker connect timeout must be greater than 0",
                name
            )));
        }

        if config.keep_alive_secs == 0 {
            return Err(ConfigError::ValidationError(format!(
                "MQTT {} broker keep alive must be greater than 0",
                name
            )));
        }

        // Validate TLS configuration
        if config.use_tls {
            if let Some(ref cert_path) = config.tls_cert_path {
                if !cert_path.exists() {
                    return Err(ConfigError::ValidationError(format!(
                        "MQTT {} broker TLS certificate file not found: {:?}",
                        name, cert_path
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate MQTT client configuration
    fn validate_mqtt_client(&self, config: &MqttClientConfig) -> Result<(), ConfigError> {
        // Validate client ID
        if config.client_id.is_empty() {
            return Err(ConfigError::ValidationError(
                "MQTT client ID cannot be empty".to_string(),
            ));
        }

        // Validate reconnect settings
        if config.auto_reconnect && config.max_reconnect_attempts == 0 {
            return Err(ConfigError::ValidationError(
                "MQTT max reconnect attempts must be greater than 0 when auto reconnect is enabled"
                    .to_string(),
            ));
        }

        if config.auto_reconnect && config.reconnect_delay_secs == 0 {
            return Err(ConfigError::ValidationError(
                "MQTT reconnect delay must be greater than 0 when auto reconnect is enabled"
                    .to_string(),
            ));
        }

        // Validate message queue size
        if config.message_queue_size == 0 {
            return Err(ConfigError::ValidationError(
                "MQTT message queue size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate MQTT topic configuration
    fn validate_mqtt_topics(&self, config: &MqttTopicConfig) -> Result<(), ConfigError> {
        // Validate topic names
        let topics = [
            (&config.prefix, "prefix"),
            (&config.heartbeat, "heartbeat"),
            (&config.device_registration, "device_registration"),
            (&config.command, "command"),
            (&config.data_upload, "data_upload"),
            (&config.alarm, "alarm"),
        ];

        for (topic, name) in topics.iter() {
            if topic.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "MQTT {} topic cannot be empty",
                    name
                )));
            }

            // Basic MQTT topic validation
            if topic.contains('#') && !topic.ends_with('#') {
                return Err(ConfigError::ValidationError(format!(
                    "MQTT {} topic: '#' wildcard must be at the end",
                    name
                )));
            }

            if topic.contains('+')
                && topic
                    .split('/')
                    .any(|part| part.contains('+') && part != "+")
            {
                return Err(ConfigError::ValidationError(format!(
                    "MQTT {} topic: '+' wildcard must be a complete topic level",
                    name
                )));
            }
        }

        // Validate QoS levels
        if config.publish_qos > 2 {
            return Err(ConfigError::ValidationError(
                "MQTT publish QoS must be 0, 1, or 2".to_string(),
            ));
        }

        if config.subscribe_qos > 2 {
            return Err(ConfigError::ValidationError(
                "MQTT subscribe QoS must be 0, 1, or 2".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate logging configuration
    fn validate_logging(&self, config: &LoggingConfig) -> Result<(), ConfigError> {
        // Validate log level
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&config.level.as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "Invalid log level: {}. Must be one of: {:?}",
                config.level, valid_levels
            )));
        }

        // Validate log format
        let valid_formats = ["json", "text"];
        if !valid_formats.contains(&config.format.as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "Invalid log format: {}. Must be one of: {:?}",
                config.format, valid_formats
            )));
        }

        // Validate file settings
        if config.file_enabled {
            if config.max_file_size_mb == 0 {
                return Err(ConfigError::ValidationError(
                    "Log max file size must be greater than 0".to_string(),
                ));
            }

            if config.max_files == 0 {
                return Err(ConfigError::ValidationError(
                    "Log max files must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate security configuration
    fn validate_security(&self, config: &SecurityConfig) -> Result<(), ConfigError> {
        // Validate JWT configuration
        if config.jwt.secret.is_empty() {
            return Err(ConfigError::ValidationError(
                "JWT secret cannot be empty".to_string(),
            ));
        }

        // Check for default JWT secret
        if config.jwt.secret.contains("your-secret-key") 
            || config.jwt.secret.contains("default") 
            || config.jwt.secret == "change-this-secret" {
            return Err(ConfigError::ValidationError(
                "JWT secret must be changed from default value. Please set a secure secret key.".to_string(),
            ));
        }

        if config.jwt.secret.len() < 32 {
            return Err(ConfigError::ValidationError(
                "JWT secret must be at least 32 characters long".to_string(),
            ));
        }

        if config.jwt.expiration_secs == 0 {
            return Err(ConfigError::ValidationError(
                "JWT expiration must be greater than 0".to_string(),
            ));
        }

        // Validate password policy
        if config.password_policy.min_length < 4 {
            return Err(ConfigError::ValidationError(
                "Password minimum length must be at least 4".to_string(),
            ));
        }

        // Validate session configuration
        if config.session.timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Session timeout must be greater than 0".to_string(),
            ));
        }

        if config.session.max_concurrent_sessions == 0 {
            return Err(ConfigError::ValidationError(
                "Max concurrent sessions must be greater than 0".to_string(),
            ));
        }

        // Validate rate limiting
        if config.rate_limiting.enabled {
            if config.rate_limiting.requests_per_minute == 0 {
                return Err(ConfigError::ValidationError(
                    "Rate limiting requests per minute must be greater than 0".to_string(),
                ));
            }

            if config.rate_limiting.auth_requests_per_minute == 0 {
                return Err(ConfigError::ValidationError(
                    "Rate limiting auth requests per minute must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate network configuration
    fn validate_network(&self, config: &NetworkConfig) -> Result<(), ConfigError> {
        // Validate static IP configuration
        if !config.interface.use_dhcp {
            if let Some(ref ip) = config.interface.static_ip {
                if ip.parse::<IpAddr>().is_err() {
                    return Err(ConfigError::ValidationError(format!(
                        "Invalid static IP address: {}",
                        ip
                    )));
                }
            } else {
                return Err(ConfigError::ValidationError(
                    "Static IP must be specified when DHCP is disabled".to_string(),
                ));
            }

            if let Some(ref gateway) = config.interface.gateway {
                if gateway.parse::<IpAddr>().is_err() {
                    return Err(ConfigError::ValidationError(format!(
                        "Invalid gateway address: {}",
                        gateway
                    )));
                }
            }
        }

        // Validate DNS configuration
        if config.dns.primary.parse::<IpAddr>().is_err() {
            return Err(ConfigError::ValidationError(format!(
                "Invalid primary DNS address: {}",
                config.dns.primary
            )));
        }

        if let Some(ref secondary) = config.dns.secondary {
            if secondary.parse::<IpAddr>().is_err() {
                return Err(ConfigError::ValidationError(format!(
                    "Invalid secondary DNS address: {}",
                    secondary
                )));
            }
        }

        // Validate NTP configuration
        if config.ntp.enabled && config.ntp.servers.is_empty() {
            return Err(ConfigError::ValidationError(
                "NTP servers cannot be empty when NTP is enabled".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate device configuration
    fn validate_device(&self, config: &DeviceConfig) -> Result<(), ConfigError> {
        // Validate serial number (if not empty)
        if !config.serial_number.is_empty() && config.serial_number.len() < 3 {
            return Err(ConfigError::ValidationError(
                "Device serial number must be at least 3 characters long".to_string(),
            ));
        }

        // Validate device name
        if config.name.is_empty() {
            return Err(ConfigError::ValidationError(
                "Device name cannot be empty".to_string(),
            ));
        }

        // Validate hardware configuration
        self.validate_hardware(&config.hardware)?;

        // Validate driver configuration
        if config.drivers.timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Driver timeout must be greater than 0".to_string(),
            ));
        }

        if config.drivers.retry_delay_ms == 0 {
            return Err(ConfigError::ValidationError(
                "Driver retry delay must be greater than 0".to_string(),
            ));
        }

        // Validate data collection configuration
        if config.data_collection.interval_secs == 0 {
            return Err(ConfigError::ValidationError(
                "Data collection interval must be greater than 0".to_string(),
            ));
        }

        if config.data_collection.batch_size == 0 {
            return Err(ConfigError::ValidationError(
                "Data collection batch size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate hardware configuration
    fn validate_hardware(&self, config: &HardwareConfig) -> Result<(), ConfigError> {
        // Validate serial ports
        for (i, port) in config.serial_ports.iter().enumerate() {
            if port.name.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} name cannot be empty",
                    i
                )));
            }

            if port.device_path.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} device path cannot be empty",
                    i
                )));
            }

            if port.baud_rate == 0 {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} baud rate must be greater than 0",
                    i
                )));
            }

            // Validate data bits
            if ![5, 6, 7, 8].contains(&port.data_bits) {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} data bits must be 5, 6, 7, or 8",
                    i
                )));
            }

            // Validate stop bits
            if ![1, 2].contains(&port.stop_bits) {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} stop bits must be 1 or 2",
                    i
                )));
            }

            // Validate parity
            let valid_parity = ["none", "even", "odd"];
            if !valid_parity.contains(&port.parity.as_str()) {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} parity must be one of: {:?}",
                    i, valid_parity
                )));
            }

            // Validate flow control
            let valid_flow_control = ["none", "hardware", "software"];
            if !valid_flow_control.contains(&port.flow_control.as_str()) {
                return Err(ConfigError::ValidationError(format!(
                    "Serial port {} flow control must be one of: {:?}",
                    i, valid_flow_control
                )));
            }
        }

        // Validate display configuration
        if let Some(ref display) = config.display {
            if display.width == 0 || display.height == 0 {
                return Err(ConfigError::ValidationError(
                    "Display width and height must be greater than 0".to_string(),
                ));
            }

            let valid_display_types = ["oled", "lcd", "e-ink"];
            if !valid_display_types.contains(&display.display_type.as_str()) {
                return Err(ConfigError::ValidationError(format!(
                    "Display type must be one of: {:?}",
                    valid_display_types
                )));
            }
        }

        // Validate LED configuration
        for (i, led) in config.leds.iter().enumerate() {
            if led.name.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "LED {} name cannot be empty",
                    i
                )));
            }
        }

        Ok(())
    }

    /// Validate monitoring configuration
    fn validate_monitoring(&self, config: &MonitoringConfig) -> Result<(), ConfigError> {
        // Validate health check configuration
        if config.health_check.enabled {
            if config.health_check.interval_secs == 0 {
                return Err(ConfigError::ValidationError(
                    "Health check interval must be greater than 0".to_string(),
                ));
            }

            if config.health_check.timeout_secs == 0 {
                return Err(ConfigError::ValidationError(
                    "Health check timeout must be greater than 0".to_string(),
                ));
            }

            if config.health_check.timeout_secs >= config.health_check.interval_secs {
                return Err(ConfigError::ValidationError(
                    "Health check timeout must be less than interval".to_string(),
                ));
            }
        }

        // Validate metrics configuration
        if config.metrics.enabled {
            if config.metrics.collection_interval_secs == 0 {
                return Err(ConfigError::ValidationError(
                    "Metrics collection interval must be greater than 0".to_string(),
                ));
            }

            let valid_formats = ["prometheus", "json"];
            if !valid_formats.contains(&config.metrics.export_format.as_str()) {
                return Err(ConfigError::ValidationError(format!(
                    "Metrics export format must be one of: {:?}",
                    valid_formats
                )));
            }
        }

        // Validate alerting configuration
        if config.alerting.enabled {
            if config.alerting.check_interval_secs == 0 {
                return Err(ConfigError::ValidationError(
                    "Alerting check interval must be greater than 0".to_string(),
                ));
            }

            // Validate alert channels
            for (i, channel) in config.alerting.channels.iter().enumerate() {
                if channel.name.is_empty() {
                    return Err(ConfigError::ValidationError(format!(
                        "Alert channel {} name cannot be empty",
                        i
                    )));
                }

                let valid_channel_types = ["email", "webhook", "mqtt", "sms"];
                if !valid_channel_types.contains(&channel.channel_type.as_str()) {
                    return Err(ConfigError::ValidationError(format!(
                        "Alert channel {} type must be one of: {:?}",
                        i, valid_channel_types
                    )));
                }
            }

            // Validate alert rules
            for (i, rule) in config.alerting.rules.iter().enumerate() {
                if rule.name.is_empty() {
                    return Err(ConfigError::ValidationError(format!(
                        "Alert rule {} name cannot be empty",
                        i
                    )));
                }

                if rule.condition.is_empty() {
                    return Err(ConfigError::ValidationError(format!(
                        "Alert rule {} condition cannot be empty",
                        i
                    )));
                }

                let valid_severities = ["low", "medium", "high", "critical"];
                if !valid_severities.contains(&rule.severity.as_str()) {
                    return Err(ConfigError::ValidationError(format!(
                        "Alert rule {} severity must be one of: {:?}",
                        i, valid_severities
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if a hostname is valid (basic validation)
    fn is_valid_hostname(&self, hostname: &str) -> bool {
        if hostname.is_empty() || hostname.len() > 253 {
            return false;
        }

        // Basic hostname validation
        hostname
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
            && !hostname.starts_with('-')
            && !hostname.ends_with('-')
            && !hostname.starts_with('.')
            && !hostname.ends_with('.')
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}
