use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Template metadata
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

/// Driver metadata
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

/// Platform binary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBinary {
    pub file_url: String,
    pub checksum: String,
    pub size: u64,
}

/// Author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub email: String,
}

/// Driver requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverRequirements {
    pub min_version: String,
}

/// Marketplace index (templates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateIndex {
    pub version: String,
    pub updated_at: String,
    pub templates: Vec<TemplateMetadata>,
}

/// Marketplace index (drivers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverIndex {
    pub version: String,
    pub updated_at: String,
    pub drivers: Vec<DriverMetadata>,
}
