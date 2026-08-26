//! tenant 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use serde::{Deserialize, Serialize};

/// Create tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
}

/// Update tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
}

/// Create API Key request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyRequest {
    pub workspace_id: String,
    pub name: String,
    pub permissions: Option<Vec<String>>,
    pub rate_limit: Option<i32>,
    pub expires_in_days: Option<i32>,
}
