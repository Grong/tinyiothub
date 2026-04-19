use serde::{Deserialize, Serialize};

/// 订阅计划
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubscriptionPlan {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub device_limit: i32,
    pub api_call_limit: i32,
    pub storage_mb: i32,
    pub user_limit: i32,
    pub price_monthly: f64,
    pub price_yearly: f64,
    pub features: String,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// 租户
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan_id: String,
    pub subscription_status: String,
    pub trial_expires_at: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: String,
    pub locale: String,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 租户查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TenantQueryParams {
    pub status: Option<String>,
    pub plan_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建租户请求
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

/// 更新租户请求
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

/// 租户使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TenantUsage {
    pub id: String,
    pub tenant_id: String,
    pub device_count: i32,
    pub api_call_count: i32,
    pub api_call_reset_at: Option<String>,
    pub storage_used_bytes: i64,
    pub user_count: i32,
    pub total_api_calls: i64,
    pub total_api_errors: i64,
    pub updated_at: String,
}

/// API Key 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiKey {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub permissions: String,
    pub rate_limit: i32,
    pub is_enabled: bool,
    pub is_revoked: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub request_count: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建 API Key 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyRequest {
    pub workspace_id: String,
    pub name: String,
    pub permissions: Option<Vec<String>>,
    pub rate_limit: Option<i32>,
    pub expires_in_days: Option<i32>,
}

/// API 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiUsageStats {
    pub total_calls: i64,
    pub success_calls: i64,
    pub error_calls: i64,
    pub avg_latency_ms: f64,
    pub period_start: String,
    pub period_end: String,
}
