//! Marketplace HTTP API 响应 DTO（与 marketplace 服务的 API 契约保持一致）。
//!
//! P5-Task25: 解除 cloud→marketplace 的编译期依赖 —— cloud 仅通过 HTTP 调用
//! marketplace，此处仅保留反序列化所需的响应形状（字段与
//! `apps/marketplace/src/types.rs` 对齐；serde 忽略多余 JSON 字段）。

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PaginatedList<T> {
    pub items: Vec<T>,
    #[allow(dead_code)]
    pub total: usize,
    #[allow(dead_code)]
    pub page: usize,
    #[allow(dead_code)]
    pub per_page: usize,
}

#[derive(Debug, Deserialize)]
pub struct Driver {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub author_name: String,
    #[serde(default)]
    pub author_email: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub downloads: i64,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub reviews: Option<i32>,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub platforms: Option<serde_json::Value>,
    #[serde(default)]
    pub requirements: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct LocalizedString {
    #[serde(default)]
    pub zh: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: LocalizedString,
    pub version: String,
    pub author: String,
    pub category: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    pub protocol_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub downloads: i64,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub reviews: Option<i32>,
    #[serde(default = "default_mit_license")]
    pub license: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_mit_license() -> String {
    "MIT".to_string()
}
