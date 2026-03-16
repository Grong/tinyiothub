//! Infrastructure Tests
//! 基础设施单元测试

// ==================== Config Tests ====================

#[cfg(test)]
mod config_tests {
    use crate::infrastructure::config::{Settings, AppSettings};
    use std::collections::HashMap;

    #[test]
    fn test_app_settings_default() {
        let settings = AppSettings::default();
        assert_eq!(settings.name, "TinyIoTHub");
    }

    #[test]
    fn test_settings_validation() {
        // Create valid settings
        let settings = Settings {
            app: AppSettings {
                name: "TinyIoTHub".to_string(),
                env: "test".to_string(),
                debug: true,
            },
            server: crate::infrastructure::config::ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                ..Default::default()
            },
            database: crate::infrastructure::config::DatabaseConfig {
                url: "sqlite:test.db".to_string(),
                ..Default::default()
            },
            security: crate::infrastructure::config::SecurityConfig {
                jwt: crate::infrastructure::config::JwtConfig {
                    secret: "this-is-a-very-long-secret-key-12345".to_string(),
                    expiration: 3600,
                },
                ..Default::default()
            },
            ..Default::default()
        };
        
        // Should not panic with valid config
        assert_eq!(settings.app.name, "TinyIoTHub");
    }
}

// ==================== Error Tests ====================

#[cfg(test)]
mod error_tests {
    use crate::shared::error::Error;

    #[test]
    fn test_error_display() {
        let err = Error::NotFound("Device not found".to_string());
        assert!(err.to_string().contains("Device not found"));
    }

    #[test]
    fn test_error_from_str() {
        let err = Error::from("test error");
        assert!(err.to_string().contains("test error"));
    }
}

// ==================== Utils Tests ====================

#[cfg(test)]
mod utils_tests {
    use crate::utils::password::{hash_password, verify_password};

    #[test]
    fn test_password_hash_and_verify() {
        let password = "test_password_123";
        let hashed = hash_password(password).unwrap();
        
        assert!(verify_password(password, &hashed).is_ok());
        assert!(verify_password("wrong_password", &hashed).is_err());
    }

    #[test]
    fn test_password_hash_unique() {
        let password = "test_password";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();
        
        // Same password should produce different hashes due to salt
        assert_ne!(hash1, hash2);
    }
}
