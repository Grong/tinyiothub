use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Invalid checksum: expected {expected}, got {actual}")]
    InvalidChecksum { expected: String, actual: String },
    
    #[error("Platform not supported: {0}")]
    PlatformNotSupported(String),
    
    #[error("Marketplace is disabled")]
    Disabled,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Installation failed: {0}")]
    InstallationFailed(String),
    
    #[error("Template error: {0}")]
    Template(String),
    
    #[error("Driver error: {0}")]
    Driver(String),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_error_display() {
        let err = MarketplaceError::NotFound("template-001".to_string());
        assert_eq!(format!("{}", err), "Resource not found: template-001");

        let err = MarketplaceError::InvalidChecksum {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Invalid checksum: expected abc123, got def456"
        );

        let err = MarketplaceError::PlatformNotSupported("windows".to_string());
        assert_eq!(format!("{}", err), "Platform not supported: windows");

        let err = MarketplaceError::Disabled;
        assert_eq!(format!("{}", err), "Marketplace is disabled");

        let err = MarketplaceError::InvalidConfig("Missing API key".to_string());
        assert_eq!(format!("{}", err), "Invalid configuration: Missing API key");

        let err = MarketplaceError::InstallationFailed("File not found".to_string());
        assert_eq!(format!("{}", err), "Installation failed: File not found");
    }

    #[test]
    fn test_marketplace_error_source() {
        // Test that errors from thiserror work correctly
        let err = MarketplaceError::NotFound("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
