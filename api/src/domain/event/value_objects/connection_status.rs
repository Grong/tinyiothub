use serde::{Deserialize, Serialize};

/// Device connection status value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// Device is online and responding
    Online,
    /// Device is offline but not in error state
    #[default]
    Offline,
    /// Device connection is in error state
    Error,
}

impl ConnectionStatus {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionStatus::Online => "online",
            ConnectionStatus::Offline => "offline",
            ConnectionStatus::Error => "error",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "online" => Some(ConnectionStatus::Online),
            "offline" => Some(ConnectionStatus::Offline),
            "error" => Some(ConnectionStatus::Error),
            _ => None,
        }
    }

    /// Check if the status indicates the device is available
    pub fn is_available(&self) -> bool {
        matches!(self, ConnectionStatus::Online)
    }

    /// Check if the status indicates an error condition
    pub fn is_error(&self) -> bool {
        matches!(self, ConnectionStatus::Error)
    }
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_status_string_conversion() {
        assert_eq!(ConnectionStatus::Online.as_str(), "online");
        assert_eq!(ConnectionStatus::Offline.as_str(), "offline");
        assert_eq!(ConnectionStatus::Error.as_str(), "error");

        assert_eq!(
            ConnectionStatus::from_str("online"),
            Some(ConnectionStatus::Online)
        );
        assert_eq!(
            ConnectionStatus::from_str("OFFLINE"),
            Some(ConnectionStatus::Offline)
        );
        assert_eq!(ConnectionStatus::from_str("invalid"), None);
    }

    #[test]
    fn test_connection_status_properties() {
        assert!(ConnectionStatus::Online.is_available());
        assert!(!ConnectionStatus::Offline.is_available());
        assert!(!ConnectionStatus::Error.is_available());

        assert!(!ConnectionStatus::Online.is_error());
        assert!(!ConnectionStatus::Offline.is_error());
        assert!(ConnectionStatus::Error.is_error());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ConnectionStatus::Online), "online");
        assert_eq!(format!("{}", ConnectionStatus::Error), "error");
    }

    #[test]
    fn test_default() {
        assert_eq!(ConnectionStatus::default(), ConnectionStatus::Offline);
    }
}
