use serde::{Deserialize, Serialize};

/// 产品实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Product {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 产品查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ProductQueryParams {
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建产品请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateProductRequest {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
}

/// 更新产品请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
}
