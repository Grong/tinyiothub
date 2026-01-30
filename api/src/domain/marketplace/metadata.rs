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
