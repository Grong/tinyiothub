use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 模板元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    pub protocol: String,
    pub manufacturer: String,
    pub description: String,
    pub tags: Vec<String>,
    pub author: AuthorInfo,
    #[serde(default)]
    pub icon: Option<String>,
    pub downloads: u64,
    pub rating: f32,
    pub reviews: u32,
    pub license: String,
    pub file_url: String,
    pub checksum: String,
    pub size: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// 驱动元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: String,
    pub description: String,
    pub tags: Vec<String>,
    pub author: AuthorInfo,
    #[serde(default)]
    pub icon: Option<String>,
    pub downloads: u64,
    pub rating: f32,
    pub reviews: u32,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
    pub platforms: HashMap<String, PlatformBinary>,
    pub requirements: DriverRequirements,
    pub created_at: String,
    pub updated_at: String,
}

/// 平台二进制文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBinary {
    pub file_url: String,
    pub checksum: String,
    pub size: u64,
}

/// 作者信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub email: String,
}

/// 驱动要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverRequirements {
    pub min_version: String,
}

/// 市场索引（模板）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateIndex {
    pub version: String,
    pub updated_at: String,
    pub templates: Vec<TemplateMetadata>,
}

/// 市场索引（驱动）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverIndex {
    pub version: String,
    pub updated_at: String,
    pub drivers: Vec<DriverMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_metadata_serialization() {
        let metadata = TemplateMetadata {
            id: "tmpl-001".to_string(),
            name: "Temperature Sensor".to_string(),
            version: "1.0.0".to_string(),
            category: "Sensors".to_string(),
            protocol: "Modbus".to_string(),
            manufacturer: "Acme Inc".to_string(),
            description: "A temperature sensor template".to_string(),
            tags: vec!["temperature".to_string(), "sensor".to_string()],
            author: AuthorInfo {
                name: "John Doe".to_string(),
                email: "john@example.com".to_string(),
            },
            icon: None,
            downloads: 1000,
            rating: 4.5,
            reviews: 50,
            license: "MIT".to_string(),
            file_url: "https://example.com/templates/temp-sensor.tar.gz".to_string(),
            checksum: "abc123".to_string(),
            size: 1024,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-15T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("tmpl-001"));
        assert!(json.contains("Temperature Sensor"));

        let deserialized: TemplateMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "tmpl-001");
        assert_eq!(deserialized.name, "Temperature Sensor");
    }

    #[test]
    fn test_driver_metadata_serialization() {
        let mut platforms = HashMap::new();
        platforms.insert(
            "x86_64-unknown-linux-gnu".to_string(),
            PlatformBinary {
                file_url: "https://example.com/driver-linux.tar.gz".to_string(),
                checksum: "def456".to_string(),
                size: 2048,
            },
        );

        let metadata = DriverMetadata {
            id: "drv-001".to_string(),
            name: "Modbus Driver".to_string(),
            version: "2.0.0".to_string(),
            protocol: "Modbus".to_string(),
            description: "A Modbus RTU/TCP driver".to_string(),
            tags: vec!["modbus".to_string(), "serial".to_string()],
            author: AuthorInfo {
                name: "Jane Doe".to_string(),
                email: "jane@example.com".to_string(),
            },
            icon: Some("icon.png".to_string()),
            downloads: 500,
            rating: 4.8,
            reviews: 30,
            license: "Apache-2.0".to_string(),
            homepage: Some("https://example.com".to_string()),
            documentation: Some("https://docs.example.com".to_string()),
            platforms,
            requirements: DriverRequirements {
                min_version: "1.0.0".to_string(),
            },
            created_at: "2024-02-01T00:00:00Z".to_string(),
            updated_at: "2024-02-10T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("drv-001"));
        assert!(json.contains("Modbus Driver"));

        let deserialized: DriverMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "drv-001");
        assert_eq!(deserialized.name, "Modbus Driver");
        assert!(deserialized.platforms.contains_key("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn test_template_index() {
        let index = TemplateIndex {
            version: "1.0.0".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            templates: vec![
                TemplateMetadata {
                    id: "tmpl-001".to_string(),
                    name: "Template 1".to_string(),
                    version: "1.0.0".to_string(),
                    category: "Sensors".to_string(),
                    protocol: "Modbus".to_string(),
                    manufacturer: "Acme".to_string(),
                    description: "Description".to_string(),
                    tags: vec![],
                    author: AuthorInfo {
                        name: "Author".to_string(),
                        email: "author@example.com".to_string(),
                    },
                    icon: None,
                    downloads: 0,
                    rating: 0.0,
                    reviews: 0,
                    license: "MIT".to_string(),
                    file_url: "".to_string(),
                    checksum: "".to_string(),
                    size: 0,
                    created_at: "".to_string(),
                    updated_at: "".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&index).unwrap();
        let deserialized: TemplateIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.templates.len(), 1);
    }

    #[test]
    fn test_driver_index() {
        let index = DriverIndex {
            version: "1.0.0".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            drivers: vec![],
        };

        let json = serde_json::to_string(&index).unwrap();
        let deserialized: DriverIndex = serde_json::from_str(&json).unwrap();
        assert!(deserialized.drivers.is_empty());
    }
}
